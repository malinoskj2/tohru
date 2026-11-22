# Tohru text assistant — three directions

Standalone design exploration based on main `563d48a3d2abfd1030bdaf736d6bbf9944db84fa`. Production shell files are unchanged. Open `index.html` for the comparison, or `panel.html?design=a`, `b`, or `c`. All assets are local and relative: the entire directory can be copied or served anywhere, with no package install, bundler, or external service. The prototype works from a file URL too; HTTP is the verified path.

| Direction | Header and navigation | Composer |
| --- | --- | --- |
| 01 · Quiet companion | One 72 px identity toolbar: mascot, Tohru, a short descriptor, history and new chat. | Inset rounded plate; provider/model choice lives in the lower left, near the send action. |
| 02 · Pearl workspace | Pearl application header, explicit Chat / History navigation, provider choice at the end of the second row. | A quieter white writing surface; keyboard hint and send share its lower row. |
| 03 · A thread to return to | Small brand strip, then a conversation title and one line of context. History and new chat stay in the brand strip. | Integrated full-width reply dock with a section label and a separate provider/send rail. |

The shared two-turn sample is deliberately identical, including paragraph, list and message spacing. The palette differences support the structural exploration; they are not the only differences. Assistant replies remain plain text on glass, user messages sit on rounded plates, and the provider stays subordinate to Tohru.

## Source references

Read before designing: `shell/assistant/popup.plm`, `state.plm`, and `README.md`; `shell/panel-ink.plm`; `shell/bar/entry.plm` and `contents.plm`; `shell/brand/README.md`; assistant SVGs. The current source uses `ai_w = 380`, `ai_h = max(400, screen height - 49)`, popup x = screen width - ai_w - 24, visible body local x = 12, and y = 37. Therefore the visible panel has a 12 px right inset, not a 24 px inset. These mockups grow leftward by 114 px to **494 logical px (exactly 1.3 × 380)**.

The bar reconstructs the actual source's x = 100, width = screen width - 200, height = 31, 10 px corners and `#160e14` at 70% opacity. Its labels and glyphs are representative, not a live system readout. The background is an explicitly reconstructed gradient desktop, not an installed wallpaper or host screenshot. `ink_glass` dark purple, light pearl, lavender, peach and soft plate colors follow `panel-ink.plm`. CSS blur approximates Pleamar frost; it is not a rendering of the production shader. System fonts are used rather than downloading fonts.

`assets/mascot.png` and `dragon.png` are byte-for-byte copies of the actual brand files. `mooncoil.svg` is from `shell/bar`; other SVGs and `LICENSE.LUCIDE` are copied from `shell/assistant`. The original mascot appears in every application header, not only the empty state.

## Interactions

- History opens searchable grouped examples. Search is local and case-insensitive. Each example opens the common sample conversation, explicitly announced as a sample; this is not real persisted history.
- New chat opens an empty state with a usable starter suggestion. The populated sample can be restored through the external mockup controls.
- Provider / model opens a dedicated selection view. A choice preserves the draft and conversation. Provider/model labels are illustrative; there is no live discovery, login, or model configuration.
- Typing enables Send. Enter sends, Shift + Enter inserts a line. Sending appends the local user turn and an explicitly demo-only response. Nothing is transmitted or saved. Reload resets the page.
- Escape returns from History or provider selection. Real native buttons, links, input and textarea support keyboard focus, visible focus outlines and activation. Conversation content is independently keyboard-scrollable. No voice or attachment capabilities are invented.
- Copy tries the browser clipboard and reports failure honestly if the preview's permissions or insecure origin prevent it. Retry reports that no request was sent.
- The external **Long conversation** control repeats the shared sample to exercise scrolling. Header and composer stay anchored while the conversation scrolls.
- The external **Show 380 px baseline** toggle draws the previous-width left edge inside the wider panel, with the same top/right/bottom anchor. It is a width reference, not a replica of the old header. On narrow screens the two widths both clamp to viewport minus 24 px.

