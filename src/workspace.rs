use bstr::BString;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Workspace {
    path: PathBuf,
}

impl Workspace {
    const IGNORE: &'static [&'static str] = &[".git"];

    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        Self { path: path.into() }
    }

    pub fn list_files(&self) -> io::Result<Vec<PathBuf>> {
        fs::read_dir(&self.path)?
            .filter_map(|entry| {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => return Some(Err(err)),
                };

                let full_path = entry.path();
                let file = full_path.strip_prefix(&self.path).expect("Has prefix");

                for ignored in Self::IGNORE {
                    if file.ends_with(ignored) {
                        return None;
                    }
                }

                if file.is_dir() {
                    unimplemented!("directories");
                }

                Some(Ok(file.to_path_buf()))
            })
            .collect()
    }

    pub fn read_file<P: AsRef<Path>>(&self, path: P) -> io::Result<BString> {
        let bytes = fs::read(self.path.join(path))?;
        Ok(bytes.into())
    }
}
