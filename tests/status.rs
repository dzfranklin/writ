#![feature(map_into_keys_values)]

mod support;
use std::{thread, time::Duration};

use support::assert_eq;
use support::*;

use writ::{FileStatus, Status};

#[test]
fn lists_untracked() -> Result {
    init();
    let (dir, mut repo) = repo_fixture()?;
    let dir = dir.path();

    write_to(dir.join("file.txt"), b"")?;
    write_to(dir.join("another.txt"), b"")?;

    let status = repo.status()?.into_values();

    assert_contains_unordered(
        status,
        [
            |s: &FileStatus| s.status() == Status::Untracked && s.path() == "another.txt",
            |s: &FileStatus| s.status() == Status::Untracked && s.path() == "file.txt",
        ],
    );

    Ok(())
}

#[test]
fn lists_as_untracked_if_not_in_index() -> Result {
    init();
    let (dir, mut repo) = repo_fixture()?;
    let dir = dir.path();

    write_to(dir.join("committed.txt"), b"")?;
    repo.add(vec!["committed.txt"])?;
    repo.commit(NAME, EMAIL, MSG)?;

    write_to(dir.join("file.txt"), b"")?;

    let status = repo.status()?.into_values();

    assert_contains_unordered(
        status,
        [
            |s: &FileStatus| s.status() == Status::Unmodified && s.path() == "committed.txt",
            |s: &FileStatus| s.status() == Status::Untracked && s.path() == "file.txt",
        ],
    );

    Ok(())
}

#[test]
fn lists_untracked_dirs_with_contents() -> Result {
    init();
    let (dir, mut repo) = repo_fixture()?;
    let dir = dir.path();

    write_to(dir.join("file.txt"), b"")?;
    write_to(dir.join("dir/another.txt"), b"")?;

    let status = repo.status()?.into_values();

    assert_contains_unordered(
        status,
        [
            |s: &FileStatus| s.status() == Status::Untracked && s.path() == "dir/another.txt",
            |s: &FileStatus| s.status() == Status::Untracked && s.path() == "file.txt",
        ],
    );

    Ok(())
}

#[test]
fn lists_untracked_files_inside_tracked_dirs() -> Result {
    init();
    let (dir, mut repo) = repo_fixture()?;
    let dir = dir.path();

    write_to(dir.join("a/b/inner.txt"), b"")?;
    repo.add(vec!["."])?;
    repo.commit(NAME, EMAIL, MSG)?;

    write_to(dir.join("a/outer.txt"), b"")?;
    write_to(dir.join("a/b/c/file.txt"), b"")?;

    let status = repo.status()?.into_values();
    assert_contains_unordered(
        status,
        [
            |s: &FileStatus| s.status() == Status::Unmodified,
            |s: &FileStatus| s.status() == Status::Untracked && s.path() == "a/b/c/file.txt",
            |s: &FileStatus| s.status() == Status::Untracked && s.path() == "a/outer.txt",
        ],
    );

    Ok(())
}

#[test]
fn doesnt_list_empty_untracked_dir() -> Result {
    init();
    let (dir, mut repo) = repo_fixture()?;
    let dir = dir.path();
    fs::create_dir(dir.join("the_dir"))?;

    let status = repo.status()?;
    assert_eq!(0, status.len());

    Ok(())
}

fn init_with_changes() -> eyre::Result<(TempDir, Repo)> {
    init();
    let (dir_h, mut repo) = repo_fixture()?;
    let dir = dir_h.path();

    write_to(dir.join("1.txt"), "one")?;
    write_to(dir.join("a/2.txt"), "two")?;
    write_to(dir.join("a/b/3.txt"), "three")?;

    // make sure timestamp comparison detects changes
    thread::sleep(Duration::from_nanos(10));

    repo.add(vec!["."])?;
    repo.commit(NAME, EMAIL, MSG)?;

    Ok((dir_h, repo))
}

fn not_unmodified_statuses(mut repo: Repo) -> eyre::Result<Vec<FileStatus>> {
    let status = repo
        .status()?
        .into_values()
        .filter(|s| s.status() != Status::Unmodified)
        .collect();
    Ok(status)
}

#[test]
fn reports_no_changed_when_no_files_are_modified() -> Result {
    let (_dir, repo) = init_with_changes()?;
    let status = not_unmodified_statuses(repo)?;
    assert_eq!(0, status.len());
    Ok(())
}

#[test]
fn reports_file_with_modified_contents() -> Result {
    let (dir, repo) = init_with_changes()?;
    let dir = dir.path();

    write_to(dir.join("1.txt"), "changed")?;
    write_to(dir.join("a/2.txt"), "modified")?;

    let status = not_unmodified_statuses(repo)?;
    assert_contains_unordered(
        status,
        [
            |s: &FileStatus| s.status() == Status::Modified && s.path() == "1.txt",
            |s: &FileStatus| s.status() == Status::Modified && s.path() == "a/2.txt",
        ],
    );

    Ok(())
}

#[test]
fn reports_mode_change_as_modified() -> Result {
    let (dir, repo) = init_with_changes()?;

    let path = dir.path().join("a/2.txt");
    let path = path.to_str().unwrap();
    run_fun!(chmod +x $path)?;

    let actual = not_unmodified_statuses(repo)?;
    assert_contains_unordered(
        actual,
        [|s: &FileStatus| s.status() == Status::Modified && s.path() == "a/2.txt"],
    );

    Ok(())
}

#[test]
fn reports_change_with_same_size_as_modified() -> Result {
    let (dir, repo) = init_with_changes()?;
    write_to(dir.path().join("a/b/3.txt"), "hello")?;

    let actual = not_unmodified_statuses(repo)?;
    assert_contains_unordered(
        actual,
        [|s: &FileStatus| s.status() == Status::Modified && s.path() == "a/b/3.txt"],
    );

    Ok(())
}

#[test]
fn reports_deleted() -> Result {
    let (dir, repo) = init_with_changes()?;
    fs::remove_file(dir.path().join("a/2.txt"))?;
    let actual = not_unmodified_statuses(repo)?;
    assert_contains_unordered(
        actual,
        [|s: &FileStatus| s.status() == Status::Deleted && s.path() == "a/2.txt"],
    );
    Ok(())
}

#[test]
fn reports_files_in_deleted_dir() -> Result {
    let (dir, repo) = init_with_changes()?;
    fs::remove_dir_all(dir.path().join("a"))?;
    let actual = not_unmodified_statuses(repo)?;
    assert_contains_unordered(
        actual,
        [
            |s: &FileStatus| s.status() == Status::Deleted && s.path() == "a/2.txt",
            |s: &FileStatus| s.status() == Status::Deleted && s.path() == "a/b/3.txt",
        ],
    );
    Ok(())
}
