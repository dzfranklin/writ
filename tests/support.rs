#![allow(dead_code)] // tests don't count
use std::{
    path::{Path, PathBuf},
    sync::Once,
};

pub use cmd_lib::run_fun;
pub use insta::assert_debug_snapshot;
pub use pretty_assertions::assert_eq;
pub use std::fs;
pub use std::os::unix::prelude::MetadataExt;
pub use tempfile::{tempdir, TempDir};
pub use writ::Repo;

static INIT: Once = Once::new();

pub fn init() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .pretty()
            .init();

        color_eyre::install().unwrap();
    });
}

pub type Result = eyre::Result<()>;

fn _all_entries<D: Into<PathBuf>>(dir: D, include_dirs: bool) -> eyre::Result<Vec<String>> {
    fn helper(dir: PathBuf, include_dirs: bool) -> eyre::Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for entry in dir.read_dir()? {
            let entry = entry?;
            let path = entry.path();
            if entry.metadata()?.is_dir() {
                if include_dirs {
                    files.push(path.to_path_buf());
                }
                files.extend(helper(path.to_path_buf(), include_dirs)?);
            } else {
                files.push(path.to_path_buf());
            }
        }

        Ok(files)
    }

    let base = dir.into();
    let base_s = base.as_os_str().to_str().unwrap();

    let files = helper(base.clone(), include_dirs)?
        .into_iter()
        .map(|d| {
            d.strip_prefix(base_s)
                .unwrap()
                .as_os_str()
                .to_str()
                .unwrap()
                .to_string()
        })
        .collect();

    Ok(files)
}

pub fn all_entries<D: Into<PathBuf>>(dir: D) -> eyre::Result<Vec<String>> {
    _all_entries(dir, true)
}

pub fn all_files<D: Into<PathBuf>>(dir: D) -> eyre::Result<Vec<String>> {
    _all_entries(dir, false)
}

pub fn create_nested_files(dir: &Path) -> Result {
    fs::create_dir_all(dir.join("dir_1/dir_a/dir_x"))?;
    fs::create_dir_all(dir.join("dir_1/dir_a/dir_y"))?;
    fs::create_dir_all(dir.join("dir_2/dir_a/dir_x"))?;

    write_to(dir.join("f"), "in /")?;
    write_to(dir.join("dir_1/f"), "in /1 #1")?;
    write_to(dir.join("dir_1/f2"), "in /1 #2")?;
    write_to(dir.join("dir_1/dir_a/f"), "in /1/a")?;
    write_to(dir.join("dir_1/dir_a/dir_x/f"), "in /1/a/x")?;
    write_to(dir.join("dir_1/dir_a/dir_y/f"), "in /1/a/y")?;
    write_to(dir.join("dir_2/dir_a/dir_x/f"), "in /2/a/x")?;

    Ok(())
}

#[macro_export]
macro_rules! hex_assert_eq {
    ($expected:expr, $actual:expr) => {{
        let expected = $expected;
        let actual = $actual;

        if expected != actual {
            let expected_dump = hexdump::hexdump_iter(expected.as_ref())
                .map(|l| format!("{}", l))
                .collect::<Vec<_>>();
            let actual_dump = hexdump::hexdump_iter(actual.as_ref())
                .map(|l| format!("{}", l))
                .collect::<Vec<_>>();

            fs::write("test_expected.bin", expected)
                .expect("Test failed, error trying to write binary output to fs for debugging");
            fs::write("test_actual.bin", actual)
                .expect("Test failed, error trying to write binary output to fs for debugging");

            eprintln!("Wrote test_expected.bin and test_actual.bin to workspace");

            pretty_assertions::assert_eq!(expected_dump, actual_dump);
        }
    }};
}

pub fn repo_fixture() -> eyre::Result<(TempDir, Repo)> {
    let dir = tempdir()?;
    let repo = Repo::init(dir.path())?;
    Ok((dir, repo))
}

pub fn write_to(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> Result {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, data)?;
    Ok(())
}

/// Assert that each item in the list is matched by at least one predicate. Predicates cannot be re-used.
pub fn assert_contains_unordered<Item, List, Preds, P>(list: List, preds: Preds)
where
    Item: Clone + std::fmt::Debug,
    List: IntoIterator<Item = Item> + std::fmt::Debug,
    Preds: Into<Vec<P>>,
    P: Fn(&Item) -> bool,
{
    let mut preds = preds.into();
    let list_debug = format!("{:#?}", list);

    'list: for item in list {
        for (i, p) in preds.iter().enumerate() {
            if p(&item) {
                preds.remove(i);
                continue 'list;
            }
        }
        panic!(
            "No predicate matches item {:?}. Got list {}",
            item, list_debug
        );
    }

    if !preds.is_empty() {
        panic!(
            "{} predicates unmatched. Got list {}",
            preds.len(),
            list_debug
        );
    }
}

pub const NAME: &str = "Example Name";
pub const EMAIL: &str = "example@example.com";
pub const MSG: &str = "Example commit message\n\nSome details.\n";
