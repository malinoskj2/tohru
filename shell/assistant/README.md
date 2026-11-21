# Assistant

The AI assistant described in [docs/ai-assistant.md](../../docs/ai-assistant.md): a chat popup and a bar entry in the bar process, and an optional voice service. The provider CLIs do the talking; Tohru never handles their credentials.

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
```

It drives the helper against fake `claude` and `codex` scripts: discovery, probes that would prompt, streaming, session resume, provider hand-over, Stop, history search and conversation IDs that try to leave the folder.
