use bstr::{BString, ByteSlice};

use crate::{locked_file, LockedFile, Oid};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Debug, Clone)]
pub struct Refs {
    path: PathBuf,
}

impl Refs {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub fn update_head(&self, oid: &Oid) -> Result<(), locked_file::Error> {
        let mut head = LockedFile::acquire(self.head_path())?;

        head.write_all(oid.to_hex().as_bytes())?;
        head.write_all(b"\n")?;

        head.commit()?;

        Ok(())
    }

    pub fn read_head(&self) -> io::Result<Option<BString>> {
        match fs::read(self.head_path()) {
            Ok(bytes) => Ok(Some(bytes.trim().into())),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn head_path(&self) -> PathBuf {
        self.path.join("HEAD")
    }
}
