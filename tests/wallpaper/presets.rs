use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        // Allocated runner must explicitly provide its tree-root .tmp media directory.
        let root = PathBuf::from(
            std::env::var_os("TOHRU_TEST_ARTIFACT_DIR")
                .expect("allocated .tmp artifact directory required"),
        );
        assert!(root.is_absolute());
        let path = root.join(format!(
            "preset-test-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&path)
            .unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn cache_has_fixed_dimensions_repeatability_and_one_temporary_name() {
    let base = Fixture::new();
    let first = generate_in(&base.0, Preset::Dusk, Palette::Mocha).unwrap();
    assert!(!first.cached);
    assert_eq!(image::image_dimensions(&first.path).unwrap(), (640, 400));
    let bytes = fs::read(&first.path).unwrap();
    assert!(bytes.len() <= MAX_PNG);
    let second = generate_in(&base.0, Preset::Dusk, Palette::Mocha).unwrap();
    assert!(second.cached);
    assert_eq!(first.path, second.path);
    assert_eq!(fs::read(second.path).unwrap(), bytes);
    assert_eq!(fs::read_dir(base.0.join(DIRECTORY)).unwrap().count(), 1);
}
#[test]
fn symlink_cache_and_existing_different_content_are_never_overwritten() {
    let base = Fixture::new();
    let outside = Fixture::new();
    std::os::unix::fs::symlink(&outside.0, base.0.join(DIRECTORY)).unwrap();
    assert!(generate_in(&base.0, Preset::Solid, Palette::Nord).is_err());
    assert_eq!(fs::read_dir(&outside.0).unwrap().count(), 0);
    fs::remove_file(base.0.join(DIRECTORY)).unwrap();
    let (dir, path, _) = directory(&base.0).unwrap();
    let mut file = at(
        &dir,
        c"solid-nord.png",
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )
    .unwrap();
    file.write_all(b"user content").unwrap();
    drop(file);
    drop(dir);
    assert!(generate_in(&base.0, Preset::Solid, Palette::Nord).is_err());
    assert_eq!(
        fs::read(path.join("solid-nord.png")).unwrap(),
        b"user content"
    );
}
#[test]
fn concurrent_owner_is_rejected_and_owned_stale_temporary_is_recovered() {
    let base = Fixture::new();
    let (dir, path, _) = directory(&base.0).unwrap();
    assert!(matches!(
        generate_in(&base.0, Preset::Moss, Palette::Mocha),
        Err(Error::Busy)
    ));
    let mut stale = at(
        &dir,
        PENDING,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )
    .unwrap();
    stale.write_all(b"partial").unwrap();
    drop(stale);
    drop(dir);
    assert!(generate_in(&base.0, Preset::Moss, Palette::Mocha).is_ok());
    assert!(!path.join(".pending-v1.png").exists());
}
#[test]
fn bounded_encoder_writer_refuses_overflow() {
    let mut output = Bounded(Vec::new());
    assert!(output.write_all(&vec![0; MAX_PNG + 1]).is_err());
    assert!(output.0.is_empty());
}

#[test]
fn newly_created_base_never_reports_complete_durability_from_leaf_syncs() {
    // Pure result accounting: no filesystem or PNG is needed for this regression.
    for cached in [false, true] {
        for (synced, preexisting, expected) in [
            (true, false, false),
            (true, true, true),
            (false, true, false),
            (false, false, false),
        ] {
            let result = Generated {
                path: PathBuf::from("/unexecuted/preset.png"),
                durable: synced,
                cached,
            }
            .with_existing_base(preexisting);
            assert_eq!(result.durable, expected);
            assert_eq!(result.cached, cached);
            assert_eq!(result.path, PathBuf::from("/unexecuted/preset.png"));
        }
    }
}
#[test]
fn base_observation_is_taken_before_recursive_creation() {
    let fixture = Fixture::new();
    let missing = fixture.0.join("new-ancestor").join("new-cache-base");
    let (path, preexisting) = prepare_parent(missing.clone()).unwrap();
    assert_eq!(path, missing);
    assert!(!preexisting);
    assert!(prepare_parent(path).unwrap().1);
}
