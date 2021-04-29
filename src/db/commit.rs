use std::{
    borrow::Cow,
    io::{self, BufRead},
};

use bstr::{BString, ByteSlice};
use tracing::warn;

use super::{author, object::ParseOidError, Author, Tree};
use crate::{db, Db, Object, ObjectBuilder, Oid};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Commit {
    pub oid: Oid<Commit>,
    pub parent: Option<Oid<Commit>>,
    pub tree: Oid<Tree>,
    pub author: db::Author,
    pub msg: BString,
}

impl Object for Commit {
    const TYPE: &'static [u8] = b"commit";

    type DeserializeError = DeserializeError;

    type Builder = Builder;

    fn oid(&self) -> Oid<Commit> {
        self.oid
    }

    fn deserialize(
        oid: Oid<Commit>,
        _len: usize,
        mut data: impl BufRead,
    ) -> Result<Self, Self::DeserializeError> {
        let mut parent = None;
        let mut tree = None;
        let mut author = None;

        let mut line = BString::from(Vec::new());
        loop {
            line.clear();
            let bytes_read = data.read_until(b'\n', &mut line)?;
            if bytes_read == 0 {
                return Err(DeserializeError::UnexpectedHeadersEnd);
            } else if bytes_read == 1 {
                break;
            }

            let i = line.find(b" ").ok_or(DeserializeError::MalformedHeader)?;
            let (key, value) = line.split_at(i);
            let value = &value[1..value.len() - 1];

            match key {
                b"parent" => {
                    let oid = Oid::parse(value).map_err(DeserializeError::ParseParent)?;
                    parent = Some(oid);
                }
                b"tree" => {
                    let oid = Oid::parse(value).map_err(DeserializeError::ParseTree)?;
                    tree = Some(oid);
                }
                b"author" => author = Some(Author::parse(value.as_bstr())?),
                _ => warn!(
                    key = ?key.to_str_lossy(),
                    value = ?value.to_str_lossy(),
                    "Unrecognized commit header"
                ),
            }
        }

        let tree = tree.ok_or(DeserializeError::MissingTree)?;
        let author = author.ok_or(DeserializeError::MissingAuthor)?;

        let mut msg = BString::from(Vec::new());
        data.read_to_end(&mut msg)?;

        Ok(Self {
            oid,
            parent,
            tree,
            author,
            msg,
        })
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Builder {
    pub parent: Option<Oid<Commit>>,
    pub tree: Oid<Tree>,
    pub author: db::Author,
    pub msg: BString,
}

impl Builder {
    pub fn new(
        parent: Option<Oid<Commit>>,
        tree: Oid<Tree>,
        author: db::Author,
        msg: impl Into<BString>,
    ) -> Self {
        Self {
            parent,
            tree,
            author,
            msg: msg.into(),
        }
    }
}

impl ObjectBuilder for Builder {
    type Object = Commit;

    fn store(self, db: &Db) -> db::StoreResult<Commit> {
        let author = self.author.serialize();

        let parent_line = if let Some(parent) = self.parent.as_ref() {
            Cow::Owned(format!("\nparent {}", parent.to_hex()))
        } else {
            Cow::Borrowed("")
        };

        let ser = format!(
            "tree {}{}\nauthor {}\ncommitter {}\n\n{}",
            self.tree.to_hex(),
            &parent_line,
            &author,
            &author,
            &self.msg
        );
        let ser = BString::from(ser);

        db.store_bytes::<Self>(ser.as_bstr())
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum DeserializeError {
    /// IO error
    Io(#[from] io::Error),
    /// Headers ended unexpectedly
    UnexpectedHeadersEnd,
    /// Malformed header
    MalformedHeader,
    /// Failed to parse oid of parent
    ParseParent(#[source] ParseOidError),
    /// Failed to parse oid of tree
    ParseTree(#[source] ParseOidError),
    /// Failed to parse author header
    ParseAuthor(#[from] author::ParseError),
    /// Header tree not present
    MissingTree,
    /// Header author not present
    MissingAuthor,
}
