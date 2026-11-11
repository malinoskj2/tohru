// Included inside the actual native logic_luau::tests module, Luau feature only.
#[test]
fn tohru_shell_role_alone_installs_shell_settings() {
    use std::process::Command;

    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir =
        Fixture(std::env::temp_dir().join(format!("tohru-role-install-{}", std::process::id())));
    for path in ["home", "config/tohru", "run/ipc"] {
        std::fs::create_dir_all(dir.0.join(path)).unwrap();
    }
    std::fs::write(dir.0.join("config/tohru/config.toml"), "[invalid\n").unwrap();
    std::fs::write(
        dir.0.join("role.plm"),
        "scene RoleFixture { surface { size: 1, 1 } }\n",
    )
    .unwrap();
    std::fs::write(
        dir.0.join("role.luau"),
        "-- No services or actions started\n",
    )
    .unwrap();
    for case in ["shell", "none", "plugin"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("logic_luau::tests::tohru_role_child_{case}"),
                "--nocapture",
                "--ignored",
            ])
            .env("TOHRU_ROLE_INSTALL_TEST_DIR", &dir.0)
            .env("TOHRU_SHELL", if case == "none" { "0" } else { "1" })
            .env("HOME", dir.0.join("home"))
            .env("XDG_CONFIG_HOME", dir.0.join("config"))
            .env("XDG_RUNTIME_DIR", dir.0.join("run"))
            .env("PLEAMAR_SOCKETS", dir.0.join("run/ipc"))
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}/absent-session", dir.0.display()),
            )
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}/absent-system", dir.0.display()),
            )
            .env(
                "PULSE_SERVER",
                format!("unix:{}/absent-pulse", dir.0.display()),
            )
            .env("WAYLAND_DISPLAY", "wayland-never-started")
            .env_remove("WAYLAND_SOCKET")
            .env_remove("CUA_INJECT_SOCKET")
            .env_remove("TOHRU_MANAGED_CONFIG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{case}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

fn tohru_role_child(case: &str) {
    use crate::logic::Script;
    use std::sync::{Arc, atomic::AtomicBool, mpsc};

    let dir = std::path::PathBuf::from(std::env::var_os("TOHRU_ROLE_INSTALL_TEST_DIR").unwrap());
    assert!(
        dir.canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap())
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("role.luau")).unwrap(),
        "-- No services or actions started\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("role.plm")).unwrap(),
        "scene RoleFixture { surface { size: 1, 1 } }\n"
    );
    let (tx, _rx) = mpsc::channel();
    let (events, _events_rx) = mpsc::channel();
    let blocked = Arc::new(AtomicBool::new(false));
    let mut script = super::LuauScript::new(
        dir.join("role.plm").to_str().unwrap(),
        dir.join("role.luau").to_str().unwrap(),
        tx.clone(),
        events,
        Arc::clone(&blocked),
    );
    let _ = script.scene();
    if case == "plugin" {
        script.prefix = Some("tohru-plugin".into());
    }
    let mut context = crate::logic::Context::for_plugin(tx, blocked);
    script.on_event(crate::scene::Event::Alarm("start"), &mut context);
    if case == "shell" {
        assert!(
            script.lua.is_none(),
            "ordinary shell still rejects malformed settings"
        );
    } else {
        let lua = script
            .lua
            .as_ref()
            .expect("disabled/plugin loads without shell TOML");
        for name in ["tohru_settings", "tohru_info"] {
            assert!(
                matches!(
                    lua.globals().get::<mlua::Value>(name).unwrap(),
                    mlua::Value::Nil
                ),
                "{case}: {name}"
            );
        }
    }
}

#[test]
#[ignore = "spawned only by isolated role installer parent test"]
fn tohru_role_child_shell() {
    tohru_role_child("shell");
}

#[test]
#[ignore = "spawned only by isolated role installer parent test"]
fn tohru_role_child_none() {
    tohru_role_child("none");
}

#[test]
#[ignore = "spawned only by isolated role installer parent test"]
fn tohru_role_child_plugin() {
    tohru_role_child("plugin");
}
