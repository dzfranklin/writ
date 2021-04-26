use std::{convert::TryInto, fmt};

use crate::{db, Db};
use bstr::BStr;
use ring::digest::{digest, SHA1_FOR_LEGACY_USE_ONLY as SHA1};

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Oid([u8; Oid::SIZE]);

impl Oid {
    pub const SIZE: usize = 20;

    pub fn parse<C: AsRef<BStr>>(hex: C) -> Result<Self, ParseOidError> {
        let oid = hex::decode(hex.as_ref())?;
        let len = oid.len();
        let oid: [u8; Self::SIZE] = oid
            .try_into()
            .map_err(|_| ParseOidError::WrongLength(len))?;
        Ok(Oid::new(oid))
    }

    pub const fn new(bytes: [u8; Oid::SIZE]) -> Self {
        Oid(bytes)
    }

    pub fn for_bytes<B: AsRef<[u8]>>(bytes: B) -> Self {
        let digest = digest(&SHA1, bytes.as_ref());
        let hash = digest.as_ref().try_into().expect("Digest has correct len");
        Self(hash)
    }

    pub const fn zero() -> Self {
        Self::new([0; Self::SIZE])
    }

    pub const fn as_bytes(&self) -> &[u8; Oid::SIZE] {
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

    fn store(&self, db: &Db) -> db::StoreResult;
}

#[derive(displaydoc::Display, thiserror::Error, Debug)]
pub enum ParseOidError {
    /// Bytes provided can't be parsed as hex
    NotHex(#[from] hex::FromHexError),
    /// Wrong length. Got {0}, expected [`Oid::SIZE`]
    WrongLength(usize),
}
