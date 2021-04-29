#![feature(map_into_keys_values)]

mod support;
use std::{thread, time::Duration};

use cmd_lib::run_cmd;
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
            |s: &FileStatus| s.workspace == Status::Untracked && s.path == "another.txt",
            |s: &FileStatus| s.workspace == Status::Untracked && s.path == "file.txt",
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
            |s: &FileStatus| s.workspace == Status::Unmodified && s.path == "committed.txt",
            |s: &FileStatus| s.workspace == Status::Untracked && s.path == "file.txt",
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
            |s: &FileStatus| s.workspace == Status::Untracked && s.path == "dir/another.txt",
            |s: &FileStatus| s.workspace == Status::Untracked && s.path == "file.txt",
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
            |s: &FileStatus| s.workspace == Status::Unmodified,
            |s: &FileStatus| s.workspace == Status::Untracked && s.path == "a/b/c/file.txt",
            |s: &FileStatus| s.workspace == Status::Untracked && s.path == "a/outer.txt",
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

fn init_with_commit() -> eyre::Result<(TempDir, Repo)> {
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
        .filter(|s| s.workspace != Status::Unmodified || s.index != Status::Unmodified)
        .collect();
    Ok(status)
}

#[test]
fn reports_no_changed_when_no_files_are_modified() -> Result {
    let (_dir, repo) = init_with_commit()?;
    let status = not_unmodified_statuses(repo)?;
    assert_eq!(0, status.len());
    Ok(())
}

#[test]
fn reports_file_with_modified_contents() -> Result {
    let (dir, repo) = init_with_commit()?;
    let dir = dir.path();

    write_to(dir.join("1.txt"), "changed")?;
    write_to(dir.join("a/2.txt"), "modified")?;

    let status = not_unmodified_statuses(repo)?;
    assert_contains_unordered(
        status,
        [
            |s: &FileStatus| s.workspace == Status::Modified && s.path == "1.txt",
            |s: &FileStatus| s.workspace == Status::Modified && s.path == "a/2.txt",
        ],
    );

    Ok(())
}

#[test]
fn reports_mode_change_as_modified() -> Result {
    let (dir, repo) = init_with_commit()?;

    let path = dir.path().join("a/2.txt");
    let path = path.to_str().unwrap();
    run_fun!(chmod +x $path)?;

    let actual = not_unmodified_statuses(repo)?;
    assert_contains_unordered(
        actual,
        [|s: &FileStatus| s.workspace == Status::Modified && s.path == "a/2.txt"],
    );

    Ok(())
}

#[test]
fn reports_change_with_same_size_as_modified() -> Result {
    let (dir, repo) = init_with_commit()?;
    write_to(dir.path().join("a/b/3.txt"), "hello")?;

    let actual = not_unmodified_statuses(repo)?;
    assert_contains_unordered(
        actual,
        [|s: &FileStatus| s.workspace == Status::Modified && s.path == "a/b/3.txt"],
    );

    Ok(())
}

#[test]
fn reports_deleted() -> Result {
    let (dir, repo) = init_with_commit()?;
    fs::remove_file(dir.path().join("a/2.txt"))?;
    let actual = not_unmodified_statuses(repo)?;
    assert_contains_unordered(
        actual,
        [|s: &FileStatus| s.workspace == Status::Deleted && s.path == "a/2.txt"],
    );
    Ok(())
}

#[test]
fn reports_files_in_deleted_dir() -> Result {
    let (dir, repo) = init_with_commit()?;
    fs::remove_dir_all(dir.path().join("a"))?;
    let actual = not_unmodified_statuses(repo)?;
    assert_contains_unordered(
        actual,
        [
            |s: &FileStatus| s.workspace == Status::Deleted && s.path == "a/2.txt",
            |s: &FileStatus| s.workspace == Status::Deleted && s.path == "a/b/3.txt",
        ],
    );
    Ok(())
}

#[test]
fn reports_file_added_to_tracked_dir() -> Result {
    let (dir, mut repo) = init_with_commit()?;
    write_to(dir.path().join("a/4.txt"), "four")?;
    repo.add(["."].iter())?;

    assert_contains_unordered(
        not_unmodified_statuses(repo)?,
        [|s: &FileStatus| {
            s.workspace == Status::Unmodified && s.index == Status::Added && s.path == "a/4.txt"
        }],
    );

    Ok(())
}

#[test]
fn reports_file_added_to_untracked_dir() -> Result {
    let (dir, mut repo) = init_with_commit()?;
    write_to(dir.path().join("d/e/5.txt"), "five")?;
    repo.add(["."].iter())?;

    assert_contains_unordered(
        not_unmodified_statuses(repo)?,
        [|s: &FileStatus| {
            s.workspace == Status::Unmodified && s.index == Status::Added && s.path == "d/e/5.txt"
        }],
    );

    Ok(())
}

#[test]
fn reports_index_modified_mode() -> Result {
    let (dir, mut repo) = init_with_commit()?;
    let dir_s = dir.path().to_str().unwrap();
    (run_cmd! {
        cd $dir_s;
        chmod +x 1.txt;
    })?;
    repo.add(["."].iter())?;

    assert_contains_unordered(
        not_unmodified_statuses(repo)?,
        [|s: &FileStatus| {
            s.workspace == Status::Unmodified && s.index == Status::Modified && s.path == "1.txt"
        }],
    );

    Ok(())
}

#[test]
fn reports_index_modified_contents() -> Result {
    let (dir, mut repo) = init_with_commit()?;
    write_to(dir.path().join("a/b/3.txt"), "changed")?;
    repo.add(["."].iter())?;

    assert_contains_unordered(
        not_unmodified_statuses(repo)?,
        [|s: &FileStatus| {
            s.workspace == Status::Unmodified
                && s.index == Status::Modified
                && s.path == "a/b/3.txt"
        }],
    );

    Ok(())
}

#[test]
fn reports_deleted_file() -> Result {
    let (dir, mut repo) = init_with_commit()?;
    let path = dir.path();
    fs::remove_file(path.join("1.txt"))?;

    // XXX: Workaround for not supporting removing from index when this was written
    fs::remove_file(path.join(".git/index"))?;
    repo.add(["."].iter())?;

    assert_contains_unordered(
        not_unmodified_statuses(repo)?,
        [|s: &FileStatus| {
            s.workspace == Status::Deleted && s.index == Status::Deleted && s.path == "1.txt"
        }],
    );

    Ok(())
}

#[test]
fn reports_all_deleted_files_in_dir() -> Result {
    let (dir, mut repo) = init_with_commit()?;
    let path = dir.path();
    fs::remove_dir_all(path.join("a"))?;

    // XXX: Workaround for not supporting removing from index when this was written
    fs::remove_file(path.join(".git/index"))?;
    repo.add(["."].iter())?;

    assert_contains_unordered(
        not_unmodified_statuses(repo)?,
        [
            |s: &FileStatus| {
                s.workspace == Status::Deleted && s.index == Status::Deleted && s.path == "a/2.txt"
            },
            |s: &FileStatus| {
                s.workspace == Status::Deleted
                    && s.index == Status::Deleted
                    && s.path == "a/b/3.txt"
            },
        ],
    );

    Ok(())
}
