use std::{
    collections::BTreeMap,
    convert::TryInto,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use crate::{locked_file, Entry, LockedFile, WithDigest};
use bstr::BString;
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use ring::digest::SHA1_FOR_LEGACY_USE_ONLY as SHA1;

#[derive(Debug)]
pub struct Index {
    entries: BTreeMap<BString, Entry>,
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

        let entries = if let Some(existing) = lock.protected_file() {
            Self::parse_from(existing)?
        } else {
            BTreeMap::new()
        };

        Ok(Self {
            entries,
            lock: Some(lock),
        })
    }

    fn parse_from(mut reader: impl Read) -> Result<BTreeMap<BString, Entry>, LoadError> {
        let mut entries = BTreeMap::new();
        let mut input = WithDigest::new(&SHA1, &mut reader);

        let mut sig = [0; 4];
        input.read_exact(&mut sig)?; // offset 0
        if sig != Self::SIG {
            return Err(Corrupt::MissingSignature.into());
        }

        let version = input.read_u32::<NetworkEndian>()?; // offset 4
        if version != Self::VERSION {
            return Err(LoadError::UnsupportedVersion(version));
        }

        let count = input.read_u32::<NetworkEndian>()?; // offset 8

        // offset 12
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
            return Err(Corrupt::IncorrectChecksum.into());
        }

        Ok(entries)
    }

    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    pub fn add(&mut self, entry: Entry) {
        self.entries.insert(entry.key().to_owned(), entry);
    }

    pub fn commit(mut self) -> Result<(), locked_file::Error> {
        let mut lock = self.lock.take().unwrap();

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

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum LoadError {
    /// Failed to lock index file
    Locking(#[from] locked_file::Error),
    /// Failed to read index file, corrupt
    Corrupt(#[from] Corrupt),
    /// Only version 2 of the index file is supported, but index is version {0}
    UnsupportedVersion(u32),
    /// Performing IO
    Io(#[from] io::Error),
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum Corrupt {
    /// Missing signature (file should begin "DIRC")
    MissingSignature,
    /// Failed checksum validation
    IncorrectChecksum,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::init;
    use insta::assert_debug_snapshot;

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

    #[test]
    fn parses_sample() -> eyre::Result<()> {
        init();

        let sample = hex::decode(SAMPLE_INDEX)?;
        let index = Index::parse_from(&*sample)?;

        assert_debug_snapshot!(index);

        Ok(())
    }
}
