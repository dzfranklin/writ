#![feature(path_try_exists)]
// TODO: Warn clippy::cargos
#![warn(clippy::all, clippy::pedantic)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate
)]

pub mod author;
pub mod blob;
pub mod commit;
pub mod database;
pub mod object;
pub mod tree;
pub mod workspace;

use author::Author;
use blob::Blob;
use commit::Commit;
use database::Database;
use object::{Object, Oid};
use tree::Tree;
use workspace::Workspace;

#[allow(unused)]
use tracing::{debug, error, info, instrument, span, warn};

use anyhow::{anyhow, Context};
use chrono::Local;
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub fn init<P: AsRef<Path>>(dir: P) -> anyhow::Result<()> {
    let root_path = normalize_path(dir.as_ref());
    let git_path = root_path.join(".git");

    if git_path.try_exists()? {
        return Err(anyhow!("{:?} already exists", git_path));
    }

    for dir in &["objects", "refs"] {
        fs::create_dir_all(git_path.join(dir)).context("Failed to populate .git")?;
    }

    info!(path = ?git_path, "Initialized empty repository");

    Ok(())
}

pub fn commit(name: String, email: String, mut msg: String) -> anyhow::Result<()> {
    if msg.is_empty() {
        return Err(anyhow!("Empty commit message"));
    }
    if !msg.ends_with("\n") {
        msg.push('\n');
    }

    let root_path = env::current_dir()?;
    let git_path = root_path.join(".git");
    let db_path = git_path.join("objects");

    let workspace = Workspace::new(root_path);
    let db = Database::new(db_path);

    let entries = workspace
        .list_files()?
        .into_iter()
        .map(|path| {
            let data = workspace.read_file(&path)?;
            let blob = Blob::new(data);
            let oid = db.store(&blob).context("Failed to store object")?;

            Ok(tree::Entry::new(path, oid))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let tree = Tree::new(entries);
    let tree_oid = db.store(&tree)?;

    let author = Author::new(name, email, Local::now());
    let commit = Commit::new(tree_oid, author, msg.clone());
    let commit_oid = db.store(&commit)?;

    info!(oid=?commit_oid, ?msg, "Commit");

    let head = git_path.join("HEAD");
    let mut head = fs::File::create(head)?;
    head.write_all(commit_oid.to_hex().as_bytes())?;

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
