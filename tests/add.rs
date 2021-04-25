#![feature(path_try_exists)]

mod support;
use support::assert_eq;
use support::*;

#[test]
fn can_basic_add() -> Result {
    init();

    let (dir_handle, repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    write_normalized(dir.join("random_name"), b"some contents")?;

    repo.add(vec!["random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    write_normalized(dir.join(".git/HEAD"), "ref: refs/heads/master")?;

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
fn can_add_executable() -> Result {
    init();

    let (dir_handle, repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    write_normalized(dir.join("random_name"), b"some contents")?;
    (run_fun! {
        cd $dir_s;
        chmod +x random_name;
    })?;

    repo.add(vec!["random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    write_normalized(dir.join(".git/HEAD"), "ref: refs/heads/master")?;

    let actual_debug = (run_fun! {
        cd $dir_s;
        git ls-files -s --debug;
    })?;

    (run_fun! {
        cd $dir_s;
        rm .git/index;
        git add random_name;
    })?;

    let expected = fs::read(dir.join(".git/index"))?;

    let expected_debug = (run_fun! {
        cd $dir_s;
        git ls-files -s --debug;
    })?;

    assert_eq!(expected_debug, actual_debug);

    hex_assert_eq!(expected, actual);

    Ok(())
}

#[test]
fn can_nested_add() -> Result {
    init();

    let (dir_handle, repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    create_nested_files(dir)?;

    let files = all_files(&dir)?;
    repo.add(files)?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    write_normalized(dir.join(".git/HEAD"), "ref: refs/heads/master")?;
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
fn can_duplicate_add() -> Result {
    init();

    let (dir_handle, repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    write_normalized(dir.join("random_name"), b"some contents")?;

    repo.add(vec!["random_name", "random_name"])?;
    repo.add(vec!["random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    write_normalized(dir.join(".git/HEAD"), "ref: refs/heads/master")?;
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
    init();
    let (_dir, repo) = repo_fixture()?;

    assert_debug_snapshot!(repo.add(vec!["random_name"]));

    Ok(())
}

#[test]
fn unreadable_add_fails() -> Result {
    init();

    let (dir_handle, repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();
    write_normalized(dir.join("random_name"), b"some contents")?;
    (run_fun! {
        cd $dir_s;
        chmod -r random_name;
    })?;

    assert_debug_snapshot!(repo.add(vec!["random_name"]));

    Ok(())
}

#[test]
fn add_fails_if_index_locked() -> Result {
    init();

    let (dir_handle, repo) = repo_fixture()?;
    let dir = dir_handle.path();
    write_normalized(dir.join("random_name"), b"some contents")?;
    write_normalized(dir.join(".git/index.lock"), b"")?;

    assert_debug_snapshot!(repo.add(vec!["random_name"]));

    Ok(())
}

#[test]
fn index_not_locked_after_failed_add() -> Result {
    init();

    let (dir_handle, repo) = repo_fixture()?;
    let dir = dir_handle.path();
    assert!(repo.add(vec!["nonexistent"]).is_err());

    assert!(!dir.join(".git/index.lock").try_exists()?);

    Ok(())
}

#[test]
fn can_add_multiple_times() -> Result {
    init();

    let (dir_handle, repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    write_normalized(dir.join("random_name"), b"some contents")?;
    write_normalized(dir.join("random_name_2"), b"some contents")?;

    repo.add(vec!["random_name", "random_name"])?;
    repo.add(vec!["random_name_2", "random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    write_normalized(dir.join(".git/HEAD"), "ref: refs/heads/master")?;
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
