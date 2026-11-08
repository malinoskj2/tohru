use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tohru_shell::settings::{Key, Origin, Owner, Value, WallpaperFit};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tohru-settings-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn user(&self) -> PathBuf {
        self.0.join("config.toml")
    }

    fn managed(&self) -> PathBuf {
        self.0.join("managed.toml")
    }

    fn owner(&self) -> Owner {
        Owner::load(self.user(), Some(self.managed())).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn idle_preferences_use_native_protocol_bounds_and_retained_layer_semantics() {
    let files = Fixture::new();
    fs::write(
        files.managed(),
        "[idle]\nlock_seconds=1800\nfade_milliseconds=2000\n",
    )
    .unwrap();
    let mut owner = files.owner();
    for (key, default, maximum) in [
        (Key::IdleLockSeconds, 1800, 4_294_967),
        (Key::IdleScreenOffUnlockedSeconds, 0, 4_294_967),
        (Key::IdleScreenOffLockedSeconds, 60, 4_294_967),
        (Key::IdleSuspendUnlockedSeconds, 7200, 4_294_967),
        (Key::IdleSuspendLockedSeconds, 5400, 4_294_967),
        (Key::IdleFadeMilliseconds, 2000, 120_000),
    ] {
        assert_eq!(owner.get(key).value, Value::Integer(default));
        for invalid in [
            Value::Integer(-1),
            Value::Integer(maximum + 1),
            Value::Bool(false),
        ] {
            let before = owner.get(key);
            assert!(owner.set(key, invalid).is_err());
            assert_eq!(owner.get(key), before);
        }
        owner.set(key, Value::Integer(maximum)).unwrap();
    }
    owner.reload().unwrap();
    assert_eq!(
        owner.get(Key::IdleFadeMilliseconds).value,
        Value::Integer(120_000)
    );
    assert!(owner.get(Key::IdleLockSeconds).runtime_override);
    let restarted = files.owner();
    assert_eq!(
        restarted.get(Key::IdleLockSeconds).value,
        Value::Integer(1800)
    );
    assert_eq!(
        restarted.get(Key::IdleFadeMilliseconds).value,
        Value::Integer(2000)
    );
    assert_eq!(
        restarted.get(Key::IdleSuspendLockedSeconds).value,
        Value::Integer(4_294_967)
    );
    fs::write(
        files.user(),
        "[idle]\nlock_seconds=0\nfade_milliseconds=120001\n",
    )
    .unwrap();
    assert!(owner.reload().is_err());
    assert_eq!(
        owner.get(Key::IdleLockSeconds).value,
        Value::Integer(4_294_967)
    );
    fs::write(files.user(), "[idle]\nfade_milliseconds=2000\n").unwrap();
    // Zero is the explicit disable value, independent for every state/action.
    owner.set(Key::IdleLockSeconds, Value::Integer(0)).unwrap();
    assert_eq!(owner.get(Key::IdleLockSeconds).value, Value::Integer(0));
}

#[test]
fn media_preferences_validate_atomically_and_survive_restart() {
    let files = Fixture::new();
    fs::write(
        files.user(),
        "# preserved\n[capture]\nfuture = 'retained'\n",
    )
    .unwrap();
    let mut owner = Owner::load(files.user(), None).unwrap();
    let edits = [
        (Key::WallpaperFolder, Value::Text("/offline/壁 紙".into())),
        (Key::CaptureSave, Value::Bool(false)),
        (Key::CaptureCopy, Value::Bool(false)),
        (
            Key::CaptureDirectory,
            Value::Text("$HOME/Pictures/region".into()),
        ),
        (
            Key::CaptureFilenamePattern,
            Value::Text("region_%Y%m%d".into()),
        ),
    ];
    for (key, candidate) in &edits {
        owner.set(*key, candidate.clone()).unwrap();
    }
    let accepted = fs::read(files.user()).unwrap();
    for (key, invalid) in [
        (Key::WallpaperFolder, Value::Text("a".repeat(4097))),
        (Key::WallpaperFolder, Value::Text("/bad\0folder".into())),
        (Key::CaptureSave, Value::Text("true".into())),
        (Key::CaptureCopy, Value::Integer(1)),
        (Key::CaptureDirectory, Value::Text("relative".into())),
        (Key::CaptureDirectory, Value::Text("~/../escape".into())),
        (Key::CaptureDirectory, Value::Text("$HOME/../escape".into())),
        (
            Key::CaptureDirectory,
            Value::Text("/absolute/../escape".into()),
        ),
        (Key::CaptureDirectory, Value::Text("/bad\0folder".into())),
        (
            Key::CaptureDirectory,
            Value::Text(format!("/{}", "a".repeat(4096))),
        ),
        (Key::CaptureFilenamePattern, Value::Text("a".repeat(1025))),
        (Key::CaptureFilenamePattern, Value::Text("../escape".into())),
        (Key::CaptureFilenamePattern, Value::Text("bad\\name".into())),
        (Key::CaptureFilenamePattern, Value::Text("bad\0name".into())),
    ] {
        let before = owner.get(key);
        assert!(owner.set(key, invalid).is_err());
        assert_eq!(owner.get(key), before);
        assert_eq!(fs::read(files.user()).unwrap(), accepted);
    }
    let restarted = Owner::load(files.user(), None).unwrap();
    for (key, value) in &edits {
        assert_eq!(&restarted.get(*key).value, value);
    }
    let written = String::from_utf8(accepted.clone()).unwrap();
    assert!(written.contains("# preserved") && written.contains("future = 'retained'"));
    fs::write(
        files.user(),
        "[capture]\nsave = true\ndirectory = '../invalid'\n",
    )
    .unwrap();
    assert!(owner.reload().is_err());
    for (key, value) in &edits {
        assert_eq!(&owner.get(*key).value, value);
    }
    fs::write(files.user(), accepted).unwrap();
    fs::write(
        files.managed(),
        "[wallpaper]\nfolder='/declared'\n[capture]\ncopy=true\n",
    )
    .unwrap();
    let mut managed = files.owner();
    managed
        .set(Key::WallpaperFolder, Value::Text("/temporary".into()))
        .unwrap();
    managed.set(Key::CaptureCopy, Value::Bool(false)).unwrap();
    managed.reload().unwrap();
    assert!(managed.get(Key::WallpaperFolder).runtime_override);
    assert_eq!(managed.get(Key::CaptureCopy).value, Value::Bool(false));
    let restarted = files.owner();
    assert_eq!(
        restarted.get(Key::WallpaperFolder).value,
        Value::Text("/declared".into())
    );
    assert_eq!(restarted.get(Key::CaptureCopy).value, Value::Bool(true));
}

#[test]
fn media_preference_defaults_and_lexical_boundaries() {
    let files = Fixture::new();
    let mut owner = Owner::load(files.user(), None).unwrap();
    for key in [Key::CaptureSave, Key::CaptureCopy] {
        assert_eq!(owner.get(key).value, Value::Bool(true));
    }
    for key in [
        Key::WallpaperFolder,
        Key::CaptureDirectory,
        Key::CaptureFilenamePattern,
    ] {
        assert_eq!(owner.get(key).value, Value::Text(String::new()));
    }
    for directory in ["", "~", "~/Pictures", "$HOME/Pictures", "/offline/path"] {
        owner
            .set(Key::CaptureDirectory, Value::Text(directory.into()))
            .unwrap();
    }
    owner
        .set(Key::WallpaperFolder, Value::Text("a".repeat(4096)))
        .unwrap();
    owner
        .set(Key::CaptureFilenamePattern, Value::Text("a".repeat(1024)))
        .unwrap();
    // Native chrono validation remains the operation's responsibility.
    owner
        .set(Key::CaptureFilenamePattern, Value::Text("%Q".into()))
        .unwrap();
}

#[test]
fn corner_size_bounds_atomic_reload_and_restart() {
    let files = Fixture::new();
    fs::write(
        files.user(),
        "# retained\n[appearance]\nfuture_key = 'kept'\n",
    )
    .unwrap();
    let mut owner = Owner::load(files.user(), None).unwrap();
    assert_eq!(owner.get(Key::CornerSize).value, Value::Integer(32));
    for invalid in [
        Value::Integer(0),
        Value::Integer(101),
        Value::Bool(true),
        Value::Text("32".into()),
    ] {
        let before = fs::read(files.user()).unwrap();
        assert!(owner.set(Key::CornerSize, invalid).is_err());
        assert_eq!(fs::read(files.user()).unwrap(), before);
        assert_eq!(owner.get(Key::CornerSize).value, Value::Integer(32));
    }
    owner.set(Key::CornerSize, Value::Integer(100)).unwrap();
    let restarted = Owner::load(files.user(), None).unwrap();
    assert_eq!(restarted.get(Key::CornerSize).value, Value::Integer(100));
    let written = fs::read_to_string(files.user()).unwrap();
    assert!(written.contains("# retained") && written.contains("future_key = 'kept'"));
    fs::write(files.user(), "[appearance]\ncorner_size = 100.5\n").unwrap();
    assert!(owner.reload().is_err());
    assert_eq!(owner.get(Key::CornerSize).value, Value::Integer(100));
    fs::write(files.user(), written).unwrap();
    fs::write(files.managed(), "[appearance]\ncorner_size = 1\n").unwrap();
    let mut managed = files.owner();
    managed.set(Key::CornerSize, Value::Integer(80)).unwrap();
    managed.reload().unwrap();
    assert_eq!(managed.get(Key::CornerSize).value, Value::Integer(80));
    assert!(managed.get(Key::CornerSize).runtime_override);
    assert_eq!(files.owner().get(Key::CornerSize).value, Value::Integer(1));
}

#[test]
fn wallpaper_map_atomic_bounds_unicode_selectors_and_restart() {
    let files = Fixture::new();
    fs::write(
        files.user(),
        "# retained\n[wallpaper]\nfuture = 'unknown'\n",
    )
    .unwrap();
    let mut owner = Owner::load(files.user(), None).unwrap();
    assert_eq!(owner.get(Key::WallpaperEnabled).value, Value::Bool(false));
    owner
        .assign_wallpaper("OFFLINE-1", Some("/offline/space image.PNG"))
        .unwrap();
    for output in [
        "",
        " DP-1",
        "DP-1\u{a0}",
        "\u{2003}DP-1",
        "DP-1,DP-2",
        "DP-1\n",
    ] {
        let before = fs::read(files.user()).unwrap();
        assert!(owner.assign_wallpaper(output, Some("/image.png")).is_err());
        assert!(owner.assign_wallpaper(output, None).is_err());
        assert_eq!(fs::read(files.user()).unwrap(), before);
    }
    for path in ["relative.png", "/unsupported.txt", "/bad\0.png"] {
        assert!(owner.assign_wallpaper("OFFLINE-1", Some(path)).is_err());
    }
    for number in 1..32 {
        owner
            .assign_wallpaper(&format!("DP-{number}"), Some("/image.webp"))
            .unwrap();
    }
    let before = fs::read(files.user()).unwrap();
    assert!(
        owner
            .assign_wallpaper("OVERFLOW", Some("/image.png"))
            .is_err()
    );
    assert_eq!(fs::read(files.user()).unwrap(), before);
    let restarted = Owner::load(files.user(), None).unwrap();
    assert_eq!(
        owner.get(Key::WallpaperAssignments),
        restarted.get(Key::WallpaperAssignments)
    );
    assert!(
        String::from_utf8(before)
            .unwrap()
            .contains("future = 'unknown'")
    );
    owner.assign_wallpaper("DP-1", None).unwrap();
    owner
        .assign_wallpaper("OVERFLOW", Some("/video.mp4"))
        .unwrap();
    let before = owner.get(Key::WallpaperAssignments);
    fs::write(
        files.user(),
        "[wallpaper.assignments]\n'\u{a0}DP-1' = '/bad.png'\n",
    )
    .unwrap();
    assert!(owner.reload().is_err());
    assert_eq!(owner.get(Key::WallpaperAssignments), before);
}

#[test]
fn managed_wallpaper_assignment_edits_retain_offline_and_reset_on_restart() {
    let files = Fixture::new();
    fs::write(
        files.managed(),
        "[wallpaper]\nenabled = false\n[wallpaper.assignments]\nOFFLINE = '/offline.png'\n",
    )
    .unwrap();
    let mut owner = files.owner();
    owner.assign_wallpaper("DP-1", Some("/new.png")).unwrap();
    owner.reload().unwrap();
    let setting = owner.get(Key::WallpaperAssignments);
    assert!(setting.managed && setting.runtime_override);
    let Value::Assignments(map) = setting.value else {
        panic!("expected assignments")
    };
    assert_eq!(map.len(), 2);
    assert_eq!(map["OFFLINE"], "/offline.png");
    assert!(!files.user().exists());
    let restarted = files.owner();
    let Value::Assignments(map) = restarted.get(Key::WallpaperAssignments).value else {
        panic!("expected assignments")
    };
    assert_eq!(map.len(), 1);
    assert_eq!(map["OFFLINE"], "/offline.png");
}

#[test]
fn precedence_reload_and_restart() {
    let files = Fixture::new();
    fs::write(
        files.user(),
        "[bar]\nmooncoil = true\npreferred_output = 'DP-1'\n",
    )
    .unwrap();
    fs::write(files.managed(), "[bar]\nmooncoil = false\n").unwrap();
    let mut owner = files.owner();
    assert_eq!(owner.get(Key::Corners).origin, Origin::Default);
    assert_eq!(owner.get(Key::PreferredOutput).origin, Origin::User);
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Nix);
    owner.set(Key::Mooncoil, Value::Bool(true)).unwrap();
    owner.reload().unwrap();
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Runtime);
    assert!(owner.get(Key::Mooncoil).managed);
    assert!(owner.get(Key::Mooncoil).runtime_override);
    assert_eq!(owner.get(Key::Mooncoil).value, Value::Bool(true));
    assert_eq!(
        fs::read_to_string(files.managed()).unwrap(),
        "[bar]\nmooncoil = false\n"
    );
    // A separate OS process loads Nix values, never the parent's override.
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "restart_child", "--nocapture"])
        .env("TOHRU_TEST_RESTART_DIR", &files.0)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn restart_child() {
    let Some(path) = std::env::var_os("TOHRU_TEST_RESTART_DIR") else {
        return;
    };
    let path = PathBuf::from(path);
    let owner = Owner::load(path.join("config.toml"), Some(path.join("managed.toml"))).unwrap();
    assert_eq!(owner.get(Key::Mooncoil).value, Value::Bool(false));
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Nix);
}

