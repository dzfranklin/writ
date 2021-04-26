use bstr::{BStr, BString, ByteSlice};

use crate::{
    db::{self, Db},
    Entry, Object,
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    ffi::OsStr,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Tree(BTreeMap<BString, Node>);

#[derive(Debug, Clone)]
pub enum Node {
    Entry(Entry),
    Tree(Tree),
}

impl Node {
    fn mode(&self) -> &BStr {
        match self {
            Node::Entry(entry) => entry.mode().as_base10(),
            Node::Tree(_) => Tree::MODE.as_bstr(),
        }
    }
}

impl Tree {
    const MODE: &'static [u8] = b"40000";

    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn from<E: Into<Vec<Entry>>>(entries: E) -> Self {
        let mut entries = entries.into();
        entries.sort_by(|a, b| a.path().cmp(b.path()));

        let mut root = Self::default();

        for entry in entries {
            let parents = entry
                .path()
                .parent()
                .map_or_else(PathBuf::new, ToOwned::to_owned);
            root.add_entry(parents, entry);
        }

        root
    }

    pub fn add_entry<P: AsRef<Path>>(&mut self, parents: P, entry: Entry) {
        let parents = parents.as_ref();
        let mut components = parents.components();
        if let Some(key) = components.next() {
            let tree_key = key.as_os_str().as_bytes();
            let tree = match self
                .0
                .entry(tree_key.into())
                .or_insert_with(|| Node::Tree(Tree::new()))
            {
                Node::Entry(_) => unreachable!("Must be Tree"),
                Node::Tree(tree) => tree,
            };

            tree.add_entry(components.as_path(), entry);
        } else {
            let key = entry.filename().as_bytes();
            self.0.insert(key.into(), Node::Entry(entry));
        }
    }
}

impl Object for Tree {
    const TYPE: &'static [u8] = b"tree";

    fn store(&self, db: &Db) -> db::StoreResult {
        let mut ser = BString::from(Vec::new());

        for (path, entry) in &self.0 {
            let oid = match entry {
                Node::Entry(entry) => Cow::Borrowed(&entry.oid),
                Node::Tree(tree) => Cow::Owned(tree.store(db)?),
            };
            let oid = oid.as_bytes();

            let path: &Path = OsStr::from_bytes(path).as_ref();
            let filename = path.file_name().expect("No trailing ..").as_bytes();

            ser.extend(entry.mode().iter());
            ser.push(b' ');
            ser.extend(filename);
            ser.push(b'\0');
            ser.extend(oid);
        }

        db.store::<Self>(ser.as_bstr())
    }
}

impl Default for Tree {
    fn default() -> Self {
        Self::new()
    }
}
