#![feature(path_try_exists, with_options)]
// TODO: Warn clippy::cargos
#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate
)]

pub mod db;
pub mod entry;
pub mod index;
pub mod locked_file;
pub mod object;
pub mod refs;
pub mod repo;
pub mod stat;
pub mod with_digest;
pub mod workspace;

pub use db::Db;
pub use entry::Entry;
pub use index::Index;
pub use locked_file::LockedFile;
pub use object::{Object, Oid};
pub use refs::Refs;
pub use repo::Repo;
pub use stat::Stat;
pub use with_digest::WithDigest;
pub use workspace::Workspace;

#[cfg(test)]
mod test_support;
