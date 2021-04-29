use bstr::{BStr, BString, ByteSlice, Utf8Error};
use chrono::{DateTime, FixedOffset, Local};
use lazy_static::lazy_static;
use regex::bytes::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    name: BString,
    email: BString,
    time: DateTime<FixedOffset>,
}

impl Author {
    const TIME_FORMAT: &'static str = "%s %z";

    pub fn new(
        name: impl Into<BString>,
        email: impl Into<BString>,
        time: DateTime<FixedOffset>,
    ) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
            time,
        }
    }

    pub fn new_local(
        name: impl Into<BString>,
        email: impl Into<BString>,
        time: DateTime<Local>,
    ) -> Self {
        let offset = time.offset();
        let time = time.with_timezone(offset);
        Self::new(name, email, time)
    }

    pub(crate) fn serialize(&self) -> BString {
        let time = self.time.format(Self::TIME_FORMAT);
        format!("{} <{}> {}", &self.name, &self.email, time).into()
    }

    pub(crate) fn parse(serialized: &BStr) -> Result<Author, ParseError> {
        lazy_static! {
            static ref RE: Regex =
                Regex::new("^(?P<name>.*) <(?P<email>.*?)> (?P<time>.*)$").unwrap();
        }

        let caps = RE
            .captures(serialized)
            .ok_or_else(|| ParseError::MatchFailed(serialized.to_owned()))?;

        let name = caps.name("name").unwrap().as_bytes().as_bstr().to_owned();
        let email = caps.name("email").unwrap().as_bytes().as_bstr().to_owned();

        let time = caps.name("time").unwrap().as_bytes();
        let time = time.to_str().map_err(|source| {
            ParseError::MalformedTimeEncoding(time.as_bstr().to_owned(), source)
        })?;
        let time = DateTime::parse_from_str(time, Self::TIME_FORMAT)
            .map_err(|e| ParseError::InvalidTime(time.to_owned(), e))?;

        Ok(Self { name, email, time })
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum ParseError {
    /// Failed to match expected pattern. Got: {0}
    MatchFailed(BString),
    /// Time is not valid utf8: {0}
    MalformedTimeEncoding(BString, #[source] Utf8Error),
    /// Failed to parse time: {0}
    InvalidTime(String, #[source] chrono::ParseError),
}
