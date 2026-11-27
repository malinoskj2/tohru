# Assistant

The AI assistant described in [docs/ai-assistant.md](../../docs/ai-assistant.md): a chat popup and a bar entry in the bar process, and an optional voice service. The provider CLIs do the talking; Tohru never handles their credentials.

## Files

| File | Contents |
|---|---|
| `state.plm` | Popup, picker, history and streaming facts; the `ai_providers`, `ai_turns` and `ai_history` models; the `assistant_*` events; the icons and the heights the popup springs between |
| `menu.plm` | The mark's right-click menu: the providers with **Sign in…** inline, New chat and History |
| `popup.plm` | The Pocket popup (direction 01): heading, provider picker, sign-in note, empty chat, the chat column, the field and history |
| `controller.luau` | Runs `tohru-assistant` for providers, chats and history, streams the answer, and keeps the bar's streaming and ready state. Takes `{provider, choose}` for the `assistant.provider` setting and returns `{open, close, toggle, new_chat, show_history, choose, signin, settle, ask, stop, streaming, on_answer, show, status}` |
| `voice.luau` | Runs `tohru-voice` while `assistant.voice` is on and kills it when off; sends voice turns to the controller and its answers back to be spoken; drives the mic dot, mouth level and overlay. Restarts a crashed service after 2, 4, … s, up to five times, and never after exit 3 (missing models or tools) |
| `needs-you.luau` | The Needs-you watcher: in Orca mode, runs `tohru-assistant needs-you poll` every `assistant.orca_needs_you_minutes` and `needs-you wake` for new items once no voice turn, speech or streaming answer is under way and neither DND nor the lock is on; backs off quietly while Orca is unreachable |
| `dragon.plm` | The speaking dragon overlay at the bottom right, drawn from the sheets in `dragon/`; a click opens the conversation, a right-click dismisses her |
| `puppet.luau` | The dragon's animator: idle, listening, thinking and talking poses, transitions, blinks, flourishes, viseme mouths and hearts, at 24 fps while she is shown |
| `dragon/` | Her sheets, eye and mouth patches, heart and glass shader; layout and provenance in its README |
| `screens.luau` | Runs `tohru-screens run` while she is on screen (`ai_dragon_open` or `ai_open`) and stops it when she goes; clears leftovers when the bar starts. Restarts a dead runner after 2, 4, … 64 s |
| `persona.md` | Tohru's persona, sent with every chat unless `assistant.persona` is off |
| `voice/` | `tohru-tide.bin`, Tohru's Kokoro voicepack (provenance in its README), and `wake-keywords.txt` for the keyword spotter |
| `*.svg` | Lucide icons (ISC; see `LICENSE.LUCIDE`) |

## The bar entry

The tinted dragon mark (`shell/brand/dragon.png`) in `services.plm`, left of the wallpaper button, behaving as direction 09:

- While an answer streams and the popup is closed, a glow in the provider's colour breathes behind the mark (a 1.6 s breath between 45 % and 100 %, fading in and out over 400 ms).
- When an answer is ready and unread, a 4 px glowing mark in that colour sits at its top right until the chat is opened.
- Right-click opens the menu (`menu.plm`).
- While the voice service has the microphone open, a small red dot sits at its foot (`ai_listening`).

It is the fourth button left of the bell and, like Bluetooth and the wallpaper button, is hidden when the output is too narrow; the information and About buttons move one place left.

## The popup

A 380 px panel docked down the right edge of the bar's monitor, from under the bar to 12 px above the foot, with the shared glass and hairline. The bar's own `screen.height` is its 42 px, so `state.plm` measures the monitor with `ai_screen_probe`, an empty 1 px background strip on the bar's outputs. `tohru assistant open|close|toggle` drive it from outside, and `tohru assistant show [ID]` opens a chat (the runner passes the current one to the provider as `TOHRU_CONVERSATION`).