On narrow screens the large external study controls are hidden to preserve the panel; a separate top notice still says this is a design demo with reconstructed desktop context, and includes a comparison link. Product controls remain usable. Below 449 px height, the mockup prioritizes staying inside the viewport over the production minimum height of 400 px. Desktop geometry is the primary target.

## Verification and evidence

Browser measurements, screenshots and interaction results are in the companion worktree evidence directory `.tmp/assistant-text-designs/`. All measurements use `getBoundingClientRect()` and actual `innerWidth` / `innerHeight`, with screenshots at device scale factor 1. Initial visual and interaction checks used Orca's browser. Repeated `Page.captureScreenshot` frame timeouts occurred in both the original Orca tab and a new tab requested with `--headless`; remaining checks and final evidence used the already installed Playwright 1.59.1 with Chromium 157 as a tool-limitation fallback. No packages were installed. These are HTTP-served HTML prototype checks, not live-desktop or native Pleamar validation.

| Viewport | Visible panel bounds, each design | Result |
| --- | --- | --- |
| 1440 × 900 | x = 934, y = 37, width = 494, height = 851; right = 12, bottom = 12 | Actual browser measurement |
| 1920 × 1080 | x = 1414, y = 37, width = 494, height = 1031; right = 12, bottom = 12 | Actual browser measurement |
| 390 × 844 | x = 12, y = 37, width = 366, height = 795; right = 12, bottom = 12 | Actual browser measurement; independent chat scroll |
| Baseline at 1440 × 900 | x = 1048, y = 37, width = 380, right = 12, bottom = 12 | Actual browser measurement; 114 px added to left |

Evidence filenames use `a`, `b`, `c` for the directions above: `*-desktop-1440.png`, `*-desktop-1440-panel.png` (focused crops), `*-placement-1920.png`, `*-narrow-390.png`, `*-baseline-1440.png`, and captures of empty/provider/history/long-scroll states. `verification.json` contains measured results and interaction checks; `verify.log` records the checking script outcome. `gallery-batch.json` is a prepared media-gallery batch, with absolute host paths and all three primary desktop screenshots marked **Needs review**. The coordinator will preserve these artifacts and publish the gallery, so this worktree does not start or modify a gallery.

All three completed checks passed: exact geometry and baseline anchor; loaded local images; no horizontal document overflow; composer inside panel; empty and populated views; draft preservation across provider selection; Shift + Enter newline; Enter and button send; history filtering and opening; Escape return; copy/retry feedback; keyboard PageDown and mouse-wheel scrolling with long content; narrow-screen provider, history and send controls. At 1440 × 900, the complete shared sample fits the conversation area in each design (client/scroll heights: A 623/623, B 592/592, C 564/564 px). Narrow layouts intentionally scroll their longer conversation. The final browser run reported no JavaScript errors, failed resource loads or external requests, and confirmed all three comparison links. Clipboard access was unavailable on this HTTP preview origin and the expected feedback was verified. Focused reading-region key handling was added after Orca's initial synthetic PageDown did not perform a default nested scroll.

Sandbox display was started with the sandbox display skill; ordinary Orca browser work did not load a model or require a GPU reservation. `resource-run`, `cpu-run`, and `ram-run` were checked and absent. Only light Python static serving, browser interactions, JavaScript syntax checking, and screenshot crops were used; no production build or heavy tests were run.

To preview independently, choose a free reserved preview port, bind `0.0.0.0`, and serve this directory with Python's standard-library HTTP server. In a sandbox use the matching `AGENT_PREVIEW_URL_<port>` for links. The worker used port 8080 successfully; a fresh location must check its own port availability.

Known limits: the glass is a CSS approximation, not a native shell render; sample history and provider choices are illustrative; clipboard behavior depends on browser security; there is no persistence, real streaming, AI, auth, service integration or production rollout. Evidence files are intentionally untracked, while editable source, local assets and this document are committed.
