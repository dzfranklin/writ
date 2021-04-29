use std::{
    ffi::{OsStr, OsString},
    fmt,
    os::unix::prelude::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
};

use bstr::{BStr, BString, ByteSlice};

use crate::core::Workspace;

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub struct WsPath(PathBuf);

impl WsPath {
    pub fn new_canonicalized(
        path: impl AsRef<Path>,
        workspace: &Workspace,
    ) -> Result<Self, NewCanonicalizeError> {
        let path = path.as_ref();
        let abs = workspace
            .path()
            .join(path)
            .canonicalize()
            .map_err(|e| NewCanonicalizeError::Io(path.to_owned(), e))?;
        if let Ok(path) = abs.strip_prefix(workspace.path()) {
            Ok(Self::new_unchecked(path))
        } else {
            Err(NewCanonicalizeError::NotInWorkspace(path.to_owned()))
        }
    }

    /// Path must be in canonical form and inside the workspace you use it with
    pub fn new_unchecked(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn new_unchecked_bytes(path: impl Into<BString>) -> Self {
        let path: BString = path.into();
        let path: Vec<u8> = path.into();
        let path = OsString::from_vec(path);
        let path = PathBuf::from(path);
        Self(path)
    }

    pub fn root() -> Self {
        Self(PathBuf::new())
    }

    pub fn as_bstr(&self) -> &BStr {
        self.0.as_os_str().as_bytes().as_bstr()
    }

    pub fn to_bstring(&self) -> BString {
        self.as_bstr().to_owned()
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    pub fn as_path_buf(&mut self) -> &mut PathBuf {
        &mut self.0
    }

    /// Panics if self is outside of workspace
    pub fn to_absolute(&self, workspace: &Workspace) -> PathBuf {
        let path = workspace.path().join(&self.0);
        if !path.starts_with(workspace.path()) {
            panic!("Workspace path outside of workspace was created: {:?}. Refusing to make absolute. Workspace: {:?}", self, workspace);
        }
        path
    }

    pub fn file_name(&self) -> &BStr {
        if let Some(name) = self.0.file_name() {
            name.as_bytes().as_bstr()
        } else {
            panic!(
                "Non-normalized path was created: {:?}. Failed to get file name",
                self,
            )
        }
    }

    pub fn parent(&self) -> WsPath {
        self.0
            .parent()
            .map_or_else(WsPath::root, |path| WsPath(path.to_owned()))
    }

    pub fn parents(&self) -> Parents {
        Parents::new(self)
    }

    pub fn components(&self) -> impl DoubleEndedIterator<Item = &BStr> {
        self.0
            .components()
            .map(|c| c.as_os_str().as_bytes().as_bstr())
    }

    pub fn parent_components(&self) -> impl DoubleEndedIterator<Item = &BStr> {
        let mut iter = self.components();
        iter.next_back();
        iter
    }

    pub fn join(&self, path: impl AsRef<Path>) -> Self {
        Self(self.0.join(path))
    }

    pub fn push(&mut self, path: impl AsRef<Path>) {
        self.0.push(path)
    }

    pub fn join_bytes(&self, path: &BStr) -> Self {
        let path = OsStr::from_bytes(path.as_bytes());
        Self(self.0.join(path))
    }

    pub fn strip_prefix(&self, base: &Self) -> Result<Self, std::path::StripPrefixError> {
        self.0.strip_prefix(&base.0).map(Self::new_unchecked)
    }
}

impl fmt::Display for WsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_bstr())
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum NewCanonicalizeError {
    /// IO error canonicalizing {0:?}
    Io(PathBuf, #[source] std::io::Error),
    /// Path {0:?} is outside the workspace
    NotInWorkspace(PathBuf),
}

impl From<WsPath> for PathBuf {
    fn from(path: WsPath) -> Self {
        path.0
    }
}

impl AsRef<Path> for WsPath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl AsRef<BStr> for WsPath {
    fn as_ref(&self) -> &BStr {
        self.as_bstr()
    }
}

impl From<WsPath> for BString {
    fn from(path: WsPath) -> Self {
        path.to_bstring()
    }
}

impl PartialOrd for WsPath {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.as_bstr().partial_cmp(other.as_bstr())
    }
}

impl Ord for WsPath {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_bstr().cmp(other.as_bstr())
    }
}

impl PartialEq<BStr> for WsPath {
    fn eq(&self, other: &BStr) -> bool {
        self.as_bstr().eq(other)
    }
}

impl PartialEq<str> for WsPath {
    fn eq(&self, other: &str) -> bool {
        self.as_bstr().eq(other.as_bytes().as_bstr())
    }
}

impl PartialEq<&str> for WsPath {
    fn eq(&self, other: &&str) -> bool {
        self.as_bstr().eq(other.as_bytes().as_bstr())
    }
}

impl PartialEq<WsPath> for str {
    fn eq(&self, other: &WsPath) -> bool {
        self.as_bytes().as_bstr().eq(other.as_bstr())
    }
}

#[derive(Debug, Clone)]
pub struct Parents<'p> {
    inner: Option<NonEmptyParents<'p>>,
}

#[derive(Debug, Clone)]
struct NonEmptyParents<'p> {
    remaining: std::path::Components<'p>,
    prefix: WsPath,
}

impl<'p> Parents<'p> {
    fn new(path: &'p WsPath) -> Self {
        let inner = path.as_path().parent().map(|parent| NonEmptyParents {
            remaining: parent.components(),
            prefix: WsPath::root(),
        });
        Self { inner }
    }
}

impl<'p> Iterator for Parents<'p> {
    type Item = WsPath;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = self.inner.as_mut()?;

        if let Some(component) = inner.remaining.next() {
            match component {
                std::path::Component::Normal(parent) => {
                    let full = inner.prefix.join(parent);
                    inner.prefix.push(component);
                    Some(full)
                }
                _ => panic!("WsPath wasn't normalized. Refusing to continue iterating over parents. Got {:?}, component: {:?}", inner, component),
            }
        } else {
            self.inner.take();
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parents() {
        let path = WsPath::new_unchecked("foo/bar/baq/buz.txt");
        let actual = path.parents().collect::<Vec<_>>();
        let expected = vec![
            WsPath::new_unchecked("foo"),
            WsPath::new_unchecked("foo/bar"),
            WsPath::new_unchecked("foo/bar/baq"),
        ];
        assert_eq!(expected, actual);
    }
}