- **Heading:** the provider's dot (hollow when signed out) and name with the fold chevron. Clicking it unfolds the picker: each installed CLI with its command and model, or **Sign in** for one that is signed out. Choosing a signed-in one saves `assistant.provider`; choosing a signed-out one shows its sign-in note. The clock swaps to history and back; + starts a new chat.
- **Empty chat:** the mascot, "Ask anything" and the provider, model and sign-in state.
- **Chat:** questions on plates at the right, answers as plain markdown text under them, newest at the foot (`stick: end`). The answer in flight is a separate measured text whose last letter is the caret in the provider's colour. Under a finished answer, Copy (`wl-copy`) and Retry. A failed answer shows its error in red; a stopped one ends in "— stopped". The chat scrolls inside the full-height panel.
- **Field:** always at the foot, "Ask Claude", "Reply to Claude", "Answering…" while it streams, when the send button becomes Stop. Return sends.
- **History:** a search field and the chats under Today, Yesterday, This week and Earlier with their provider's dot, time and a voice label; a search shows the matching line under the title. Up and Down move, Return opens.
- **Signed out:** "Gemini isn't signed in", **Sign in…**, which opens its login in a terminal and checks again when the terminal closes, and the command it runs.

Each provider's colour is fixed by its position in the known list (peach, yellow, green, lavender, mauve), from the active palette.

### Streaming and ready

The bar process owns the one answer in flight, so its state survives closing the popup: `ai_streaming` and `ai_stream_slot` while it streams, then `ai_ready` and `ai_ready_slot` when it finishes while the popup is closed or showing another chat. Opening the popup goes to that chat and clears `ai_ready`.

| Event | Effect |
|---|---|
| `assistant_open`, `assistant_close`, `assistant_toggle` | Open, close or toggle the popup |
| `assistant_picker`, `assistant_choose <row>`, `assistant_signin <row>` | Fold or unfold the picker; use a provider; sign in to one |
| `assistant_history`, `assistant_new` | Swap to history and back; start a new chat |
| `assistant_send`, `assistant_stop` | Send the field (or stop the answer streaming here); stop the answer in flight |
| `assistant_copy`, `assistant_retry` | Copy the last answer; send the last question again |
| `assistant_pick <row>`, `assistant_move <delta>`, `assistant_activate`, `assistant_search` | Open a history row; move in history or the picker; open the selected row; search again |
| `assistant_dragon`, `assistant_dismiss` | Click and right-click on the summoned dragon: open her conversation, or end the conversation and hide her at once |
| `assistant_menu`, `assistant_menu_close`, `assistant_show_history` | Toggle or close the mark's menu; open the popup on history |
| `assistant_alert <0\|1>` | Surface a proactive alert (`tohru assistant alert`): reload the matching open chat or light up `ai_ready`, and notify `voice.luau` to speak when `1` and neither locked nor in DND |

## The service: `bin/tohru-assistant`

A native helper (`desktop/src/assistant`) that the bar runs with `sys.spawn`. Each command prints JSON on stdout. `tohru assistant memory|export|import …` runs it too.

| Command | Output |
|---|---|
| `providers` | The installed CLIs among `claude`, `codex`, `gemini`, `agy` and `opencode`, in that order, with `slot` (their fixed palette position, 0 to 4), `signed_in` and `model`. Anything else on `PATH` is never listed |
| `signin ID` | Opens the CLI's login in a terminal, waits for the terminal to close, then prints that provider's line again |
| `chat --provider ID [--conversation CID] [--voice] [--draft] [--interrupted TEXT] [--model M] [--effort E] [--persona on\|off] [--persona-file F] [--memory on\|off] [--orca auto\|true\|false]` | Reads the prompt on stdin and prints `start` (with the model), `delta` (text), then `done`, `stopped`, `needs_tools` or `error` events, one per line. `--draft` runs tool-less and read-only on a forked/isolated session (`claude`, `codex` only), emits `needs_tools` when an action needs tools, and never writes to SQLite; `--interrupted` tells the model what was already spoken without changing stored user text |
| `commit --provider ID [--conversation CID] [--voice] [--session SID]` | Saves a committed speculative draft (`{prompt, answer}` JSON on stdin) and its forked session ID to SQLite without invoking a provider |
| `rollback CID` | Drops the latest unspoken user fragment and its reply from history (and deletes the conversation if empty) on voice continuation |
| `remember CID [--model M] [--effort E] [--memory on\|off]` | Extracts memories from a finished conversation with one headless call; prints `remembered` with the count, or nothing to do |
| `memory list [--json]`, `memory forget ID`, `memory clear` | What is remembered, and dropping it |
| `export [FILE]`, `import FILE [--merge\|--replace --yes] [--only history\|memory\|relationship]` | Versioned JSON of everything, written 0600; import validates the whole file first |
| `history [--query Q] [--provider ID] [--since T] [--limit N]` | Conversations, newest first, with the matching snippet when `--query` is given |
| `show CID`, `delete CID` | One conversation, or remove it (UUIDs supported in `orca` mode) |
| `sync` | Lightweight background check for new Orca bot replies (`orca` mode); no-op in `local` mode |
| `needs-you poll`, `needs-you wake UID:EVENT…` | `poll` reads the bot's Needs you items and prints the ones not yet announced (`ok`, `baseline`, `pending`); the first poll only records what is waiting. `wake` sends the given pending items to the bot's Home Chat in one `[tohru-desktop:needs-you]` message and marks them announced. State is `orca-needs-you.json` in the data folder (`orca` mode only) |
| `orca init [--bot NAME] [--no-migrate] [--force-soul]` | Seeds `~/.orca/bots/<slug>/SOUL.md` and migrates SQLite memories into `MEMORY.md` without altering `assistant.sqlite` |

