use std::{borrow::Cow, fmt};

use bstr::{BStr, BString};
use crypto::{digest::Digest, sha1::Sha1};

#[derive(Clone)]
pub struct Oid([u8; Oid::SIZE]);

impl Oid {
    pub const SIZE: usize = 20;

    pub fn from<B: AsRef<[u8]>>(bytes: B) -> Self {
        let mut hasher = Sha1::new();
        hasher.input(bytes.as_ref());
        let mut hash = [0; Oid::SIZE];
        hasher.result(&mut hash);
        Self(hash)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }
}

impl fmt::Debug for Oid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Oid").field(&self.to_hex()).finish()
    }
}

pub trait Object: fmt::Debug + Clone {
    const TYPE: &'static [u8];

    fn serialize(&self) -> Cow<BStr>;
}
