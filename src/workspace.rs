use crate::Stat;

use bstr::BString;
use std::{
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};
use tracing::debug;

#[derive(Debug, Clone)]
pub struct Workspace {
    path: PathBuf,
}

impl Workspace {
    const IGNORE: &'static [&'static [u8]] = &[b".git"];

    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub fn find_files<P: AsRef<Path>>(
        &self,
        paths: Vec<P>,
    ) -> Result<Vec<PathBuf>, FindFilesError> {
        let mut files = Vec::new();

        for rel_path in paths {
            let abs_path = self.path.join(rel_path).canonicalize()?;

            // we re-compute this to canonicalize
            let rel_path = abs_path
                .strip_prefix(&self.path)
                .map_err(|_| FindFilesError::OutsideOfWorkspace(abs_path.clone()))?;

            let meta = abs_path.metadata()?;

            if Self::is_ignored(&rel_path) {
                continue;
            }

            if meta.is_dir() {
                debug!("Listing directory {:?}", rel_path);

                let children = abs_path
                    .read_dir()?
                    .map(|e| Ok(rel_path.join(e?.file_name())))
                    .collect::<Result<Vec<_>, io::Error>>()?;

                files.extend(self.find_files(children)?);
            } else if meta.is_file() {
                debug!("Listed file {:?}", rel_path);

                files.push(rel_path.into());
            } else {
                return Err(FindFilesError::InvalidFileType(rel_path.into()));
            }
        }

        Ok(files)
    }

    fn is_ignored(rel_path: &Path) -> bool {
        Self::IGNORE
            .iter()
            .any(|&ignored| rel_path.as_os_str().as_bytes() == ignored)
    }

    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> io::Result<BString> {
        let bytes = fs::read(self.path.join(path))?;
        Ok(bytes.into())
    }

    pub fn stat<P: AsRef<Path>>(&self, path: P) -> io::Result<Stat> {
        self.path.join(path).metadata().map(Stat::new)
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum FindFilesError {
    /// {0:?} is neither a file nor a directory.
    InvalidFileType(PathBuf),
    /// Path {0:?} is outside the workspace
    OutsideOfWorkspace(PathBuf),
    /// IO
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::init_logs;
    use insta::assert_debug_snapshot;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn excludes_git_dir() -> anyhow::Result<()> {
        init_logs();

        let dir = tempdir()?;
        let dir = dir.path();

        fs::create_dir(dir.join(".git"))?;
        fs::write(dir.join(".git/a"), "foo")?;
        fs::write(dir.join("b"), "foo")?;

        let workspace = Workspace::new(dir);
        let actual = workspace.find_files(vec!["."])?;

        let expected: Vec<PathBuf> = vec!["b".into()];

        assert_eq!(expected, actual);

        Ok(())
    }

    #[test]
    fn find_files() -> anyhow::Result<()> {
        init_logs();

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
