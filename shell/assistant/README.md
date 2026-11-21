# Assistant

The AI assistant described in [docs/ai-assistant.md](../../docs/ai-assistant.md): a chat popup and a bar entry in the bar process, and an optional voice service. The provider CLIs do the talking; Tohru never handles their credentials.

## Files

| File | Contents |
|---|---|
| `state.plm` | Popup, picker, history and streaming facts; the `ai_providers`, `ai_turns` and `ai_history` models; the `assistant_*` events; the icons and the heights the popup springs between |
| `menu.plm` | The mark's right-click menu: the providers with **Sign in…** inline, New chat and History |
| `popup.plm` | The Pocket popup (direction 01): heading, provider picker, sign-in note, empty chat, the chat column, the field and history |
| `controller.luau` | Runs `tohru-assistant` for providers, chats and history, streams the answer, and keeps the bar's streaming and ready state. Takes `{provider, choose}` for the `assistant.provider` setting and returns `{open, close, toggle, new_chat, show_history, choose, signin, settle, ask, stop, streaming, on_answer, show, status}` |
| `*.svg` | Lucide icons (ISC; see `LICENSE.LUCIDE`) |

## The bar entry

The tinted dragon mark (`shell/brand/dragon.png`) in `services.plm`, left of the wallpaper button, behaving as direction 09:

- While an answer streams and the popup is closed, a glow in the provider's colour breathes behind the mark (a 1.6 s breath between 45 % and 100 %, fading in and out over 400 ms).
- When an answer is ready and unread, a 4 px glowing mark in that colour sits at its top right until the chat is opened.
- Right-click opens the menu (`menu.plm`).
- While the voice service has the microphone open, a small red dot sits at its foot (`ai_listening`).

It is the fourth button left of the bell and, like Bluetooth and the wallpaper button, is hidden when the output is too narrow; the information and About buttons move one place left.

## The popup

A 280 px panel under the bar's dragon button (`services.plm`), with the shared glass and hairline, kept 8 px from the output's edges. The surface is always 458 px tall so the glass never captures the panel's own earlier frames while it grows.

- **Heading:** the provider's dot (hollow when signed out) and name with the fold chevron. Clicking it unfolds the picker: each installed CLI with its command and model, or **Sign in** for one that is signed out. Choosing a signed-in one saves `assistant.provider`; choosing a signed-out one shows its sign-in note. The clock swaps to history and back; + starts a new chat.
- **Empty chat:** the mascot, "Ask anything" and the provider, model and sign-in state.
- **Chat:** questions on plates at the right, answers as plain markdown text under them, newest at the foot (`stick: end`). The answer in flight is a separate measured text whose last letter is the caret in the provider's colour. Under a finished answer, Copy (`wl-copy`) and Retry. A failed answer shows its error in red; a stopped one ends in "— stopped". The panel grows with the chat from 230 to 440 px.
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

A Python helper (standard library only) that the bar runs with `sys.spawn`. Each command prints JSON on stdout.

| Command | Output |
|---|---|
| `providers` | The installed CLIs among `claude`, `codex`, `gemini`, `agy` and `opencode`, in that order, with `slot` (their fixed palette position, 0 to 4), `signed_in` and `model`. Anything else on `PATH` is never listed |
| `signin ID` | Opens the CLI's login in a terminal, waits for the terminal to close, then prints that provider's line again |
| `chat --provider ID [--conversation CID] [--voice]` | Reads the prompt on stdin and prints `start`, `delta` (text), then `done`, `stopped` or `error` events, one per line |
| `history [--query Q] [--provider ID] [--since T] [--limit N]` | Conversations, newest first, with the matching snippet when `--query` is given |
| `show CID`, `delete CID` | One conversation, or remove it |

- **Auth probes** never prompt: stdin is closed, each probe has 6 s, and they run in parallel. `claude auth status` and `codex login status` where the CLI has one; otherwise the credential file (`~/.gemini/oauth_creds.json`, `~/.gemini/antigravity-cli/antigravity-oauth-token`, `~/.local/share/opencode/auth.json`). **Sign in…** runs `claude auth login`, `codex login`, `opencode auth login`, or the bare CLI for `gemini` and `agy`, in `TOHRU_TERMINAL`, `xdg-terminal-exec`, `$TERMINAL -e` or the first common terminal found.
- **Models** come from each CLI's own config (`~/.claude/settings.json`, `~/.codex/config.toml`, `~/.gemini/settings.json`, `opencode.json`); `agy` has none to read.
- **The runner** starts the CLI in an empty scratch directory (`$XDG_STATE_HOME/tohru/assistant/scratch`) in its own process group, with no tools where the CLI can turn them off: Claude gets `--tools ""`, `--strict-mcp-config` and only project settings (so no user hooks); Codex runs `--sandbox read-only` with no MCP servers; Antigravity runs in `plan` mode; Gemini keeps its default approval mode, which refuses tools in headless runs. Each conversation keeps one resumable session per provider (`--session-id`/`--resume`, `codex exec resume`, `--conversation`, `--resume`, `--session`). The first turn sent to a provider that joins a conversation part-way carries the earlier transcript.
- **Stop** is SIGINT to the helper, which passes it to the CLI's process group, saves the partial answer marked `stopped` and prints `stopped`.
- **History** is one JSON file per conversation in `$XDG_DATA_HOME/tohru/assistant/chats/` (mode 0700): `id`, `title` (the first question, up to 48 characters), `provider`, `created`, `updated`, `voice`, the CLI `sessions` and `turns` (`role`, `text`, `time`, and `provider`, `stopped`, `error` or `voice`). Writes are atomic. Search is case-insensitive over titles and every turn. `TOHRU_ASSISTANT_DATA` and `TOHRU_ASSISTANT_SCRATCH` move both folders (the tests use them).

Only `claude`, `codex` and `agy` were run for real while this was written. The Gemini and OpenCode event formats follow their documentation and are unverified.

## Tests

```sh
python3 tests/assistant/service.py
lua5.4 tests/assistant/controller.lua
```

The first drives the helper against fake `claude` and `codex` scripts: discovery, probes that would prompt, streaming, session resume, provider hand-over, Stop, history search and conversation IDs that try to leave the folder.

The second drives `controller.luau` with stand-ins for `run`, `spawn` and `kill`: provider choice, streaming, Stop, the ready mark, sign-in, history rows and a voice turn.
