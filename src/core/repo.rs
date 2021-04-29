use std::{
    collections::BTreeMap,
    env, fmt, fs,
    io::{self},
    path::{Path, PathBuf},
};

use crate::core::{
    db::{self, object, tree, Blob, Commit, Tree},
    index::{
        self,
        entry::{self, Entry, StatusChatty},
    },
    refs,
    ws::{self, ListFilesError, ReadFileError, StatFileError},
    Db, FileStatus, Index, IndexMut, ObjectBuilder, Oid, Refs, Status, Workspace, WsPath,
};
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
    pub fn add<I, P>(&mut self, files: I) -> Result<(), AddError>
    where
        I: IntoIterator<Item = P> + fmt::Debug,
        P: AsRef<Path>,
    {
        let workspace = &self.workspace;
        let db = &self.db;
        self.index.reload()?;
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
    pub fn status(&mut self) -> Result<BTreeMap<WsPath, FileStatus>, StatusError> {
        let head = if let Some(head) = self.refs.head()? {
            let tree = self.db.load(head)?.tree;
            self.db.load_tree_files(&WsPath::root(), tree)?
        } else {
            BTreeMap::new()
        };

        self.index.reload()?;
        let mut index = self
            .index
            .modify()
            .map_err(|e| StatusError::UpdateIndex(e.into()))?;

        let work = &self.workspace;

        let mut ws_statuses = BTreeMap::new();
        let mut index_statuses = BTreeMap::new();

        for path in self.workspace.list_files()? {
            let ws_status = Self::workspace_status_of(work, &mut index, &path)?;
            let index_status = Self::index_status_of(&index, &head, &path)?;
            debug!("{path} in workspace, so ws: {ws_status:?}, idx: {index_status:?}");
            ws_statuses.insert(path.clone(), ws_status);
            index_statuses.insert(path, index_status);
        }

        for entry in index.entries() {
            if !ws_statuses.contains_key(&entry.path) {
                debug!("{} in idx but not ws, so ws: Status::Deleted", entry.path);
                ws_statuses.insert(entry.path.clone(), Status::Deleted);
            }
        }

        for (path, _file) in head {
            if !index.is_tracked_file(&path) {
                debug!("{path} in head but not idx, so idx: Status::Deleted",);
                index_statuses.insert(path, Status::Deleted);
            }
        }

        index
            .commit()
            .map_err(|e| StatusError::UpdateIndex(e.into()))?;

        let mut statuses = BTreeMap::new();

        for (path, ws_status) in ws_statuses {
            let index_status = index_statuses.remove(&path).unwrap_or(Status::Untracked);
            statuses.insert(
                path.clone(),
                FileStatus {
                    path,
                    workspace: ws_status,
                    index: index_status,
                },
            );
        }

        for (path, index_status) in index_statuses {
            statuses.insert(
                path.clone(),
                FileStatus {
                    path,
                    workspace: Status::Deleted,
                    index: index_status,
                },
            );
        }

        Ok(statuses)
    }

    pub fn workspace_status_of(
        work: &Workspace,
        index: &mut IndexMut,
        path: &WsPath,
    ) -> Result<Status, StatusError> {
        let status = if let Some(entry) = index.entry(path) {
            match entry.index_status_chatty(work)? {
                StatusChatty::Unmodified => Status::Unmodified,
                StatusChatty::UnmodifiedButNewStat(new_stat) => {
                    index.update_stat(path, new_stat).expect("Entry exists");
                    Status::Unmodified
                }
                StatusChatty::Modified => Status::Modified,
                StatusChatty::Deleted => Status::Deleted,
            }
        } else {
            Status::Untracked
        };
        Ok(status)
    }

    #[allow(clippy::option_if_let_else)]
    pub fn index_status_of(
        index: &Index,
        head: &BTreeMap<WsPath, tree::FileNode>,
        path: &WsPath,
    ) -> Result<Status, StatusError> {
        let index_entry = if let Some(index_entry) = index.entry(path) {
            index_entry
        } else {
            return Ok(Status::Untracked);
        };

        let status = if let Some(head_file) = head.get(path) {
            if head_file.mode == index_entry.mode() && head_file.oid == index_entry.oid {
                Status::Unmodified
            } else {
                Status::Modified
            }
        } else {
            Status::Added
        };

        Ok(status)
    }

    pub fn show_head(&mut self) -> eyre::Result<()> {
        let head = self.refs.head()?.ok_or_else(|| eyre::eyre!("No HEAD"))?;
        let commit = self.db.load::<Commit>(head)?;
        eprintln!("HEAD: {}\n", head);
        self.print_tree(commit.tree, 0)?;
        Ok(())
    }

    fn print_tree(&mut self, tree: Oid<Tree>, level: usize) -> eyre::Result<()> {
        let tree = self.db.load::<Tree>(tree)?;
        let level_prefix = " ".repeat(level * 4);
        for node in tree.direct_children() {
            match node {
                db::tree::Node::File(db::tree::FileNode { name, mode, oid }) => {
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
    /// Failed to reload index
    ReloadIndex(#[from] index::LoadError),
    /// Failed to open index of modifications
    OpenIndex(#[from] index::OpenForModificationsError),
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
    /// Failed to reload index
    ReloadIndex(#[from] index::LoadError),
    /// Failed to get head oid
    GetHeadOid(#[from] refs::ReadError),
    /// Failed to load head commit
    LoadHeadCommit(#[from] db::LoadError<db::Commit>),
    /// Failed to load of tree from head
    LoadHeadTree(#[from] db::LoadError<db::Tree>),
    /// Failed to list files
    ListFiles(#[from] ListFilesError),
    /// Failed to check if file unchanged
    IsUnchanged(#[from] entry::IsUnchangedError),
    /// Failed to update index with new stat
    UpdateIndex(#[from] index::ModifyError),
}
