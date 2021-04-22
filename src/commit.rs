use std::borrow::Cow;

use bstr::BStr;

use crate::{Author, Object, Oid};

#[derive(Debug, Clone)]
pub struct Commit {
    tree: Oid,
    author: Author,
    msg: String,
}

impl Commit {
    pub fn new(tree: Oid, author: Author, msg: String) -> Self {
        Self { tree, author, msg }
    }
}

impl Object for Commit {
    const TYPE: &'static [u8] = b"commit";

    fn serialize(&self) -> Cow<BStr> {
        let author = self.author.serialize();
        let ser = format!(
            "tree {}\nauthor {}\ncommitter {}\n\n{}",
            self.tree.to_hex(),
            &author,
            &author,
            &self.msg
        );
        Cow::Owned(ser.into())
    }
}
