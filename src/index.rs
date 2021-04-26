use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryInto,
    io::{self, Read, Write},
    path::Path,
};

use crate::{locked_file, Entry, LockedFile, WithDigest, WsPath};
use bstr::BString;
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use ring::digest::SHA1_FOR_LEGACY_USE_ONLY as SHA1;

#[derive(Debug)]
pub struct Index {
    entries: BTreeMap<BString, Entry>,
    parents: BTreeMap<BString, BTreeSet<BString>>,
    lock: Option<LockedFile>,
}

impl Index {
    const SIG: &'static [u8] = b"DIRC";
    const VERSION: u32 = 2;
    const CHECKSUM_LEN: usize = 20;

    /// Creates if doesn't already exist
    pub fn load<P: AsRef<Path>>(git_dir: P) -> Result<Self, LoadError> {
        let file = git_dir.as_ref().join("index");
        let lock = LockedFile::acquire(file)?;

        let mut this = Self {
            entries: BTreeMap::new(),
            parents: BTreeMap::new(),
            lock: None,
        };

        if let Some(existing) = lock.protected_file() {
            this.load_from(existing)?;
        }

        this.lock = Some(lock);

        Ok(this)
    }

    /// Committing a virtual `Index` will panic.
    #[allow(dead_code)] // Used for tests
    pub(crate) fn new_virtual() -> Self {
        Self {
            entries: BTreeMap::new(),
            parents: BTreeMap::new(),
            lock: None,
        }
    }

