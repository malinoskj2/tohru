# Native output sound checkpoint

Root scene `tohru` uses pinned Pleamar declarative `service audio` and direct
`audio.step`, `audio.volume`, `audio.mute` commands. The declarative service keeps
one subscription through scene/logic reload; this avoids creating another raw
`sys.watch` thread every VM replacement. It observes ordinary audio events for
the bar; no high-rate `audio.level`/microphone meter or closed-panel stream starts.

The bar sound icon opens a native xdg popup. Native placement anchors its 300px
panel at the sound button center minus150px, 6px below the31px bar; native slide
constraints keep it onscreen. Sound header uses j2bar15px/600 type,6px padding,
40px header and30px row; SVG speaker paths are from j2bar's OSD speaker with its
MIT notice retained. Mocha text/sky palette follows the source. This is a native
glass/interaction candidate, not accepted1:1 graphics/motion or glass parity.
Per-app/input/device controls remain subsequent sound scope.

Public CLI, reaching only an already running root scene:

```sh
tohru panel toggle sound
tohru panel open sound
tohru panel close
tohru audio volume up
tohru audio volume down
tohru audio mute toggle
tohru audio status --json
```

Volume keys move5%, preserve mute, and native service clamps writes to0..1.
Slider writes are bounded0..1, coalesce at most one pending50ms timer, flush the
last value on release/close, and never poll. Observations stay authoritative;
status distinguishes awaiting_report from last_report, missing from zero/unmuted,
and command errors from successful native calls. The status is a last observation,
not a promise of live server health; pinned native audio has no disconnect status.
No volume key claims asynchronous system completion merely from CLI acceptance.

Tab selects mute/slider; arrows adjust5%, Return/Space on mute toggles, Escape
closes. Parent asks exclusive keyboard only while panel is open and releases it
on close. Native pointer-open popup grabs use a recent real pointer serial;
outside click then follows compositor popup_done. CLI-open without recent pointer
serial has no native outside grab: this remains an explicit pending user decision,
not claimed as parity. No layer catcher, serial workaround or native patch added.

User accepted native every-output bars, superseding preferred single-output
policy. Root now uses screens:all, which also permits native root popup compilation;
no singleton-each compiler fix. bar.preferred_output is deprecated/inert legacy
configuration; preserve old TOML instead of deleting it. No fallback selector.

Exact OSD liquid/slosh (44x200,1600ms hold) and panel liquid-meter visual parity
remain separate verification/decision work. No simplified OSD or custom Slosh
implementation is substituted. The frame API gap remains independent.

CPU tests in tests/sound/logic.rs execute the real controller using mlua at the
native-call/timer boundary; GUI/native service/input tests require an exclusive
private compositor/GPU slot and private PulseAudio null sink, never live audio.
