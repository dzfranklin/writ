use crate::{Object, Oid};
use flate2::{write::ZlibEncoder, Compression};
use tempfile::NamedTempFile;

use std::{
    fs,
    io::{self, ErrorKind, Write},
    path::PathBuf,
};

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub fn store<O: Object>(&self, object: &O) -> io::Result<Oid> {
        let o_type = O::TYPE;
        let content = &object.serialize();
        let size = content.len().to_string();

        let mut ser = Vec::with_capacity(o_type.len() + 1 + size.len() + 1 + content.len());
        ser.extend(o_type);
        ser.push(b' ');
        ser.extend(size.as_bytes());
        ser.push(b'\0');
        ser.extend(content.iter());

        let oid = Oid::from(&ser);
        self.write_object(&oid, &ser)?;
        Ok(oid)
    }

    fn write_object(&self, oid: &Oid, content: &[u8]) -> io::Result<()> {
        let oid = oid.to_hex();
        let path = self.path.join(&oid[0..2]);
        let name = &oid[2..];

        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(content)?;
        let content = enc.finish()?;

        // We use a temp file to get an atomic write
        let mut temp = NamedTempFile::new()?;
        temp.write_all(&content)?;

        let target = path.join(name);
        match fs::rename(temp.path(), &target) {
            Err(err) if err.kind() == ErrorKind::NotFound => {
                fs::create_dir(&path)?;
                fs::rename(temp.path(), &target)?;
            }
            Err(err) => return Err(err),
            Ok(()) => (),
        }

        Ok(())
    }
}
