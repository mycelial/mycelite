use journal::Journal;

#[cfg(test)]
use tempfile;

#[test]
fn test_journal_not_exists() {
    // create named temp file and delete
    let name = &tempfile::NamedTempFile::new().unwrap();
    std::fs::remove_file(name).unwrap();
    let res = Journal::try_from(name);
    assert!(res.is_err());
    let err = res.unwrap_err();
    assert!(err.journal_not_exists());
}
