use std::{fs, os::linux::fs::MetadataExt, os::unix::fs::PermissionsExt, time::SystemTime};

use bstr::{BStr, ByteSlice};
use tracing::warn;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Mode {
    Regular,
    Executable,
}

#[derive(Debug, Clone)]
pub struct Stat(fs::Metadata);

impl Mode {
    const EXECUTABLE: u32 = 0o10_07_55;
    const REGULAR: u32 = 0o10_06_44;

    pub fn as_base10(self) -> &'static BStr {
        match self {
            Self::Regular => b"100644".as_bstr(),
            Self::Executable => b"100755".as_bstr(),
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            Self::Regular => Self::REGULAR,
            Self::Executable => Self::EXECUTABLE,
        }
    }

    /// Unrecognized modes are considered [`Self::Regular`]
    pub fn from_u32(val: u32) -> Self {
        match val {
            Self::REGULAR => Self::Regular,
            Self::EXECUTABLE => Self::Executable,
            val => {
                warn!("Assuming unrecognized mode to be Mode::Regular: {:#o}", val);
                Self::Regular
            }
        }
    }
}

impl Stat {
    pub fn new(meta: fs::Metadata) -> Self {
        Self(meta)
    }

    pub fn mode(&self) -> Mode {
        if self.is_executable() {
            Mode::Executable
        } else {
            Mode::Regular
        }
    }

    pub fn ctime(&self) -> SystemTime {
        self.0.created().expect("OS supports ctime")
    }

    pub fn mtime(&self) -> SystemTime {
        self.0.modified().expect("OS supports mtime")
    }

    #[allow(clippy::clippy::cast_possible_truncation)]
    pub fn dev(&self) -> u32 {
        self.0.st_dev() as _
    }

    #[allow(clippy::clippy::cast_possible_truncation)]
    pub fn ino(&self) -> u32 {
        self.0.st_ino() as _
    }

    pub fn uid(&self) -> u32 {
        self.0.st_uid() as _
    }

    pub fn gid(&self) -> u32 {
        self.0.st_gid()
    }

    #[allow(clippy::clippy::cast_possible_truncation)]
    pub fn size(&self) -> u32 {
        self.0.st_size() as _
    }

    pub fn is_executable(&self) -> bool {
        let permissions = self.0.permissions();
        self.0.is_file() && permissions.mode() & 0o111 != 0
    }
}
