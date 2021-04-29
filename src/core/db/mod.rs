pub mod author;
pub mod blob;
pub mod cache;
pub mod commit;
pub mod object;
pub mod tree;

pub use author::Author;
pub use blob::Blob;
pub use commit::Commit;
pub use object::{Object, ObjectBuilder, Oid, UntypedOid};
pub use tree::Tree;

use bstr::{BString, ByteSlice};
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use tempfile::NamedTempFile;

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, ErrorKind, Read, Write},
    num::ParseIntError,
    path::PathBuf,
};

use self::cache::Cache;
use crate::core::WsPath;

/// Note: Cloning doesn't keep the cache
#[derive(Debug)]
pub struct Db {
    path: PathBuf,
    cache: Cache,
}

impl Db {
    pub fn new<P: Into<PathBuf>>(git_dir: P) -> Self {
        Self {
            path: git_dir.into().join("objects"),
            cache: Cache::new(),
        }
    }

    pub fn load<O: Object>(&mut self, oid: Oid<O>) -> Result<O, LoadError<O>> {
        if let Some(cached) = self.cache.get(&oid) {
            return Ok(cached.clone());
        }

        let (len, bytes) = self.load_bytes(O::TYPE, &oid)?;
        let object = O::deserialize(oid, len, bytes).map_err(|e| LoadError::Deserialize(oid, e))?;
        self.cache.insert(oid, object.clone());

        Ok(object)
    }

    pub fn load_tree_file(
        &mut self,
        mut tree: Oid<Tree>,
        path: &WsPath,
    ) -> Result<Option<tree::FileNode>, LoadError<Tree>> {
        for parent in path.parents() {
            if let Some(tree::Node::Tree { oid: next_tree, .. }) =
                self.load(tree)?.direct_child(parent.file_name())
            {
                tree = *next_tree;
            } else {
                return Ok(None);
            }
        }

        if let Some(tree::Node::File(file)) = self.load(tree)?.direct_child(path.file_name()) {
            Ok(Some(file.clone()))
        } else {
            Ok(None)
        }
    }

    pub fn load_tree_files(
        &mut self,
        root: &WsPath,
        tree: Oid<Tree>,
    ) -> Result<BTreeMap<WsPath, tree::FileNode>, LoadError<Tree>> {
        let mut files = BTreeMap::new();
        self.load_all_tree_files_into(&mut files, root, tree)?;
        Ok(files)
    }

    fn load_all_tree_files_into(
        &mut self,
        files: &mut BTreeMap<WsPath, tree::FileNode>,
        root: &WsPath,
        tree: Oid<Tree>,
    ) -> Result<(), LoadError<Tree>> {
        let tree = self.load(tree)?;
        for child in tree.direct_children() {
            match child {
                tree::Node::File(file) => {
                    let path = root.join_bytes(child.name());
                    files.insert(path, file.clone());
                }
                tree::Node::Tree { oid, .. } => {
                    let next_root = root.join_bytes(child.name());
                    self.load_all_tree_files_into(files, &next_root, *oid)?;
                }
            }
        }
        Ok(())
    }

    /// Doesn't cache
    fn load_bytes<O: Object>(
        &self,
        expected_type: &[u8],
        oid: &Oid<O>,
    ) -> Result<(usize, impl BufRead), LoadBytesError<O>> {
        let path = self.oid_path(&oid);
        let file = match File::open(&path) {
            Ok(file) => Ok(file),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                Err(LoadBytesError::NotFound(*oid))
            }
            Err(err) => Err(LoadBytesError::Open(*oid, err)),
        }?;

        let mut bytes = BufReader::new(ZlibDecoder::new(file));

        let mut o_type = vec![0; expected_type.len() + 1];
        bytes
            .read_exact(&mut o_type)
            .map_err(|e| LoadBytesError::ReadPrefix(*oid, e))?;
        let sep = o_type.pop().unwrap();
        if sep != b' ' {
            return Err(LoadBytesError::Corrupt(*oid));
        }
        if o_type != expected_type {
            return Err(LoadBytesError::WrongType {
                oid: *oid,
                expected: expected_type.into(),
                actual: o_type.into(),
            });
        }

