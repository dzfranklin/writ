use std::borrow::Cow;

use bstr::{BStr, BString, ByteSlice};

use crate::Object;

#[derive(Debug, Clone)]
pub struct Blob(BString);

impl Blob {
    pub fn new<B: Into<BString>>(bytes: B) -> Self {
        Self(bytes.into())
    }
}

impl Object for Blob {
    const TYPE: &'static [u8] = b"blob";

    fn serialize(&self) -> Cow<BStr> {
        Cow::Borrowed(self.0.as_bstr())
    }
}