    fn load_from(&mut self, mut reader: impl Read) -> Result<(), LoadError> {
        let mut input = WithDigest::new(&SHA1, &mut reader);

        let mut sig = [0; 4];
        input.read_exact(&mut sig)?; // offset 0
        if sig != Self::SIG {
            return Err(CorruptError::MissingSignature.into());
        }

        let version = input.read_u32::<NetworkEndian>()?; // offset 4
        if version != Self::VERSION {
            return Err(LoadError::UnsupportedVersion(version));
        }

        let count = input.read_u32::<NetworkEndian>()?; // offset 8

        // offset 12
        for _ in 0..count {
            let entry = Entry::parse_from_index(&mut input)?;
            self.add(entry);
        }

        let expected_checksum = input.finish();
        let expected_checksum: [u8; Self::CHECKSUM_LEN] =
            expected_checksum.as_ref().try_into().unwrap();

        let mut actual_checksum = [0; Self::CHECKSUM_LEN];
        reader.read_exact(&mut actual_checksum)?;

        if actual_checksum != expected_checksum {
            return Err(CorruptError::IncorrectChecksum.into());
        }

        Ok(())
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    pub fn add(&mut self, entry: Entry) {
        for parent in entry.path().iter_parents() {
            self.parents
                .entry(parent)
                .or_insert_with(BTreeSet::new)
                .insert(entry.path().to_bstring());
        }

        self.discard_conflicts_with(&entry.path());

        self.entries.insert(entry.key().to_owned(), entry);
    }

    fn discard_conflicts_with(&mut self, path: &WsPath) {
        // If the new entry is lib/index/foo, remove lib and index.
        for parent in path.iter_parents() {
            self.entries.remove(&parent);
        }

        // If the new entry is lib, remove lib/index/foo and lib/index
        let key = path.as_bstr();
        if let Some(conflicts) = self.parents.get(key) {
            let mut to_remove = Vec::new();

            for conflict in conflicts {
                to_remove.push(conflict.clone());
            }

            for path in to_remove {
                self.remove(&WsPath::new_unchecked_bytes(path))
                    .expect("Parents out of sync");
            }
        }
    }

    fn remove(&mut self, path: &WsPath) -> Option<Entry> {
        if let Some(entry) = self.entries.remove(path.as_bstr()) {
            for parent in path.iter_parents() {
                if let Some(children) = self.parents.get_mut(&parent) {
                    children.remove(path.as_bstr());
                    if children.is_empty() {
                        self.parents.remove(&parent);
                    }
                }
            }

            Some(entry)
        } else {
            None
        }
    }

    pub fn commit(mut self) -> Result<(), CommitError> {
        let mut lock = self.lock.take().expect("Has a lock");

        let mut out = WithDigest::new(&SHA1, &mut lock);

        out.write_all(Self::SIG)?; // offset 0
        out.write_u32::<NetworkEndian>(Self::VERSION)?; // offset 4

        let size = self.entries.len().try_into().expect("Len overflowed");
        out.write_u32::<NetworkEndian>(size)?; // offset 8

        for entry in self.entries() {
            entry.write_to_index(&mut out)?;
        }

        let hash = out.finish();
        lock.write_all(hash.as_ref())?; // offset

        lock.commit()?;

        Ok(())
    }
}

impl Eq for Index {}

impl PartialEq for Index {
    fn eq(&self, other: &Self) -> bool {
        self.entries().eq(other.entries())
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
/// Failed to load index
pub enum LoadError {
    /// Failed to lock index file
    Locking(#[from] locked_file::Error),
    /// Failed to read index file, corrupt
    Corrupt(#[from] CorruptError),
    /// Only version 2 of the index file is supported, but index is version {0}
    UnsupportedVersion(u32),
    /// Performing IO
    Io(#[from] io::Error),
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
/// Failed to commit index
pub struct CommitError(#[from] io::Error);

#[derive(Debug, displaydoc::Display, thiserror::Error)]
/// Corrupt index
pub enum CorruptError {
    /// Missing signature (file should begin "DIRC")
    MissingSignature,
    /// Failed checksum validation
    IncorrectChecksum,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{test_support::init, WsPath};
    use insta::assert_debug_snapshot;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_sample() -> eyre::Result<()> {
        init();

        let sample = hex::decode(SAMPLE_INDEX)?;
        let mut index = Index::new_virtual();
        index.load_from(&*sample)?;

        assert_debug_snapshot!(index);

        Ok(())
    }

    #[test]
    fn handles_replacing_file_with_directory_of_same_name() {
        init();

        let mut index = Index::new_virtual();

        index.add(Entry::zeroed(WsPath::new_unchecked("alice.txt")));
        index.add(Entry::zeroed(WsPath::new_unchecked("bob.txt")));

        index.add(Entry::zeroed(WsPath::new_unchecked("alice.txt/nested.txt")));

        let actual = index
            .entries()
            .map(|e| e.path().to_path_buf())
            .collect::<Vec<_>>();

        let expected: Vec<PathBuf> = vec!["alice.txt/nested.txt".into(), "bob.txt".into()];

        assert_eq!(expected, actual);
    }

    #[test]
    fn handles_replacing_a_dir_with_a_file() {
        init();

        let mut index = Index::new_virtual();

        index.add(Entry::zeroed(WsPath::new_unchecked("alice.txt")));
        index.add(Entry::zeroed(WsPath::new_unchecked("nested/bob.txt")));

        index.add(Entry::zeroed(WsPath::new_unchecked("nested")));

        let actual = index
            .entries()
            .map(|e| e.path().to_path_buf())
            .collect::<Vec<_>>();

        let expected: Vec<PathBuf> = vec!["alice.txt".into(), "nested".into()];

        assert_eq!(expected, actual);
    }

    #[test]
    fn handles_replacing_a_dir_with_children_with_a_file() {
        init();

        let mut index = Index::new_virtual();

        index.add(Entry::zeroed(WsPath::new_unchecked("alice.txt")));
        index.add(Entry::zeroed(WsPath::new_unchecked("nested/bob.txt")));
        index.add(Entry::zeroed(WsPath::new_unchecked(
            "nested/inner/claire.txt",
        )));

        index.add(Entry::zeroed(WsPath::new_unchecked("nested")));

        let actual = index
            .entries()
            .map(|e| e.path().to_path_buf())
            .collect::<Vec<_>>();

        let expected: Vec<PathBuf> = vec!["alice.txt".into(), "nested".into()];

        assert_eq!(expected, actual);
    }

    const SAMPLE_INDEX: &str = "\
4449524300000002000000036084db442e8f6d7c6084db442e8f6d7c0000\
fd0100a421bd000081a4000003e8000003e800000000e69de29bb2d1d643\
4b8b29ae775ad8c2e48c539100186469725f312f6469725f322f7365636f\
6e645f6c6576656c00006084db481b40719f6084db481b40719f0000fd01\
00a61504000081a4000003e8000003e800000000e69de29bb2d1d6434b8b\
29ae775ad8c2e48c539100186469725f312f6469725f332f7365636f6e64\
5f6c6576656c00006084db1a2effa5806084db1a2effa5800000fd0100a2\
2b6b000081a4000003e8000003e800000000e69de29bb2d1d6434b8b29ae\
775ad8c2e48c53910009746f705f6c6576656c0085bde0cb5dcb4b232b32\
51b3181191a55cb2fe98";
}
