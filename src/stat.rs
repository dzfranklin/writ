use std::{
    convert::TryInto,
    fs,
    os::linux::fs::MetadataExt,
    time::{Duration, SystemTime},
};
use tracing::warn;

use bstr::{BStr, ByteSlice};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Stat {
    pub ctime: SystemTime,
    pub mtime: SystemTime,
    pub dev: u32,
    pub ino: u32,
    pub mode: Mode,
    pub uid: u32,
    pub gid: u32,
    pub size: u32,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Mode {
    Regular,
    Executable,
}

impl Stat {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::similar_names
    )]
    pub fn from(meta: &fs::Metadata) -> Self {
        Self {
            ctime: Self::systemtime_from_epoch(meta.st_ctime() as u32, meta.st_ctime_nsec() as u32),
            mtime: Self::systemtime_from_epoch(meta.st_mtime() as u32, meta.st_mtime_nsec() as u32),
            dev: meta.st_dev() as u32,
            ino: meta.st_ino() as u32,
            mode: Mode::from_u32(meta.st_mode()),
            uid: meta.st_uid(),
            gid: meta.st_gid(),
            size: meta.st_size() as u32,
        }
    }

    pub fn zeroed() -> Self {
        Self {
            ctime: SystemTime::UNIX_EPOCH,
            mtime: SystemTime::UNIX_EPOCH,
            dev: 0,
            ino: 0,
            mode: Mode::Regular,
            uid: 0,
            gid: 0,
            size: 0,
        }
    }

    pub fn ctime_epoch(&self) -> (u32, u32) {
        Self::systemtime_to_epoch(self.ctime)
    }

    pub fn mtime_epoch(&self) -> (u32, u32) {
        Self::systemtime_to_epoch(self.mtime)
    }

    fn systemtime_to_epoch(time: SystemTime) -> (u32, u32) {
        let dur = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Not before epoch");

        let secs: u32 = dur.as_secs().try_into().expect("Time overflowed");

        (secs, dur.subsec_nanos())
    }

    pub(crate) fn systemtime_from_epoch(secs: u32, nanos: u32) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::new(u64::from(secs), nanos)
    }
}

impl Mode {
    const EXECUTABLE: u32 = 0o10_07_55;
    const REGULAR: u32 = 0o10_06_44;

    const REGULAR_S: &'static [u8] = b"100644";
    const EXECUTABLE_S: &'static [u8] = b"100755";

    pub fn as_base8(self) -> &'static BStr {
        match self {
            Self::Regular => Self::REGULAR_S.as_bstr(),
            Self::Executable => Self::EXECUTABLE_S.as_bstr(),
        }
    }

    pub fn as_u32(self) -> u32 {
        match self {
            Self::Regular => Self::REGULAR,
            Self::Executable => Self::EXECUTABLE,
        }
    }

    pub fn from_u32(val: u32) -> Self {
        let is_executable = val & 0o111 != 0;
        if is_executable {
            Self::Executable
        } else {
            Self::Regular
        }
    }

    /// Unrecognized modes are considered [`Self::Regular`]
    pub fn from_base8(bytes: &BStr) -> Self {
        match bytes.as_bytes() {
            Self::REGULAR_S => Self::Regular,
            Self::EXECUTABLE_S => Self::Executable,
            _ => {
                warn!("Assuming unrecognized mode {} to be regular", bytes);
                Self::Regular
            }
        }
    }
}