- **Auth probes** never prompt: stdin is closed, each probe has 6 s, and they run in parallel. `claude auth status` and `codex login status` where the CLI has one; otherwise the credential file (`~/.gemini/oauth_creds.json`, `~/.gemini/antigravity-cli/antigravity-oauth-token`, `~/.local/share/opencode/auth.json`). **Sign in…** runs `claude auth login`, `codex login`, `opencode auth login`, or the bare CLI for `gemini` and `agy`, in `TOHRU_TERMINAL`, `xdg-terminal-exec`, `$TERMINAL -e` or the first common terminal found.
- **Models** come from `assistant.models.<cli>.model` and `.effort`, with Tohru's defaults when empty (see [docs/ai-assistant.md](../../docs/ai-assistant.md#as-built)); `providers --models JSON` reports what each will use.
- **The runner** starts the CLI in an empty scratch directory (`$XDG_STATE_HOME/tohru/assistant/scratch`) in its own process group. Chat turns get full tool access with no approval prompts: Claude runs `--permission-mode bypassPermissions` with the user's MCP servers and only project settings (so no user hooks); Codex runs `--dangerously-bypass-approvals-and-sandbox`; Antigravity `--dangerously-skip-permissions`; Gemini `--approval-mode yolo`; OpenCode keeps its own configured permissions. Memory extraction stays tool-less (`--tools ""` and `--strict-mcp-config`, Codex `--sandbox read-only`, Antigravity `plan` mode). Each conversation keeps one resumable session per provider (`--session-id`/`--resume`, `codex exec resume`, `--conversation`, `--resume`, `--session`). The first turn sent to a provider that joins a conversation part-way carries the earlier transcript.
- **Orca context** (`--orca auto|true|false`, from `assistant.orca`): unless `false`, chat looks for the Orca CLI shim (`$ORCA_USER_DATA_PATH/linux-orca-cli-shim/orca`, `$XDG_CONFIG_HOME/orca/linux-orca-cli-shim/orca`, `~/.config/orca/linux-orca-cli-shim/orca`) or `orca-ide` on `PATH` (never bare `orca`). When found, it probes `<cli> status --json` (2 s timeout, cached 10 s in `$XDG_STATE_HOME/tohru/assistant/orca-status.json`), exports `ORCA_CLI_COMMAND` and `ORCA_USER_DATA_PATH` to the provider, and adds a short Orca prompt section pointing every provider at `<cli> skills get <name>` and `<cli> skills list`. Inherited `ORCA_*` terminal/worktree variables are stripped from provider processes, and Claude's Orca pane-status hooks are not loaded. Global agent skill directories are never modified automatically; to install Orca's discovery stubs into interactive CLI configs explicitly, run `<cli> skills install --skill orca-cli --skill orchestration --skill orca-computer-use --agent claude,codex,antigravity,gemini,opencode`.
- **Stop** is SIGINT to the helper, which passes it to the CLI's process group, saves the partial answer marked `stopped` and prints `stopped`.
- **History and memory** live in `$XDG_DATA_HOME/tohru/assistant.sqlite` (0600, WAL, FTS5). Conversations keep `title` (the first question, up to 48 characters), `provider`, `voice`, the CLI `sessions` and their turns. Search covers titles and every turn. `TOHRU_ASSISTANT_DATA` and `TOHRU_ASSISTANT_SCRATCH` move the data and scratch folders (the tests use them).

