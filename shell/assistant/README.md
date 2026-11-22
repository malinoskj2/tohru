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
| `dragon.plm` | The speaking dragon overlay at the bottom right, drawn from the sheets in `dragon/`; a click opens the conversation |
| `puppet.luau` | The dragon's animator: idle, listening, thinking and talking poses, transitions, blinks, flourishes, viseme mouths and hearts, at 24 fps while she is shown |
| `dragon/` | Her sheets, eye and mouth patches, heart and glass shader; layout and provenance in its README |
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
| `assistant_menu`, `assistant_menu_close`, `assistant_show_history` | Toggle or close the mark's menu; open the popup on history |

## The service: `bin/tohru-assistant`

A Python helper (standard library only) that the bar runs with `sys.spawn`. Each command prints JSON on stdout. `tohru assistant memory|export|import …` runs it too.

| Command | Output |
|---|---|
| `providers` | The installed CLIs among `claude`, `codex`, `gemini`, `agy` and `opencode`, in that order, with `slot` (their fixed palette position, 0 to 4), `signed_in` and `model`. Anything else on `PATH` is never listed |
| `signin ID` | Opens the CLI's login in a terminal, waits for the terminal to close, then prints that provider's line again |
| `chat --provider ID [--conversation CID] [--voice] [--model M] [--effort E] [--persona on\|off] [--persona-file F] [--memory on\|off] [--orca auto\|true\|false]` | Reads the prompt on stdin and prints `start` (with the model), `delta` (text), then `done`, `stopped` or `error` events, one per line |
| `remember CID [--model M] [--effort E] [--memory on\|off]` | Extracts memories from a finished conversation with one headless call; prints `remembered` with the count, or nothing to do |
| `memory list [--json]`, `memory forget ID`, `memory clear` | What is remembered, and dropping it |
| `export [FILE]`, `import FILE [--merge\|--replace --yes] [--only history\|memory\|relationship]` | Versioned JSON of everything, written 0600; import validates the whole file first |
| `history [--query Q] [--provider ID] [--since T] [--limit N]` | Conversations, newest first, with the matching snippet when `--query` is given |
| `show CID`, `delete CID` | One conversation, or remove it |

- **Auth probes** never prompt: stdin is closed, each probe has 6 s, and they run in parallel. `claude auth status` and `codex login status` where the CLI has one; otherwise the credential file (`~/.gemini/oauth_creds.json`, `~/.gemini/antigravity-cli/antigravity-oauth-token`, `~/.local/share/opencode/auth.json`). **Sign in…** runs `claude auth login`, `codex login`, `opencode auth login`, or the bare CLI for `gemini` and `agy`, in `TOHRU_TERMINAL`, `xdg-terminal-exec`, `$TERMINAL -e` or the first common terminal found.
- **Models** come from `assistant.models.<cli>.model` and `.effort`, with Tohru's defaults when empty (see [docs/ai-assistant.md](../../docs/ai-assistant.md#as-built)); `providers --models JSON` reports what each will use.
- **The runner** starts the CLI in an empty scratch directory (`$XDG_STATE_HOME/tohru/assistant/scratch`) in its own process group. Chat turns get full tool access with no approval prompts: Claude runs `--permission-mode bypassPermissions` with the user's MCP servers and only project settings (so no user hooks); Codex runs `--dangerously-bypass-approvals-and-sandbox`; Antigravity `--dangerously-skip-permissions`; Gemini `--approval-mode yolo`; OpenCode keeps its own configured permissions. Memory extraction stays tool-less (`--tools ""` and `--strict-mcp-config`, Codex `--sandbox read-only`, Antigravity `plan` mode). Each conversation keeps one resumable session per provider (`--session-id`/`--resume`, `codex exec resume`, `--conversation`, `--resume`, `--session`). The first turn sent to a provider that joins a conversation part-way carries the earlier transcript.
- **Orca context** (`--orca auto|true|false`, from `assistant.orca`): unless `false`, chat looks for the Orca CLI shim (`$ORCA_USER_DATA_PATH/linux-orca-cli-shim/orca`, `$XDG_CONFIG_HOME/orca/linux-orca-cli-shim/orca`, `~/.config/orca/linux-orca-cli-shim/orca`) or `orca-ide` on `PATH` (never bare `orca`). When found, it probes `<cli> status --json` (2 s timeout, cached 10 s in `$XDG_STATE_HOME/tohru/assistant/orca-status.json`), exports `ORCA_CLI_COMMAND` and `ORCA_USER_DATA_PATH` to the provider, and adds a short Orca prompt section pointing every provider at `<cli> skills get <name>` and `<cli> skills list`. Inherited `ORCA_*` terminal/worktree variables are stripped from provider processes, and Claude's Orca pane-status hooks are not loaded. Global agent skill directories are never modified automatically; to install Orca's discovery stubs into interactive CLI configs explicitly, run `<cli> skills install --skill orca-cli --skill orchestration --skill orca-computer-use --agent claude,codex,antigravity,gemini,opencode`.
- **Stop** is SIGINT to the helper, which passes it to the CLI's process group, saves the partial answer marked `stopped` and prints `stopped`.
- **History and memory** live in `$XDG_DATA_HOME/tohru/assistant.sqlite` (0600, WAL, FTS5). Conversations keep `title` (the first question, up to 48 characters), `provider`, `voice`, the CLI `sessions` and their turns. Search covers titles and every turn. `TOHRU_ASSISTANT_DATA` and `TOHRU_ASSISTANT_SCRATCH` move the data and scratch folders (the tests use them).

