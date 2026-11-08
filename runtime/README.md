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
