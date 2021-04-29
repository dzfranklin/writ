use test_support::assert_eq;
use test_support::*;

#[test]
fn can_basic_commit() -> Result {
    init();

    let (actual, mut repo) = repo_fixture()?;
    let actual = actual.path();

    write_to(actual.join("file.txt"), "File contents\n")?;
    repo.add(vec!["file.txt"])?;
    repo.commit(NAME.to_string(), EMAIL.to_string(), MSG.to_string())?;

    let expected = tempdir()?;
    let expected = expected.path();
    let expected_s = expected.to_str().unwrap();
    write_to(expected.join("file.txt"), "File contents\n")?;
    (run_fun! {
        cd $expected_s;
        git init;
        git config user.name $NAME;
        git config user.email $EMAIL;
        git config --global gc.auto 0;
        git add file.txt;
        git commit -m $MSG;
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
fn can_commit_with_nested_files() -> Result {
    init();

    let msg = "Message";

    let (actual, mut repo) = repo_fixture()?;
    let actual = actual.path();
    create_nested_files(actual)?;
    repo.add(vec!["."])?;

    repo.commit(NAME.to_string(), EMAIL.to_string(), msg.to_string())?;

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
