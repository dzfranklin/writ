use std::{
    env, fs,
    io::{self},
    path::{Path, PathBuf},
};

use crate::{db, index, object, refs, ws, Db, Entry, Index, Object, Oid, Refs, Workspace};
use chrono::Local;
use tracing::info;

#[derive(Debug, Clone)]
pub struct Repo {
    git_dir: PathBuf,
    workspace: Workspace,
    db: Db,
    refs: Refs,
}

impl Repo {
    pub fn new(workspace: impl Into<PathBuf>) -> Result<Self, ReadError> {
        let workspace_dir = workspace.into();
        let workspace_dir = workspace_dir
            .canonicalize()
            .map_err(|e| ReadError::Io(workspace_dir, e))?;

        let git_dir = workspace_dir.join(".git");

        if !git_dir
            .try_exists()
            .map_err(|e| ReadError::Io(git_dir.clone(), e))?
        {
            return Err(ReadError::NotRepo(workspace_dir));
        }

        let workspace = Workspace::new(workspace_dir);
        let db = Db::new(&git_dir);
        let refs = Refs::new(&git_dir);

        info!("Opened repository {:?}", workspace);

        Ok(Self {
            git_dir,
            workspace,
            db,
            refs,
        })
    }

    pub fn for_current_dir() -> Result<Self, ForCurrentDirError> {
        let dir = env::current_dir()?;
        Ok(Self::new(dir)?)
    }

    pub fn init(workspace: impl Into<PathBuf>) -> Result<Self, InitError> {
        let workspace_dir = workspace.into();

        fs::create_dir_all(&workspace_dir)
            .map_err(|e| InitError::CreateWorkspace(workspace_dir.clone(), e))?;

        let git_dir = workspace_dir.join(".git");
        if git_dir
            .try_exists()
            .map_err(|e| InitError::Open(git_dir.clone(), e))?
        {
            return Err(InitError::Exists(git_dir));
        }

        for child in &["objects", "refs"] {
            let child = git_dir.join(child);
            fs::create_dir_all(&child).map_err(|e| InitError::Write(child, e))?;
        }

        let workspace = Workspace::new(workspace_dir);
        let db = Db::new(&git_dir);
        let refs = Refs::new(&git_dir);

        info!("Initialized repository in {:?}", workspace);

        Ok(Self {
            git_dir,
            workspace,
            db,
            refs,
        })
    }

    pub fn load_index(&self) -> Result<Index, index::LoadError> {
        Index::load(&self.git_dir)
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn refs(&self) -> &Refs {
        &self.refs
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn add<P>(&self, files: Vec<P>) -> Result<(), AddError>
    where
        P: AsRef<Path>,
    {
        let workspace = self.workspace();
        let db = self.db();
        let mut index = self.load_index()?;

        for file in workspace.find_files(files)? {
            let data = workspace.read_file(&file).map_err(AddError::ReadFile)?;
            let stat = workspace.stat(&file).map_err(AddError::ReadFile)?;

            let blob = db::Blob::new(data);
            let oid = blob.store(&db)?;
            index.add(Entry::from(file, oid, &stat));
        }

        index.commit()?;

        Ok(())
    }

    pub fn commit(&self, name: String, email: String, mut msg: String) -> Result<(), CommitError> {
        if msg.is_empty() {
            return Err(CommitError::EmptyMessage);
        }
        if !msg.ends_with('\n') {
            msg.push('\n');
        }

        let db = self.db();
        let index = self.load_index()?;
        let refs = self.refs();

        let entries: Vec<_> = index.entries().map(Clone::clone).collect();

        let root = db::Tree::from(entries);
        let root_oid = root.store(&db)?;

        let parent = refs.read_head()?.map(Oid::parse).transpose()?;
        let author = db::Author::new(name, email, Local::now());
        let commit = db::Commit::new(parent, root_oid, author, msg.clone());
        let commit_oid = commit.store(&db)?;
        refs.update_head(&commit_oid)?;

        info!(oid=?commit_oid, ?msg, "Commit");

        Ok(())
    }

    // pub fn status(&self) {

    // }
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
    FindFiles(#[from] ws::FindFilesError),
    /// Failed to read file
    ReadFile(#[source] io::Error),
    /// Failed to store blob
    StoreBlob(#[from] db::StoreError),
    /// Failed to commit changes to index
    CommitIndex(#[from] index::CommitError),
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum CommitError {
    /// Empty commit message
    EmptyMessage,
    /// Failed to load index
    LoadIndex(#[from] index::LoadError),
    /// Failed to store blob
    StoreBlob(#[from] db::StoreError),
    /// Failed to read ref
    ReadRef(#[from] refs::ReadError),
    /// Failed to parse oid of parent commit
    ParseParentOid(#[from] object::ParseOidError),
    /// Failed to update ref
    UpdateRef(#[from] refs::UpdateError),
}