#[test]
fn unmanaged_writes_preserve_future_keys_comments_and_external_edits() {
    let files = Fixture::new();
    let mut owner = Owner::load(files.user(), None).unwrap();
    fs::write(
        files.user(),
        "# keep this comment\n[future]\nvalue = [1,2,3]\n[bar]\npreferred_output = 'HDMI-A-1'\n",
    )
    .unwrap();
    owner.set(Key::Mooncoil, Value::Bool(true)).unwrap();
    let written = fs::read_to_string(files.user()).unwrap();
    assert!(written.contains("# keep this comment"));
    assert!(written.contains("value = [1,2,3]"));
    let restarted = Owner::load(files.user(), None).unwrap();
    assert_eq!(restarted.get(Key::Mooncoil).value, Value::Bool(true));
    assert_eq!(
        restarted.get(Key::PreferredOutput).value,
        Value::Text("HDMI-A-1".into())
    );
}

#[test]
fn invalid_reload_is_atomic_and_does_not_discard_override() {
    let files = Fixture::new();
    fs::write(files.user(), "[bar]\npreferred_output = 'DP-1'\n").unwrap();
    fs::write(files.managed(), "[bar]\nmooncoil = false\n").unwrap();
    let mut owner = files.owner();
    owner.set(Key::Mooncoil, Value::Bool(true)).unwrap();
    fs::write(files.user(), "[bar]\npreferred_output = 'DP-2'\n").unwrap();
    fs::write(files.managed(), "[appearance]\ncorners = 'invalid'\n").unwrap();
    assert!(owner.reload().is_err());
    assert_eq!(
        owner.get(Key::PreferredOutput).value,
        Value::Text("DP-1".into())
    );
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Runtime);
    fs::write(files.managed(), "[bar]\nmooncoil = false\n").unwrap();
    owner.reload().unwrap();
    assert_eq!(
        owner.get(Key::PreferredOutput).value,
        Value::Text("DP-2".into())
    );
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Runtime);
}

