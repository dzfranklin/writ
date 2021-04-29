pub mod path;
pub use path::WsPath;

use crate::core::Stat;

use bstr::BString;
use std::{
    fmt, fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct Workspace {
    path: PathBuf,
}

impl Workspace {
    const IGNORE: &'static [&'static [u8]] = &[b".git"];

    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    #[instrument(err)]
    pub fn find_files<I, P>(&self, paths: I) -> Result<Vec<WsPath>, ListFilesError>
    where
        I: IntoIterator<Item = P> + fmt::Debug,
        P: AsRef<Path>,
    {
        let mut files = Vec::new();

        for rel_path in paths {
            let rel_path = rel_path.as_ref();
            let abs_path = self
                .path
                .join(rel_path)
                .canonicalize()
                .map_err(|e| ListFilesError::Canonicalize(rel_path.to_owned(), e))?;
            self.list_files_in(&abs_path, &mut files)?;
        }

        Ok(files)
    }

    #[instrument(err)]
    pub fn list_files(&self) -> Result<Vec<WsPath>, ListFilesError> {
        let mut files = Vec::new();
        self.list_files_in(&self.path, &mut files)?;
        Ok(files)
    }

    fn list_files_in(
        &self,
        abs_path: &Path,
        files: &mut Vec<WsPath>,
    ) -> Result<(), ListFilesError> {
        // we re-compute this to canonicalize
        let rel_path = abs_path
            .strip_prefix(&self.path)
            .map_err(|_| ListFilesError::OutsideOfWorkspace(abs_path.to_owned()))?;

        let meta = abs_path
            .metadata()
            .map_err(|e| ListFilesError::GetMetadata(abs_path.to_owned(), e))?;

        if Self::is_ignored(&rel_path) {
            return Ok(());
        }

        if meta.is_dir() {
            let children = abs_path
                .read_dir()
                .map_err(|e| ListFilesError::ReadDir(abs_path.to_owned(), e))?
                .map(|entry| {
                    let entry =
                        entry.map_err(|e| ListFilesError::ReadDirEntry(abs_path.to_owned(), e))?;
                    Ok(rel_path.join(entry.file_name()))
                })
                .collect::<Result<Vec<_>, ListFilesError>>()?;

            files.extend(self.find_files(children)?);
        } else if meta.is_file() {
            files.push(WsPath::new_unchecked(rel_path));
        } else {
            return Err(ListFilesError::InvalidFileType(rel_path.into()));
        }

        Ok(())
    }

    fn is_ignored(rel_path: &Path) -> bool {
        Self::IGNORE
            .iter()
            .any(|&ignored| rel_path.as_os_str().as_bytes() == ignored)
    }

    pub fn read_file(&self, path: &WsPath) -> Result<BString, ReadFileError> {
        let bytes = fs::read(path.to_absolute(self)).map_err(|e| ReadFileError(path.clone(), e))?;
        Ok(bytes.into())
    }

    pub fn stat(&self, path: &WsPath) -> Result<Stat, StatFileError> {
        self.path
            .join(path)
            .metadata()
            .map(|m| Stat::from(&m))
            .map_err(|e| StatFileError(path.clone(), e))
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
/// Failed to stat file {0:?}
pub struct StatFileError(WsPath, io::Error);

impl StatFileError {
    pub(crate) fn is_not_found(&self) -> bool {
        self.1.kind() == io::ErrorKind::NotFound
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
/// Failed to read file {0:?}
pub struct ReadFileError(WsPath, io::Error);

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum ListFilesError {
    /// {0:?} is neither a file nor a directory.
    InvalidFileType(PathBuf),
    /// Path {0:?} is outside the workspace
    OutsideOfWorkspace(PathBuf),
    /// Failed to canonicalize path {0:?}
    Canonicalize(PathBuf, #[source] io::Error),
    /// Failed to get metadata of {0:?}
    GetMetadata(PathBuf, #[source] io::Error),
    /// Failed to read directory {0:?}
    ReadDir(PathBuf, #[source] io::Error),
    /// Failed to read entry of directory {0:?}
    ReadDirEntry(PathBuf, #[source] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::init;
    use insta::assert_debug_snapshot;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn excludes_git_dir() -> eyre::Result<()> {
        init();

        let dir = tempdir()?;
        let dir = dir.path();

        fs::create_dir(dir.join(".git"))?;
        fs::write(dir.join(".git/a"), "foo")?;
        fs::write(dir.join("b"), "foo")?;

        let workspace = Workspace::new(dir);
        let actual = workspace.find_files(vec!["."])?;

        let expected = vec![WsPath::new_unchecked("b")];

        assert_eq!(expected, actual);

        Ok(())
    }

    #[test]
    fn find_files() -> eyre::Result<()> {
        init();

        let dir = tempdir()?;
        let dir = dir.path();

        fs::create_dir_all(dir.join("dir_1/dir_a"))?;
        fs::create_dir_all(dir.join("dir_1/dir_b"))?;

        fs::write(dir.join("a"), "")?;
        fs::write(dir.join("b"), "")?;

        fs::write(dir.join("dir_1/c"), "")?;
        fs::write(dir.join("dir_1/d"), "")?;

        fs::write(dir.join("dir_1/dir_a/e"), "")?;

        let workspace = Workspace::new(dir);

        let mut files = workspace.find_files(vec!["a"])?;
        files.sort();
        assert_debug_snapshot!("a", files);

        let mut files = workspace.find_files(vec!["."])?;
        files.sort();
        assert_debug_snapshot!("dot", files);

        let mut files = workspace.find_files(vec!["dir_1"])?;
        files.sort();
        assert_debug_snapshot!("dir_1", files);

        Ok(())
    }
}
