use std::{convert::TryInto, fmt, io::BufRead};

use crate::{db, Db};
use bstr::{BStr, BString};
use ring::digest::{digest, SHA1_FOR_LEGACY_USE_ONLY as SHA1};

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Oid([u8; Oid::SIZE]);

impl Oid {
    pub const SIZE: usize = 20;
    pub(crate) const HEX_SIZE: usize = 40;

    pub fn parse(hex: impl AsRef<[u8]>) -> Result<Self, ParseOidError> {
        let hex = hex.as_ref();
        let len = hex.len();
        let hex = hex
            .try_into()
            .map_err(|_| ParseOidError::WrongHexSize(len))?;
        Ok(Self::parse_sized(hex)?)
    }

    pub fn parse_sized(hex: &[u8; Self::HEX_SIZE]) -> Result<Self, ParseSizedOidError> {
        let oid = hex::decode(hex.as_ref())?;
        let len = oid.len();
        let oid: [u8; Self::SIZE] = oid
            .try_into()
            .map_err(|_| ParseSizedOidError::WrongSize(len))?;
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

    type DeserializeError: std::error::Error + 'static;

    fn deserialize(
        oid: Oid,
        len: usize,
        data: impl BufRead,
    ) -> Result<Self, Self::DeserializeError>;

    fn compute_oid(serialized: &BStr) -> Oid {
        let mut with_prefix = Self::serialized_prefix(&serialized);
        with_prefix.reserve_exact(serialized.len());
        with_prefix.extend(serialized.iter());
        Oid::for_bytes(with_prefix)
    }

    fn serialized_prefix(serialized: &BStr) -> BString {
        let o_type = Self::TYPE;
        let size = serialized.len().to_string();

        let ser = Vec::with_capacity(o_type.len() + 1 + size.len() + 1);
        let mut ser = BString::from(ser);

        ser.extend(o_type);
        ser.push(b' ');
        ser.extend(size.as_bytes());
        ser.push(b'\0');

        ser
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ParseOidError {
    /// Wrong hex length. Expected [`Oid::HEX_SIZE`]
    #[error("Wrong hex length. Got {0}, expected `Oid::HEX_SIZE`")]
    WrongHexSize(usize),
    #[error(transparent)]
    Parse(#[from] ParseSizedOidError),
}

#[derive(displaydoc::Display, thiserror::Error, Debug)]
pub enum ParseSizedOidError {
    /// Bytes provided can't be parsed as hex
    NotHex(#[from] hex::FromHexError),
    /// Wrong length. Got {0}, expected [`Oid::SIZE`]
    WrongSize(usize),
}

#[cfg(test)]
mod tests {
    use crate::db::Blob;
    use bstr::ByteSlice;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn computes_correct_oid_for_empty_blob() {
        let oid = Blob::compute_oid(b"".as_bstr());
        assert_eq!(oid.to_hex(), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
    }

    #[test]
    fn computes_correct_oid_for_blob() {
        let oid = Blob::compute_oid(b"hello".as_bstr());
        assert_eq!(oid.to_hex(), "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0");
    }
}
