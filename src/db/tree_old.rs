use bstr::{BStr, BString, ByteSlice};

use crate::{
    db::{self, Db},
    index,
    stat::Mode,
    Object, Oid, WsPath,
};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    io::{self, BufRead},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Tree {
    path: WsPath,
    nodes: BTreeMap<BString, Node>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Node {
    pub oid: Oid,
    pub path: WsPath,
    pub node_type: NodeType,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum NodeType {
    Tree,
    Entry(Mode),
}

impl Tree {
    const MODE: &'static [u8] = b"40000";

    pub fn new(path: WsPath) -> Self {
        Self {
            path,
            nodes: BTreeMap::new(),
        }
    }

    pub fn insert_direct_child(&mut self, oid: Oid, name: impl Into<BString>, mode: Mode) {
        let name = name.into();
        let child = Node {
            oid,
            path: self.path.join_bytes(name.as_bstr()),
            node_type: NodeType::Entry(mode),
        };
        self.nodes.insert(name, child);
    }

    /// Panics if not a descendent of this tree
    pub fn insert_descendent(&mut self, db: &Db, oid: Oid, path: WsPath, node_type: NodeType) {
        let parents = path
            .parent()
            .strip_prefix(&self.path)
            .expect("Not a descendent")
            .iter_parents();

        if let Some(subpath) = parents.next() {
            self.insert_descendent(db, oid, path, node_type) // what would the oid be?
        }
    }

    /// Returns Some<Oid> if the entry this would be inserted into is a stub
    /// that needs to be loaded first.
    pub fn add_entry<P: AsRef<Path>>(&mut self, parents: P, entry: Entry) -> Option<Oid> {
        let parents = parents.as_ref();
        let mut components = parents.components();
        if let Some(key) = components.next() {
            let key = key.as_os_str();
            let child_path = self.path.as_ref().unwrap_or(&WsPath::root()).join(key);
            let key = key.as_bytes();

            let tree = match self
                .nodes
                .entry(key.into())
                .or_insert_with(|| Node::Tree(Tree::new(child_path)))
            {
                Node::Tree(tree) => tree,
                Node::Entry(_) => unreachable!("Must be tree"),
                Node::Stub(oid) => return Some(*oid),
            };

            tree.add_entry(components.as_path(), entry)
        } else {
            let key = entry.filename().as_bytes();
            self.nodes.insert(key.into(), Node::Entry(entry));
            None
        }
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    pub fn get_node(&self, key: impl AsRef<BStr>) -> Option<&Node> {
        self.nodes.get(key.as_ref())
    }

    fn deserialize_at(parent: WsPath, mut data: impl BufRead) -> Result<Self, DeserializeError> {
        let mut nodes = BTreeMap::new();

        while let Some((key, node)) = Node::deserialize(&parent, &mut data)? {
            nodes.insert(key, node);
        }

        Ok(Self { path: None, nodes })
    }
}

impl Object for Tree {
    const TYPE: &'static [u8] = b"tree";

    fn store(&self, db: &Db) -> db::StoreResult {
        let mut ser = BString::from(Vec::new());

        for (path, entry) in &self.nodes {
            let oid = match entry {
                Node::Tree(tree) => tree.store(db)?,
                Node::Entry(entry) => entry.oid,
                Node::Stub(oid) => *oid,
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

    type DeserializeError = DeserializeError;

    fn deserialize(
        _oid: Oid,
        _len: usize,
        mut data: impl BufRead,
    ) -> Result<Self, Self::DeserializeError> {
        Self::deserialize_at(WsPath::root(), data)
    }
}

impl Default for Tree {
    fn default() -> Self {
        Self::new_root()
    }
}

impl Node {
    fn mode(&self) -> &BStr {
        match self {
            Node::Entry(entry) => entry.mode.as_base8(),
            Node::Tree(_) | Node::Stub(_) => Tree::MODE.as_bstr(),
        }
    }

    fn deserialize(
        parent: &WsPath,
        mut data: impl BufRead,
    ) -> Result<Option<(BString, Self)>, DeserializeError> {
        let mut mode = Vec::new();
        let bytes_read = data.read_until(b' ', &mut mode)?;
        if bytes_read == 0 {
            return Ok(None);
        }
        mode.pop().unwrap();

        let mut name = BString::from(Vec::new());
        data.read_until(b'\0', &mut name)?;
        name.pop().unwrap();
        let path = parent.join_bytes(&name.as_bstr());

        let mut oid = [0; Oid::SIZE];
        data.read_exact(&mut oid)?;
        let oid = Oid::new(oid);

        let entry = if mode == Tree::MODE {
            Self::Stub(oid)
        } else {
            let mode = Mode::from_base8(mode.as_bstr());
            Self::Entry(Entry { oid, mode, path })
        };

        Ok(Some((name, entry)))
    }
}

impl Entry {
    pub fn filename(&self) -> &BStr {
        self.path.file_name().as_bstr()
    }
}

impl From<index::Entry> for Entry {
    fn from(entry: index::Entry) -> Self {
        Self {
            oid: entry.oid,
            mode: entry.mode(),
            path: entry.path,
        }
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
/// Error deserializing tree
pub struct DeserializeError(#[from] io::Error);

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("Node {0:?} not found")]
    NotFound(BString),
    #[error(transparent)]
    Db(#[from] db::LoadError<Tree>),
}