Only `claude`, `codex` and `agy` were run for real while this was written. The Gemini and OpenCode event formats follow their documentation and are unverified.

## The voice service: `tohru-voice`

Rust ([`voice/`](../../voice)), installed as `tohru-voice`, with sherpa-onnx and ONNX Runtime. It captures 16 kHz mono with `parec` (from any `echo-cancel` source, or `TOHRU_VOICE_SOURCE`, else the default source), plays with `pacat` (to any `echo-cancel` sink, or `TOHRU_VOICE_SINK`, else the default sink), and talks JSON lines: events on stdout (`ready` with the `dragon` sheet folder, `listening`, `conversation`, `wake`, `hearing` once follow-up speech yields words (never for a wordless onset or a barge-in backchannel), `draft`, `draft_cancel`, `turn`, `settled` once the owner has not carried on within 352 ms of quiet after a turn (held while new speech decides, at most 2.5 s), `barge` with optional `spoken` or `continuation`, `speaking` with optional `paused` or `interrupted`, `sentence` as each sentence starts, `mouth` with `level` and `viseme`, `paused`, `error`), commands on stdin (`say`, `hush`, `draft`, `thinking`, `end`, `tts`). Models are explicit paths: `--vad-model`, `--asr-model`, `--turn-model` (optional), `--tts-model` and, for `--wake kws`, `--kws-model`, from the matching `assistant.*_model` settings. It checks them before loading anything and exits with code 3 and one `error` naming every missing setting; `ready` carries a `warning` for a missing Kokoro or a bad Smart Turn file. Nothing is downloaded. The high-VRAM profile (`assistant.voice_profiles.high`) runs `tohru-voice-qwen3-worker` under `gpu-run` while Tohru is on screen, over a private Unix socket, and unloads it after `hidden_unload_seconds`. It pauses while the session is locked.

## Tests

```sh
cargo build --release --manifest-path desktop/Cargo.toml
cargo test --manifest-path desktop/Cargo.toml
python3 tests/assistant/service.py
python3 tests/assistant/discord.py
cargo test --manifest-path voice/Cargo.toml
lua5.4 tests/assistant/controller.lua
lua5.4 tests/assistant/voice-controller.lua
lua5.4 tests/assistant/needs-you.lua
lua5.4 tests/assistant/puppet.lua
lua5.4 tests/assistant/screens.lua
```

`service.py` drives the native helper through `bin/tohru-assistant` against fake `claude`, `codex`, `agy`, `gemini` and `opencode` scripts: discovery, probes that would prompt, streaming, session resume, provider hand-over, Stop, history search, conversation IDs that try to leave the folder, models and rejected models, the persona per CLI, Orca CLI detection/caching/env isolation/prompt injection per CLI (including never resolving bare `orca`, Orca not running, and `--orca false`), memory extraction (malformed JSON rejected, the 600-token budget, nothing spawned with memory off), forgetting, migrations, and an export/import round trip. `cargo test` in `voice/` covers wake gating with and without the keyword spotter, follow-ups, spoken text shaping, the Kokoro load policy, sink/source selection, AEC flat barge threshold, adaptive echo coupling, pause-then-resume/barge, cancel-while-thinking the echo guard and the high-VRAM profile selector and worker socket.

`controller.lua` drives `controller.luau` with stand-ins for `run`, `spawn` and `kill`: provider choice, models, streaming, Stop, the ready mark, sign-in, history rows, a voice turn, the memory hand-off and the Memory view. `voice-controller.lua` drives `voice.luau`: gating, turns, the spoken greeting for the wake word alone, spoken refusals, spoken answers, paused/interrupted speaking, barge-in and cancel-while-thinking, the overlay, loading the sheets, settings and restarts. `puppet.lua` drives `puppet.luau`: poses, routes through the drawn moves, squash swaps, dips, visemes, paused/interrupted speech and hearts. The voice crate also checks the phoneme-to-mouth mapping and timelines.

## Screen captures: `tohru-screens`

While she is on screen, the summoned dragon or the chat popup, the bar runs `tohru-screens run` ([`desktop/src/screens.rs`](../../desktop/src/screens.rs)). It captures every monitor with `grim -o NAME -t jpeg -q 85` at once and then every 3 s, into `$XDG_RUNTIME_DIR/tohru-screens/` (tmpfs; folders 0700, files 0600):

