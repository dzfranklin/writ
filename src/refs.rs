use bstr::{BStr, BString, ByteSlice};

use crate::{locked_file, LockedFile, Oid};
use std::{
    ffi::OsStr,
    fs,
    io::{self, Write},
    os::unix::prelude::OsStrExt,
    path::PathBuf,
};

#[derive(Debug, Clone)]
pub struct Refs {
    path: PathBuf,
}

impl Refs {
    const HEAD: &'static [u8] = b"HEAD";

    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub fn update_ref(&self, ref_name: &BStr, oid: &Oid) -> Result<(), UpdateError> {
        let path = self.ref_path(ref_name);
        let mut lock =
            LockedFile::acquire(path).map_err(|e| UpdateError::Lock(ref_name.to_owned(), e))?;

        lock.write_all(oid.to_hex().as_bytes())
            .map_err(|e| UpdateError::Write(ref_name.to_owned(), e))?;
        lock.write_all(b"\n")
            .map_err(|e| UpdateError::Write(ref_name.to_owned(), e))?;
        lock.commit()
            .map_err(|e| UpdateError::Write(ref_name.to_owned(), e))?;

        Ok(())
    }

    pub fn update_head(&self, oid: &Oid) -> Result<(), UpdateError> {
        self.update_ref(Self::HEAD.as_bstr(), oid)
    }

    pub fn read_ref(&self, ref_name: &BStr) -> Result<Option<BString>, ReadError> {
        match fs::read(self.ref_path(ref_name)) {
            Ok(bytes) => Ok(Some(bytes.trim().into())),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(ReadError(ref_name.to_owned(), err)),
        }
    }

    pub fn read_head(&self) -> Result<Option<BString>, ReadError> {
        self.read_ref(Self::HEAD.as_bstr())
    }

    fn ref_path(&self, ref_name: &BStr) -> PathBuf {
        self.path.join(OsStr::from_bytes(ref_name.as_bytes()))
    }
}

/// Failed to read ref {0}
#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub struct ReadError(BString, #[source] io::Error);

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum UpdateError {
    /// Error writing ref {0}
    Write(BString, #[source] io::Error),
    /// Error locking ref {0} for writing
    Lock(BString, #[source] locked_file::Error),
}
