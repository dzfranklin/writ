use bstr::{BStr, ByteSlice};
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use std::{convert::TryInto, fmt, io};
use tracing::{debug, instrument};

use crate::core::{
    db::{object::OID_SIZE, Blob},
    stat::{self, Mode},
    ws::{ReadFileError, StatFileError},
    Oid, Stat, Workspace, WsPath,
};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Entry {
    pub oid: Oid<Blob>,
    pub stat: Stat,
    pub flags: Flags,
    pub path: WsPath,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Flags {
    path_len: PathLen,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PathLen {
    Exactly(usize),
    MaxOrGreater,
}

impl Entry {
    const BLOCK_SIZE: usize = 8;
    const PATH_OFFSET: usize = 62;

    pub fn new(path: impl Into<WsPath>, oid: Oid<Blob>, stat: Stat) -> Self {
        let path = path.into();
        Self {
            oid,
            stat,
            flags: Flags::from_path(&path),
            path,
        }
    }

    pub fn key(&self) -> &BStr {
        self.path.as_ref()
    }

    pub fn filename(&self) -> &BStr {
        self.path.file_name().as_bstr()
    }

    pub fn mode(&self) -> stat::Mode {
        self.stat.mode
    }

    pub fn update_stat(&mut self, stat: Stat) -> Stat {
        let old = self.stat;
        self.stat = stat;
        old
    }

    #[instrument(err)]
    pub(crate) fn index_status_chatty(
        &self,
        workspace: &Workspace,
    ) -> Result<StatusChatty, IsUnchangedError> {
        let new_stat = match workspace.stat(&self.path) {
            Ok(stat) => stat,
            Err(err) if err.is_not_found() => {
                debug!("Determined deleted based on stat failure");
                return Ok(StatusChatty::Deleted);
            }
            Err(err) => return Err(err.into()),
        };

        if self.stat.size != new_stat.size || self.stat.mode != new_stat.mode {
            debug!(
                "Determined changed based on size or mode. other: {:?}",
                new_stat
            );
            return Ok(StatusChatty::Modified);
        }

        if self.times_match(&new_stat) {
            debug!(
                "Determined unchanged based on timestamps. other: {:?}",
                new_stat
            );
            return Ok(StatusChatty::Unmodified);
        }

        let new_data = workspace.read_file(&self.path)?;
        let new_oid = Blob::oid_for_file(new_data.as_bstr());

        if self.oid == new_oid {
            debug!("Determined unchanged based on hash of contents");
            Ok(StatusChatty::UnmodifiedButNewStat(new_stat))
        } else {
            debug!(
                "Determined changed based on hash of contents. other_oid: {:?}",
                new_oid
            );
            Ok(StatusChatty::Modified)
        }
    }

    fn times_match(&self, other: &Stat) -> bool {
        self.stat.mtime == other.mtime && self.stat.ctime == other.ctime
    }

    #[allow(clippy::similar_names)] // unixisms
    pub fn write_to_index(&self, writer: &mut impl io::Write) -> io::Result<()> {
        let (ctime_i, ctime_n) = self.stat.ctime_epoch();
        writer.write_u32::<NetworkEndian>(ctime_i)?; // offset 0
        writer.write_u32::<NetworkEndian>(ctime_n)?; // offset 4

        let (mtime_i, mtime_n) = self.stat.mtime_epoch();
        writer.write_u32::<NetworkEndian>(mtime_i)?; // offset 8
        writer.write_u32::<NetworkEndian>(mtime_n)?; // offset 12

        writer.write_u32::<NetworkEndian>(self.stat.dev)?; // offset 16
        writer.write_u32::<NetworkEndian>(self.stat.ino)?; // offset 24
        writer.write_u32::<NetworkEndian>(self.stat.mode.as_u32())?; // offset 28
        writer.write_u32::<NetworkEndian>(self.stat.uid)?; // offset 32
        writer.write_u32::<NetworkEndian>(self.stat.gid)?; // offset 36
        writer.write_u32::<NetworkEndian>(self.stat.size)?; // offset 40

        writer.write_all(self.oid.as_bytes())?; // offset 60
        writer.write_u16::<NetworkEndian>(self.flags.as_u16())?; // offset 62

        let path = self.path.as_bstr();
        writer.write_all(path)?;
        for _ in 0..Self::padding_size(path) {
            writer.write_all(b"\0")?;
        }

        Ok(())
    }

    #[allow(clippy::similar_names)] // unixisms
    pub fn parse_from_index(reader: &mut impl io::Read) -> io::Result<Self> {
        let ctime_i = reader.read_u32::<NetworkEndian>()?; // offset 0
        let ctime_n = reader.read_u32::<NetworkEndian>()?; // offset 4
        let ctime = Stat::systemtime_from_epoch(ctime_i, ctime_n);

        let mtime_i = reader.read_u32::<NetworkEndian>()?; // offset 8
        let mtime_n = reader.read_u32::<NetworkEndian>()?; // offset 12
        let mtime = Stat::systemtime_from_epoch(mtime_i, mtime_n);

        let dev = reader.read_u32::<NetworkEndian>()?; // offset 16
        let ino = reader.read_u32::<NetworkEndian>()?; // offset 24

        let mode = reader.read_u32::<NetworkEndian>()?; // offset 28
        let mode = Mode::from_u32(mode);

        let uid = reader.read_u32::<NetworkEndian>()?; // offset 32
        let gid = reader.read_u32::<NetworkEndian>()?; // offset 36
        let size = reader.read_u32::<NetworkEndian>()?; // offset 40

        let stat = Stat {
            ctime,
            mtime,
            dev,
            ino,
            mode,
            uid,
            gid,
            size,
        };

        let mut oid = [0; OID_SIZE];
        reader.read_exact(&mut oid)?; // offset 60
        let oid = Oid::new(oid);

        let flags = reader.read_u16::<NetworkEndian>()?; // offset 62
        let flags = Flags::from_u16(flags);

        let mut path = Vec::new();
        loop {
            let byte = reader.read_u8()?;
            if byte == b'\0' {
                break;
            }
            path.push(byte);
        }
        let padding_size = Self::padding_size(&path);
        // we already read one null byte
        if padding_size > 1 {
            for _ in 0..padding_size - 1 {
                reader.read_u8()?;
            }
        }
        let path = WsPath::new_unchecked_bytes(path);

        Ok(Self {
            oid,
            stat,
            flags,
            path,
        })
    }

    fn padding_size(path: &[u8]) -> usize {
        let len = Self::PATH_OFFSET + path.len();
        // See <https://stackoverflow.com/a/11642218>
        (Self::BLOCK_SIZE - (len % Self::BLOCK_SIZE)) % Self::BLOCK_SIZE
    }
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entry {} {:?}", self.path, self.oid)
    }
}

impl Flags {
    fn from_path(path: &WsPath) -> Self {
        Self {
            path_len: PathLen::from(path),
        }
    }

    fn from_u16(val: u16) -> Self {
        let val = val as usize;
        let path_len = if val <= PathLen::MAX {
            PathLen::Exactly(val)
        } else {
            PathLen::MaxOrGreater
        };
        Self { path_len }
    }

    fn as_u16(&self) -> u16 {
        match self.path_len {
            PathLen::Exactly(len) => len.try_into().expect("len < MAX"),
            #[allow(clippy::cast_possible_truncation)]
            PathLen::MaxOrGreater => PathLen::MAX as u16,
        }
    }
}

impl PathLen {
    pub const MAX: usize = 0xfff;

    fn from(path: &WsPath) -> Self {
        let path = path.as_bstr();
        if path.len() <= Self::MAX {
            Self::Exactly(path.len())
        } else {
            Self::MaxOrGreater
        }
    }
}

#[derive(Debug, displaydoc::Display, thiserror::Error)]
pub enum IsUnchangedError {
    /// Failed to stat file
    Stat(#[from] StatFileError),
    /// Failed to read file
    Read(#[from] ReadFileError),
}

pub(crate) enum StatusChatty {
    Unmodified,
    UnmodifiedButNewStat(Stat),
    Modified,
    Deleted,
}