| Path | Contents |
|---|---|
| `<output>/latest.jpg` | Symbolic link to that monitor's newest frame |
| `<output>/<ms>-<serial>.jpg` | The last 10 frames of that monitor, older ones deleted |
| `all/` | Used instead when `wayland-info` names no output: one combined image |
| `index.json` | `monitors`: each `output`, `latest`, `captured_ms` and `frames` |

Outputs are read with `wayland-info` at the start, every 20 captures and after a failed capture; an unplugged monitor's folder is deleted. Nothing is captured while the session is locked or inactive (Tohru's lock cycle endpoint, or logind's `LockedHint`/`Active` when `XDG_SESSION_ID` is set), and every capture is deleted when the lock is seen. Nothing is uploaded, sent or logged.

Every capture is deleted when she goes (SIGTERM), when the bar closes its stdin or exits, and by a reaper process outside the runner's process group when the runner is killed outright. `tohru-screens clear` at bar start and every new runner remove what a lost reaper left. One runner owns the folder through `tohru-screens.lock` beside it; a new one stops the old one first. The folder itself stays, so it can be bind-mounted.

`tohru-assistant chat` adds a `[Screen captures: …]` note with each monitor's `latest.jpg` and age to the system prompt, and Orca turns carry the same note, so a question about the screen reads these instead of a new screenshot; `persona.md` and the Orca soul addendum fall back to `grim` when there are none. The agent sandbox launcher in `~/nix` mounts the folder read-only at the same path into Tohru's bot sandbox only, so her Home turns read these frames; a bot on another machine falls back to a screenshot of the desktop. The folder must be on tmpfs or ramfs: the runner refuses a persistent disk, including a `TOHRU_SCREENS_DIR` override.

Only the summoned dragon and the popup count. The AFK scenes (`ai_actor_open`) and the standalone sheet turn (`ai_turn_shown`) also draw her, but play while the owner is away, so they take no captures. A stop or a lock during a capture ends it at once: the grab in flight is killed and nothing more of that round is kept.

## Streaming voice diagnostics

See [voice-pipeline.md](../../docs/voice-pipeline.md) for protocol/lifecycle details,
source selection, AEC provenance, journal commands and reproducible benchmarks.
`controller.on_speech` publishes ordinary committed start/delta events separately
from terminal `on_answer` notifications; speculative drafts never publish deltas to
speech. The voice service inherits stderr into the bar's existing journal stream.
Live wake-accepted transcript text is transient UI state above the dragon.

Optional built-in [Discord bot support](../../docs/discord.md) reads bounded channel history and posts through the provider CLI's shell tool. Configure `assistant.discord.*` in Tohru, default off; one explicit valid `allowed_user_id` is required before any Discord API access, and history returns only that author's messages. No MCP, Orca or Nix service is required.

### Desktop entrances

**Contextual dragon entrances** are enabled by default. Settings → Appearance controls them; an explicit saved or managed `false` remains respected. To enable them again or choose their deterministic sequence, use the native settings owner:

```sh
tohru config set assistant.contextual_entrances true
tohru config set assistant.entrance_seed 12345
tohru config set assistant.entrance_variant auto
```

An eligible fresh appearance can use a short rise from below, a glide from the right, a sweep from the lower left, or a window wrap. The controller selects once, after geometry checks, with weights 3, 2, 1 and 4 respectively. The last route is excluded in automatic mode. At least 30 seconds separate starts. There are no idle excursions. The seed accepts integers from 0 through 2147483646. Use `rise`, `right`, `sweep` or `wrap` in place of `auto` to force an eligible route. An ineligible forced route gives an ordinary appearance. Managed settings follow the same runtime override rules as reduced motion. Reducing motion keeps the compact character and its normal short visibility transition.

The PRNG uses the configured seed and the per-VM eligible event ordinal. Restarting the bar resets the ordinal and history. The last selection can be read with `pleamar --say tohru 'get ai_flight_trace'`. It records seed, ordinal, previous variant, chosen variant and scene geometry. The pure `flight-path.luau` sampler returns a sprite center in output-local logical pixels, heading in radians, bank in degrees, normalized phase and altitude blend. It has no sprite, speech or compositor dependency.

The bar supplies the existing native output allow-list, including its preferred-output setting. Before an entrance, the flight controller chooses the focused output if eligible and otherwise the first eligible output. It pins one named Pleamar surface there for the appearance. A focus change leaves a visible dragon in place. Unplugging her output releases the old path and settles her on a surviving output. When output names are unavailable she uses the ordinary compact presentation.

