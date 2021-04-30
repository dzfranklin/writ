use test_support::assert_eq;
use test_support::*;

#[test]
fn can_basic_add() -> Result {
    init();

    let (dir_handle, mut repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    write_to(dir.join("random_name"), b"some contents")?;

    repo.add(vec!["random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    write_to(dir.join(".git/HEAD"), "ref: refs/heads/master")?;

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

#[ignore] // TODO: Figure out why this test is flaky
#[test]
fn can_add_executable() -> Result {
    init();

    let (dir_handle, mut repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    write_to(dir.join("random_name"), b"some contents")?;
    (run_fun! {
        cd $dir_s;
        chmod +x random_name;
    })?;

    repo.add(vec!["random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    write_to(dir.join(".git/HEAD"), "ref: refs/heads/master")?;

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

    let (dir_handle, mut repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    create_nested_files(dir)?;

    let files = all_files(&dir)?;
    repo.add(files)?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    write_to(dir.join(".git/HEAD"), "ref: refs/heads/master")?;
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

    let (dir_handle, mut repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    write_to(dir.join("random_name"), b"some contents")?;

    repo.add(vec!["random_name", "random_name"])?;
    repo.add(vec!["random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    write_to(dir.join(".git/HEAD"), "ref: refs/heads/master")?;
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
    let (_dir_handle, mut repo) = repo_fixture()?;

    assert_debug_snapshot!(repo.add(vec!["nonexistent"]));

    Ok(())
}

#[test]
fn unreadable_add_fails() -> Result {
    init();

    let (dir_handle, mut repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();
    write_to(dir.join("random_name"), b"some contents")?;
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

    let (dir_handle, mut repo) = repo_fixture()?;
    let dir = dir_handle.path();
    write_to(dir.join("random_name"), b"some contents")?;
    write_to(dir.join(".git/index.lock"), b"")?;

    assert_debug_snapshot!(repo.add(vec!["random_name"]));

    Ok(())
}

#[test]
fn index_not_locked_after_failed_add() -> Result {
    init();

    let (dir_handle, mut repo) = repo_fixture()?;
    let dir = dir_handle.path();
    assert!(repo.add(vec!["nonexistent"]).is_err());

    assert!(!dir.join(".git/index.lock").try_exists()?);

    Ok(())
}

#[test]
fn can_add_multiple_times() -> Result {
    init();

    let (dir_handle, mut repo) = repo_fixture()?;
    let dir = dir_handle.path();
    let dir_s = dir.to_str().unwrap();

    write_to(dir.join("random_name"), b"some contents")?;
    write_to(dir.join("random_name_2"), b"some contents")?;

    repo.add(vec!["random_name", "random_name"])?;
    repo.add(vec!["random_name_2", "random_name"])?;
    let actual = fs::read(dir.join(".git/index"))?;

    // Needed for git to accept
    write_to(dir.join(".git/HEAD"), "ref: refs/heads/master")?;
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

#[test]
fn can_add_file_where_name_is_multiple_of_padding_size() -> Result {
    init();
    let (ws, mut repo) = repo_fixture()?;
    write_to(ws.path().join(".github/workflows/main.yml"), "")?;
    repo.add(vec![".github/workflows/main.yml"])?;
    repo.status()?;
    Ok(())
}
