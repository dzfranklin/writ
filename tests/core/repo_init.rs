use test_support::*;

#[test]
fn can_init() -> Result {
    init();
    let (dir, _repo) = repo_fixture()?;
    assert_debug_snapshot!(all_entries(dir.path().join(".git"))?);

    Ok(())
}

#[test]
fn can_init_nonexistent_ws() -> Result {
    init();
    let dir = tempdir()?;
    let subdir = dir.path().join("subdir");

    let _repo = Repo::init(&subdir)?;
    assert_debug_snapshot!(all_entries(subdir.join(".git"))?);

    Ok(())
}
