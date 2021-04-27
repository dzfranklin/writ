use crate::WsPath;

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub struct FileStatus {
    path: WsPath,
    status: Status,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Status {
    Untracked,
    Modified,
    Unmodified,
    Deleted,
}

impl FileStatus {
    pub fn new(path: WsPath, status: Status) -> Self {
        Self { path, status }
    }

    pub fn path(&self) -> &WsPath {
        &self.path
    }

    pub fn status(&self) -> Status {
        self.status
    }
}
