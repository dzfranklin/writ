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
pub mod stat;
pub mod with_digest;
pub mod workspace;

pub use db::Db;
pub use entry::Entry;
pub use index::Index;
pub use locked_file::LockedFile;
pub use object::{Object, Oid};
pub use refs::Refs;
pub use stat::Stat;
pub use with_digest::WithDigest;
pub use workspace::Workspace;

#[cfg(test)]
mod test_support;

#[allow(unused)]
use tracing::{debug, error, info, instrument, span, warn};

use chrono::Local;
use eyre::{eyre, Context};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn init<P: AsRef<Path>>(dir: P) -> eyre::Result<()> {
    let root_path = normalize_path(dir.as_ref());
    let git_path = root_path.join(".git");

    if git_path.try_exists()? {
        return Err(eyre!("{:?} already exists", git_path));
    }

    for dir in &["objects", "refs"] {
        fs::create_dir_all(git_path.join(dir)).context("Failed to populate .git")?;
    }

    info!(path = ?git_path, "Initialized empty repository");

    Ok(())
}

pub fn commit<P: AsRef<Path>>(
    root_path: P,
    name: String,
    email: String,
    mut msg: String,
) -> eyre::Result<()> {
    if msg.is_empty() {
        return Err(eyre!("Empty commit message"));
    }
    if !msg.ends_with('\n') {
        msg.push('\n');
    }

    let root_path = root_path.as_ref();
    let git_path = root_path.join(".git");

    let mut db = Db::new(&git_path);
    let index = Index::load(&git_path)?;
    let refs = Refs::new(&git_path);

    let entries: Vec<_> = index.entries().map(Clone::clone).collect();

    let root = db::Tree::from(entries);
    let root_oid = root.store(&mut db)?;

    let parent = refs.read_head()?.map(Oid::parse).transpose()?;
    let author = db::Author::new(name, email, Local::now());
    let commit = db::Commit::new(parent, root_oid, author, msg.clone());
    let commit_oid = commit.store(&mut db)?;
    refs.update_head(&commit_oid)?;

    info!(oid=?commit_oid, ?msg, "Commit");

    Ok(())
}

pub fn add<R, P>(root_path: R, files: Vec<P>) -> eyre::Result<()>
where
    R: AsRef<Path>,
    P: AsRef<Path>,
{
    let root_path = root_path.as_ref();
    let git_path = root_path.join(".git");

    let workspace = Workspace::new(&root_path);
    let mut db = Db::new(&git_path);
    let mut index = Index::load(&git_path)?;

    for file in workspace.find_files(files)? {
        let data = workspace.read_file(&file)?;
        let stat = workspace.stat(&file)?;

        let blob = db::Blob::new(data);
        let oid = blob.store(&mut db)?;
        index.add(Entry::from(file, oid, &stat));
    }

    index.commit()?;

    Ok(())
}

fn normalize_path(path: &Path) -> PathBuf {
    // From Cargo
    // See <https://github.com/rust-lang/cargo/blob/fede83ccf973457de319ba6fa0e36ead454d2e20/src/cargo/util/paths.rs#L61>
    use std::path::Component;

    let mut components = path.components().peekable();
    let mut ret = components.peek().cloned().map_or_else(PathBuf::new, |c| {
        components.next();
        PathBuf::from(c.as_os_str())
    });

    for component in components {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {
                ret.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                ret.pop();
            }
            Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}
