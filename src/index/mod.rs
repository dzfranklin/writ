pub mod entry;
pub use entry::Entry;

use std::{
    collections::{BTreeMap, BTreeSet},
    convert::TryInto,
    fs::File,
    io::{self, Read, Write},
    ops::Deref,
    path::{Path, PathBuf},
};

use crate::{locked_file, LockedFile, Stat, WithDigest, WsPath};
use bstr::BString;
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use ring::digest::SHA1_FOR_LEGACY_USE_ONLY as SHA1;
use tracing::debug;

type EntriesMap = BTreeMap<BString, Entry>;

#[derive(Debug, Clone)]
pub struct Index {
    entries: EntriesMap,
    path: PathBuf,
}

impl Index {
    const SIG: &'static [u8] = b"DIRC";
    const VERSION: u32 = 2;
    const CHECKSUM_LEN: usize = 20;

    pub fn load<P: AsRef<Path>>(git_dir: P) -> Result<Self, LoadError> {
        let path = Self::file_path(git_dir);
        let entries = Self::load_entries(&path)?;

        Ok(Self { entries, path })
    }

    /// Reload the index from disk. You don't need to do this after using
    /// [`Self::modify`] on this instance, this is for getting changes made by
    /// external programs.
    pub fn reload(&mut self) -> Result<(), LoadError> {
        let entries = Self::load_entries(&self.path)?;
        self.entries = entries;
        Ok(())
    }

    fn load_entries(path: &Path) -> Result<EntriesMap, LoadError> {
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                debug!("Index does not exist");
                return Ok(BTreeMap::new());
            }
            Err(err) => return Err(err.into()),
        };

        Self::load_entries_from(file)
    }

    fn load_entries_from(mut reader: impl Read) -> Result<EntriesMap, LoadError> {
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
        let mut entries = BTreeMap::new();
        for _ in 0..count {
            let entry = Entry::parse_from_index(&mut input)?;
            entries.insert(entry.key().to_owned(), entry);
        }

        let expected_checksum = input.finish();
        let expected_checksum: [u8; Self::CHECKSUM_LEN] =
            expected_checksum.as_ref().try_into().unwrap();

        let mut actual_checksum = [0; Self::CHECKSUM_LEN];
        reader.read_exact(&mut actual_checksum)?;

        if actual_checksum != expected_checksum {
            return Err(CorruptError::IncorrectChecksum.into());
        }

        Ok(entries)
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    pub fn modify(&mut self) -> Result<IndexMut, OpenForModificationsError> {
        IndexMut::new(self)
    }

    pub fn is_tracked(&self, path: &WsPath) -> bool {
        self.entries.contains_key(path.as_bstr())
    }

    pub fn entry(&self, path: &WsPath) -> Option<&Entry> {
        self.entries.get(path.as_bstr())
    }

    fn file_path(git_dir: impl AsRef<Path>) -> PathBuf {
        git_dir.as_ref().join("index")
    }
}

type ParentsMap = BTreeMap<BString, BTreeSet<BString>>;

#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct IndexMut<'i> {
    index: &'i mut Index,
    parents: ParentsMap,
    lock: Option<LockedFile>,
}

impl<'i> IndexMut<'i> {
    fn new(index: &'i mut Index) -> Result<Self, OpenForModificationsError> {
        let lock = LockedFile::acquire(&index.path)?;

        let mut parents = BTreeMap::new();
        for entry in index.entries() {
            Self::populate_parents_for(&mut parents, entry)
        }

        Ok(Self {
            index,
            parents,
            lock: Some(lock),
        })
    }

    pub fn add(&mut self, entry: Entry) {
        Self::populate_parents_for(&mut self.parents, &entry);
        self.discard_conflicts_with(&entry.path);
        self.index.entries.insert(entry.key().to_owned(), entry);
    }

    fn populate_parents_for(parents: &mut ParentsMap, entry: &Entry) {
        for parent in entry.path.parents() {
            parents
                .entry(parent.as_bstr().to_owned())
                .or_insert_with(BTreeSet::new)
                .insert(entry.path.to_bstring());
        }
    }

