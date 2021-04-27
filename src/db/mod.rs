pub mod author;
pub mod blob;
pub mod commit;
pub mod tree;

pub use author::Author;
pub use blob::Blob;
use byteorder::{NetworkEndian, ReadBytesExt};
pub use commit::Commit;
pub use tree::Tree;

use crate::{Object, Oid};
use bstr::{BStr, BString, ByteSlice};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use tempfile::NamedTempFile;

use std::{
    convert::TryInto,
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, ErrorKind, Read, Write},
    num::ParseIntError,
    path::PathBuf,
};

#[derive(Debug, Clone)]
pub struct Db {
    path: PathBuf,
}

impl Db {
    pub fn new<P: Into<PathBuf>>(git_dir: P) -> Self {
        Self {
            path: git_dir.into().join("objects"),
        }
    }

    pub fn store<O: Object>(&self, content: &BStr) -> StoreResult {
        fn helper(db: &Db, oid: &Oid, prefix: &[u8], content: &[u8]) -> io::Result<()> {
            let path = db.oid_path(oid);

            if path.exists() {
                return Ok(());
            }

            let mut temp = NamedTempFile::new()?;

            {
                let mut writer = BufWriter::new(&mut temp);
                let mut writer = ZlibEncoder::new(&mut writer, Compression::default());

                writer.write_all(prefix)?;
                writer.write_all(content)?;
            }

            // We use a temp file to get an atomic write
            temp.flush()?;

            match fs::rename(temp.path(), &path) {
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    fs::create_dir(&path.parent().expect("has parent"))?;
                    fs::rename(temp.path(), &path)?;
                }
                Err(err) => return Err(err),
                Ok(()) => (),
            }

            Ok(())
        }

        let oid = O::compute_oid(content);
        let prefix = O::serialized_prefix(content);
        helper(self, &oid, &prefix, content).map_err(|e| StoreError(oid, e))?;
        Ok(oid)
    }

    pub fn load<O: Object>(&self, oid: Oid) -> Result<O, LoadError<O>> {
        let (len, bytes) = self.load_bytes(O::TYPE, &oid)?;
        O::deserialize(oid, len, bytes).map_err(|e| LoadError::Deserialize(oid, e))
    }

    pub(crate) fn load_bytes(
        &self,
        expected_type: &[u8],
        oid: &Oid,
    ) -> Result<(usize, impl BufRead), LoadBytesError> {
        let path = self.oid_path(&oid);
        let file = match File::open(&path) {
            Ok(file) => Ok(file),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                Err(LoadBytesError::NotFound(*oid))
            }
            Err(err) => Err(LoadBytesError::Open(*oid, err)),
        }?;

        let mut bytes = BufReader::new(ZlibDecoder::new(file));

        let mut o_type = vec![0; expected_type.len() + 1];
        bytes
            .read_exact(&mut o_type)
            .map_err(|e| LoadBytesError::ReadPrefix(*oid, e))?;
        let sep = o_type.pop().unwrap();
        if sep != b' ' {
            return Err(LoadBytesError::Corrupt(*oid));
        }
        if o_type != expected_type {
            return Err(LoadBytesError::WrongType {
                oid: *oid,
                expected: expected_type.into(),
                actual: o_type.into(),
            });
        }

        let mut len = BString::from(Vec::new());
        bytes
            .read_until(b'\0', &mut len)
            .map_err(|e| LoadBytesError::ReadPrefix(*oid, e))?;
        len.pop().unwrap();
        let len = len
            .to_str()
            .map_err(|e| LoadBytesError::ParseLenToBytes(*oid, e))?;
        let len: usize = len
            .parse()
            .map_err(|e| LoadBytesError::ParseLenToInt(*oid, e))?;

        Ok((len, bytes))
    }

    fn oid_path(&self, oid: &Oid) -> PathBuf {
        let oid = oid.to_hex();
        let dir = self.path.join(&oid[0..2]);
        let name = &oid[2..];
        dir.join(name)
    }
}

pub type StoreResult = Result<Oid, StoreError>;

#[derive(Debug, thiserror::Error, displaydoc::Display)]
/// Failed to store {0:?}
pub struct StoreError(Oid, #[source] io::Error);

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum LoadError<O: Object> {
    /// Failed to load bytes of object {0:?}
    LoadBytes(#[from] LoadBytesError),
    /// Failed to deserialize {0:?}
    Deserialize(Oid, #[source] O::DeserializeError),
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum LoadBytesError {
    /// {0:?} not found in database
    NotFound(Oid),
    /// Failed to open the file for {0:?} in the database
    Open(Oid, #[source] io::Error),
    /// Failed to read the prefix from the file for {0:?} in the database
    ReadPrefix(Oid, #[source] io::Error),
    /// Database entry for {0:?} is corrupt
    Corrupt(Oid),
    /// Expected oid {oid:?} to have type {expected}, got {actual}
    WrongType {
        oid: Oid,
        expected: BString,
        actual: BString,
    },
    /// Failed to parse bytes of length of {0:?} as utf-8
    ParseLenToBytes(Oid, #[source] bstr::Utf8Error),
    /// Failed to parse length of {0:?}
    ParseLenToInt(Oid, #[source] ParseIntError),
}
