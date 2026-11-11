# tohru-hyprland

Independent optional Hyprland plugin extracted for Pleamar migration phase 4:
render-only focus dips, thin application titlebars with double-click maximize,
parent-attached file chooser sheets, and launcher-triggered tilt/shrink/shade/blur.
No shell process/library is linked. No legacy glass, panel orchestration, game
policy, shell service or IPC server is included. The portable launcher and shell
must work without this plugin and on Sway.

The coordinator will move this directory into its own repository and integrate
its package/configuration in Nix. No desktop configuration has been activated.

## Build

Requires Linux, a C++23 GCC toolchain (`-fno-gnu-unique`), make and pkg-config,
plus the **exact candidate compositor's** Hyprland development headers and
transitive dependencies: pixman, libdrm, pangocairo, libinput, libudev,
wayland-server, xkbcommon and Lua (reviewed headers use 5.5.0).

```sh
cd effects
make check
make check-deps
make check-abi
make -j1
make install PREFIX=/desired/staging/prefix
```

With Nix, the standalone flake pins the reference nixpkgs and upstream Hyprland:

```sh
nix build path:./effects
nix develop path:./effects -c make -C effects -j1  # when invoked from tohru root
```

From the extracted standalone repository use `nix build .` and
`nix develop -c make -j1`. Flake dev-shell cwd is the caller's cwd.
For the candidate session with existing compositor patches, call `package.nix`
with that exact compositor derivation rather than assuming equal version strings
are sufficient:

```nix
pkgs.callPackage ./effects/package.nix {
  hyprland = candidateHyprland;
}
```

No compositor is built or launched by `make`. Nix may need dependency builds;
coordinate them before invoking a build in a shared worker environment. The
current source has compiled against existing Hyprland 0.56.2 development outputs;
see VALIDATION.md for evidence and remaining gates. See UPSTREAM.md for the exact
source revision, API hash, licenses and local-patch caveat.

## Private candidate use

Only use a disposable compositor session with a private display/bus. Explicitly
select that instance for every command. **Never load this beside the old bundled
j2bar plugin, hyprfocus, hyprbars, hyprsheet or hyprrecede**: they share hooks,
config keys and rule/decorations/animation ownership. Keep the old desktop's
plugin selection intact. Nix owns permanent loading, rules and animation config.

After compositor startup, with no matching launcher layer open:

```sh
hyprctl -i "$TEST_INSTANCE" plugin load /absolute/path/libtohru-hyprland.so
hyprctl -i "$TEST_INSTANCE" desktop-effects-capabilities
hyprctl -i "$TEST_INSTANCE" eval "dofile('/absolute/path/effects/examples/effects.lua')"
# Exercise candidate clients and launcher, then unload in the same private instance:
hyprctl -i "$TEST_INSTANCE" plugin unload /absolute/path/libtohru-hyprland.so
```

`examples/effects.lua` supplies authored selected-effect defaults from j2bar,
including the app whitelist and exact effect animation curves/speeds. These are
source defaults, not a verified live configuration. It intentionally supplies
only these four effect settings/rules; global compositor styling stays in Nix.
The example uses the proposed new launcher namespace and must wait for the
user's namespace API gap decision: F confirmed Pleamar currently hardcodes
`pleamar` for layers. See LAUNCHER-CONTRACT.md.

## Settings and boundaries

Preserved native config groups are `plugin:hyprfocus`, `plugin:hyprbars`,
`plugin:hyprsheet` and `plugin:hyprrecede`; all existing keys and component defaults
remain in their source modules. The example records all selected settings and
custom animation leaves. Focus modes accept `none`, `flash`, `shrink`, `slide`
and comma combinations; render shrink holds client resize and never relayouts
the application's content. Focus/sheet/recession matching uses C++ ECMAScript
regex, while Hyprland window rules use RE2.

Hyprbars retains its upstream button API and rules `hyprbars:no_bar`,
`hyprbars:bar_color`, `hyprbars:title_color`. This compositor configuration API is
not a shell plugin system. Configured titlebars require a compositor exclusion
rule: the component's raw default enables bars for ordinary windows, whereas the
old shell authored whitelist is `^(Alacritty|mpv)$`. The example preserves that
whitelist with a negative rule. The built-in maximize action token remains
`j2bar:maximize`; empty disables double-click, other strings retain upstream
compositor command execution behavior. Do not pass the old typed word `maximize`
directly: it was translated by j2bar's former settings owner.

The old shell's TOML settings owner/transaction engine is not extracted. Nix
must translate retained settings into native compositor configuration; runtime
Settings UI integration is not supplied by this package. Default authored focus
settings (0.99 shrink, both keyboard/mouse, chooser exclusion) differ from raw
component defaults (0.95, keyboard flash/mouse none). Recession's raw default
namespace also differs from the former shell's broader launcher/polkit pattern.
Do not infer active desktop settings from either set of defaults.

Build compatibility and copied algorithms are checked; loading/unloading,
visual parity, damage, orientation/scaling and performance are **unverified for
the extracted package**. Follow VALIDATION.md before candidate acceptance.

Accepted review fixes retain focus restoration state through both animation legs
and restore geometry/opacity/config before releasing held client sizing on unload.
Sheet cleanup preserves the parent's previous optional noFocus set-property slot
and observable external false/unset writes. `make check` runs focused restoration
checks plus provenance; Nix checkPhase runs those checks alongside ABI rejection.
See UPSTREAM.md for intentional extraction changes and the pinned API's
indistinguishable same-value override-write limitation.
