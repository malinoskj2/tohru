//! Bounded discovery of login users, optional user avatars, and desktop sessions.
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_PASSWD_BYTES: u64 = 128 * 1024;
const MAX_PASSWD_LINES: usize = 256;
const MAX_DESKTOP_BYTES: u64 = 64 * 1024;
const MAX_SESSIONS_PER_DIR: usize = 64;
const MAX_ACCOUNTS_FILE_BYTES: u64 = 16 * 1024;
const MAX_AVATAR_BYTES: u64 = 16 * 1024 * 1024;
const MIN_HUMAN_UID: u32 = 1000;
/// NixOS `nixbld*` build users begin at UID 30001; `nobody` is 65534.
const MAX_HUMAN_UID: u32 = 30000;

const LOGIN_WRAPPER_SCRIPT: &str = "for f in /etc/profiles/per-user/\"$USER\"/etc/profile.d/hm-session-vars.sh \"$HOME/.nix-profile/etc/profile.d/hm-session-vars.sh\"; do [ -r \"$f\" ] && . \"$f\"; done; exec \"$@\"";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserAccount {
    pub name: String,
    pub display_name: String,
    pub uid: u32,
    pub home: PathBuf,
    pub shell: String,
    pub avatar: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionKind {
    Wayland,
    X11,
}

impl SessionKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Wayland => "wayland",
            Self::X11 => "x11",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionEntry {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub desktop_names: Vec<String>,
    pub kind: SessionKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionLaunch {
    pub cmd: Vec<String>,
    pub env: Vec<String>,
}

/// Discover interactive login users from `<root>/etc/passwd` with resolved optional avatars.
#[must_use]
pub fn discover_users(
    root: &Path,
    hidden_users: &[String],
    configured_avatars: &BTreeMap<String, PathBuf>,
) -> Vec<UserAccount> {
    let Ok(file) = File::open(root.join("etc/passwd")) else {
        return Vec::new();
    };
    let mut text = String::new();
    if file
        .take(MAX_PASSWD_BYTES + 1)
        .read_to_string(&mut text)
        .is_err()
        || text.len() as u64 > MAX_PASSWD_BYTES
    {
        return Vec::new();
    }

    let mut users = Vec::new();
    for line in text.lines().take(MAX_PASSWD_LINES) {
        let fields: Vec<_> = line.splitn(8, ':').collect();
        let [name, _, uid, _gid, gecos, home, shell] = fields.as_slice() else {
            continue;
        };
        let Ok(uid) = uid.parse::<u32>() else {
            continue;
        };
        if !(MIN_HUMAN_UID..MAX_HUMAN_UID).contains(&uid) {
            continue;
        }
        if name.is_empty()
            || name.chars().any(char::is_control)
            || hidden_users.iter().any(|hidden| hidden == name)
        {
            continue;
        }
        if !is_login_shell(shell) {
            continue;
        }
        let display_name = gecos
            .split(',')
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
            .unwrap_or(name)
            .to_owned();
        let home_path = PathBuf::from(home);
        let avatar = resolve_avatar_in(root, name, &home_path, configured_avatars);
        users.push(UserAccount {
            name: (*name).to_owned(),
            display_name,
            uid,
            home: home_path,
            shell: (*shell).to_owned(),
            avatar,
        });
    }

    users.sort_by(|a, b| a.uid.cmp(&b.uid).then_with(|| a.name.cmp(&b.name)));
    users
}

fn is_login_shell(shell: &str) -> bool {
    let trimmed = shell.trim();
    if trimmed.is_empty() {
        return false;
    }
    let base = Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed);
    !matches!(base, "nologin" | "false" | "sync" | "halt" | "shutdown")
}

/// Resolve an optional avatar image for `username` using the host filesystem root `/`.
#[must_use]
pub fn resolve_user_avatar(
    username: &str,
    home: &Path,
    configured_avatars: &BTreeMap<String, PathBuf>,
) -> Option<PathBuf> {
    resolve_avatar_in(Path::new("/"), username, home, configured_avatars)
}

/// Resolve an optional avatar image in priority order:
/// 1. Configured declarative path (`services.tohru-greeter.avatars.<user>`)
/// 2. `AccountsService` (`Icon=` in `var/lib/AccountsService/users/<user>`, else `icons/<user>`)
/// 3. System faces directory (`etc/tohru/avatars/<user>{,.png,.jpg,.jpeg,.webp,.svg}`)
/// 4. Home directory fallback (`<home>/.face`, `<home>/.face.icon`)
#[must_use]
pub fn resolve_avatar_in(
    root: &Path,
    username: &str,
    home: &Path,
    configured_avatars: &BTreeMap<String, PathBuf>,
) -> Option<PathBuf> {
    if username.is_empty()
        || username.contains('/')
        || username.contains("..")
        || username.chars().any(char::is_control)
    {
        return None;
    }

    if let Some(path) = configured_avatars.get(username)
        && is_readable_avatar(path)
    {
        return Some(path.clone());
    }

    if let Some(icon_path) = read_accounts_service_icon(root, username)
        && is_readable_avatar(&icon_path)
    {
        return Some(icon_path);
    }

    let accounts_icon = root.join("var/lib/AccountsService/icons").join(username);
    if is_readable_avatar(&accounts_icon) {
        return Some(accounts_icon);
    }

    let system_dir = root.join("etc/tohru/avatars");
    for suffix in ["", ".png", ".jpg", ".jpeg", ".webp", ".svg"] {
        let candidate = system_dir.join(format!("{username}{suffix}"));
        if is_readable_avatar(&candidate) {
            return Some(candidate);
        }
    }

    for name in [".face", ".face.icon"] {
        let candidate = home.join(name);
        if is_readable_avatar(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn read_accounts_service_icon(root: &Path, username: &str) -> Option<PathBuf> {
    let path = root.join("var/lib/AccountsService/users").join(username);
    let file = File::open(path).ok()?;
    let mut text = String::new();
    file.take(MAX_ACCOUNTS_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() as u64 > MAX_ACCOUNTS_FILE_BYTES {
        return None;
    }
    let mut in_user_section = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_user_section = trimmed == "[User]";
            continue;
        }
        if !in_user_section {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        if key.trim() == "Icon" {
            let value = value.trim();
            if value.starts_with('/') && !value.chars().any(char::is_control) {
                return Some(PathBuf::from(value));
            }
        }
    }
    None
}

fn is_readable_avatar(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_AVATAR_BYTES {
        return false;
    }
    File::open(path).is_ok()
}

/// Discover available Wayland and X11 desktop sessions across `data_dirs`.
#[must_use]
pub fn discover_sessions(data_dirs: &[PathBuf], path_env: Option<&str>) -> Vec<SessionEntry> {
    let mut seen = HashSet::new();
    let mut sessions = Vec::new();

    for (subdir, kind) in [
        ("wayland-sessions", SessionKind::Wayland),
        ("xsessions", SessionKind::X11),
    ] {
        for base in data_dirs {
            let dir = base.join(subdir);
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            let mut paths: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("desktop"))
                })
                .take(MAX_SESSIONS_PER_DIR)
                .collect();
            paths.sort();

            for path in paths {
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if stem.is_empty() || seen.contains(stem) {
                    continue;
                }
                if let Some(entry) = parse_desktop_session(&path, stem, kind, path_env) {
                    seen.insert(stem.to_owned());
                    sessions.push(entry);
                }
            }
        }
    }

    sessions.sort_by(|a, b| {
        session_rank(a)
            .cmp(&session_rank(b))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    sessions
}

fn session_rank(session: &SessionEntry) -> u8 {
    match (session.kind, session.id.as_str()) {
        (SessionKind::Wayland, "hyprland-uwsm") => 0,
        (SessionKind::Wayland, "hyprland") => 1,
        (SessionKind::Wayland, _) => 2,
        (SessionKind::X11, _) => 3,
    }
}

fn parse_desktop_session(
    path: &Path,
    id: &str,
    kind: SessionKind,
    path_env: Option<&str>,
) -> Option<SessionEntry> {
    let file = File::open(path).ok()?;
    let mut text = String::new();
    file.take(MAX_DESKTOP_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() as u64 > MAX_DESKTOP_BYTES {
        return None;
    }

    let mut in_entry = false;
    let mut name: Option<String> = None;
    let mut exec: Option<String> = None;
    let mut try_exec: Option<String> = None;
    let mut desktop_names: Vec<String> = Vec::new();
    let mut hidden = false;
    let mut no_display = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_entry = trimmed == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Name" if name.is_none() => name = Some(value.to_owned()),
            "Exec" => exec = Some(value.to_owned()),
            "TryExec" => try_exec = Some(value.to_owned()),
            "DesktopNames" => {
                desktop_names = value
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
                    .map(ToOwned::to_owned)
                    .collect();
            }
            "Hidden" => hidden = value.eq_ignore_ascii_case("true"),
            "NoDisplay" => no_display = value.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }

    if hidden || no_display {
        return None;
    }
    if let Some(binary) = try_exec.as_deref().filter(|s| !s.is_empty())
        && !command_exists(binary, path_env)
    {
        return None;
    }

    let name = name.filter(|s| !s.is_empty())?;
    let exec = exec.filter(|s| !s.is_empty())?;
    Some(SessionEntry {
        id: id.to_owned(),
        name,
        exec,
        desktop_names,
        kind,
    })
}

fn command_exists(binary: &str, path_env: Option<&str>) -> bool {
    let candidate = Path::new(binary);
    if candidate.is_absolute() {
        return is_executable_file(candidate);
    }
    let Some(path_env) = path_env else {
        return false;
    };
    std::env::split_paths(path_env).any(|dir| is_executable_file(&dir.join(binary)))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Collect session search directories from `TOHRU_GREETER_SESSIONS_DIRS`, `XDG_DATA_DIRS`,
/// and standard system fallbacks.
#[must_use]
pub fn default_session_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for var in ["TOHRU_GREETER_SESSIONS_DIRS", "XDG_DATA_DIRS"] {
        if let Some(raw) = std::env::var_os(var) {
            for entry in std::env::split_paths(&raw) {
                if !entry.as_os_str().is_empty() && !dirs.contains(&entry) {
                    dirs.push(entry);
                }
            }
        }
    }
    for fallback in [
        "/run/current-system/sw/share",
        "/usr/local/share",
        "/usr/share",
    ] {
        let path = PathBuf::from(fallback);
        if !dirs.contains(&path) {
            dirs.push(path);
        }
    }
    dirs
}

/// Build the `greetd` `start_session` command line and environment variables for `session`.
///
/// Sessions launched through `uwsm` already load login-shell environment variables and
/// import them into `systemd --user`, so they run directly. Raw compositor commands
/// (such as `start-hyprland` or `sway`) are wrapped in a login shell that sources NixOS
/// `/etc/profile` and Home Manager's `hm-session-vars.sh` before `exec`.
#[must_use]
pub fn build_session_launch(session: &SessionEntry) -> Option<SessionLaunch> {
    let parsed = parse_desktop_exec(&session.exec)?;
    if parsed.is_empty() {
        return None;
    }

    let mut env = vec![format!("XDG_SESSION_TYPE={}", session.kind.as_str())];
    if !session.desktop_names.is_empty() {
        env.push(format!(
            "XDG_CURRENT_DESKTOP={}",
            session.desktop_names.join(":")
        ));
        env.push(format!("XDG_SESSION_DESKTOP={}", session.desktop_names[0]));
    }

    let is_uwsm = Path::new(&parsed[0])
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|base| base == "uwsm");

    let cmd = if is_uwsm {
        parsed
    } else {
        let mut wrapped = vec![
            "/bin/sh".to_owned(),
            "-lc".to_owned(),
            LOGIN_WRAPPER_SCRIPT.to_owned(),
            "sh".to_owned(),
        ];
        wrapped.extend(parsed);
        wrapped
    };

    Some(SessionLaunch { cmd, env })
}

/// Split a Freedesktop `Exec=` line into argv tokens, dropping `%`-field codes.
#[must_use]
pub fn parse_desktop_exec(exec: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut has_token = false;

    for ch in exec.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            has_token = true;
            continue;
        }
        match ch {
            '\\' if !in_single => {
                escaped = true;
            }
            '\'' if !in_double => {
                in_single = !in_single;
                has_token = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                has_token = true;
            }
            c if c.is_ascii_whitespace() && !in_single && !in_double => {
                if has_token {
                    let token = std::mem::take(&mut current);
                    push_exec_token(&mut args, &token);
                    has_token = false;
                }
            }
            _ => {
                current.push(ch);
                has_token = true;
            }
        }
    }

    if in_single || in_double || escaped {
        return None;
    }
    if has_token {
        push_exec_token(&mut args, &current);
    }
    (!args.is_empty()).then_some(args)
}

fn push_exec_token(args: &mut Vec<String>, token: &str) {
    if matches!(
        token,
        "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%i" | "%c" | "%k" | "%v" | "%m"
    ) {
        return;
    }
    args.push(token.replace("%%", "%"));
}
