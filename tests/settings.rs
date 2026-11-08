use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tohru_shell::settings::{Key, Origin, Owner, Value};

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
