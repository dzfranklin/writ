use std::{
    collections::BTreeMap,
    io::{self, BufRead},
};

use bstr::{BStr, BString, ByteSlice};

use crate::core::{db, stat, Db, Object, Oid, WsPath};

use super::{object::OID_SIZE, Blob, ObjectBuilder, UntypedOid};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Tree {
    oid: Oid<Tree>,
    nodes: BTreeMap<BString, Node>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Node {
    File(FileNode),
    Tree { name: BString, oid: Oid<Tree> },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FileNode {
    pub oid: Oid<Blob>,
    pub name: BString,
    pub mode: stat::Mode,
}

impl Tree {
    const MODE: &'static [u8] = b"40000";

    pub fn direct_children(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    pub fn direct_child(&self, name: &BStr) -> Option<&Node> {
        self.nodes.get(name)
    }
}

impl Object for Tree {
    const TYPE: &'static [u8] = b"tree";

    type Builder = Builder;
    type DeserializeError = DeserializeError;

    fn oid(&self) -> Oid<Self> {
        self.oid
    }

    fn deserialize(
        oid: Oid<Self>,
        _len: usize,
        mut data: impl std::io::BufRead,
    ) -> Result<Self, Self::DeserializeError> {
        let mut nodes = BTreeMap::new();

        while let Some(node) = Node::deserialize(&mut data)? {
            nodes.insert(node.name().to_owned(), node);
        }

        Ok(Self { oid, nodes })
    }
}

impl Node {
    pub fn untyped_oid(&self) -> UntypedOid {
        match self {
            Self::File(FileNode { oid, .. }) => oid.into_untyped(),
            Self::Tree { oid, .. } => oid.into_untyped(),
        }
    }

    pub fn name(&self) -> &BStr {
        match self {
            Node::File(FileNode { name, .. }) | Node::Tree { name, .. } => name.as_bstr(),
        }
    }

    fn deserialize(mut data: impl BufRead) -> Result<Option<Self>, DeserializeError> {
        let mut mode = Vec::new();
        let bytes_read = data.read_until(b' ', &mut mode)?;
        if bytes_read == 0 {
            return Ok(None);
        }
        mode.pop().unwrap();

        let mut name = BString::from(Vec::new());
        data.read_until(b'\0', &mut name)?;
        name.pop().unwrap();

        let mut oid = [0; OID_SIZE];
        data.read_exact(&mut oid)?;
        let oid = UntypedOid::new(oid);

        let entry = if mode == Tree::MODE {
            Self::Tree {
                oid: oid.to_typed(),
                name,
            }
        } else {
            let mode = stat::Mode::from_base8(mode.as_bstr());
            Self::File(FileNode {
                oid: oid.to_typed(),
                mode,
                name,
            })
        };

        Ok(Some(entry))
    }
}

type BuilderNodes = BTreeMap<BString, SerializeNode>;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Builder {
    trees: Vec<Option<BuilderNodes>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct EntryBuilder {
    pub oid: Oid<Blob>,
    pub path: WsPath,
    pub mode: stat::Mode,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            trees: vec![Some(BTreeMap::new())],
        }
    }

    pub fn entries(mut self, entries: impl IntoIterator<Item = EntryBuilder>) -> Self {
        for desc in entries {
            let mut parent = 0;

            for name in desc.path.parent_components() {
                let next = if let Some(existing) = self.tree(parent).get(name) {
                    match existing {
                        SerializeNode::Tree(existing) => *existing,
                        SerializeNode::Entry { .. } => panic!("Directory has same name as file"),
                    }
                } else {
                    self.trees.push(Some(BTreeMap::new()));
                    self.trees.len() - 1
                };

                self.tree(parent)
                    .insert(name.to_owned(), SerializeNode::Tree(next));

                parent = next;
            }

            self.tree(parent).insert(
                desc.path.file_name().to_owned(),
                SerializeNode::Entry {
                    oid: desc.oid,
                    mode: desc.mode,
                },
            );
        }

        self
    }

    fn tree(&mut self, i: usize) -> &mut BuilderNodes {
        self.trees[i].as_mut().expect("Subroot remains")
    }

    fn store_subroot(&mut self, db: &Db, subroot: usize) -> db::StoreResult<Tree> {
        let mut out = BString::from(Vec::new());

        for (name, entry) in self.trees[subroot].take().expect("Not already serialized") {
            let (oid, mode) = match entry {
                SerializeNode::Entry { oid, mode } => {
                    (oid.into_untyped(), mode.as_base8().as_bytes())
                }
                SerializeNode::Tree(tree) => {
                    let oid = self.store_subroot(db, tree)?;
                    (oid.into_untyped(), Tree::MODE)
                }
            };

            out.extend_from_slice(mode);
            out.push(b' ');
            out.extend_from_slice(name.as_bytes());
            out.push(b'\0');
            out.extend_from_slice(oid.as_bytes());
        }

        db.store_bytes::<Self>(&out)
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectBuilder for Builder {
    fn store(mut self, db: &Db) -> db::StoreResult<Tree> {
        self.store_subroot(db, 0)
    }

    type Object = Tree;
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum SerializeNode {
    Tree(usize),
    Entry { oid: Oid<Blob>, mode: stat::Mode },
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
/// Error deserializing tree
pub struct DeserializeError(#[from] io::Error);

#[cfg(test)]
mod test {
    use insta::assert_debug_snapshot;

    use crate::core::stat::Mode;

    use super::*;

    #[test]
    fn builder_entries() {
        let builder = Builder::new().entries(vec![
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("top_level"),
                mode: Mode::Regular,
            },
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("top_level2"),
                mode: Mode::Regular,
            },
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("singly_nested/child"),
                mode: Mode::Regular,
            },
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("doubly_nested/inner/child"),
                mode: Mode::Regular,
            },
        ]);
        assert_debug_snapshot!(builder);
    }

    #[test]
    fn builder_entries_more_complex() {
        let builder = Builder::new().entries(vec![
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("f"),
                mode: Mode::Regular,
            },
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("dir_1/f"),
                mode: Mode::Regular,
            },
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("dir_2/dir_a/dir_x/f"),
                mode: Mode::Regular,
            },
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("dir_1/f2"),
                mode: Mode::Regular,
            },
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("dir_1/dir_a/f"),
                mode: Mode::Regular,
            },
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("dir_1/dir_a/dir_x/f"),
                mode: Mode::Regular,
            },
            EntryBuilder {
                oid: Oid::zero(),
                path: WsPath::new_unchecked("dir_1/dir_a/dir_y/f"),
                mode: Mode::Regular,
            },
        ]);
        assert_debug_snapshot!(builder);
    }
}