On Hyprland, the read-only helper matches the mapped flight layer's namespace and parent Pleamar PID in the compositor's per-output layer inventory, then joins that output name to its monitor ID, scale, transform, workspace and global origin. Window coordinates stay logical. Rotation swaps physical mode axes before dividing by scale. A wrap requires that the focused, visible window belongs to the verified monitor and workspace, has corner clearance, and leaves the landing site clear. Identically sized outputs remain distinguishable. The helper polls every 180 ms during an entrance, caps each reply at 256 KiB and scans at most 256 clients. Missing or ambiguous layer ownership, foreign-toplevel-only compositors and unsuitable rectangles use edge routes. Known fullscreen and special-workspace reports suppress decorative travel.

The first appearance during the existing three-second sheet warmup stays stationary. Once warm, a flight synchronously selects the direction-correct registered body key and holds the center for 200 ms before travel. The puppet cycles twelve registered wing frames while airborne. Rightward sweeps and wraps use the right-facing glide, right-edge arrivals face left, and ascents use the upward-right key. Paths maintain that horizontal heading through deceleration. At 80% of travel the wings enter a 760 ms registered turn directly to the current conversational posture. Each direction has a landing for pondering, attentive listening and neutral hover. The puppet stages the actual destination key under the last drawing and resumes there. A new utterance interrupts the entrance. The static landing atlas and its per-frame rectangles are generated by `tools/dragon/landing.py`.

### Sheet-B turn

`dragon/turn-sheet-b.webp` holds the Sheet-B model's turn, 12 × 10 cells of 240 px drawn at 166 logical px: cells 0–59 turn from profile right to the front, 60–119 turn back, at 24 fps. `sheet-turn.luau` plays it. For development, `tools/anim-debug sheet-b-turn` shows the whole turn and back on its own, without the illustrated keys, then releases the sheet. With `assistant.sheet_b_turn = true` (default `false`) the right glide's landing uses the first half instead of its `landing.webp` strip, stretching the landing to 2.5 s, and a right-click dismiss plays the second half before she hides. The other body art is still the illustrated model, so those two turns cut between models at their ends; the setting is for when the rest of the dragon moves to Sheet B. The sheet binds only while one of them can play, and one transparent pixel of it stays drawn while bound so it is decoded before a landing needs it.

For the wrap, the actor travels inside one native surface below applications throughout the flight. Hyprland composites the actual windows, including their corner shape and translucency; overlapping applications can hide the character too. Only an unobstructed landing is eligible. The same surface returns to overlay at the idle position, with no actor transfer on landing. Layer-shell provides broad levels, so there is no way to choose a precise slot between two application windows. Decorations extending beyond client geometry, other layer-shell overlays, and a window changing between IPC samples can affect the result. Only the character hit rectangle accepts pointer input and the surface requests no keyboard. A new utterance, interaction, DND, geometry or topology change abandons the route; its brief visibility transition brings the actor back to the conversation. Session lock hides normal surfaces at the compositor. The Tohru locker endpoint also suppresses motion during a lock cycle. Third-party lock/unlock transitions have no portable notification in this adapter.

#### Reproducing the native fixtures

From the checkout, run the following with Lua 5.4 and Python 3. The recording command uses a sandbox with its private nested Hyprland output set to 1920×1080, a built Tohru Pleamar 0.7.27 executable, foot, jq and wf-recorder. Set `FLIGHT_RUNTIME` to that executable and `FLIGHT_RECORDER` if wf-recorder is outside PATH. Use the site's resource admission wrapper on Slurm.

```sh
lua tests/assistant/flight.lua
lua tests/assistant/flight-voice.lua
cargo test --manifest-path desktop/Cargo.toml flight_scene
python3 tests/assistant/flight-demo.py --voice
"$FLIGHT_RUNTIME" --check shell/bar.plm
"$FLIGHT_RUNTIME" --check .tmp/flight-voice-demo/flight.plm
sandbox-display up --hyprland
gpu-run --vram 2GiB --name "Native entrance recordings" -- tests/assistant/record-flight.sh
```

