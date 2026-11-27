# Screen captures

While Tohru is on screen, the bar keeps rolling captures of every monitor in
`$XDG_RUNTIME_DIR/tohru-screens/` so a question about the screen reads them
instead of taking a new screenshot. They must exist only while she is shown.

## Sub-features

- `appear`: the chat popup or the summoned dragon starts `tohru-screens run`;
  one capture per monitor at once, then every 3 s, at most 10 per monitor.
- `hide`: closing the popup or dismissing the dragon deletes every capture.
- `recovery`: a killed runner is restarted while she is shown, leaving no
  earlier frames; a killed bar leaves an empty folder.
- `lock`: nothing is captured while the session is locked, and the lock deletes
  what was there.
- `agent`: `tohru-assistant chat` and Orca turns carry a `[Screen captures: …]`
  note naming each monitor's `latest.jpg`.

## How to get to it (user POV)

- Summon her by voice, or run `tohru assistant open`; close with
  `tohru assistant close`, a right-click on the dragon, or by letting her go.
- Ask "what's on my screen?"; she reads `<output>/latest.jpg`.

## Driving it with the desktop driver

Preconditions: baseline private native instance and runtime/GPU lease from the
[index](README.md#desktop-driver), a writable `/dev/shm` (the driver puts the
captures there through `TOHRU_SCREENS_DIR`), and `wayland-info` on `PATH` so frames are
filed under `HEADLESS-1` rather than `all`.

- **Journey.** `tools/verify/desktop.py run --feature screens --runtime "$verify_runtime"`
  drives `tohru assistant close|open|close`, then SIGKILLs the runner while open
  and checks it restarts without the earlier frames, then SIGKILLs the bar and
  checks the folder empties. `screens-open.json`, `screens-closed.json` and
  `screens-after-bar-exit.json` list paths, modes and the `latest.jpg` links.
  Images are never copied into evidence.
- **Lifecycle tests.** `cargo test --manifest-path desktop/Cargo.toml --test screens`
  covers start, ring buffer, outputs, lock purge, stop, stdin EOF, SIGTERM,
  SIGKILL reaping, `clear`, runner takeover, a stop during a slow grab, a lock
  in the middle of a round, a stop during stalled output discovery, a lock
  during a slow grab with frames already published, and refusing a persistent
  disk, with fake
  `grim`/`wayland-info`.
  `lua5.4 tests/assistant/screens.lua` covers the bar's start/stop and back-off.

## Unverified

- The summoned dragon (`ai_dragon_open`) needs the voice service; only the popup
  path is driven.
- The lock gate needs a real logind session or the native locker; it is covered
  only by the fake gate in the lifecycle tests.
- Whether a provider actually reads the captures is not checked, nor Tohru's
  bot sandbox reading them through the `~/nix` read-only mount.