#[test]
fn invalid_values_and_failed_writes_do_not_apply() {
    let files = Fixture::new();
    let mut owner = Owner::load(files.user(), None).unwrap();
    assert!(owner.set(Key::Mooncoil, Value::Text("bad".into())).is_err());
    assert!(
        owner
            .set(Key::PreferredOutput, Value::Text("bad\noutput".into()))
            .is_err()
    );
    fs::create_dir(files.user()).unwrap();
    assert!(owner.set(Key::Mooncoil, Value::Bool(true)).is_err());
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Default);
}

#[cfg(unix)]
#[test]
fn symlink_to_managed_is_never_rewritten() {
    let files = Fixture::new();
    fs::write(files.managed(), "[bar]\nmooncoil = false\n").unwrap();
    std::os::unix::fs::symlink(files.managed(), files.user()).unwrap();
    assert!(Owner::load(files.user(), Some(files.managed())).is_err());
    let mut owner = Owner::load(files.user(), None).unwrap();
    assert!(owner.set(Key::Mooncoil, Value::Bool(true)).is_err());
    assert_eq!(
        fs::read_to_string(files.managed()).unwrap(),
        "[bar]\nmooncoil = false\n"
    );
}

#[test]
fn oversized_and_missing_managed_files_are_errors() {
    let files = Fixture::new();
    assert!(Owner::load(files.user(), Some(files.managed())).is_err());
    fs::write(files.user(), "#".repeat(1024 * 1024 + 1)).unwrap();
    assert!(Owner::load(files.user(), None).is_err());
}

