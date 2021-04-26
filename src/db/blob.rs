use bstr::{BString, ByteSlice};

use crate::{
    db::{self, Db},
    Object,
};

#[derive(Debug, Clone)]
pub struct Blob(BString);

impl Blob {
    pub fn new<B: Into<BString>>(bytes: B) -> Self {
        Self(bytes.into())
    }
}

impl Object for Blob {
    const TYPE: &'static [u8] = b"blob";

    fn store(&self, db: &Db) -> db::StoreResult {
        let contents = self.0.as_bstr();
        db.store::<Self>(contents)
    }
}
