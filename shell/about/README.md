# About

The About Tohru popup: the Mooncoil mark, the OS name and version, six system facts, a More Info button and the configuration repository. It runs in the bar process and is reached from the bar's About button, which `bar.about_button` turns on, or with `tohru panel toggle about`.

## Files

| File | Contents |
|---|---|
| `state.plm` | `about_visible`, the name, version and error texts, the `about_specs` model and the `about_*` events |
| `popup.plm` | The 360×430 px popup, centred horizontally 60 px from the top |
| `controller.luau` | Fills the popup from a `tohru_info` snapshot. Returns `{open, close, toggle, refresh}` |
| `entry.plm`, `entry.luau` | A standalone scene with an About button, for development |

## How it is wired

`shell/bar.plm` includes `about/state.plm` and `about/popup.plm`. `shell/bar.luau` calls `require("about/controller")(tohru_info.snapshot, more)`, where `more(page)` closes the other popups and opens the information popup on that page. More Info calls it with page 0, System.

| Event | Effect |
|---|---|
| `about_open`, `about_close`, `about_toggle` | Open, close or toggle. Opening takes a fresh snapshot |
| `about_refresh` | Takes a fresh snapshot |
| `about_more` | Opens the information popup on its System page |

The six facts are Name (host name), Processor, Graphics, Memory, Startup disk and Configuration (the system's configuration revision). Startup disk is shown only when the device mounted at `/` is one of the reported block devices; otherwise it, like any fact the snapshot lacks, reads Unknown. The snapshot is read only when the popup opens or is refreshed.

The repository line, `github.com/malinoskj2/nix`, is selectable text, not a link.

## Running and tests

```sh
lua tests/info/controllers.lua
pleamar --check shell/about/entry.plm
TOHRU_SHELL=1 pleamar --scene shell/about/entry.plm --answer-as tohru-about
```

Run the Lua test from the repository root with Lua 5.4. In the standalone scene More Info sends `emit info_open 0` to a separate `tohru-info` process, so start `TOHRU_SHELL=1 pleamar --scene shell/info/entry.plm --answer-as tohru-info` as well.

## Known limitations

When opened from the CLI, the popup closes with Escape, its close button or `tohru panel close`, not with a click outside it.
