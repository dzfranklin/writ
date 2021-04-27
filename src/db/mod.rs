pub mod author;
pub mod blob;
pub mod commit;
pub mod tree;

pub use author::Author;
pub use blob::Blob;
pub use commit::Commit;
pub use tree::Tree;

use crate::{Object, Oid};
use bstr::BStr;
use flate2::{write::ZlibEncoder, Compression};
use tempfile::NamedTempFile;

use std::{
    fs,
    io::{self, ErrorKind, Write},
    path::PathBuf,
};

#[derive(Debug, Clone)]
pub struct Db {
    path: PathBuf,
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum StoreError {
    /// Failed to perform IO
    Io(#[from] io::Error),
}

pub type StoreResult = Result<Oid, StoreError>;

impl Db {
    pub fn new<P: Into<PathBuf>>(git_dir: P) -> Self {
        Self {
            path: git_dir.into().join("objects"),
        }
    }

    pub fn store<O: Object>(&self, content: &BStr) -> Result<Oid, StoreError> {
        let oid = O::compute_oid(content);
        self.store_as(&oid, content)?;
        Ok(oid)
    }

    pub fn store_as(&self, oid: &Oid, content: &[u8]) -> io::Result<()> {
        let oid = oid.to_hex();
        let dir = self.path.join(&oid[0..2]);
        let name = &oid[2..];
        let path = dir.join(name);

        if path.exists() {
            return Ok(());
        }

        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(content)?;
        let content = enc.finish()?;

        // We use a temp file to get an atomic write
        let mut temp = NamedTempFile::new()?;
        temp.write_all(&content)?;
        temp.flush()?;

        match fs::rename(temp.path(), &path) {
            Err(err) if err.kind() == ErrorKind::NotFound => {
                fs::create_dir(&dir)?;
                fs::rename(temp.path(), &path)?;
            }
            Err(err) => return Err(err),
            Ok(()) => (),
        }

        Ok(())
    }
}
