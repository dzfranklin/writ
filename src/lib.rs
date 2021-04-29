#![feature(
    path_try_exists,
    with_options,
    associated_type_defaults,
    map_first_last,
    const_fn,
    debug_non_exhaustive,
    format_args_capture
)]
// TODO: Warn clippy::cargos
#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate
)]

pub mod db;
pub mod index;
pub mod locked_file;
pub mod refs;
pub mod repo;
pub mod stat;
pub mod status;
pub mod with_digest;
pub mod ws;

pub use db::{Db, Object, ObjectBuilder, Oid};
pub use index::{Index, IndexMut};
pub use locked_file::LockedFile;
pub use refs::Refs;
pub use repo::Repo;
pub use stat::Stat;
pub use status::{FileStatus, Status};
pub use with_digest::WithDigest;
pub use ws::Workspace;
pub use ws::WsPath;

#[cfg(test)]
mod test_support;
