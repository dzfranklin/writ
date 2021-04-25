#![feature(assert_matches)]

use std::path::{Path, PathBuf};

use cmd_lib::run_fun;
use insta::assert_debug_snapshot;
use pretty_assertions::assert_eq;
use std::fs;
use tempfile::tempdir;

fn init_logs() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .pretty()
        .try_init();
}

type Result = anyhow::Result<()>;

fn _all_entries<D: Into<PathBuf>>(dir: D, include_dirs: bool) -> anyhow::Result<Vec<String>> {
    fn helper(dir: PathBuf, include_dirs: bool) -> anyhow::Result<Vec<PathBuf>> {
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

fn all_entries<D: Into<PathBuf>>(dir: D) -> anyhow::Result<Vec<String>> {
    _all_entries(dir, true)
}

fn all_files<D: Into<PathBuf>>(dir: D) -> anyhow::Result<Vec<String>> {
    _all_entries(dir, false)
}

fn create_nested_files(dir: &Path) -> Result {
    fs::create_dir_all(dir.join("dir_1/dir_a/dir_x"))?;
    fs::create_dir_all(dir.join("dir_1/dir_a/dir_y"))?;
    fs::create_dir_all(dir.join("dir_2/dir_a/dir_x"))?;

    fs::write(dir.join("f"), "in /")?;
    fs::write(dir.join("dir_1/f"), "in /1 #1")?;
    fs::write(dir.join("dir_1/f2"), "in /1 #2")?;
    fs::write(dir.join("dir_1/dir_a/f"), "in /1/a")?;
    fs::write(dir.join("dir_1/dir_a/dir_x/f"), "in /1/a/x")?;
    fs::write(dir.join("dir_1/dir_a/dir_y/f"), "in /1/a/y")?;
    fs::write(dir.join("dir_2/dir_a/dir_x/f"), "in /2/a/x")?;

    Ok(())
}

macro_rules! hex_assert_eq {
    ($expected:expr, $actual:expr) => {{
        if $expected != $actual {
            let expected = hexdump::hexdump_iter(&*$expected)
                .map(|l| format!("{}", l))
                .collect::<Vec<_>>();
            let actual = hexdump::hexdump_iter(&*$actual)
                .map(|l| format!("{}", l))
                .collect::<Vec<_>>();
            assert_eq!(expected, actual);
        }
    }};
}

#[test]
fn init() -> Result {
    init_logs();
    let dir = tempdir()?;
    writ::init(dir.path())?;
    assert_debug_snapshot!(all_entries(dir.path().join(".git"))?);

    Ok(())
}

const NAME: &str = "Example Name";
const EMAIL: &str = "example@example.com";

#[test]
fn basic_commit() -> Result {
    init_logs();

    let msg = "Message";

    let actual = tempdir()?;
    let actual = actual.path();
    writ::init(actual)?;
    fs::write(actual.join("file.txt"), "File contents\n")?;
    writ::add(actual, vec!["file.txt"])?;

    writ::commit(actual, NAME.to_string(), EMAIL.to_string(), msg.to_string())?;

    let expected = tempdir()?;
    let expected = expected.path();
    let expected_s = expected.to_str().unwrap();
    fs::write(expected.join("file.txt"), "File contents\n")?;
    (run_fun! {
        cd $expected_s;
        git init;
        git config user.name $NAME;
        git config user.email $EMAIL;
        git config --global gc.auto 0;
        git add file.txt;
        git commit -m $msg;
    })?;

    let actual = all_files(actual.join(".git/objects"))?;
    let expected: Vec<_> = all_files(expected.join(".git/objects"))?
        .into_iter()
        .filter(|s| !(s.contains("pack") || s.contains("info")))
        .collect();
    assert_eq!(expected, actual);

    Ok(())
}

#[test]
fn commit_with_nested_files() -> Result {
    init_logs();

    let msg = "Message";

    let actual = tempdir()?;
    let actual = actual.path();
    writ::init(actual)?;
    create_nested_files(actual)?;
    writ::add(actual, vec!["."])?;

    writ::commit(actual, NAME.to_string(), EMAIL.to_string(), msg.to_string())?;

    let expected = tempdir()?;
    let expected = expected.path();
    let expected_s = expected.to_str().unwrap();
    create_nested_files(expected)?;
    (run_fun! {
        cd $expected_s;
        git init;
        git config user.name $NAME;
        git config user.email $EMAIL;
        git config --global gc.auto 0;
        git add *;
        git commit -m $msg;
    })?;

    let actual = all_files(actual.join(".git/objects"))?;
    let expected: Vec<_> = all_files(expected.join(".git/objects"))?
        .into_iter()
        .filter(|s| !(s.contains("pack") || s.contains("info")))
        .collect();
    assert_eq!(expected, actual);

    Ok(())
}

#[test]
fn basic_add() -> Result {
    init_logs();

    let dir_handle = tempdir()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    writ::init(dir)?;
    fs::write(dir.join("random_name"), b"some contents")?;

    writ::add(dir, vec!["random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    fs::write(dir.join(".git/HEAD"), "ref: refs/heads/master")?;

    (run_fun! {
        cd $dir_s;
        rm .git/index;
        git add random_name;
    })?;

    let expected = fs::read(dir.join(".git/index"))?;

    std::mem::forget(dir_handle);

    hex_assert_eq!(expected, actual);

    Ok(())
}

#[test]
fn nested_add() -> Result {
    init_logs();

    let dir_handle = tempdir()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    writ::init(dir)?;
    create_nested_files(dir)?;

    let files = all_files(&dir)?;
    writ::add(dir, files)?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    fs::write(dir.join(".git/HEAD"), "ref: refs/heads/master")?;
    (run_fun! {
        cd $dir_s;
        rm .git/index;
    })?;
    for file in all_files(&dir)? {
        (run_fun! {
            cd $dir_s;
            git add $file;
        })?;
    }
    let expected = fs::read(dir.join(".git/index"))?;

    hex_assert_eq!(expected, actual);

    Ok(())
}

#[test]
fn duplicate_add() -> Result {
    init_logs();

    let dir_handle = tempdir()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    writ::init(dir)?;
    fs::write(dir.join("random_name"), b"some contents")?;

    writ::add(dir, vec!["random_name", "random_name"])?;
    writ::add(dir, vec!["random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    fs::write(dir.join(".git/HEAD"), "ref: refs/heads/master")?;
    (run_fun! {
        cd $dir_s;
        rm .git/index;
    })?;
    for file in all_files(&dir)? {
        (run_fun! {
            cd $dir_s;
            git add $file;
        })?;
    }
    let expected = fs::read(dir.join(".git/index"))?;

    hex_assert_eq!(expected, actual);

    Ok(())
}

#[test]
fn nonexistent_add_fails() -> Result {
    init_logs();

    let dir_handle = tempdir()?;
    let dir = dir_handle.path();

    writ::init(dir)?;
    assert!(writ::add(dir, vec!["random_name"]).is_err());

    Ok(())
}

#[test]
fn can_add_multiple_times() -> Result {
    init();

    let dir_handle = tempdir()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    writ::init(dir)?;
    fs::write(dir.join("random_name"), b"some contents")?;
    fs::write(dir.join("random_name_2"), b"some contents")?;

    writ::add(dir, vec!["random_name", "random_name"])?;
    writ::add(dir, vec!["random_name_2", "random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    fs::write(dir.join(".git/HEAD"), "ref: refs/heads/master")?;
    (run_fun! {
        cd $dir_s;
        rm .git/index;
        git add random_name;
        git add random_name_2;
    })?;
    let expected = fs::read(dir.join(".git/index"))?;

    hex_assert_eq!(expected, actual);

    Ok(())
}
