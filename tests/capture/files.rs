use super::*;

fn folder() -> PathBuf {
    let root = std::env::current_dir().unwrap().join(".tmp");
    fs::create_dir_all(&root).unwrap();
    let path = root.join(format!(
        "capture-save-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&path).unwrap();
    path
}

// The file publisher accepts already-encoded PNG; encoding is native's responsibility.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nencoded-fixture";

#[test]
fn publishes_complete_private_png_and_never_overwrites() {
    let dir = folder();
    let path = dir.join("new/capture.png");
    assert!(matches!(
        save_png(&path, PNG).unwrap(),
        SaveOutcome::Saved(_)
    ));
    assert_eq!(fs::read(&path).unwrap(), PNG);
    assert!(save_png(&path, PNG).is_err());
    assert_eq!(fs::read(&path).unwrap(), PNG);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn existing_symlink_is_not_followed_or_replaced() {
    use std::os::unix::fs::symlink;
    let dir = folder();
    let target = dir.join("target");
    fs::write(&target, b"unchanged").unwrap();
    let path = dir.join("link.png");
    symlink(&target, &path).unwrap();
    assert!(save_png(&path, PNG).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"unchanged");
    assert!(
        fs::symlink_metadata(&path)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_path_and_encoding_fail_without_creating_destination() {
    let dir = folder();
    let path = dir.join("invalid.png");
    assert!(save_png(&path, b"bad").is_err());
    assert!(!path.exists());
    assert!(save_png(Path::new("relative.png"), PNG).is_err());
    assert!(save_png(&dir.join("../traversal.png"), PNG).is_err());
    assert!(save_png(&dir.join("wrong.txt"), PNG).is_err());
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    fs::remove_dir_all(dir).unwrap();
}
