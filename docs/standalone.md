# Standalone Wayland sessions

Tohru supports Linux with systemd/logind and a compositor advertising
`zwlr_layer_shell_v1`. Layer-shell is required even for Settings. Nix is optional;
there is no dependency on a personal Nix checkout, model mount, output name or
wallpaper service. The release currently targets x86_64 Linux/glibc 2.35+.
Other architectures require a source build and are unverified.

## Capabilities, not compositor names

| Function | Backend | Degradation |
|---|---|---|
| Bar placement | Native `wl_output` names, `xdg-output` names on older outputs | Independent of workspace/window protocols; disconnected preferred outputs fall back to connected outputs |
| Window title and focused output | Hyprland IPC, otherwise `wlr-foreign-toplevel-management` | Title unavailable; launcher/session menu fall back to a live output |
| Workspaces | Hyprland IPC, otherwise `ext-workspace-v1` | No fabricated workspaces; ribbon disappears while bar remains |
| Workspace activation | Reported Hyprland IDs; ext-workspace handle capability | Only advertised activation operations are enabled |
| Glass | Existing capture/blur protocols | Glass effects may be reduced independently of bar visibility |
| Lock | `ext-session-lock-v1`, PAM | Unsupported protocol is an error, never a successful lock |
| Idle | swayidle with idle-notify, logind and inhibitors | Optional; existing desktop idle ownership is the default |
| Shortcuts | Compositor-managed runtime observations; opt-in GlobalShortcuts portal owner | No capture/config rewrites; unavailable/denied grants remain explicit |

Generic workspace IDs are connection-scoped handles, separate from names, and
are never inferred from names. Ten ribbon cells show the page containing the
active workspace; compositor shortcuts reach other pages. Hyprland's existing
positive-ID IPC path remains; special/scratchpad workspaces are not advertised by
that backend. Sway without ext-workspace still gets bars and foreign-toplevel
window titles; this branch does not claim Sway workspace IPC support. Niri,
COSMIC, river and others are supported only for protocols actually advertised.
No compatibility certification for those compositors is implied by policy tests.

Output order changes do not change names. Scale/transform and surface recreation
remain the native runtime's responsibility. Do not use a comma in a monitor name:
the runtime's named-screen selector uses commas as separators. A compositor must
provide usable `wl_output` v4 or `xdg-output` v2 names for named routing. Wallpaper
discovery reads the same standard names and logical size/scale/rotation metadata
from `wayland-info`; doctor marks missing/unknown discovery instead of inventing
outputs.

## Dependencies and layout

Required: the **matching patched runtime**, Bash, Python **3.11+** (including
tomllib), coreutils (`timeout`, `sha256sum`), util-linux (`setsid`, `flock`), systemd
user manager, logind and D-Bus. The runtime additionally needs the libraries listed
in [manual-install.md](manual-install.md): libxkbcommon, PAM, Wayland, EGL and a
Vulkan driver. Python 3.10 from stock Ubuntu 22.04 alone is insufficient for the
standalone helpers, even though its glibc can run the release binary.

Feature dependencies: `wayland-info` (wayland-utils) for doctor/wallpaper output
queries; mpvpaper for wallpaper ownership; FFmpeg/ffprobe, jq, findutils and
coreutils for thumbnails; swayidle for Tohru-owned idle; wl-copy for assistant
copy; xdg-terminal-exec for terminal apps. NetworkManager or iwd provides network
control, BlueZ provides Bluetooth; missing services degrade separately. PulseAudio
or PipeWire-Pulse provides audio. Notifications and polkit require exclusive
ownership. A system font is required; Noto Sans is recommended, with cosmic-text's
system fallback when the requested family is absent. Fonts are not redistributed.

