use bstr::BString;
use chrono::{DateTime, Local};
use std::fmt::Write;

#[derive(Debug, Clone)]
pub struct Author {
    name: String,
    email: String,
    time: DateTime<Local>,
}

impl Author {
    pub fn new(name: String, email: String, time: DateTime<Local>) -> Self {
        Self { name, email, time }
    }

    pub(crate) fn serialize(&self) -> BString {
        let time = self.time.format("%s %z");
        format!("{} <{}> {}", &self.name, &self.email, time).into()
    }
}
