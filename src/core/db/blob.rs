use std::io::{self, BufRead};

use bstr::{BStr, BString, ByteSlice};

use crate::core::{
    db::{self, Db},
    Object, ObjectBuilder, Oid,
};

#[derive(Debug, Clone)]
pub struct Blob {
    pub bytes: BString,
    pub oid: Oid<Blob>,
}

impl Blob {
    pub fn oid_for_file(file: &BStr) -> Oid<Self> {
        let mut bytes = Db::serialized_prefix(Self::TYPE, &file);
        bytes.extend_from_slice(file.as_bytes());
        Oid::for_serialized_bytes(&bytes)
    }
}

impl Object for Blob {
    const TYPE: &'static [u8] = b"blob";

    type Builder = Builder;

    type DeserializeError = io::Error;

    fn oid(&self) -> Oid<Self> {
        self.oid
    }

    fn deserialize(
        oid: Oid<Self>,
        len: usize,
        mut data: impl BufRead,
    ) -> Result<Self, Self::DeserializeError> {
        let mut bytes: BString = Vec::with_capacity(len).into();
        data.read_exact(&mut bytes)?;
        Ok(Self { bytes, oid })
    }
}

#[derive(Debug, Clone)]
pub struct Builder(BString);

impl Builder {
    pub fn new<B: Into<BString>>(bytes: B) -> Self {
        Self(bytes.into())
    }
}

impl ObjectBuilder for Builder {
    type Object = Blob;

    fn store(self, db: &Db) -> db::StoreResult<Blob> {
        let contents = self.0.as_bstr();
        db.store_bytes::<Self>(contents)
    }
}
