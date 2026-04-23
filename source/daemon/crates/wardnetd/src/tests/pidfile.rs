use crate::pidfile::PidfileGuard;

#[test]
fn write_creates_file_with_current_pid() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("wardnetd.pid");

    let guard = PidfileGuard::write(&path).expect("write should succeed");

    let contents = std::fs::read_to_string(&path).expect("file should exist");
    let written_pid: u32 = contents
        .trim()
        .parse()
        .expect("contents should be a valid PID");
    assert_eq!(written_pid, std::process::id());

    drop(guard);
}

#[test]
fn drop_removes_pidfile() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("wardnetd.pid");

    let guard = PidfileGuard::write(&path).expect("write should succeed");
    assert!(path.exists(), "pidfile should exist after write");

    drop(guard);
    assert!(!path.exists(), "pidfile should be removed after drop");
}

#[test]
fn write_fails_gracefully_on_unwritable_path() {
    let path = std::path::Path::new("/nonexistent-directory/wardnetd.pid");
    let result = PidfileGuard::write(path);
    assert!(
        result.is_err(),
        "should return an error for an unwritable path"
    );
}

#[test]
fn drop_is_silent_when_file_already_removed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("wardnetd.pid");

    let guard = PidfileGuard::write(&path).expect("write should succeed");
    std::fs::remove_file(&path).expect("manual remove");

    // Drop should not panic even though the file is already gone.
    drop(guard);
}