Debian 12 provides [wayland-utils](https://packages.debian.org/bookworm/wayland-utils),
[swayidle](https://packages.debian.org/bookworm/swayidle),
[pulseaudio-utils](https://packages.debian.org/bookworm/pulseaudio-utils),
[libspa-0.2-modules](https://packages.debian.org/bookworm/libspa-0.2-modules) and
[fonts-noto-core](https://packages.debian.org/bookworm/fonts-noto-core), for example:

```sh
sudo apt install wayland-utils swayidle pulseaudio-utils libspa-0.2-modules fonts-noto-core
```

These are administrator instructions, never executed by Tohru. Install mpvpaper
through your distro or build the [upstream project](https://github.com/GhostNaN/mpvpaper)
against its documented Wayland/libmpv dependencies; do not fetch an arbitrary
binary. Optional voice dependencies are described below. Nix packaging supplies
these client tools and speech Python dependencies (when withVoice is true), but
never downloads models. Release tarballs include helpers, scenes, `share/tohru`
setup data and these docs; they rely on distribution packages for external tools.
Keep that layout intact, for example under `/opt/tohru`, and put its `bin` on PATH.

## Services and environment

Generate into a staging directory first. This writes files only; it does not
install, enable, stop or start anything:

```sh
tohru session-units --output ./tohru-units --prefix /opt/tohru/bin \
  --components bar,session,launcher
# Add --managed /absolute/path/managed.toml when using a managed overlay.
```

`lock` is always generated as an **on-demand** Type=notify service. Components can
also include notifications, polkit, wallpaper, idle and shortcuts, each with explicit owner
configuration where applicable. The default standalone set does not take over
notifications. Home Manager retains its existing default notification ownership;
set `programs.tohru.systemd.components` to choose it explicitly. Both paths read
`share/tohru/session/services.json`, with identical restart, target, logging and
launcher-child policy. Do not generate standalone units on top of HM units.

After review, install and enable only your chosen units:

```sh
mkdir -p ~/.config/systemd/user
cp -n ./tohru-units/*.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable tohru-bar tohru-session tohru-launcher
```

Arrange this **inside the compositor session**, after its environment exists and
before the graphical target starts (UWSM normally does this for you):

```sh
systemctl --user import-environment WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_ID PATH XDG_DATA_DIRS
# Import HYPRLAND_INSTANCE_SIGNATURE on Hyprland, SWAYSOCK on Sway.
# Import TOHRU_MANAGED_CONFIG if it is not in generated units.
dbus-update-activation-environment --systemd WAYLAND_DISPLAY XDG_CURRENT_DESKTOP PATH XDG_DATA_DIRS
systemctl --user start graphical-session.target
```

Use the same target for startup and shutdown; an unmanaged compositor must stop it
on exit: `systemctl --user stop graphical-session.target`. A user manager has one
graphical-session environment: simultaneous independent compositor sessions need
separate systemd instances/target strategy; these untemplated user services do not
claim multiseat isolation. Wallpaper/locker runtime paths themselves are scoped to
the Wayland display/session. Change an existing target with `--target NAME.target`
or HM's `systemd.target`. Generated units escape spaces and systemd specifiers.

Restart with `systemctl --user restart tohru-bar`; inspect
`journalctl --user -u tohru-bar -b`. Launcher uses KillMode=process so restarting it
preserves launched apps. Disable with `systemctl --user disable --now UNIT`;
remove only the generated unit files you installed, then daemon-reload. Do not
stop/remove an active locker as a routine disable step. Never run another
notification daemon or polkit agent alongside Tohru's owner.

Native TOML is `$XDG_CONFIG_HOME/tohru/config.toml` (default `~/.config/tohru`).
`TOHRU_MANAGED_CONFIG` overlays it. Existing managed visual settings can retain
session-only overrides; lifecycle helpers always read managed values from disk.
Fonts and compositor shortcuts are read-only in Settings when managed. Restart
components after changing fonts, wallpaper/idle policy or voice environment.
`tohru config set KEY VALUE` uses the running bar's native validated owner; edit
TOML directly when the bar is stopped.

## Wallpaper ownership

Default: disabled. Configure and then start the generated wallpaper unit:

```toml
[wallpaper]
enabled = true
backend = "mpvpaper"
directory = "/home/alice/Pictures/Wallpapers"
# assignments = "/home/alice/.local/state/tohru/wallpaper/assignments.json"
executable = "mpvpaper"
options = "no-audio loop-file=inf"
```

One foreground supervisor starts one mpvpaper child per assigned connected output,
uses private per-session IPC sockets, restores persisted assignments, reconciles
hotplug every three seconds and terminates **only its own children**. Configured
options cannot replace the control socket or load mpv config/scripts. Another
desktop's wallpaper processes/sockets are never adopted or killed. Explicitly
stop your desktop's other wallpaper owner before choosing Tohru ownership.

The default library is `~/Pictures/Wallpapers`, assignments are under
`$XDG_STATE_HOME/tohru/wallpaper/assignments.json`, and preview caches under
`$XDG_CACHE_HOME/tohru/wallpaper`. Existing assignments can be copied intentionally
to that file or selected with `wallpaper.assignments`; no migration from personal
script directories runs automatically. Assignment writes are atomic and locked.

`tohru-wallpaper scan|status` reports disabled/unavailable states without starting
a backend. `apply OUTPUT FILE` changes playback and persists only after backend
success; `load OUTPUT FILE [SECONDS]` is a nonpersistent preview. `pause OUTPUT
yes|no`, `seek OUTPUT SECONDS` and `stop OUTPUT` control owned children. Stop clears
the assignment; restarting the service restores remaining assignments. Still
JPEG/PNG/WebP and video MP4/WebM/MKV/MOV/GIF are supported as libmpv/FFmpeg support
them. Preview, snapshot, seek and pause are not advertised as independent
capabilities for an absent backend. No media is transcoded on apply.

## Lock and idle

The session menu invokes `tohru-session-action`, which starts `tohru-lock.service`.
Only the runtime's **ext-session-lock locked event** sends systemd READY. A skipped,
failed or timed-out service never authorizes the menu's suspend. Direct `tohru
lock` still runs the existing secure locker wrapper. PAM permits and plain-style
fallback remain native; accepting a request or setting logind LockedHint is never
used as evidence of locking. LockedHint is set after protocol confirmation and
cleared after a successful native unlock.

Default `idle.owner = "external"` keeps your existing manager. To give Tohru idle
ownership, configure:

```toml
[idle]
owner = "tohru" # external | disabled | tohru
lock_seconds = 300
suspend_seconds = 900 # 0 disables; otherwise greater than lock_seconds
```

Install/enable the idle unit and stop the other idle manager yourself. Tohru
refuses startup when a known same-user swayidle/hypridle/wayidle/xidlehook runs;
unknown managers cannot be detected universally. A per-session flock prevents
another Tohru owner. It invokes maintained swayidle with `-w`, explicit commands,
lock-session and before-sleep handlers; it does not change compositor configs.
Existing external managers should call `tohru-session-action lock` on lock/sleep
and wait for it, or use the foreground `tohru-lock` according to their readiness
contract. Do not run two lock owners. Use doctor and the journal to diagnose
missing units, protocols, PAM or inhibitors.

swayidle's logind **delay** inhibitor is finite. An external suspend can proceed
when logind's InhibitDelayMaxSec expires even if locking failed; this mechanism
cannot guarantee safety against forced/privileged suspend. Administrators should
measure cold locker startup, review the logind delay limit and validate suspend
policy. Menu/Tohru idle suspend requests require successful confirmed-lock startup.
Do not treat Type=notify readiness as a claim about later physical monitor state.
See [swayidle's manual](https://github.com/swaywm/swayidle/blob/master/swayidle.1.scd).

Create/review a distro-appropriate `/etc/pam.d/tohru-lock` as an administrator;
Tohru never installs PAM policy. Existing fallbacks are pleamar, hyprlock, then
login. Do not copy another distro's PAM stack blindly. On Hyprland enable
`misc:allow_session_lock_restore = true` for crash recovery. Other compositors may
keep an abandoned session locked without allowing a replacement; test recovery
in a disposable session, retain TTY access, and be ready to terminate/restart that
session. A compositor refusing a duplicate lock is not retried as a new owner.

## Fonts and shortcuts

```toml
[appearance]
font_text = "Noto Sans"
font_display = "Noto Sans"
font_mono = "DejaVu Sans Mono"
```

These map all current scene text/display roles (including lock and greeter) while
retaining weights, sizes, spacing and tabular-numeral features. Missing families
use the runtime's normal system fallback. Settings exposes common choices; native
TOML/config-set accepts arbitrary family names. Restart components to apply.
The historical SF Pro scene names are role markers; proprietary fonts are not
required. Greeter fonts must be installed system-wide and readable by its user.

Settings → Shortcuts displays ownership, normalized bindings and possible duplicate
chords from Hyprland's **runtime** `hyprctl -j binds`, without rewriting config.
Sway is labeled Sway-managed; its get_config response is not claimed to enumerate
active bindings. Other compositor adapters display unsupported/unavailable state.
Compositor-managed mode never registers keys or allows Tohru to edit them. Refresh
reads a new runtime snapshot; it does not parse a file and call it active state.

For **Tohru-owned actions**, explicitly choose the separate portal mode:

```toml
[shortcuts]
mode = "portal" # default compositor; disabled also supported
# Optional requests, not actual registrations or forced bindings:
launcher_trigger = "<Super>space"
session_trigger = ""
assistant_trigger = ""
```

Generate/install the optional `shortcuts` component's unit, then start it yourself.
It needs Python's maintained **dbus-fast** client (tested at 5.2.0; Nix packaging
provides it). Install it yourself in the service's Python environment, e.g.
`python3 -m pip install dbus-fast==5.2.0` in your chosen virtual environment; there
is no runtime installer. Source provenance/hash is in
[provenance/portal-python.json](provenance/portal-python.json). Releases keep this
optional dependency separate from voice; no model package is involved.

Settings → Shortcuts → **Set up Tohru action shortcuts** requests desktop grants.
The portal owns the grant/configuration dialog. Only the fixed launcher, session
menu and assistant toggle actions can dispatch; no shell commands can be supplied
through config or D-Bus. The displayed trigger descriptions come from the
backend's response/ShortcutsChanged signal. Preferred-trigger TOML strings are
hints, never claims about actual granted keys. Partial, denied, cancelled, pending,
revoked, unknown and unavailable states remain distinct. Managed ownership mode
is read-only; managed preferred-trigger hints are not overwritten by portal UI.

A portal v2 backend enables **Configure granted shortcuts**, which opens its own
configuration UI. v1 supports initial registration but has no ConfigureShortcuts
operation; Settings labels that limit. Setup/configure actions are explicit and
do not automatically copy/register the compositor's existing bindings. Choose
non-conflicting triggers in the desktop dialog. Compositor mode still shows
Hyprland's own bindings read-only and never takes ownership of them.

The foreground owner holds one app-scoped portal session and one private control
socket, rejects duplicate owners, and closes requests/session on shutdown.
Activation requires a known action granted to the current session by the current
portal owner. Portal restart/revocation clears active state. A new session can
list **previous** grants, but they are labeled inactive until explicit Setup binds
the current session; no automatic grant prompt loop runs on service/portal restart.
`tohru shortcuts [status|setup|configure]` uses the same control path. Config/policy
changes require restarting the optional service. To disable, set mode to
compositor/disabled and stop the portal owner unit; its session closes.

The implementation follows the [primary GlobalShortcuts API](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html).
That API lists this application's registered shortcuts, not every compositor's
bindings. Backend support/desktop grant UI remains a live-session verification
gate; private D-Bus protocol tests are not a certification of every portal backend.
No host grants or dialogs were requested during development.

## Opt-in PipeWire echo cancellation

```sh
tohru echo-cancel # prints the shared HM/standalone drop-in for inspection
tohru echo-cancel --output ~/.config/pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf
```

The writer uses exclusive creation and refuses to overwrite existing files,
including HM-managed files. It never restarts PipeWire or changes default routes.
After reviewing, restart your user PipeWire session yourself when interrupting
audio is acceptable. PipeWire must include `libpipewire-module-echo-cancel` and the
WebRTC SPA AEC library. HM's `assistant.voice.echoCancel` is now **false by default**;
set it explicitly to install the identical source file.

The source/sink have priority zero. Select `tohru-echo-cancel-source` with
`assistant.voice_source` and send assistant playback through the paired virtual
sink (the voice service's existing automatic echo-sink selection does this when
available). Acoustic cancellation needs the matching playback reference, valid
links and physical testing. Detection reports configuration, node/module presence,
selection and **unknown effectiveness** separately. Removing only this drop-in
and restarting PipeWire removes Tohru's configured module; do not delete someone
else's audio configuration. See [PipeWire's module documentation](https://docs.pipewire.org/page_module_echo_cancel.html).

## Voice without Nix

There is **no dependency installer or model downloader**. Chat works with
`assistant.voice = false`. Choose an existing CPython 3.11 virtual environment or
provide one yourself. A reproducible **candidate**, not an inference-tested new
lock, is numpy 2.2.6, onnxruntime 1.23.2 (CPU), sherpa-onnx 1.12.40, misaki 0.9.4,
spaCy 3.8.11, phonemizer 3.3.0 and num2words 0.5.14. The upstream PyPI metadata,
selected CPython 3.11 Linux wheels/source artifact URLs and SHA-256 hashes are in
[provenance/voice-python.json](provenance/voice-python.json), checked 2026-11-22.
Transitive dependencies are not hash-locked; retain your environment's resolved
freeze/lock and review it. Nix keeps using its pinned package-set dependencies.

For an administrator/user performing setup themselves:

```sh
python3.11 -m venv ~/.local/share/tohru/voice-env
~/.local/share/tohru/voice-env/bin/python -m pip install \
  numpy==2.2.6 onnxruntime==1.23.2 sherpa-onnx==1.12.40 \
  'misaki[en]==0.9.4' spacy==3.8.11 phonemizer==3.3.0 num2words==0.5.14
# Explicit spaCy data install, required by misaki's English phonemizer:
~/.local/share/tohru/voice-env/bin/python -m pip install \
  https://github.com/explosion/spacy-models/releases/download/en_core_web_sm-3.8.0/en_core_web_sm-3.8.0-py3-none-any.whl
```

Install espeak-ng and its native shared library, plus `parec`, `pacat` and `pactl`
(usually pulseaudio-utils), through your distro. Use a CPU onnxruntime wheel
matching Python/architecture/glibc; do not install the GPU extra just for Tohru.
The [spaCy model release](https://github.com/explosion/spacy-models/releases/tag/en_core_web_sm-3.8.0)
is explicit data setup, separate from speech weights. Its digest is not recorded
here; verify upstream provenance. Tohru checks for it before using misaki and will
not invoke spaCy's implicit downloader. No installation commands above run from
the service. An environment missing optional Kokoro dependencies uses espeak-ng;
the missing dependency is reported.

```toml
[assistant]
voice_python = "/home/alice/.local/share/tohru/voice-env/bin/python"
# Optional explicit environment TOML; not a shell script and never sourced:
voice_environment = "/home/alice/.config/tohru/voice-environment.toml"
vad_model = "/srv/models/silero-vad/silero_vad.onnx"
asr_model = "/srv/models/zipformer-en"
# turn_model = "/srv/models/smart-turn/smart-turn-v3.2-cpu.onnx"
voice_wake = "asr"
# kws_model = "/srv/models/kws"
tts_engine = "espeak-ng" # or kokoro with its artifacts below
# tts_model = "/srv/models/kokoro"
```

An optional environment file contains `[environment]` string entries, e.g.
`LD_LIBRARY_PATH`, `PATH` and `OMP_NUM_THREADS`. It is read only by voice, never
sourced as code or printed by doctor. Existing `TOHRU_VOICE_SOURCE`, native model
paths and Nix interpreter wrappers remain supported. Offline environment defaults
are set before service startup. Restart the bar/voice process after changes.

Speech artifacts are listed with revisions, licenses, sizes and available hashes
in [configuration.md#voice-models](configuration.md#voice-models):

- Required: Silero VAD ONNX file and streaming sherpa-onnx Zipformer ASR folder
  (encoder, decoder, joiner and tokens). Missing ones disable listening with a
  diagnostic; text chat remains available.
- Optional: Smart Turn v3.2 ONNX; absence/failure uses silence-based end-of-turn.
- Optional: KWS Zipformer folder only for `voice_wake = "kws"`; default ASR wake
  does not need it. Selecting KWS makes its files required.
- TTS fallback: espeak-ng needs no speech weights. Kokoro needs the ONNX graph,
  tokenizer vocabulary and a chosen voicepack. Tohru's Tide voicepack is shipped;
  stock voices need the matching upstream voices files. Arbitrary absolute model
  paths remain supported; MODEL_ROOT/by-name is not consulted at runtime.

Obtain and hash-check artifacts yourself using that table. Doctor checks paths and
Python module discoverability without loading models, accessing the microphone,
contacting providers or downloading anything. Existing isolated worker lifecycle,
stream cancellation, chunk playback and journal diagnostics are retained. A
configured environment file requires checking dependencies inside that environment
manually; doctor labels that observation unknown rather than ignoring its env.

## Optional greetd installation

The shell never requires a greeter. For a new distribution, an administrator must
review greetd's distro package, PAM stack, seat permissions and recovery first.
Tohru does not install/enable a display manager, edit PAM or replace an existing
manager. The same native greetd IPC, secret handling, session Exec quoting, plain
fallback and AccountsService avatar path remain shared with the NixOS module.

Templates under `share/tohru/greeter/` provide:

| File | Administrator destination / purpose |
|---|---|
| greetd.toml | `/etc/greetd/config.toml`, dedicated greeter user on VT1 |
| tohru-greetd-host | `/usr/local/libexec/tohru-greetd-host`, Sway host and optional tuigreet fallback |
| tohru-sway.conf | `/etc/greetd/tohru-sway.conf`, isolated layer-shell compositor |
| tohru-greetd-session | `/usr/local/libexec/tohru-greetd-session`, runtime/environment and compositor exit |
| tohru.toml | `/etc/greetd/tohru.toml`, read-only greeter appearance/behavior |
| 50-tohru-greeter.rules | Optional `/etc/polkit-1/rules.d/50-tohru-greeter.rules`, scoped logind power actions |

Install the Tohru release in a root-owned, non-world-writable location with its
full runtime/scene/share layout; templates assume its wrappers are available at
`/usr/local/bin/tohru-greeter`. Adapt absolute paths to your prefix (a symlink to
the real wrapper preserves its readlink-based scene lookup). Install greetd,
Sway, tuigreet and a system-wide font; root and greeter must both be able to read
all runtime libraries, scenes, shaders, fonts and chosen images. Use the distro's
greetd service/PAM policy and its greeter account. Give that account a writable
`/var/lib/tohru-greeter` state directory, not writable installation/config files.
Use the distro seat manager/logind rules; grant video/render groups only where
its device policy requires them. Do not run the greeter as your desktop user.

Session discovery reads `/usr/local/share/{wayland-sessions,xsessions}` and
`/usr/share/{wayland-sessions,xsessions}`, XDG_DATA_DIRS and the optional explicit
`TOHRU_GREETER_SESSIONS_DIRS` base directories. Desktop files must have valid Exec
entries and installed executables. Non-UWSM sessions launch through a login shell;
optional Home Manager session variables are sourced only when readable. UWSM
keeps its direct launch path. User discovery honors `/etc/login.defs` UID_MIN
(default 1000), skips nologin/nobody/build users and accepts high enterprise UIDs;
it does not impose a Nix build-user numeric cutoff.

Use AccountsService icons or administrator-provided readable avatar files. A
0700 home must not be opened up just to show `.face`. `tohru avatar set FILE`
uses AccountsService's authorized path; templates can set `[greeter.avatars]`.
Power actions default off; enabling the UI alone does not grant permissions. The
optional example polkit rule grants only three exact logind actions for an active,
local greeter subject, and deliberately does not grant ignore-inhibitor or
multi-session power actions. Review your distro policy before enabling it.

Validate prepared TOML with `tohru greeter-check --config PATH --settings PATH`;
run `tohru doctor` under the intended greeter identity with the intended environment
for file/font/runtime/protocol visibility. No validator authenticates or starts a
login session. First test under a disposable VM/nested compositor; native greetd
and PAM integration needs the existing NixOS VM tests or a distro test machine.
Do not enable a real display manager just to test this branch.

Keep tty2–tty6 and another login path available. Preserve the existing manager's
configuration and know how to restore its service from a TTY. If Sway fails, the
host template invokes tuigreet; Tohru's styled greeter itself retries plain mode.
Forcing software rendering may require `PLEAMAR_SWAPCHAIN=1`, as documented by the
existing NixOS greeter integration. A failed greeter must not trap users without
TTY/recovery access.

## Doctor and validation limits

`tohru doctor` prints actionable human output; `tohru doctor --json` emits schema 1
with named checks, state, detail and action. The exit contract below distinguishes required failures, config errors and
unknown required observations; optional unsupported features do not alone fail
the core checks. Every subprocess has a deadline and capped output.
Missing tools, denied buses and unavailable services are observations, not reasons
to install/enable anything. Credentials, environment values and full config are
never included. `systemctl show-environment` is tested for reachability and its
contents discarded.

Doctor is useful with no shell running. It checks runtime patch markers,
Wayland protocols/output discovery, systemd/logind environment, service ownership,
notifications/polkit risks, client executables, wallpaper, PAM, fonts,
audio/network/Bluetooth, voice paths/dependencies, AEC and shortcut ownership.
Installed files, configured paths, loaded modules/nodes and selection remain
different states; neither acoustic success nor real compositor/PAM safety is
claimed by static checks. See [portability-verification.md](portability-verification.md)
for this branch's actual checks and outstanding resource/live-session gates.

Doctor automation contract (JSON `summary.exit_code` matches the process exit):

| Exit | Summary status | Meaning |
|---|---|---|
| 0 | healthy | Required checks passed; optional warnings can remain |
| 1 | required-failures | A required dependency/session capability or enabled feature failed |
| 2 | configuration-error | Config is malformed, inaccessible or violates lifecycle validation |
| 3 | required-unknown | No definite required failure, but required support could not be established |

Every check has `required` and `severity`. `summary.required_failures`,
`required_unknown` and `warnings` list check names, without secrets. Voice model
and capture dependencies are required when voice is enabled; optional TTS uses
fallback. Wallpaper executable/discovery are required only for enabled ownership;
idle tools/lock protocol/PAM are required for Tohru idle ownership. Portal client, interface and current registration checks are required only in portal mode. Missing optional
tools never alone make a healthy core fail. Missing diagnostics may leave a
required capability unknown, returning 3 rather than certifying it.

Before lock/sleep success, the helper also challenges a private **native current
cycle** status socket. Only the compositor-confirmed cycle reports held; unlock,
refusal and re-engage clear it, and a crashed process cannot answer. Stale systemd
active/READY state is insufficient. The endpoint accepts no control/password input.
This is a fresh observation, not an atomic guarantee against a user subsequently
unlocking or a compositor subsequently terminating. Real suspend/inhibitor and PAM
validation still require the live gates in the verification report.

Native and helper values share names, defaults, bounds, absolute-path rules and
managed precedence. Lifecycle helpers deliberately reject malformed/inaccessible
configuration instead of activating an owner with ambiguous policy; the native
visual owner can report ignored invalid individual values as warnings. Both
reject aliasing writable and managed files. Wallpaper/idle policy changes require
a service restart: disabling the TOML field alone does not kill a running owner.
Player crash retries are limited to one attempt per output per five seconds after
a failed start; disconnects clear stale retry/errors. Child/socket tests establish
these policies, not rendered playback or running-session systemd teardown.