#[test]
fn serialized_size_limit_rejects_growth_without_changing_disk_or_memory() {
    let files = Fixture::new();
    let original = format!("#{}\n", "x".repeat(1024 * 1024 - 2));
    fs::write(files.user(), &original).unwrap();
    let mut owner = Owner::load(files.user(), None).unwrap();
    assert!(matches!(
        owner.set(Key::Mooncoil, Value::Bool(true)),
        Err(tohru_shell::settings::Error::TooLarge(_))
    ));
    assert_eq!(fs::read_to_string(files.user()).unwrap(), original);
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Default);
    assert_eq!(owner.get(Key::Mooncoil).value, Value::Bool(false));
    let restarted = Owner::load(files.user(), None).unwrap();
    assert_eq!(restarted.get(Key::Mooncoil).value, Value::Bool(false));
    assert_eq!(fs::read_dir(&files.0).unwrap().count(), 1);

    // An edit whose resulting document is exactly at the limit remains usable.
    let suffix = "[bar]\nmooncoil = true\n";
    let prefix = format!("#{}\n", "x".repeat(1024 * 1024 - suffix.len() - 2));
    fs::write(files.user(), &prefix).unwrap();
    let mut owner = Owner::load(files.user(), None).unwrap();
    owner.set(Key::Mooncoil, Value::Bool(true)).unwrap();
    assert_eq!(fs::metadata(files.user()).unwrap().len(), 1024 * 1024);
    let restarted = Owner::load(files.user(), None).unwrap();
    assert_eq!(restarted.get(Key::Mooncoil).value, Value::Bool(true));
}

