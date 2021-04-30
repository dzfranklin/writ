use crate::core::WsPath;

#[derive(Debug, Clone, Eq, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub struct FileStatus {
    pub path: WsPath,
    pub index: Status,
    pub workspace: Status,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Status {
    Untracked,
    Modified,
    Unmodified,
    Deleted,
    Added,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Untracked => "untracked",
            Status::Modified => "modified",
            Status::Unmodified => "unmodified",
            Status::Deleted => "deleted",
            Status::Added => "added",
        }
    }
}