The recorder prints a unique worktree-local evidence directory. It runs the real voice controller, puppet, renderer, path sampler and IPC adapter. A fixture-only fake voice executable delivers service events over a FIFO. The private terminal moves or closes in the geometry interruption cases. The hearing and speaking cases interrupt active flight with voice service events and assert the actor is back on the conversational surface. `cold-warm.mp4` shows a first appearance during sheet warmup and a subsequent eligible wrap in the same process. Other recordings start fresh processes to preserve the production cooldown. Each has a frame timeline, geometry selection and layer snapshot. `FLIGHT_OUTPUT=FLIGHT-PORTRAIT` selects the portrait fixture window and recorder output when that private display exists. The native focused-window provider supplies the focus name; the harness supplies the output allow-list from the private monitor inventory. The full bar gets this list from its native output subscription. No microphone or model is used.

### Borrowing the desktop actor

The bar constructs `flight.actor` from `assistant/flight-actor`. The caller supplies idle eligibility, resume/input/lock notifications and the episode timeline. A lease can run with contextual entrances and voice disabled. It uses the same pinned, keyboard-free surface as an entrance and has an empty pointer region throughout external playback. No second input surface, voice service or model is required.

* `actor.acquire(ready, stopped)` returns a numeric generation or nil when busy. It maps the selected output and warms the shared artwork. `ready(token, scene)` arrives asynchronously. `stopped(token, reason)` runs once after release, preemption or timeout. Rejected and stale tokens cannot affect the actor. Allow up to five seconds for acquisition and at most 55 seconds total for an episode.
* `scene.w/h` are output-local logical dimensions. `output` is the selected name. Verified Hyprland snapshots additionally expose `output_id, ox, oy, scale, rotation, workspace` and an optional focused `target={id,x,y,w,h}` in those same local coordinates. `fullscreen, inhibited, session_active, unlocked` are tri-state booleans, with nil meaning unknown. This provider reports fullscreen from Hyprland and active/locked state from the named logind session when available. Compositor idle inhibition needs the caller's idle source. Known lock or inactive session prevents playback. The caller must also release synchronously when its own activity, session or inhibition source changes.
* `actor.play(token, sample)` returns `true,"preparing"` or `true,"ready"`. Submit the initial sample repeatedly at its fixed start position and advance the scene clock only once ready. The sample supplies `x,y` in logical pixels at the sprite center, `pose`, and optional `opacity, airborne` (0–1), `bank` (±90 degrees), `rotation` (±360 degrees), `scale` (0.05–12), and boolean `behind`. A behind sample needs a verified target and uses the compositor bottom layer beneath ordinary windows. First pose preparation takes 200 ms. Width, height, source generation, output, window changes and activity invalidate the lease. Invalid samples return false and stop playback.
* Poses include `fly-left`, `fly-right`, `fly-up` and registered puppet keys such as `nested_hover`. Before acquiring, `actor.register_skin(absolute_path, rows)` accepts an 8-column square-cell sheet with 1–64 rows. Use `pose="skin", skin_frame=0..8*rows-1`. `actor.register_closeup(absolute_path)` registers a separate square image used by `pose="closeup"`. Pleamar decodes these sources at their own resolution up to its texture limit. Registration stores a path without binding the texture. Only the current supplemental pose binds its image. Switching away or releasing the lease clears the live binding and lets the runtime reclaim its cached texture. Each new supplemental binding holds position at near-zero opacity for three seconds, so the caller must pause its clock during preparation. This is a conservative warm interval. The pinned runtime exposes no GPU readiness notification. Dynamic fog, bone articulation and gaze need artwork or renderer support supplied by their owner.
* `actor.release(token)` returns true only for the current generation and removes its presentation immediately. Voice, speech/hearing, DND, fullscreen, output loss and compositor scene changes take priority. The observer checks Hyprland at 180 ms intervals and logind about once per second. The caller can preempt earlier via release. No idle policy starts from the actor itself.

A ready lease can select its artwork using the verified scene. Pass its token as the last argument to `register_skin(path, rows, token)` or `register_closeup(path, token)` before its first play. Stale tokens and registration after preparation starts are rejected.

Use `acquire(ready, stopped, {quiet=true})` for a stationary rest when reduced motion is enabled. It accepts a single fixed `neutral`, `nested_hover`, `reclined_hover` or skin frame on the output. Only opacity can change. Scale is limited to 1, with zero flight, rotation, banking, airborne shadow offset and occlusion. Movement or a new pose invalidates the quiet lease. Voice and activity retain priority.
