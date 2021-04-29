use std::{
    collections::BTreeMap,
    env, fmt, fs,
    io::{self},
    path::{Path, PathBuf},
};

use crate::{
    db::{self, object, Blob, Commit, Tree},
    index::{
        self,
        entry::{self, Entry, StatusChatty},
    },
    refs,
    ws::{self, ListFilesError, ReadFileError, StatFileError},
    Db, FileStatus, Index, ObjectBuilder, Oid, Refs, Status, Workspace, WsPath,
};
use bstr::BString;
use chrono::Local;
use tracing::{debug, instrument};

#[derive(Debug, Clone)]
pub struct Repo {
    git_dir: PathBuf,
    pub workspace: Workspace,
    pub db: Db,
    pub refs: Refs,
    pub index: Index,
}

impl Repo {
    #[instrument(err)]
    pub fn new(workspace: impl Into<PathBuf> + fmt::Debug) -> Result<Self, ReadError> {
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
        let index = Index::load(&git_dir)?;

        Ok(Self {
            git_dir,
            workspace,
            db,
            refs,
            index,
        })
    }

    pub fn for_current_dir() -> Result<Self, ForCurrentDirError> {
        let dir = env::current_dir()?;
        Ok(Self::new(dir)?)
    }

    #[instrument(err)]
    pub fn init(workspace: impl Into<PathBuf> + fmt::Debug) -> Result<Self, InitError> {
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
        let index = Index::load(&git_dir)?;

        Ok(Self {
            git_dir,
            workspace,
            db,
            refs,
            index,
        })
    }

    #[instrument(err)]
    pub fn add<P>(&mut self, files: Vec<P>) -> Result<(), AddError>
    where
        P: AsRef<Path> + fmt::Debug,
    {
        let workspace = &self.workspace;
        let db = &self.db;
        let mut index = self.index.modify()?;

        for file in workspace.find_files(files)? {
            let data = workspace.read_file(&file)?;
            let stat = workspace.stat(&file)?;

            let oid = db::blob::Builder::new(data).store(&db)?;
            let entry = Entry::new(file, oid, stat);

            debug!("Adding {:?}", entry);
            index.add(entry);
        }

        index.commit()?;

        Ok(())
    }

    #[instrument(err)]
    pub fn commit(
        &mut self,
        name: impl Into<String> + fmt::Debug,
        email: impl Into<String> + fmt::Debug,
        msg: impl Into<String> + fmt::Debug,
    ) -> Result<(), CommitError> {
        let mut msg = msg.into();
        if msg.is_empty() {
            return Err(CommitError::EmptyMessage);
        }
        if !msg.ends_with('\n') {
            msg.push('\n');
        }

        let name = name.into();
        let email = email.into();

        let db = &self.db;
        let refs = &self.refs;
        let index = &self.index;

        let entries = index.entries().map(|entry| db::tree::EntryBuilder {
            oid: entry.oid,
            path: entry.path.clone(),
            mode: entry.mode(),
        });

        let root = db::tree::Builder::new().entries(entries).store(&db)?;

        let parent = refs.head()?;
        let author = db::Author::new_local(name, email, Local::now());
        let commit = db::commit::Builder::new(parent, root, author, msg).store(db)?;
        refs.update_head(&commit)?;

        Ok(())
    }

    /// Unlike git, this lists files only. Children of untracked directories are
    /// reported instead of reporting the directory itself.
    #[instrument(err)]
    pub fn status(&mut self) -> Result<BTreeMap<BString, FileStatus>, StatusError> {
        let mut status = self
            .workspace
            .list_files()?
            .into_iter()
            .map(|path| {
                let key = path.to_bstring();
                let status = self.status_of(path)?;
                Ok((key, status))
            })
            .collect::<Result<BTreeMap<_, _>, StatusError>>()?;

        for entry in self.index.entries() {
            if !status.contains_key(entry.key()) {
                let value = FileStatus::new(entry.path.clone(), Status::Deleted);
                status.insert(entry.key().to_owned(), value);
            }
        }

        Ok(status)
    }

