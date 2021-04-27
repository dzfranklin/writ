use std::io::{self, BufRead};

use bstr::{BString, ByteSlice};

use crate::{
    db::{self, Db},
    Object, Oid,
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

    type DeserializeError = io::Error;

    fn deserialize(
        _oid: Oid,
        len: usize,
        mut data: impl BufRead,
    ) -> Result<Self, Self::DeserializeError> {
        let mut buf: BString = Vec::with_capacity(len).into();
        data.read_exact(&mut buf)?;
        Ok(Self(buf))
    }
}
