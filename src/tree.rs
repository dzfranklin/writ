use bstr::{BStr, BString, ByteSlice};

use crate::{Object, Oid};
use std::{
    borrow::Cow,
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Entry {
    name: PathBuf,
    oid: Oid,
}

impl Entry {
    pub fn new<P: Into<PathBuf>>(name: P, oid: Oid) -> Self {
        Self {
            name: name.into(),
            oid,
        }
    }

    pub fn name(&self) -> &Path {
        &self.name.as_path()
    }

    fn serialize_name(&self) -> &[u8] {
        use std::os::unix::ffi::OsStrExt;
        self.name.as_os_str().as_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct Tree(Vec<Entry>);

impl Tree {
    const MODE: &'static [u8] = b"100644";

    const NAME_SIZE_GUESS: usize = 20;
    const ENTRY_SIZE: usize = Self::MODE.len() + 1 + Self::NAME_SIZE_GUESS + 1 + Oid::SIZE;

    pub fn new(mut entries: Vec<Entry>) -> Self {
        entries.sort_by(|a, b| a.name().cmp(b.name()));
        Self(entries)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.0
    }
}

impl Object for Tree {
    const TYPE: &'static [u8] = b"tree";

    fn serialize(&self) -> Cow<BStr> {
        let mut ser = Vec::with_capacity(Self::ENTRY_SIZE * self.entries().len());

        for entry in self.entries() {
            let oid = entry.oid.as_bytes();
            let name = entry.serialize_name();

            ser.extend(Self::MODE);
            ser.push(b' ');
            ser.extend(name);
            ser.push(b'\0');
            ser.extend(oid);
        }

        Cow::Owned(ser.into())
    }
}
