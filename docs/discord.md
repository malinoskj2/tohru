# Discord bot integration

Tohru can read text-channel history and send plain text as **your bot**, when you ask in a normal assistant conversation. This is optional and off by default. It uses Discord HTTPS REST API v10 directly, Python 3.11+ and its standard library, and the installed assistant provider CLI's shell tool. It needs no Nix configuration, MCP server, Orca bot, connector, Discord SDK or background service. The release bundle and source checkout keep the helper and module together in `bin/`; the repository Nix package installs the same files.

## Set up your bot

1. Create an application in the [Discord Developer Portal](https://discord.com/developers/applications). Open **Bot**, create/reset its bot token, and store the raw token in a private file. Do not use your personal Discord account token, OAuth client secret or a webhook URL.
2. To read ordinary message text, enable **Message Content Intent** on the Bot page and obtain approval if Discord requires it for your application. This restriction also affects REST message content. Posting alone needs no privileged intent. Presence and Server Members intents are unnecessary. See Discord's [Message Content guidance](https://docs.discord.com/developers/events/gateway#message-content-intent).
3. Under OAuth2 URL Generator (or Installation), select **Guild Install** and the **bot** scope, then invite the bot to the server you control. This integration does not need `applications.commands` or slash commands. See [Discord bot authorization](https://docs.discord.com/developers/topics/oauth2#bot-authorization-flow).
4. Grant **View Channel** and **Read Message History** for reading; add **Send Messages** for posting. For threads, grant **Send Messages in Threads** and ensure the bot has access to the thread. Avoid Administrator. Channel overrides can deny permissions granted by the server role. See [Discord permissions](https://docs.discord.com/developers/topics/permissions).
5. Enable Developer Mode in Discord's Advanced settings, then copy the one allowed user's ID, the server ID and the specific channel IDs. Use **Copy User ID** on that user's profile; names and nicknames are not IDs. Threads have their own IDs and must be explicitly allowlisted. Tohru never discovers or fills in the user ID from messages, account lookups or guild membership.

Missing Read Message History can yield an empty list; missing Message Content access can yield messages with empty content. An empty result alone does not prove that the channel is empty. See [Get Channel Messages and message objects](https://docs.discord.com/developers/resources/message#get-channel-messages).

## Configure Tohru

Use the normal `$XDG_CONFIG_HOME/tohru/config.toml` (default `~/.config/tohru/config.toml`). Keep the token in a separate file owned by your user, mode `0600` or `0400`; symlinks and group/world access are rejected. Create it with a private editor, never by putting a token in shell arguments or history:

```sh
install -d -m 700 ~/.config/tohru
(umask 077; touch ~/.config/tohru/discord-token)
chmod 600 ~/.config/tohru/discord-token
${EDITOR:-vi} ~/.config/tohru/discord-token
```

The file contains only the raw token on one line. Example configuration (IDs are placeholders):

```toml
[assistant.discord]
enabled = true
read = true
post = false
desktop_first = false
desktop_timeout_seconds = 60
allowed_user_id = "456789012345678901"
token_file = "/home/me/.config/tohru/discord-token"
guild_id = "123456789012345678"
channels = "234567890123456789,345678901234567890"
```

All booleans default to `false`; all strings default to empty. The desktop timeout defaults to 60 seconds. One explicit `allowed_user_id`, a guild and 1–32 explicit channel IDs are required. Missing, blank or malformed `allowed_user_id` disables all Discord functionality, including posting and channel metadata requests, before any API access; ordinary chat and voice keep working. The native setting accepts an empty string to clear setup, or a canonical nonzero unsigned 64-bit decimal ID (ASCII digits, no whitespace or leading zeros); malformed IDs are rejected by `config set`. The REST adapter independently checks this requirement for disk configuration and effective runtime overrides. No wildcard, DM access, channel discovery or cross-server access is provided. The single-guild scope is intentional.

With the bar running, the existing configuration commands persist these settings:

```sh
tohru config set assistant.discord.allowed_user_id 456789012345678901
tohru config set assistant.discord.guild_id 123456789012345678
tohru config set assistant.discord.channels '234567890123456789,345678901234567890'
tohru config set assistant.discord.token_file /home/me/.config/tohru/discord-token
tohru config set assistant.discord.read true
tohru config set assistant.discord.post false
tohru config set assistant.discord.enabled true
tohru-discord status
```

`status` checks local setup without contacting Discord or showing the token. It does not verify authentication or permissions. If you edit the file directly, use `tohru config reload` to refresh the shell's settings snapshot. Native settings' managed layer (`TOHRU_MANAGED_CONFIG`) and temporary runtime overrides follow the usual precedence; nothing requires a managed layer. Changes to Discord settings through the shell cancel an active assistant turn. File changes during a tool request are checked again before each API operation; a changed configuration rejects the call. Token files are reloaded for each HTTP request, so rotation needs no service restart. An HTTP post already accepted by Discord cannot be undone by disabling the integration.

As an alternative to `token_file`, start the **Tohru bar/assistant process** with `TOHRU_DISCORD_BOT_TOKEN` in its environment, supplied securely by your session or launcher. An explicit token file takes precedence; an unreadable file never falls back to the environment. This is a normal environment option, not Nix injection. Tohru strips this variable from provider children. Prefer the file: environment variables may be inspected by other processes running as your user. Never store an inline token in TOML: no inline token setting is supported. Assistant exports include the nonsecret settings and file path, not token file contents or the environment token.

Assistant integrations currently use TOML/`tohru config set`; there is no separate integration editor in the graphical Settings window. A missing or invalid Discord configuration leaves chat and voice working. Enabled but incomplete configuration gives the assistant a nonsecret setup error so it can explain the problem. Disable everything or just one capability at any time:

```sh
tohru config set assistant.discord.post false
tohru config set assistant.discord.read false
tohru config set assistant.discord.enabled false
```

## Desktop first, Discord fallback

To try reaching you on the desktop before using Discord, opt in:

```sh
tohru config set assistant.discord.desktop_timeout_seconds 60
tohru config set assistant.discord.desktop_first true
```

`desktop_first` defaults to `false`. The timeout defaults to 60 seconds and accepts
10–300 seconds. These are normal native settings, including managed TOML and live
runtime overrides. Enabling this option does not enable Discord or authorize
unsolicited messages; the existing user-ID, channel, read/post and user-intent
requirements still apply.

When enabled, **every bot post** first shows the proposed message in a local
notification with **Got it**. Clicking that action (or the notification's default
action) acknowledges the message and suppresses the Discord post entirely. The
tool returns `sent: false`, `delivery: "desktop"`; this is successful desktop
delivery and must not be retried as a failed Discord post. This strict policy also
applies to explicit channel-post requests; there is no per-call bypass.

A successful notification submission does not prove you saw it. Closing or
ignoring it without acknowledging waits out the configured timeout, then permits
one Discord post. A missing notification helper, failed notification service, or
other reported delivery failure permits fallback immediately. The result reports
`delivery: "discord"` and `desktop_attempt: "unavailable"` or `"unacknowledged"`.
Neither result proves physical presence, attention, or reading the Discord message.
DND, a covered toast, or a desktop without action support may result in timeout.

Install `notify-send` from libnotify (`libnotify-bin` on Debian/Ubuntu,
`libnotify` on distributions such as Arch/Fedora), with support for `--wait` and
`--action`. The Nix package includes it. `tohru doctor` checks the helper when this
policy is enabled; `tohru-discord status` reports helper availability without
posting or checking the live notification service. The notification runs in the
assistant process's graphical session, using that session's D-Bus environment.
No voice/TTS call or separate provider call is needed.

The wait belongs to the current assistant turn. Cancellation, disabling posting,
or changing effective settings aborts it without falling through to Discord.
The notification helper is terminated/reaped on cancellation or timeout; the
notification itself requests the same expiry, although the desktop may choose
its own expiry behavior. After waiting, configuration is revalidated and the
existing 15-second REST deadline begins. No unattended background queue or
automatic schedule is introduced, and a fallback cannot outlive its turn.
Credential and broker environment variables are stripped from the notification
child. No Discord channel metadata or message request is made if the desktop
acknowledges first.

## Ask Tohru

Select `assistant.backend = "local"` (the default) for these built-in tools. The opt-in `orca` backend sends requests to a separate Orca bot session, which may run remotely: Tohru does not forward its local Unix socket, tool commands or bot credentials to that session. When Discord is enabled, remote turns include an availability note so the bot can explain the limitation. `assistant.orca` only controls local CLI context and is distinct from the backend setting and Home Manager's `assistant.orcaBot` options. An explicitly configured Orca-offline fallback to local providers uses the same guarded local Discord path.

Examples: “Read a page of my messages in Discord channel 234567890123456789 and summarize them.” Or, with posting enabled and an allowed user ID configured: “Post ‘The release is ready’ in Discord channel 234567890123456789.” No posting is performed as a setup check. Setting the user ID alone grants no posting permission.

The provider uses its actual shell/command tool to invoke the packaged `tohru-discord` command. Tohru gives normal turns a private local Unix socket; the helper sends requests to the assistant process, which loads credentials and contacts Discord. The token is absent from helper arguments, tool descriptions, provider environments and application logs. There is no MCP registration, tool-call JSON emulation or prompt-only replacement for an API action. The available read/post commands are described only when enabled, and the broker independently enforces each toggle and scope on execution. The static CLI can still be invoked with a disabled verb, which returns an error without a network request.

| Provider | Integration path | Verification |
|---|---|---|
| Claude | Native shell tool; discovery in appended system prompt | Fake CLI exercises packaged helper and live local broker |
| Codex | Native command tool; discovery in developer instructions | Same |
| Gemini | Native shell tool; discovery in every turn's input | Same |
| Antigravity (`agy`) | Native shell tool; discovery in every turn's input | Same |
| OpenCode | Native shell tool; discovery in every turn's input | Same |

These paths rely on the CLI's shell tool being available in Tohru's normal tool-enabled mode. User/provider policies can still disallow commands. No live model/account was tested for this feature; fake CLIs verify routing and enforcement, not a model's willingness to choose a tool. Memory extraction and speculative voice drafts receive no Discord socket or tool guidance. A voice request needing tools falls back to the normal turn path. This is the existing architecture's full shell access, not a sandbox against a malicious provider: same-user agents can access the user's files. Never treat prompt guidance as a credential security boundary.

## Reading and posting behavior

Reading is **on demand**, examining one REST history page of at most 50 entries per call (default 20). Before any message content reaches tools, provider prompts or logs, the execution boundary discards every entry whose `author.id` does not exactly equal `allowed_user_id`. Missing/malformed authors, other users, and nonmatching bots/webhooks are discarded; names, nicknames, mentions, member data and content never establish identity. Only the matching author's ID, timestamp and text are returned, in newest-first order, with an explicit untrusted-data label. Nested referenced messages and embeds are not forwarded.

The requested limit bounds examined entries, not matching results: mixed pages can return fewer messages, and an all-discarded page can return none while more history remains. There is no automatic scan to fill the result. Continue older history with `--before` and the returned `next_before`, or newer history with `--after` and `next_after`; these ID-only cursors include discarded entries so empty filtered pages can progress without revealing their content. A full page or size truncation sets `possibly_more`; a page without valid message IDs has null cursors. Size truncation keeps whole matching messages and advances only across consumed entries in the requested direction. Each additional page requires another bounded tool call and counts against the turn's call budget.

Attachments, embeds, reactions, rich messages and files are not fetched separately or sent. There is no Gateway connection, live event receiver, polling loop, unread-message store, incoming-message wake-up, automatic reply or user-configured automation engine. This filter covers the integration's sole reception path: on-demand REST history through the local tool broker. Incoming content cannot itself activate the assistant or authorize tools.

Posts accept 1–2000 characters from stdin and disable all parsed mentions, including `@everyone`. Only explicit user requests/authorization should lead to posting. The assistant is instructed never to follow commands embedded in Discord content or let messages authorize a post. The execution boundary enforces feature and channel controls; semantic user intent remains the responsibility of the assistant's existing tool-using policy.

Requests are serialized across Tohru turns using a state lock under `$XDG_STATE_HOME/tohru/discord`. There are at most 12 tool calls per turn, a 15-second operation deadline, 4-second socket timeouts, 256 KiB HTTP body limits and about 48 KiB of message data per result. A private broker serves one request at a time, with a bounded socket backlog and request size. Turning off or stopping a turn closes its broker and interrupts active I/O; rate-limit waits are cancellable. Lock waits count against the deadline.

Tohru honors `retry_after` on HTTP 429 and `X-RateLimit-Reset-After` when a bucket is exhausted. It conservatively shares the cooldown across all routes and turns, including global limits. It retries a 429 at most once, only within the deadline; long cooldowns return an actionable error and remain in effect for subsequent calls. See [Discord HTTP rate limits](https://docs.discord.com/developers/topics/rate-limits). There are no retries for permission/authentication errors, network failures or server failures. A failed/timed-out post may have reached Discord: inspect the channel before retrying to avoid duplicates. Sharing one bot token with other applications requires those applications to respect limits too.

API error bodies and fetched message contents are not logged. HTTP 401 means check/reset the token; 403 means check permissions/access; 404 means the channel is missing or inaccessible. Malformed/oversized replies produce bounded, nonsecret errors. Tool results reach the provider and can appear in that provider's own tool transcripts; summaries or quoted content can become part of Tohru's conversation history. Do not read sensitive channels you do not want sent to your configured provider.

## Local verification

From a checkout, with Python 3.11+ and Rust installed:

```sh
python3 -B tests/assistant/discord.py
python3 -B tests/assistant/service.py
cargo test --locked --manifest-path runtime/Cargo.toml
bin/tohru-discord status
```

The Discord suite uses a local fake HTTP server and fake provider CLIs. It covers missing/blank/invalid required author setup, exact author filtering and spoofed/missing authors, mixed/all-discarded pages, bounded cursors/results in both directions, live configuration changes, posting gates, scoped guild/channel access, credentials/redaction, malformed responses, permissions/auth errors, rate limits, broker shutdown/cancellation and provider shell routing without discarded content. Native settings and controller fixtures cover validation, persistence, effective settings propagation and cancellation on author-ID changes. It sends no live Discord traffic. `status` is a safe local smoke check; a real read requires an enabled integration, a configured user ID and a normal assistant turn. Posting requires a separate explicit user request.
