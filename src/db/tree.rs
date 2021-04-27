use bstr::{BStr, BString, ByteSlice};

use crate::{
    db::{self, Db},
    index,
    stat::Mode,
    Object, Oid, WsPath,
};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    ffi::OsStr,
    io::{self, BufRead},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Tree {
    path: Option<WsPath>,
    nodes: BTreeMap<BString, Node>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Node {
    Loaded(LoadedNode),
    UnloadedTree(Oid),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LoadedNode {
    Entry(Entry),
    Tree(Tree),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Entry {
    pub oid: Oid,
    pub mode: Mode,
    pub path: WsPath,
}

impl Tree {
    const MODE: &'static [u8] = b"40000";

    pub fn new_root() -> Self {
        Self {
            path: None,
            nodes: BTreeMap::new(),
        }
    }

    pub fn new(path: WsPath) -> Self {
        Self {
            path: Some(path),
            nodes: BTreeMap::new(),
        }
    }

    pub fn from<E: Into<Vec<Entry>>>(entries: E) -> Self {
        let mut entries = entries.into();
        entries.sort_by(|a, b| a.path.cmp(&b.path));

        let mut root = Self::default();

        for entry in entries {
            let parents = entry
                .path
                .parent()
                .map_or_else(PathBuf::new, ToOwned::to_owned);
            root.add_entry(parents, entry);
        }

        root
    }

    /// Panics if the tree entry would be inserted into isn't loaded
    pub fn add_entry<P: AsRef<Path>>(&mut self, parents: P, entry: Entry) {
        let parents = parents.as_ref();
        let mut components = parents.components();
        if let Some(key) = components.next() {
            let key = key.as_os_str();
            let child_path = self.path.as_ref().unwrap_or(&WsPath::root()).join(key);
            let key = key.as_bytes();

            let tree = match self
                .nodes
                .entry(key.into())
                .or_insert_with(|| Node::Loaded(LoadedNode::Tree(Tree::new(child_path))))
            {
                Node::Loaded(node) => match node {
                    LoadedNode::Entry(_) => unreachable!("Must be Tree"),
                    LoadedNode::Tree(tree) => tree,
                },
                Node::UnloadedTree(_) => panic!("Must be loaded"),
            };

            tree.add_entry(components.as_path(), entry);
        } else {
            let key = entry.filename().as_bytes();
            self.nodes
                .insert(key.into(), Node::Loaded(LoadedNode::Entry(entry)));
        }
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    pub fn keys(&self) -> impl Iterator<Item = &BStr> {
        self.nodes.keys().map(|k| k.as_bstr())
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

    pub fn load(&mut self, key: impl Into<BString>, db: &Db) -> Result<&LoadedNode, LoadError> {
        self.load_mut(key, db).map(|n| &*n)
    }

    pub fn load_mut(
        &mut self,
        key: impl Into<BString>,
        db: &Db,
    ) -> Result<&mut LoadedNode, LoadError> {
        let key = key.into();
        let node = self
            .nodes
            .get(&key)
            .ok_or(LoadError::NotFound(key.clone()))?;

        match node {
            Node::Loaded(_) => Ok(self.nodes.get_mut(&key).unwrap().unwrap_loaded_mut()),
            Node::UnloadedTree(oid) => {
                let parent = self.path.clone().unwrap_or(WsPath::root());

                let (_len, data) = db
                    .load_bytes(Tree::TYPE, oid)
                    .map_err(|e| LoadError::Db(e.into()))?;

                let tree = Tree::deserialize_at(parent, data)
                    .map_err(|e| db::LoadError::<Tree>::Deserialize(*oid, e))?;

                {
                    self.nodes
                        .insert(key.clone(), Node::Loaded(LoadedNode::Tree(tree)));
                }
                let node = self.nodes.get_mut(&key).unwrap().unwrap_loaded_mut();

                Ok(node)
            }
        }
    }
}

impl Object for Tree {
    const TYPE: &'static [u8] = b"tree";

    fn store(&self, db: &Db) -> db::StoreResult {
        let mut ser = BString::from(Vec::new());

        for (path, entry) in &self.nodes {
            let oid = match entry {
                Node::Loaded(node) => match node {
                    LoadedNode::Entry(entry) => Cow::Borrowed(&entry.oid),
                    LoadedNode::Tree(tree) => Cow::Owned(tree.store(db)?),
                },
                Node::UnloadedTree(oid) => Cow::Borrowed(oid),
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
            Node::Loaded(node) => match node {
                LoadedNode::Entry(entry) => entry.mode.as_base8(),
                LoadedNode::Tree(_) => Tree::MODE.as_bstr(),
            },
            Node::UnloadedTree(_) => Tree::MODE.as_bstr(),
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
            Self::UnloadedTree(oid)
        } else {
            let mode = Mode::from_base8(mode.as_bstr());
            Self::Loaded(LoadedNode::Entry(Entry { oid, mode, path }))
        };

        Ok(Some((name, entry)))
    }

    pub fn unwrap_loaded(&self) -> &LoadedNode {
        match self {
            Node::Loaded(node) => node,
            Node::UnloadedTree(_) => panic!("Unwrapped Node as LoadedNode, but was UnloadedTree"),
        }
    }

    pub fn unwrap_unloaded(&self) -> &Oid {
        match self {
            Node::Loaded(_) => panic!("Unwrapped Node as UnloadedTree, but was Loaded"),
            Node::UnloadedTree(oid) => oid,
        }
    }

    pub fn unwrap_unloaded_mut(&mut self) -> &mut Oid {
        match self {
            Node::Loaded(_) => panic!("Unwrapped Node as UnloadedTree, but was Loaded"),
            Node::UnloadedTree(oid) => oid,
        }
    }

    pub fn unwrap_loaded_mut(&mut self) -> &mut LoadedNode {
        match self {
            Node::Loaded(node) => node,
            Node::UnloadedTree(_) => panic!("Unwrapped Node as LoadedNode, but was UnloadedTree"),
        }
    }
}

impl LoadedNode {
    pub fn unwrap_tree(&self) -> &Tree {
        match self {
            LoadedNode::Entry(_) => panic!("Unwrapped LoadedNode as Tree, but was Entry"),
            LoadedNode::Tree(tree) => tree,
        }
    }
    pub fn unwrap_entry(&self) -> &Entry {
        match self {
            LoadedNode::Entry(entry) => entry,
            LoadedNode::Tree(_) => panic!("Unwrapped LoadedNode as Tree, but was Tree"),
        }
    }

    pub fn unwrap_tree_mut(&mut self) -> &mut Tree {
        match self {
            LoadedNode::Entry(_) => panic!("Unwrapped LoadedNode as Tree, but was Entry"),
            LoadedNode::Tree(tree) => tree,
        }
    }
    pub fn unwrap_entry_mut(&mut self) -> &mut Entry {
        match self {
            LoadedNode::Entry(entry) => entry,
            LoadedNode::Tree(_) => panic!("Unwrapped LoadedNode as Tree, but was Tree"),
        }
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

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::Db;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[allow(clippy::similar_names)]
    #[test]
    fn round_trips() -> eyre::Result<()> {
        let mut original = Tree::from(vec![
            Entry {
                oid: Oid::for_bytes(b"foo"),
                mode: Mode::Regular,
                path: WsPath::new_unchecked("nested/world"),
            },
            Entry {
                oid: Oid::for_bytes(b"bar"),
                mode: Mode::Executable,
                path: WsPath::new_unchecked("nested/world2"),
            },
            Entry {
                oid: Oid::for_bytes(b"fuzz"),
                mode: Mode::Regular,
                path: WsPath::new_unchecked("nested/nested2/fuzz"),
            },
            Entry {
                oid: Oid::for_bytes(b"foo"),
                mode: Mode::Regular,
                path: WsPath::new_unchecked("top_level"),
            },
        ]);

        let dir = tempdir()?;
        let dir = dir.path();
        fs::create_dir(dir.join("objects"))?;
        let db = Db::new(dir);

        let oid = original.store(&db)?;
        let mut deserialized = db.load::<Tree>(oid)?;

        assert_eq!(
            original.get_node("top_level").unwrap(),
            deserialized.get_node("top_level").unwrap()
        );

        let nested_original = original.load_mut("nested", &db)?.unwrap_tree_mut();
        let nested_deserialized = deserialized.load_mut("nested", &db)?.unwrap_tree_mut();

        let nested2_original = nested_original
            .nodes
            .remove("nested".as_bytes().as_bstr())
            .unwrap();
        let nested2_deserialized = nested_deserialized
            .nodes
            .remove("nested2".as_bytes().as_bstr())
            .unwrap();

        assert_eq!(nested_original, nested_deserialized);
        assert_eq!(nested2_original, nested2_deserialized);

        Ok(())
    }
}