    fn discard_conflicts_with(&mut self, path: &WsPath) {
        // If the new entry is lib/index/foo, remove lib and index.
        for parent in path.parents() {
            self.index.entries.remove(parent.as_bstr());
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

    pub fn update_stat(
        &mut self,
        path: &WsPath,
        stat: Stat,
    ) -> Result<Stat, NonexistentEntryError> {
        let entry = self
            .index
            .entries
            .get_mut(path.as_bstr())
            .ok_or(NonexistentEntryError)?;
        let old = entry.update_stat(stat);
        Ok(old)
    }

    pub fn remove(&mut self, path: &WsPath) -> Option<Entry> {
        if let Some(entry) = self.index.entries.remove(path.as_bstr()) {
            for parent in path.parents() {
                if let Some(children) = self.parents.get_mut(parent.as_bstr()) {
                    children.remove(path.as_bstr());
                    if children.is_empty() {
                        self.parents.remove(parent.as_bstr());
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

        out.write_all(Index::SIG)?; // offset 0
        out.write_u32::<NetworkEndian>(Index::VERSION)?; // offset 4

        let size = self.index.entries.len().try_into().expect("Len overflowed");
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

impl Deref for IndexMut<'_> {
    type Target = Index;

    fn deref(&self) -> &Self::Target {
        &self.index
    }
}

impl Eq for IndexMut<'_> {}

impl PartialEq for IndexMut<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.entries().eq(other.entries())
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
/// Failed to load index
pub enum LoadError {
    /// Failed to read index file, corrupt
    Corrupt(#[from] CorruptError),
    /// Only version 2 of the index file is supported, but index is version {0}
    UnsupportedVersion(u32),
    /// Performing IO
    Io(#[from] io::Error),
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum ModifyError {
    /// Failed to open index for modifications
    Open(#[from] OpenForModificationsError),
    /// Failed to commit changes
    Commit(#[from] CommitError),
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum OpenForModificationsError {
    /// Failed to lock index file
    Locking(#[from] locked_file::Error),
    /// Performing IO
    Io(#[from] io::Error),
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
/// Entry does not exit
pub struct NonexistentEntryError;

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
    use crate::{test_support::init, Oid, WsPath};
    use insta::assert_debug_snapshot;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_sample() -> eyre::Result<()> {
        init();

        let sample = hex::decode(SAMPLE_INDEX)?;
        let actual = Index::load_entries_from(&*sample)?;

        assert_debug_snapshot!(actual);

        Ok(())
    }

    fn index_fixture() -> eyre::Result<(tempfile::NamedTempFile, Index)> {
        let file = tempfile::NamedTempFile::new()?;
        let index = Index {
            entries: BTreeMap::<BString, Entry>::new(),
            path: file.path().to_owned(),
        };

        Ok((file, index))
    }

    fn entry_fixture(path: impl Into<PathBuf>) -> Entry {
        Entry::new(WsPath::new_unchecked(path), Oid::zero(), Stat::zeroed())
    }

    #[test]
    fn populates_parents_correctly() {
        init();

        let mut actual = BTreeMap::new();
        let entry = entry_fixture("dir_1/dir_2/second_level");
        IndexMut::populate_parents_for(&mut actual, &entry);
        assert_debug_snapshot!("parents_after_entry_1", actual);

        let entry = entry_fixture("dir_1/dir_3/second_level");
        IndexMut::populate_parents_for(&mut actual, &entry);
        assert_debug_snapshot!("parents_after_entry_2", actual);
    }

    #[test]
    fn handles_replacing_file_with_directory_of_same_name() -> eyre::Result<()> {
        init();

        let (_file, mut index) = index_fixture()?;
        let mut index = index.modify()?;

        index.add(entry_fixture("alice.txt"));
        index.add(entry_fixture("bob.txt"));

        index.add(entry_fixture("alice.txt/nested.txt"));

        let actual = index.entries().map(|e| e.path.clone()).collect::<Vec<_>>();

        let expected = vec![
            WsPath::new_unchecked("alice.txt/nested.txt"),
            WsPath::new_unchecked("bob.txt"),
        ];

        assert_eq!(expected, actual);

        Ok(())
    }

    #[test]
    fn handles_replacing_a_dir_with_a_file() -> eyre::Result<()> {
        init();

        let (_file, mut index) = index_fixture()?;
        let mut index = index.modify()?;

        index.add(entry_fixture("alice.txt"));
        index.add(entry_fixture("nested/bob.txt"));

        index.add(entry_fixture("nested"));

        let actual = index.entries().map(|e| e.path.clone()).collect::<Vec<_>>();

        let expected = vec![
            WsPath::new_unchecked("alice.txt"),
            WsPath::new_unchecked("nested"),
        ];

        assert_eq!(expected, actual);

        Ok(())
    }

    #[test]
    fn handles_replacing_a_dir_with_children_with_a_file() -> eyre::Result<()> {
        init();

        let (_file, mut index) = index_fixture()?;
        let mut index = index.modify()?;

        index.add(entry_fixture("alice.txt"));
        index.add(entry_fixture("nested/bob.txt"));
        index.add(entry_fixture("nested/inner/claire.txt"));

        index.add(entry_fixture("nested"));

        let actual = index.entries().map(|e| e.path.clone()).collect::<Vec<_>>();

        let expected = vec![
            WsPath::new_unchecked("alice.txt"),
            WsPath::new_unchecked("nested"),
        ];

        assert_eq!(expected, actual);

        Ok(())
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