        let mut len = BString::from(Vec::new());
        bytes
            .read_until(b'\0', &mut len)
            .map_err(|e| LoadBytesError::ReadPrefix(*oid, e))?;
        len.pop().unwrap();
        let len = len
            .to_str()
            .map_err(|e| LoadBytesError::ParseLenToBytes(*oid, e))?;
        let len: usize = len
            .parse()
            .map_err(|e| LoadBytesError::ParseLenToInt(*oid, e))?;

        Ok((len, bytes))
    }

    /// Doesn't cache
    pub fn store_bytes<OB: ObjectBuilder>(&self, content: &[u8]) -> StoreResult<OB::Object> {
        fn helper<O: Object>(db: &Db, oid: &Oid<O>, bytes: &[u8]) -> io::Result<()> {
            let path = db.oid_path(oid);

            if path.exists() {
                return Ok(());
            }

            let mut temp = NamedTempFile::new()?;

            {
                let mut writer = BufWriter::new(&mut temp);
                let mut writer = ZlibEncoder::new(&mut writer, Compression::default());
                writer.write_all(bytes)?;
            }

            // We use a temp file to get an atomic write
            temp.flush()?;

            match fs::rename(temp.path(), &path) {
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    fs::create_dir(&path.parent().expect("has parent"))?;
                    fs::rename(temp.path(), &path)?;
                }
                Err(err) => return Err(err),
                Ok(()) => (),
            }

            Ok(())
        }

        let o_type = OB::Object::TYPE;

        let mut bytes = Self::serialized_prefix(o_type, content);
        bytes.extend_from_slice(content);
        let oid = Oid::for_serialized_bytes(&bytes);

        helper::<OB::Object>(self, &oid, &bytes).map_err(|e| StoreError(oid, e))?;

        Ok(oid)
    }

    fn serialized_prefix(o_type: &[u8], serialized: &[u8]) -> Vec<u8> {
        let size = serialized.len().to_string();

        let mut ser = Vec::with_capacity(o_type.len() + 1 + size.len() + 1);

        ser.extend(o_type);
        ser.push(b' ');
        ser.extend(size.as_bytes());
        ser.push(b'\0');

        ser
    }

    fn oid_path<O: Object>(&self, oid: &Oid<O>) -> PathBuf {
        let oid = oid.to_hex();
        let dir = self.path.join(&oid[0..2]);
        let name = &oid[2..];
        dir.join(name)
    }
}

impl Clone for Db {
    fn clone(&self) -> Self {
        Self::new(&self.path)
    }
}

pub type StoreResult<O> = Result<Oid<O>, StoreError<O>>;

#[derive(Debug, thiserror::Error, displaydoc::Display)]
/// Failed to store {0:?}
pub struct StoreError<O: Object>(Oid<O>, #[source] io::Error);

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum LoadError<O: Object + 'static> {
    /// Failed to load bytes of object {0:?}
    LoadBytes(#[from] LoadBytesError<O>),
    /// Failed to deserialize {0:?}
    Deserialize(Oid<O>, #[source] O::DeserializeError),
}

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum LoadBytesError<O: Object + 'static> {
    /// {0:?} not found in database
    NotFound(Oid<O>),
    /// Failed to open the file for {0:?} in the database
    Open(Oid<O>, #[source] io::Error),
    /// Failed to read the prefix from the file for {0:?} in the database
    ReadPrefix(Oid<O>, #[source] io::Error),
    /// Database entry for {0:?} is corrupt
    Corrupt(Oid<O>),
    /// Expected oid {oid:?} to have type {expected}, got {actual}
    WrongType {
        oid: Oid<O>,
        expected: BString,
        actual: BString,
    },
    /// Failed to parse bytes of length of {0:?} as utf-8
    ParseLenToBytes(Oid<O>, #[source] bstr::Utf8Error),
    /// Failed to parse length of {0:?}
    ParseLenToInt(Oid<O>, #[source] ParseIntError),
}