#[test]
fn wallpaper_fit_validates_atomically_and_retains_layer_lifetimes() {
    let files = Fixture::new();
    fs::write(files.managed(), "").unwrap();
    fs::write(files.user(), "# retain me\n[wallpaper]\nfuture='kept'\n").unwrap();
    let mut owner = files.owner();
    assert_eq!(
        owner.get(Key::WallpaperFit).value,
        Value::WallpaperFit(WallpaperFit::Fill)
    );
    for mode in [
        WallpaperFit::Fill,
        WallpaperFit::Fit,
        WallpaperFit::Centre,
        WallpaperFit::Tile,
    ] {
        assert_eq!(WallpaperFit::parse(mode.as_str()).unwrap(), mode);
        owner
            .set(Key::WallpaperFit, Value::WallpaperFit(mode))
            .unwrap();
        assert_eq!(
            files.owner().get(Key::WallpaperFit).value,
            Value::WallpaperFit(mode)
        );
    }
    let before = owner.get(Key::WallpaperFit);
    let disk = fs::read(files.user()).unwrap();
    for invalid in [
        Value::Text("fill".into()), // Raw text cannot enter the typed domain.
        Value::Text("cover".into()),
        Value::Text("contain".into()),
        Value::Text("center".into()),
        Value::Text("Fill".into()),
        Value::Text("tile ".into()),
        Value::Text(String::new()),
        Value::Bool(true),
        Value::Integer(1),
    ] {
        assert!(owner.set(Key::WallpaperFit, invalid).is_err());
        assert_eq!(owner.get(Key::WallpaperFit), before);
        assert_eq!(fs::read(files.user()).unwrap(), disk);
    }
    assert!(
        owner
            .set(
                Key::WallpaperFolder,
                Value::WallpaperFit(WallpaperFit::Fill)
            )
            .is_err()
    );
    let persisted = fs::read_to_string(files.user()).unwrap();
    assert!(persisted.contains("# retain me") && persisted.contains("future='kept'"));
    for invalid in ["cover", "contain", "center", "Fill", "tile ", "", "bad"] {
        assert!(WallpaperFit::parse(invalid).is_err());
        fs::write(
            files.user(),
            format!("[wallpaper]\nfit='{invalid}'\nenabled=true\n"),
        )
        .unwrap();
        let rejected_disk = fs::read(files.user()).unwrap();
        assert!(owner.reload().is_err());
        assert_eq!(owner.get(Key::WallpaperFit), before);
        assert_eq!(owner.get(Key::WallpaperEnabled).value, Value::Bool(false));
        assert_eq!(fs::read(files.user()).unwrap(), rejected_disk);
    }
    fs::write(files.user(), persisted).unwrap();
    fs::write(files.managed(), "[wallpaper]\nfit='fit'\n").unwrap();
    owner.reload().unwrap();
    assert!(owner.get(Key::WallpaperFit).managed);
    let disk = fs::read(files.user()).unwrap();
    owner
        .set(Key::WallpaperFit, Value::WallpaperFit(WallpaperFit::Centre))
        .unwrap();
    owner.reload().unwrap();
    assert_eq!(
        owner.get(Key::WallpaperFit).value,
        Value::WallpaperFit(WallpaperFit::Centre)
    );
    assert!(owner.get(Key::WallpaperFit).runtime_override);
    assert_eq!(fs::read(files.user()).unwrap(), disk);
    assert_eq!(
        files.owner().get(Key::WallpaperFit).value,
        Value::WallpaperFit(WallpaperFit::Fit)
    );
}
