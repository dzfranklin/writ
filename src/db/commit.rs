use std::borrow::Cow;

use bstr::{BString, ByteSlice};

use crate::{db, Db, Object, Oid};

#[derive(Debug, Clone)]
pub struct Commit {
    parent: Option<Oid>,
    tree: Oid,
    author: db::Author,
    msg: String,
}

impl Commit {
    pub fn new(parent: Option<Oid>, tree: Oid, author: db::Author, msg: String) -> Self {
        Self {
            parent,
            tree,
            author,
            msg,
        }
    }
}

impl Object for Commit {
    const TYPE: &'static [u8] = b"commit";

    fn store(&self, db: &Db) -> db::StoreResult {
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

        db.store::<Self>(ser.as_bstr())
    }
}
