use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Folder(PathBuf);

impl Folder {
    fn new() -> Self {
        let root = std::env::current_dir().unwrap().join(".tmp");
        fs::create_dir_all(&root).unwrap();
        let path = root.join(format!(
            "wallpaper-catalog-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn catalog_is_canonical_sorted_nonrecursive_and_read_only() {
    let dir = Folder::new();
    fs::write(dir.0.join("z.WEBM"), b"video fixture").unwrap();
    fs::write(dir.0.join("a.png"), b"image fixture").unwrap();
    fs::write(dir.0.join("ignore.txt"), b"private unrelated file").unwrap();
    fs::create_dir(dir.0.join("nested")).unwrap();
    fs::write(dir.0.join("nested/skip.jpg"), b"nested").unwrap();
    let result = catalog(&dir.0).unwrap();
    assert_eq!(
        result,
        vec![
            dir.0.join("a.png").to_str().unwrap(),
            dir.0.join("z.WEBM").to_str().unwrap()
        ]
    );
    assert_eq!(fs::read(dir.0.join("a.png")).unwrap(), b"image fixture");
    assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 4);
}

#[test]
fn malformed_missing_empty_and_unsupported_inputs_are_honest() {
    let dir = Folder::new();
    assert!(matches!(
        catalog(Path::new("relative")),
        Err(Error::InvalidFolder)
    ));
    assert!(matches!(catalog(&dir.0.join("missing")), Err(Error::Io(_))));
    assert!(catalog(&dir.0).unwrap().is_empty());
    assert_eq!(kind(Path::new("a.GIF")), Some(Kind::Video));
    assert_eq!(kind(Path::new("a.jpeg")), Some(Kind::Image));
    assert_eq!(kind(Path::new("a.avi")), None);
}

#[test]
fn rejects_entry_overflow_instead_of_silently_truncating() {
    let dir = Folder::new();
    for i in 0..=MAX_ENTRIES {
        fs::write(dir.0.join(format!("{i}.txt")), b"").unwrap();
    }
    assert!(matches!(catalog(&dir.0), Err(Error::TooLarge)));
}

#[cfg(unix)]
#[test]
fn ignores_symlinks_even_when_they_point_at_media() {
    use std::os::unix::fs::symlink;
    let dir = Folder::new();
    let outside = Folder::new();
    fs::write(outside.0.join("secret.png"), b"never read").unwrap();
    symlink(outside.0.join("secret.png"), dir.0.join("alias.png")).unwrap();
    assert!(catalog(&dir.0).unwrap().is_empty());
    symlink(&outside.0, dir.0.join("chosen-folder")).unwrap();
    assert_eq!(
        catalog(&dir.0.join("chosen-folder")).unwrap(),
        vec![outside.0.join("secret.png").to_str().unwrap()]
    );
}

#[cfg(unix)]
#[test]
fn rejects_non_utf8_media_names_without_lossy_aliasing() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let dir = Folder::new();
    fs::write(dir.0.join(OsString::from_vec(b"bad\xff.png".to_vec())), b"").unwrap();
    assert!(matches!(catalog(&dir.0), Err(Error::InvalidEntry)));
}

#[test]
fn bounds_json_size_before_serializing_long_paths() {
    let dir = Folder::new();
    for i in 0..700 {
        fs::write(dir.0.join(format!("{i:04}{}.png", "x".repeat(190))), b"").unwrap();
    }
    assert!(matches!(catalog(&dir.0), Err(Error::TooLarge)));
}

#[test]
fn chosen_media_identities_are_regular_canonical_and_bounded() {
    let dir = Folder::new();
    let path = dir.0.join("chosen.PNG");
    fs::write(&path, b"metadata only").unwrap();
    assert_eq!(
        identities(std::slice::from_ref(&path)).unwrap(),
        vec![path.to_str().unwrap()]
    );
    assert!(identities(&vec![path.clone(); 33]).is_err());
    assert!(identities(&[dir.0.join("missing.png")]).is_err());
    assert!(identities(&[dir.0.clone()]).is_err());
    assert!(identities(&[PathBuf::from("relative.png")]).is_err());
    assert!(identities(&[dir.0.join("unrelated.txt")]).is_err());
    assert!(identities(&[]).unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn assigned_alias_identity_can_point_to_a_regular_target_without_media_extension() {
    use std::os::unix::fs::symlink;
    let dir = Folder::new();
    let target = dir.0.join("original-image");
    let alias = dir.0.join("alias.png");
    fs::write(&target, b"metadata only").unwrap();
    symlink(&target, &alias).unwrap();
    assert_eq!(
        identities(&[alias]).unwrap(),
        vec![target.to_str().unwrap()]
    );
    let directory_alias = dir.0.join("directory.png");
    symlink(&dir.0, &directory_alias).unwrap();
    assert!(identities(&[directory_alias]).is_err());
}
