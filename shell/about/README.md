# About

The About Tohru popup, a 312 px glass capsule matching the information popup: a header tile with the Mooncoil mark, the OS name and version and the version as a large thin number, a frosted tile of six system facts, the configuration repository and a More Info button. It runs in the bar process and is reached from the bar's About button, which `bar.about_button` turns on, or with `tohru panel toggle about`.

## Files

| File | Contents |
|---|---|
| `state.plm` | `about_visible`, the name, version, short version and error texts, the `about_specs` model and the `about_*` events |
| `popup.plm` | The 312×368 px capsule, centred horizontally 37 px from the top, on the same frosted glass as the information popup (`water.wgsl`, shared with the network and Bluetooth popups, at rest unfolded) |
| `controller.luau` | Fills the popup from a `tohru_info` snapshot. Returns `{open, close, toggle, refresh}` |
| `entry.plm`, `entry.luau` | A standalone scene with an About button, for development |

## How it is wired

`shell/bar.plm` includes `about/state.plm` and `about/popup.plm`. `shell/bar.luau` calls `require("about/controller")(tohru_info.snapshot, more)`, where `more(page)` closes the other popups and opens the information popup's overview, folded, with that page chosen. More Info, or Return, calls it with page 0, System.

| Event | Effect |
|---|---|
| `about_open`, `about_close`, `about_toggle` | Open, close or toggle. Opening takes a fresh snapshot |
| `about_refresh` | Takes a fresh snapshot |
| `about_more` | Opens the information popup's overview with System chosen |

The large number is the version's leading `major.minor` (`26.05` from `26.05 (Yarara)`), or "—". The six facts are Name (host name), Processor, Graphics, Memory, Startup disk and Configuration (the system's configuration revision). Startup disk is shown only when the device mounted at `/` is one of the reported block devices; otherwise it, like any fact the snapshot lacks, reads Unknown. The snapshot is read only when the popup opens or is refreshed.

The repository line, `github.com/malinoskj2/nix`, is selectable text, not a link.

## Running and tests

```sh
lua tests/info/controllers.lua
lua tests/info/overview.lua
pleamar --check shell/about/entry.plm
TOHRU_SHELL=1 pleamar --scene shell/about/entry.plm --answer-as tohru-about
```

Run the Lua test from the repository root with Lua 5.4. In the standalone scene More Info sends `emit info_open` to a separate `tohru-info` process, so start `TOHRU_SHELL=1 pleamar --scene shell/info/entry.plm --answer-as tohru-info` as well.

## Known limitations

When opened from the CLI, the popup closes with Escape or `tohru panel close`, not with a click outside it.
