use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};
use tracing::{error, warn};

#[derive(Debug, thiserror::Error, displaydoc::Display)]
pub enum Error {
    /// Error performing IO
    Io(#[from] io::Error),
    /// Failed to acquire lock
    Contested,
    /// File does not exist
    NotFound,
}

#[derive(Debug)]
pub struct LockedFile {
    path: PathBuf,
    lock_path: PathBuf,
    /// Always exists until cancelled/committed
    lock: Option<fs::File>,
    /// May not exist
    protected: Option<fs::File>,
}

impl LockedFile {
    pub fn acquire<P: Into<PathBuf>>(path: P) -> Result<Self, Error> {
        let path = path.into();
        let lock_path = path.with_extension("lock");

        let lock = match fs::File::with_options()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(lock) => lock,
            Err(err) => {
                return match err.kind() {
                    io::ErrorKind::AlreadyExists => Err(Error::Contested),
                    io::ErrorKind::NotFound => Err(Error::NotFound),
                    _ => Err(Error::Io(err)),
                }
            }
        };

        let protected = match fs::File::open(&path) {
            Ok(protected) => Ok(Some(protected)),
            Err(err) => match err.kind() {
                io::ErrorKind::AlreadyExists => Err(Error::Contested),
                io::ErrorKind::NotFound => Ok(None),
                _ => Err(Error::Io(err)),
            },
        }?;

        Ok(Self {
            path,
            lock_path,
            lock: Some(lock),
            protected,
        })
    }

    /// The file protected by the lock, not the lock file. Will not see updates
    /// until you commit the lock.
    pub fn protected_file(&self) -> Option<&fs::File> {
        self.protected.as_ref()
    }

    pub fn commit(mut self) -> io::Result<()> {
        let mut lock = self.lock.take().unwrap();
        lock.flush()?;
        drop(lock);

        if let Some(protected) = self.protected.take() {
            drop(protected);
        }

        fs::rename(&self.lock_path, &self.path)?;

        Ok(())
    }

    pub fn cancel(mut self) -> io::Result<()> {
        self._cancel()
    }

    fn _cancel(&mut self) -> io::Result<()> {
        let lock = self.lock.take().unwrap();
        drop(lock);
        fs::remove_file(&self.lock_path)
    }

    fn expect_lock(&mut self) -> &mut fs::File {
        self.lock.as_mut().unwrap()
    }
}

impl Write for LockedFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.expect_lock().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.expect_lock().flush()
    }
}

impl Drop for LockedFile {
    fn drop(&mut self) {
        if self.lock.is_some() {
            warn!(path=?self.path, "LockedFile never committed, cancelling");
            if let Err(err) = self._cancel() {
                error!("Failed to remove lock: {:?}", err);
            }
        }
    }
}
