# Runtime source and patches

Pleamar 0.7.27 is fetched at dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86;
`pin.json` records the archive SHA256 and upstream repository. Upstream HEAD
was checked on 2026-11-07 and still matched. No prototype vendor is copied.
Upstream's BSD-3-Clause license/THIRD-PARTY notices remain in its source archive.
The shell's own initial settings implementation is MIT.

`0001-shell-settings.patch` adds the focused `tohru-shell` path dependency and
one binding installation hook, gated by TOHRU_SHELL=1 and excluding plugins.
`settings_luau.rs` owns the process singleton and Luau boundary. No native service,
renderer, frame listener, notification or layer namespace behavior is patched.
`Cargo.lock` starts from the pinned upstream lock and adds only anyhow, the local
library, and toml_writer (needed for comment-preserving TOML writes). The library
uses upstream's already selected toml_edit 0.25.15, not a second parser version.

```sh
./runtime/prepare.sh
source runtime/dev-env.sh # only needed for native headers in the supplied sandbox
cargo test --release --locked --manifest-path .tmp/pleamar-runtime/Cargo.toml --lib tohru_settings:: --jobs 4
cargo build --release --locked --manifest-path .tmp/pleamar-runtime/Cargo.toml --bin pleamar --jobs 4
.tmp/pleamar-runtime/target/release/pleamar --check shell/bar.plm
TOHRU_RUNTIME="$PWD/.tmp/pleamar-runtime/target/release/pleamar" ./bin/tohru-run
./bin/tohru status --json # set TOHRU_RUNTIME to the same binary for local use
```

Run only in an explicitly selected private display/bus during development.
Pleamar normally refreshes editor skills in HOME on launch; use a private HOME
and XDG directories in tests. Do not point test commands at a live desktop.
The initial scene is a settings-backed first-output bar, not visual parity or a
workspace feature. `bar.preferred_output` and `appearance.corners` currently store
and report settings only; output fallback/reconnect and corner overlays are pending.
`bar.mooncoil` immediately shows/hides the bar mark.

Nix: import `default.nix { pkgs = <explicit pinned nixpkgs>; }`; optional reusable
Home Manager module `nix/home-manager.nix` supplies the package and managed TOML
path, with no service/autostart activation. It never writes into /home/jesse/nix.
A Nix build and NixOS/system integration remain separate verification gates.

## Verified initial checkpoint

The real settings entry point was tested on private NVIDIA RTX 5090 Sway/GLES2
with the app on Vulkan: native CLI writes, managed overrides, config reload,
actual Luau logic reload, invalid reload rejection, unknown/comment preservation,
unmanaged persistence, and process restart reset. Test runner owns and stops its
app/compositor; the wrapper owns the private bus. It writes raw logs and one PNG
under the worktree-root `.tmp/` with absolute paths printed on completion.

After obtaining the exclusive GPU slot, run:

```sh
./tests/native-settings.sh --runtime "$PWD/.tmp/pleamar-runtime/target/release/pleamar"
```

This wrapper reproduces the explicit private bus configuration needed in the
sandbox (the default /etc/dbus-1/session.conf is absent). It uses short runtime
paths under `.tmp/` to respect Unix socket limits. The wrapper was syntax-checked;
the equivalent explicit private-bus command was used for the recorded passing run.
CPU-only CLI checks: `python3 tests/cli.py`.

Remaining gates: Nix package build, Home Manager evaluation with a real consumer,
Hyprland runtime validation, preferred/focused/first-output fallback and reconnect,
hotplug/scaling/rotation, exact 240Hz workspace motion and stop-at-rest frame
integration (native API decision pending), full retained bar contents and 1:1
visuals, native glass review, and all performance/presentation/soak gates.
No notifications, authentication, idle, wallpaper or other singleton services
are activated by this foundation. Native frame/namespace/notification gaps stay
unpatched until the user's decisions. Independent review follows integration.

## Root flake interface

The root `flake.nix` locks nixpkgs revision
`4fe25e12eb9f4c744966dd3de5a05246cf098601`. It exports only `x86_64-linux`;
no ARM/macOS build or support is claimed. Pleamar keeps its separate runtime pin.

```sh
nix build .#tohru            # also packages.x86_64-linux.default
nix build .#pleamar-runtime  # pinned upstream with approved shell settings hook
nix run .#shell             # also nix run . ; starts only the initial shell bar
nix run .#tohru -- status --json
```

The command runner still follows native display warning/fallback behavior,
explicitly accepted by the user; zero running instances fail. The initial bar
is a foundation slice, not workspace/output/visual parity.

A consuming flake may follow its own pinned nixpkgs and import the module:

```nix
inputs.tohru.url = "path:/path/to/tohru";
inputs.tohru.inputs.nixpkgs.follows = "nixpkgs";
# In a Home Manager configuration's modules:
{
  imports = [ inputs.tohru.homeManagerModules.tohru ]; # .default is an alias
  programs.tohru = {
    enable = true;
    settings.bar.mooncoil = false;
    settings.bar.preferred_output = "DP-1";
  };
}
```

The module uses its consumer's `pkgs`; `programs.tohru.package` remains
replaceable. It installs a package and managed TOML environment path only, without
activating services or autostart. `nix/tests/home-manager.nix` accepts actual
Home Manager, nixpkgs and Tohru flake values for enabled/disabled consumer checks.
No NixOS module is exported because this slice supplies no NixOS-specific behavior.

When the independent E-owned `effects/package.nix` is present after integration,
`packages.x86_64-linux.desktop-effects` calls it separately; it is never a shell
dependency and never loaded automatically. E's source is absent from this F-only
checkout, so that optional output is not declared/tested here. The package's
reviewed candidate requires Hyprland **0.56.2**, exact matching compositor headers
and runtime ABI including any local compositor patches; the version string alone
does not prove compatibility. With a custom candidate, use the effect package's
`override { hyprland = candidate; }` before building, and keep loading an explicit
integration step. A changed nixpkgs/compositor requires those compatibility checks
again. No plugin loading/configuration or existing Nix repository is modified.

The optional effect package uses the separately locked `effects-nixpkgs` input at
E's reviewed revision `8ce4ef6cb6f871616146b9fe26d2a5ae594e94fe`; the shell's
newer nixpkgs pin does not supply Hyprland 0.56.2. Keep this input independent of
the shell nixpkgs follow unless the replacement package set has the exact reviewed
candidate compositor. The native portable shell neither builds nor loads this
plugin as a dependency.