Only `claude`, `codex` and `agy` were run for real while this was written. The Gemini and OpenCode event formats follow their documentation and are unverified.

## The voice service: `bin/tohru-voice`

Python with numpy, onnxruntime and sherpa-onnx; misaki for Kokoro. It reads 16 kHz mono from the default source with `parec`, plays with `pacat`, and talks JSON lines: events on stdout (`ready` with the `dragon` sheet folder, `listening`, `conversation`, `wake`, `hearing` while follow-up speech is heard, `turn`, `barge`, `speaking`, `sentence` as each sentence starts, `mouth` with `level` and `viseme`, `paused`, `error`), commands on stdin (`say`, `hush`, `thinking`, `end`, `tts`). Models are explicit paths: `--vad-model`, `--asr-model`, `--turn-model` (optional), `--tts-model` and, for `--wake kws`, `--kws-model`, from the matching `assistant.*_model` settings. It checks them before loading anything and exits with code 3 and one `error` naming every missing setting; `ready` carries a `warning` for a missing Kokoro or a bad Smart Turn file. Nothing is downloaded. Kokoro runs in a child process (`--tts-worker`) so freeing it returns its memory. It pauses while the session is locked.

## Tests

```sh
python3 tests/assistant/service.py
python3 tests/assistant/voice.py
lua5.4 tests/assistant/controller.lua
lua5.4 tests/assistant/voice-controller.lua
lua5.4 tests/assistant/puppet.lua
```

`service.py` drives the helper against fake `claude`, `codex`, `agy`, `gemini` and `opencode` scripts: discovery, probes that would prompt, streaming, session resume, provider hand-over, Stop, history search, conversation IDs that try to leave the folder, models and rejected models, the persona per CLI, Orca CLI detection/caching/env isolation/prompt injection per CLI (including never resolving bare `orca`, Orca not running, and `--orca false`), memory extraction (malformed JSON rejected, the 600-token budget, nothing spawned with memory off), forgetting, migrations, and an export/import round trip. `voice.py` covers wake gating with and without the keyword spotter, follow-ups, spoken text shaping, the Kokoro load policy and the echo guard (the last needs numpy).

`controller.lua` drives `controller.luau` with stand-ins for `run`, `spawn` and `kill`: provider choice, models, streaming, Stop, the ready mark, sign-in, history rows, a voice turn, the memory hand-off and the Memory view. `voice-controller.lua` drives `voice.luau`: gating, turns, the spoken greeting for the wake word alone, spoken refusals, spoken answers, barge-in, the overlay, loading the sheets, settings and restarts. `puppet.lua` drives `puppet.luau`: poses, routes through the drawn moves, squash swaps, dips, visemes and hearts. `voice.py` also checks the phoneme-to-mouth mapping and timelines.
