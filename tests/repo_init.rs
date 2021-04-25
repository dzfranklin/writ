mod support;
use support::assert_eq;
use support::*;

#[test]
fn can_init() -> Result {
    init();
    let (dir, _repo) = repo_fixture()?;
    assert_debug_snapshot!(all_entries(dir.path().join(".git"))?);

    Ok(())
}
