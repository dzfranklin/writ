use std::{
    env, fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

use crate::{db, index, workspace, Db, Entry, Index, Object, Oid, Refs, Workspace};
use chrono::Local;
use eyre::eyre;
use tracing::info;

#[derive(Debug, Clone)]
pub struct Repo {
    git: PathBuf,
    workspace: PathBuf,
}

impl Repo {
    pub fn new(workspace: impl Into<PathBuf>) -> Result<Self, ReadError> {
        let workspace = workspace.into();
        let workspace = workspace
            .canonicalize()
            .map_err(|e| ReadError::Io(workspace, e))?;

        let git = workspace.join(".git");

        if !git
            .try_exists()
            .map_err(|e| ReadError::Io(git.clone(), e))?
        {
            return Err(ReadError::NotRepo(workspace));
        }

        Ok(Self { git, workspace })
    }

    pub fn for_current_dir() -> Result<Self, ForCurrentDirError> {
        let dir = env::current_dir()?;
        Ok(Self::new(dir)?)
    }

    pub fn init(workspace: impl Into<PathBuf>) -> Result<Self, InitError> {
        let workspace = workspace.into();
        let workspace = workspace
            .canonicalize()
            .map_err(|e| InitError::Open(workspace, e))?;
        let git = workspace.join(".git");

        if git
            .try_exists()
            .map_err(|e| InitError::Open(git.clone(), e))?
        {
            return Err(InitError::Exists(git));
        }

        for child in &["objects", "refs"] {
            let child = git.join(child);
            fs::create_dir_all(&child).map_err(|e| InitError::Write(child, e))?;
        }

        info!("Initialized repository in {:?}", workspace);

        Ok(Self { git, workspace })
    }

    pub fn db(&self) -> Db {
        Db::new(&self.git)
    }

    pub fn load_index(&self) -> Result<Index, index::LoadError> {
        Index::load(&self.git)
    }

    pub fn refs(&self) -> Refs {
        Refs::new(&self.git)
    }

    pub fn workspace(&self) -> Workspace {
        Workspace::new(&self.workspace)
    }

    pub fn add<P>(&self, files: Vec<P>) -> Result<(), AddError>
    where
        P: AsRef<Path>,
    {
        let workspace = self.workspace();
        let mut db = self.db();
        let mut index = self.load_index()?;

        for file in workspace.find_files(files)? {
            let data = workspace.read_file(&file).map_err(AddError::ReadFile)?;
            let stat = workspace.stat(&file).map_err(AddError::ReadFile)?;

            let blob = db::Blob::new(data);
            let oid = blob.store(&mut db)?;
            index.add(Entry::from(file, oid, &stat));
        }

        index.commit()?;

        Ok(())
    }

    pub fn commit(&self, name: String, email: String, mut msg: String) -> eyre::Result<()> {
        if msg.is_empty() {
            return Err(eyre!("Empty commit message"));
        }
        if !msg.ends_with('\n') {
            msg.push('\n');
        }

        let mut db = self.db();
        let index = self.load_index()?;
        let refs = self.refs();

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
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
// Failed in initialize repository
pub enum InitError {
    /// Directory {0:?} already exists
    Exists(PathBuf),
    /// Failed to open directory {0:?} to initialize
    Open(PathBuf, #[source] io::Error),
    /// Failed to populate {0:?}
    Write(PathBuf, #[source] io::Error),
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
/// Failed to read a directory as a git repository.
pub enum ReadError {
    /// The directory {0:?} is not a git repository
    NotRepo(PathBuf),
    /// IO error while checking if directory {0:?} is a git repository
    Io(PathBuf, #[source] io::Error),
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
/// Failed to read the current directory as a git repository.
pub enum ForCurrentDirError {
    /// Failed to get the current directory
    GetCurrentDir(#[from] io::Error),
    /// Failed to read git repository
    ReadError(#[from] ReadError),
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum AddError {
    /// Failed to load index
    LoadIndex(#[from] index::LoadError),
    /// Failed to find files provided in repository
    FindFiles(#[from] workspace::FindFilesError),
    /// Failed to read file
    ReadFile(#[source] io::Error),
    /// Failed to store blob
    StoreBlob(#[from] db::StoreError),
    /// Failed to commit changes to index
    CommitIndex(#[from] index::CommitError),
}
