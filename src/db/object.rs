use std::{convert::TryInto, fmt, io::BufRead, marker::PhantomData};

use crate::{db, Db};
use ring::digest::{digest, SHA1_FOR_LEGACY_USE_ONLY as SHA1};

pub struct Oid<O: Object> {
    inner: UntypedOid,
    _ty: PhantomData<O>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct UntypedOid([u8; OID_SIZE]);

pub const OID_SIZE: usize = 20;
pub(crate) const OID_HEX_SIZE: usize = 40;

/// Note that none of the constructors check the type of the Oid.
impl<O: Object> Oid<O> {
    pub const fn new(bytes: [u8; OID_SIZE]) -> Self {
        Self::from_untyped(UntypedOid::new(bytes))
    }

    pub const fn zero() -> Self {
        Self::new([0; OID_SIZE])
    }

    pub fn parse(hex: impl AsRef<[u8]>) -> Result<Self, ParseOidError> {
        let inner = UntypedOid::parse(hex)?;
        Ok(Self::from_untyped(inner))
    }

    pub fn parse_sized(hex: &[u8; OID_HEX_SIZE]) -> Result<Self, ParseSizedOidError> {
        let inner = UntypedOid::parse_sized(hex)?;
        Ok(Self::from_untyped(inner))
    }

    pub(crate) fn for_serialized_bytes<B: AsRef<[u8]>>(bytes: B) -> Self {
        let inner = UntypedOid::for_bytes(bytes);
        Self::from_untyped(inner)
    }

    pub fn into_untyped(self) -> UntypedOid {
        self.inner
    }

    pub fn as_untyped(&self) -> &UntypedOid {
        &self.inner
    }

    pub const fn from_untyped(oid: UntypedOid) -> Self {
        Self {
            inner: oid,
            _ty: PhantomData,
        }
    }

    fn type_as_str(&self) -> &'static str {
        &std::str::from_utf8(O::TYPE).expect("TYPE always utf8")
    }
}

impl<O: Object> std::ops::Deref for Oid<O> {
    type Target = UntypedOid;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl UntypedOid {
    pub const fn new(bytes: [u8; OID_SIZE]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self::new([0; OID_SIZE])
    }

    pub fn parse(hex: impl AsRef<[u8]>) -> Result<Self, ParseOidError> {
        let hex = hex.as_ref();
        let len = hex.len();
        let hex = hex
            .try_into()
            .map_err(|_| ParseOidError::WrongHexSize(len))?;
        Ok(Self::parse_sized(hex)?)
    }

    pub fn parse_sized(hex: &[u8; OID_HEX_SIZE]) -> Result<Self, ParseSizedOidError> {
        let oid = hex::decode(hex.as_ref())?;
        let oid: [u8; OID_SIZE] = oid.try_into().expect("size known correct");
        Ok(Self::new(oid))
    }

    pub fn for_bytes<B: AsRef<[u8]>>(bytes: B) -> Self {
        let digest = digest(&SHA1, bytes.as_ref());
        let hash = digest.as_ref().try_into().expect("Digest has correct len");
        Self::new(hash)
    }

    pub const fn as_bytes(&self) -> &[u8; OID_SIZE] {
        &self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }

    pub fn to_typed<O: Object>(self) -> Oid<O> {
        Oid::from_untyped(self)
    }
}

impl<O: Object> Clone for Oid<O> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner,
            _ty: PhantomData,
        }
    }
}

impl<O: Object> Copy for Oid<O> {}

impl<O: Object> std::hash::Hash for Oid<O> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state)
    }
}

impl<O: Object> fmt::Debug for Oid<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Oid")
            .field("bytes", &self.to_hex())
            .field("type", &self.type_as_str())
            .finish()
    }
}

impl<O: Object> fmt::Display for Oid<O> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", &self.type_as_str(), self.to_hex())
    }
}

impl<O: Object> PartialEq for Oid<O> {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl<O: Object> Eq for Oid<O> {}

pub trait Object: fmt::Debug + Clone {
    const TYPE: &'static [u8];

    type Builder: ObjectBuilder;

    type DeserializeError: std::error::Error + 'static;

    fn oid(&self) -> Oid<Self>;

    fn deserialize(
        oid: Oid<Self>,
        len: usize,
        data: impl BufRead,
    ) -> Result<Self, Self::DeserializeError>;
}

#[allow(clippy::module_name_repetitions)]
pub trait ObjectBuilder: fmt::Debug + Clone {
    type Object: Object;

    fn store(self, db: &Db) -> db::StoreResult<Self::Object>;
}

#[derive(thiserror::Error, Debug)]
pub enum ParseOidError {
    /// Wrong hex length. Expected [`Oid::HEX_SIZE`]
    #[error("Wrong hex length. Got {0}, expected `Oid::HEX_SIZE`")]
    WrongHexSize(usize),
    #[error(transparent)]
    Parse(#[from] ParseSizedOidError),
}

#[derive(thiserror::Error, displaydoc::Display, Debug)]
/// Failed to parse Oid
pub struct ParseSizedOidError(#[from] hex::FromHexError);
