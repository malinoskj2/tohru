use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread;

use super::GreeterSnapshot;
use super::config::{GreeterConfig, GreeterStyle};
use super::discovery::{
    SessionEntry, SessionKind, build_session_launch, discover_sessions, discover_users,
    parse_desktop_exec, resolve_avatar_in,
};
use super::ipc::{
    AuthMessageType, Client, ErrorType, IpcError, MAX_FRAME_BYTES, Response, read_frame,
    write_frame,
};
use super::state::{LastState, load as load_state, save as save_state};
use crate::settings::LockStyle;

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "tohru-greeter-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_file(path: &Path, content: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn greetd_ipc_handles_multistep_auth_error_cancel_and_start() {
    let (client_stream, mut server_stream) = UnixStream::pair().unwrap();

    let server = thread::spawn(move || {
        let req = read_frame(&mut server_stream).unwrap();
        let req: serde_json::Value = serde_json::from_slice(&req).unwrap();
        assert_eq!(req["type"], "create_session");
        assert_eq!(req["username"], "jesse");
        write_frame(
            &mut server_stream,
            br#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password: "}"#,
        )
        .unwrap();

        let req = read_frame(&mut server_stream).unwrap();
        let req: serde_json::Value = serde_json::from_slice(&req).unwrap();
        assert_eq!(req["type"], "post_auth_message_response");
        assert_eq!(req["response"], "wrong");
        write_frame(
            &mut server_stream,
            br#"{"type":"error","error_type":"auth_error","description":"Incorrect password"}"#,
        )
        .unwrap();

        let req = read_frame(&mut server_stream).unwrap();
        let req: serde_json::Value = serde_json::from_slice(&req).unwrap();
        assert_eq!(req["type"], "cancel_session");
        write_frame(&mut server_stream, br#"{"type":"success"}"#).unwrap();

        let req = read_frame(&mut server_stream).unwrap();
        let req: serde_json::Value = serde_json::from_slice(&req).unwrap();
        assert_eq!(req["type"], "create_session");
        write_frame(
            &mut server_stream,
            br#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password: "}"#,
        )
        .unwrap();

        let req = read_frame(&mut server_stream).unwrap();
        let req: serde_json::Value = serde_json::from_slice(&req).unwrap();
        assert_eq!(req["response"], "hunter2");
        write_frame(
            &mut server_stream,
            br#"{"type":"auth_message","auth_message_type":"info","auth_message":"Touch security key"}"#,
        )
        .unwrap();

        let req = read_frame(&mut server_stream).unwrap();
        let req: serde_json::Value = serde_json::from_slice(&req).unwrap();
        assert_eq!(req["type"], "post_auth_message_response");
        assert!(req.get("response").is_none());
        write_frame(&mut server_stream, br#"{"type":"success"}"#).unwrap();

        let req = read_frame(&mut server_stream).unwrap();
        let req: serde_json::Value = serde_json::from_slice(&req).unwrap();
        assert_eq!(req["type"], "start_session");
        assert_eq!(req["cmd"][0], "uwsm");
        assert_eq!(req["env"][0], "XDG_SESSION_TYPE=wayland");
        write_frame(&mut server_stream, br#"{"type":"success"}"#).unwrap();
    });

    let mut client = Client::from_stream(client_stream).unwrap();
    assert_eq!(
        client.create_session("jesse").unwrap(),
        Response::AuthMessage {
            auth_message_type: AuthMessageType::Secret,
            auth_message: "Password: ".into(),
        }
    );
    assert_eq!(
        client
            .post_auth_message_response(Some("wrong".into()))
            .unwrap(),
        Response::Error {
            error_type: ErrorType::AuthError,
            description: "Incorrect password".into(),
        }
    );
    assert_eq!(client.cancel_session().unwrap(), Response::Success);
    assert!(matches!(
        client.create_session("jesse").unwrap(),
        Response::AuthMessage {
            auth_message_type: AuthMessageType::Secret,
            ..
        }
    ));
    assert_eq!(
        client
            .post_auth_message_response(Some("hunter2".into()))
            .unwrap(),
        Response::AuthMessage {
            auth_message_type: AuthMessageType::Info,
            auth_message: "Touch security key".into(),
        }
    );
    assert_eq!(
        client.post_auth_message_response(None).unwrap(),
        Response::Success
    );
    assert_eq!(
        client
            .start_session(
                vec!["uwsm".into(), "start".into()],
                vec!["XDG_SESSION_TYPE=wayland".into()],
            )
            .unwrap(),
        Response::Success
    );
    server.join().unwrap();
}

#[test]
fn greetd_ipc_rejects_oversized_frame() {
    let mut oversized = (u32::try_from(MAX_FRAME_BYTES).unwrap() + 1)
        .to_ne_bytes()
        .to_vec();
    oversized.extend_from_slice(b"{}");
    let err = read_frame(&mut oversized.as_slice()).unwrap_err();
    assert!(matches!(err, IpcError::FrameTooLarge(_)));
}

#[test]
fn discover_users_filters_system_nixbld_nologin_and_hidden() {
    let dir = TempDir::new("users");
    write_file(
        &dir.path().join("etc/passwd"),
        concat!(
            "root:x:0:0:root:/root:/bin/sh\n",
            "greeter:x:998:998:greetd user:/var/empty:/run/current-system/sw/bin/nologin\n",
            "jesse:x:1000:100:Jesse M,,,:/home/jesse:/run/current-system/sw/bin/zsh\n",
            "guest:x:1001:100::/home/guest:/bin/bash\n",
            "service:x:1002:100:Svc:/var/lib/svc:/sbin/nologin\n",
            "hidden:x:1003:100:Hidden:/home/hidden:/bin/zsh\n",
            "nixbld1:x:30001:30000:Nix build user 1:/var/empty:/run/current-system/sw/bin/nologin\n",
            "nixbld2:x:30002:30000:Nix build user 2:/var/empty:/bin/sh\n",
            "nobody:x:65534:65534:Nobody:/var/empty:/bin/false\n",
        )
        .as_bytes(),
    );

    let hidden = vec!["nobody".into(), "greeter".into(), "hidden".into()];
    let users = discover_users(dir.path(), &hidden, &BTreeMap::new());
    assert_eq!(users.len(), 2);
    assert_eq!(users[0].name, "jesse");
    assert_eq!(users[0].display_name, "Jesse M");
    assert_eq!(users[1].name, "guest");
    assert_eq!(users[1].display_name, "guest");
}

#[test]
fn avatar_resolution_follows_priority_and_ignores_missing_or_empty() {
    let dir = TempDir::new("avatars");
    let root = dir.path();
    let home = root.join("home/jesse");
    fs::create_dir_all(&home).unwrap();

    // Nothing set yet -> None
    assert_eq!(
        resolve_avatar_in(root, "jesse", &home, &BTreeMap::new()),
        None
    );

    // 4. ~/.face fallback
    let home_face = home.join(".face");
    write_file(&home_face, b"png-bytes");
    assert_eq!(
        resolve_avatar_in(root, "jesse", &home, &BTreeMap::new()),
        Some(home_face.clone())
    );

    // 3. /etc/tohru/avatars/ overrides ~/.face
    let etc_avatar = root.join("etc/tohru/avatars/jesse.png");
    write_file(&etc_avatar, b"etc-png");
    assert_eq!(
        resolve_avatar_in(root, "jesse", &home, &BTreeMap::new()),
        Some(etc_avatar.clone())
    );

    // 2. AccountsService icons/<user> overrides /etc/tohru/avatars
    let as_icon = root.join("var/lib/AccountsService/icons/jesse");
    write_file(&as_icon, b"accounts-png");
    assert_eq!(
        resolve_avatar_in(root, "jesse", &home, &BTreeMap::new()),
        Some(as_icon.clone())
    );

    // 2a. AccountsService users/<user> Icon= overrides default icons/<user>
    let custom_as = root.join("custom-as-icon.png");
    write_file(&custom_as, b"custom-as");
    write_file(
        &root.join("var/lib/AccountsService/users/jesse"),
        format!("[User]\nIcon={}\n", custom_as.display()).as_bytes(),
    );
    assert_eq!(
        resolve_avatar_in(root, "jesse", &home, &BTreeMap::new()),
        Some(custom_as.clone())
    );

    // 1. Declarative NixOS config overrides AccountsService
    let nix_avatar = root.join("store-avatar.webp");
    write_file(&nix_avatar, b"nix-webp");
    let mut configured = BTreeMap::new();
    configured.insert("jesse".into(), nix_avatar.clone());
    assert_eq!(
        resolve_avatar_in(root, "jesse", &home, &configured),
        Some(nix_avatar)
    );

    // Reject path traversal usernames
    assert_eq!(resolve_avatar_in(root, "../root", &home, &configured), None);
}

#[test]
fn session_discovery_honors_try_exec_dedup_ranking_and_launch_wrapping() {
    let dir = TempDir::new("sessions");
    let bin_dir = dir.path().join("bin");
    let uwsm_bin = bin_dir.join("uwsm");
    write_file(&uwsm_bin, b"#!/bin/sh\n");
    fs::set_permissions(&uwsm_bin, fs::Permissions::from_mode(0o755)).unwrap();

    let share = dir.path().join("share");
    write_file(
        &share.join("wayland-sessions/hyprland.desktop"),
        b"[Desktop Entry]\nName=Hyprland\nExec=/nix/store/x/bin/start-hyprland\nDesktopNames=Hyprland\n",
    );
    write_file(
        &share.join("wayland-sessions/hyprland-uwsm.desktop"),
        format!(
            "[Desktop Entry]\nName=Hyprland (uwsm-managed)\nExec={} start -e -D Hyprland hyprland.desktop\nTryExec={}\nDesktopNames=Hyprland\n",
            uwsm_bin.display(),
            uwsm_bin.display()
        )
        .as_bytes(),
    );
    write_file(
        &share.join("wayland-sessions/missing.desktop"),
        b"[Desktop Entry]\nName=Missing\nExec=/no/such/bin\nTryExec=/no/such/bin\n",
    );
    write_file(
        &share.join("wayland-sessions/hidden.desktop"),
        b"[Desktop Entry]\nName=Hidden\nExec=sway\nNoDisplay=true\n",
    );
    write_file(
        &share.join("xsessions/openbox.desktop"),
        b"[Desktop Entry]\nName=Openbox\nExec=openbox-session %f\nDesktopNames=OPENBOX\n",
    );

    let sessions = discover_sessions(&[share], Some(bin_dir.to_str().unwrap()));
    assert_eq!(sessions.len(), 3);
    assert_eq!(sessions[0].id, "hyprland-uwsm");
    assert_eq!(sessions[1].id, "hyprland");
    assert_eq!(sessions[2].id, "openbox");
    assert_eq!(sessions[2].kind, SessionKind::X11);

    // UWSM launches directly without double shell wrapping
    let uwsm_launch = build_session_launch(&sessions[0]).unwrap();
    assert_eq!(
        uwsm_launch.cmd[0],
        format!("'{}'", uwsm_bin.to_str().unwrap())
    );
    assert!(uwsm_launch.env.contains(&"XDG_SESSION_TYPE=wayland".into()));
    assert!(
        uwsm_launch
            .env
            .contains(&"XDG_CURRENT_DESKTOP=Hyprland".into())
    );

    // Non-UWSM wraps in login shell so NixOS + Home Manager env vars are loaded
    let direct_launch = build_session_launch(&sessions[1]).unwrap();
    assert_eq!(direct_launch.cmd[0], "'/bin/sh'");
    assert_eq!(direct_launch.cmd[1], "'-lc'");
    assert_eq!(
        direct_launch.cmd.last().map(String::as_str),
        Some("'/nix/store/x/bin/start-hyprland'")
    );

    // Field codes stripped cleanly
    assert_eq!(
        parse_desktop_exec("openbox-session %f \"--flag two\" %%"),
        Some(vec![
            "openbox-session".into(),
            "--flag two".into(),
            "%".into()
        ])
    );
    assert_eq!(parse_desktop_exec("unterminated \"quote"), None);
}

#[test]
fn greeter_config_and_state_tolerate_bad_files_and_round_trip() {
    let dir = TempDir::new("config-state");
    let cfg_path = dir.path().join("greeter.toml");
    write_file(
        &cfg_path,
        concat!(
            "[greeter]\n",
            "style = \"horizon-meniscus\"\n",
            "preferred_output = \"DP-2\"\n",
            "default_user = \"jesse\"\n",
            "default_session = \"hyprland-uwsm\"\n",
            "allow_power_actions = false\n",
            "[greeter.avatars]\n",
            "jesse = \"/nix/store/avatar.png\"\n",
        )
        .as_bytes(),
    );
    let cfg = GreeterConfig::load(Some(&cfg_path));
    assert_eq!(cfg.style, GreeterStyle::Styled(LockStyle::HorizonMeniscus));
    assert_eq!(cfg.preferred_output.as_deref(), Some("DP-2"));
    assert_eq!(cfg.default_user.as_deref(), Some("jesse"));
    assert!(!cfg.allow_power_actions);
    assert_eq!(
        cfg.avatars.get("jesse"),
        Some(&PathBuf::from("/nix/store/avatar.png"))
    );
    assert!(cfg.warnings.is_empty());

    // Malformed TOML falls back to defaults with a warning instead of failing
    let bad_path = dir.path().join("bad.toml");
    write_file(&bad_path, b"[greeter\n");
    let bad = GreeterConfig::load(Some(&bad_path));
    assert_eq!(bad.style, GreeterStyle::DEFAULT);
    assert_eq!(bad.warnings.len(), 1);

    // State persistence round-trips and tolerates unknown future fields
    let state_dir = dir.path().join("state");
    let saved = LastState {
        last_user: Some("jesse".into()),
        last_session: Some("hyprland-uwsm".into()),
    };
    save_state(&state_dir, &saved).unwrap();
    assert_eq!(load_state(&state_dir), saved);

    write_file(
        &state_dir.join("state.json"),
        br#"{"last_user":"alice","last_session":"hyprland","future_field":true}"#,
    );
    assert_eq!(
        load_state(&state_dir),
        LastState {
            last_user: Some("alice".into()),
            last_session: Some("hyprland".into()),
        }
    );

    // Snapshot combines config, state, users, and sessions cleanly
    let fake_root = dir.path().join("root");
    write_file(
        &fake_root.join("etc/passwd"),
        b"jesse:x:1000:100:Jesse:/home/jesse:/bin/zsh\nalice:x:1001:100:Alice:/home/alice:/bin/zsh\n",
    );
    let share = dir.path().join("share");
    write_file(
        &share.join("wayland-sessions/hyprland.desktop"),
        b"[Desktop Entry]\nName=Hyprland\nExec=start-hyprland\n",
    );
    let snap = GreeterSnapshot::load_from(
        &fake_root,
        Some(&cfg_path),
        Some(&state_dir),
        &[share],
        None,
        "home".into(),
    );
    // Persisted last_user "alice" exists in passwd and takes precedence over default_user "jesse"
    assert_eq!(snap.selected_user, "alice");
    assert_eq!(snap.selected_session, "hyprland");
    assert_eq!(snap.hostname, "home");
}

/// Run `launch.cmd` the way greetd does: joined with spaces through `/bin/sh -c`.
fn greetd_argv(launch: &super::discovery::SessionLaunch, dump: &Path) -> Vec<String> {
    let out = dump.with_extension("out");
    let status = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(launch.cmd.join(" "))
        .env("ARGDUMP_OUT", &out)
        .env("HOME", dump.parent().unwrap())
        .env("USER", "nobody-tohru-test")
        .status()
        .unwrap();
    assert!(status.success());
    let raw = fs::read(&out).unwrap();
    fs::remove_file(&out).unwrap();
    raw.split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8(s.to_vec()).unwrap())
        .collect()
}

#[test]
fn session_launch_survives_greetd_space_join_and_sh_c() {
    let dir = TempDir::new("launch-quote");
    let script =
        b"#!/bin/sh\nfor a in \"$0\" \"$@\"; do printf '%s\\0' \"$a\"; done > \"$ARGDUMP_OUT\"\n";
    let bin_dir = dir.path().join("my bin");
    let dump = bin_dir.join("argdump");
    let uwsm = bin_dir.join("uwsm");
    for bin in [&dump, &uwsm] {
        write_file(bin, script);
        fs::set_permissions(bin, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let exec_tail = r#""two words" 'it'\''s' "$HOME" "semi;colon" back\\slash for"#;
    let expected_tail: Vec<String> = [
        "two words",
        "it's",
        "$HOME",
        "semi;colon",
        "back\\slash",
        "for",
    ]
    .map(String::from)
    .to_vec();

    for bin in [&dump, &uwsm] {
        let entry = SessionEntry {
            id: "quoted".into(),
            name: "Quoted".into(),
            exec: format!("\"{}\" {exec_tail}", bin.display()),
            desktop_names: vec!["Quoted".into()],
            kind: SessionKind::Wayland,
        };
        let launch = build_session_launch(&entry).unwrap();
        let mut expected = vec![bin.to_str().unwrap().to_owned()];
        expected.extend(expected_tail.iter().cloned());
        assert_eq!(
            greetd_argv(&launch, &dump),
            expected,
            "cmd: {}",
            launch.cmd.join(" ")
        );
    }
}

#[test]
fn session_entry_smoke() {
    let entry = SessionEntry {
        id: "hyprland".into(),
        name: "Hyprland".into(),
        exec: "start-hyprland".into(),
        desktop_names: vec!["Hyprland".into()],
        kind: SessionKind::Wayland,
    };
    let launch = build_session_launch(&entry).unwrap();
    assert!(
        launch
            .env
            .iter()
            .any(|v| v == "XDG_SESSION_DESKTOP=Hyprland")
    );
}

#[test]
fn discover_users_honors_distro_uid_min_and_accepts_enterprise_uids() {
    let dir = TempDir::new("portable-uids");
    write_file(
        &dir.path().join("etc/login.defs"),
        b"# distro policy\nUID_MIN 500\n",
    );
    write_file(
        &dir.path().join("etc/passwd"),
        concat!(
            "legacy:x:500:500:Legacy:/home/legacy:/bin/sh\n",
            "enterprise:x:100001:100001:Enterprise:/home/enterprise:/bin/bash\n",
            "nixbld1:x:30001:30000:Build:/var/empty:/bin/sh\n",
            "nobody:x:65534:65534:Nobody:/var/empty:/bin/sh\n",
        )
        .as_bytes(),
    );
    let users = discover_users(dir.path(), &[], &BTreeMap::new());
    assert_eq!(
        users
            .iter()
            .map(|user| user.name.as_str())
            .collect::<Vec<_>>(),
        ["legacy", "enterprise"]
    );
}