    pub fn status_of(&mut self, path: WsPath) -> Result<FileStatus, StatusError> {
        let status = if let Some(entry) = self.index.entry(&path) {
            match entry.status_chatty(&self.workspace)? {
                StatusChatty::Unmodified => Status::Unmodified,
                StatusChatty::UnmodifiedButNewStat(new_stat) => {
                    let mut index = self
                        .index
                        .modify()
                        .map_err(|e| StatusError::UpdateIndex(e.into()))?;
                    index.update_stat(&path, new_stat).expect("Entry exists");
                    index
                        .commit()
                        .map_err(|e| StatusError::UpdateIndex(e.into()))?;
                    Status::Unmodified
                }
                StatusChatty::Modified => Status::Modified,
                StatusChatty::Deleted => Status::Deleted,
            }
        } else {
            Status::Untracked
        };

        Ok(FileStatus::new(path, status))
    }

    pub fn show_head(&self) -> eyre::Result<()> {
        let head = self.refs.head()?.ok_or_else(|| eyre::eyre!("No HEAD"))?;
        let commit = self.db.load::<Commit>(head)?;
        eprintln!("HEAD: {}\n", head);
        self.print_tree(commit.tree, 0)?;
        Ok(())
    }

    fn print_tree(&self, tree: Oid<Tree>, level: usize) -> eyre::Result<()> {
        let tree = self.db.load::<Tree>(tree)?;
        let level_prefix = " ".repeat(level * 4);
        for node in tree.direct_children() {
            match node {
                db::tree::Node::File { name, mode, oid } => {
                    println!("{}{} {} ({:?})", level_prefix, oid, name, mode)
                }
                db::tree::Node::Tree { name, oid } => {
                    println!("{}{} {}/", level_prefix, oid, name);
                    self.print_tree(*oid, level + 1)?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
// Failed in initialize repository
pub enum InitError {
    /// Directory {0:?} already exists
    Exists(PathBuf),
    /// Failed to create workspace directory {0:?}
    CreateWorkspace(PathBuf, #[source] io::Error),
    /// Failed to open directory {0:?} to initialize
    Open(PathBuf, #[source] io::Error),
    /// Failed to populate {0:?}
    Write(PathBuf, #[source] io::Error),
    /// Failed to open index
    OpenIndex(#[from] index::LoadError),
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
/// Failed to read a directory as a git repository.
pub enum ReadError {
    /// The directory {0:?} is not a git repository
    NotRepo(PathBuf),
    /// IO error while checking if directory {0:?} is a git repository
    Io(PathBuf, #[source] io::Error),
    /// Failed to open index
    OpenIndex(#[from] index::LoadError),
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
    LoadIndex(#[from] index::OpenForModificationsError),
    /// Failed to find files provided in repository
    FindFiles(#[from] ws::ListFilesError),
    /// Failed to stat file
    Stat(#[from] StatFileError),
    /// Failed to read file
    Read(#[from] ReadFileError),
    /// Failed to store file
    StoreBlob(#[from] db::StoreError<Blob>),
    /// Failed to commit changes to index
    CommitIndex(#[from] index::CommitError),
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum CommitError {
    /// Empty commit message
    EmptyMessage,
    /// Failed to load index
    LoadIndex(#[from] index::OpenForModificationsError),
    /// Failed to store tree
    StoreTree(#[from] db::StoreError<Tree>),
    /// Failed to store commit
    StoreCommit(#[from] db::StoreError<Commit>),
    /// Failed to read ref
    ReadRef(#[from] refs::ReadError),
    /// Failed to parse oid of parent commit
    ParseParentOid(#[from] object::ParseOidError),
    /// Failed to update ref
    UpdateRef(#[from] refs::UpdateError),
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum StatusError {
    /// Failed to list files
    ListFiles(#[from] ListFilesError),
    /// Failed to check if file unchanged
    IsUnchanged(#[from] entry::IsUnchangedError),
    /// Failed to update index with new stat
    UpdateIndex(#[from] index::ModifyError),
}
