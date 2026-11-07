# Migration options for j2bar

Research date: 7 November 2026. This is a migration proposal based on local code, Nix configuration, upstream source and documentation. No candidate has been benchmarked or substituted into the running desktop. Effort estimates below are planning allowances, not measured delivery times.

**Recommendation: pursue an Amane feasibility prototype first, with Quickshell as the fallback.** Amane matches the preference for Rust and supports custom shader widgets. It can plausibly host this desktop, but full parity requires extensions and custom shell code. Keep Noctalia with a pixel-stream companion in contention if it passes transparency and performance checks. Pleamar is the most interesting other recent framework, but its current shader interface is more restrictive for this workload.

## What has to survive

The migration target is the current desktop, not simply a row of status modules. The repository's [ownership inventory](../j2bar/docs/REPLACING_NOCTALIA.md), [effects contract](../j2bar/docs/EFFECTS.md), [plugin contract](../j2bar/docs/HYPRLAND_PLUGIN.md), [GPU API](../j2bar/docs/LUA_GPU.md) and [README](../j2bar/README.md) establish these requirements:

| Area | Parity requirement |
|---|---|
| Bar | Liquid34 geometry: 31 px height, 100 px side insets, flared top connection, glass, typography, filled icons, title positioning, current output selection/fallback behavior. |
| Motion | Workspace wave/spring behavior, liquid vessels and glow, hover/press transitions, launcher tint, panel-specific accents. Preserve the actual motion equations and resting behavior. |
| Panels | Calendar, sound, network, system menu, About and Settings; anchoring 6 px below the bar; Escape and outside-click dismissal; keyboard behavior. |
| Glass | Shaped compositor glass, adaptive ink/accent contrast and initial sampling; this depends on compositor integration, not just a fragment shader. |
| Desktop services | Notifications/history/actions/DND, launcher/dmenu, OSD, lock/idle/inhibitors, polkit, screenshots, wallpapers, power and brightness. |
| Specialized behavior | Focus dips, title bars, file-chooser sheets, window recession; game launch identity, background limiting and audio policy; Assistant and privileged Settings actions. |
| Extensibility | Inventory actually enabled Lua plugins. The current GPU API also supports feedback and multipass graphs; matching the built-in bar does not automatically preserve that API. |

The workspace design reference explicitly specifies a fixed 1/240-second simulation and stopping at rest. Its prose is not permission to substitute a generic easing curve. Match the running implementation and use [the design reference](../j2bar/docs/design/workspace-indicator/README.md) to resolve motion details; its illustrative mock-bar dimensions differ from today's README.

I also inspected `/home/jesse/nix`. The current checkout imports j2bar through `hosts/common/optional/j2bar.nix`, binds its commands in `users/jesse/hyprland-desktop/hyprland/hyprland.lua`, and loads its compositor plugin from `look.lua`. It declares Hyprland 0.56.2. The Nix module says the bar is launched manually from a local build and leaves settings in the user configuration. Therefore a migration must inventory the actual running binary and live configuration before implementation; this checkout alone is not the complete runtime state.

## Comparison

These are assessments of fit for *our* requirements, not general rankings. “Shader support” means an application-author interface, not merely a GPU-accelerated renderer.

| Option | Shader route | What we maintain | Assessment |
|---|---|---|---|
| **Amane + custom Rust shell** | Public GLSL/WGSL rectangle shaders; custom frame requests. Texture/feedback extensions needed for full GPU API parity. | Our UI, specialized services, small framework patch series, Hyprland plugin. | Preferred prototype. Strong language fit; young upstream and explicit gaps. |
| **Custom Quickshell UI** | Qt Quick `ShaderEffect`; Qt 6 shader baking; native render extension if feedback needs exceed QML facilities. | Our QML UI, motion model, specialized services and Hyprland plugin. | Best fallback: established toolkit and relevant Wayland integrations. |
| **Noctalia + renderer companion** | Luau UI embeds streamed pixels from our renderer. No direct custom-shader API found in reviewed plugin docs. | Small renderer, Luau widgets/panels, specialized services and Hyprland plugin. | Potentially least shell work if visual and input parity pass. Transparency is unresolved. |
| **Pleamar + custom scenes / selected Vitreus components** | WGSL scene/group shaders; eight scalar values plus two colors in the documented custom-shader interface. | Scenes, Luau logic, shader/interface extensions, specialized services and Hyprland plugin. | Promising recent alternative; exact simulation and GPU resources need investigation. |
| **DMS/Dux plugins or separate custom bar** | QML/Qt Quick shader path. | Widgets, possibly bar/panel host overrides, compositor integration and specialized services. | More existing desktop UI; exact geometry may force substantial customization. |
| **AGS/Astal, Ignis/Fabric, Wayle, Ironbar, Eww/Waybar** | GTK widgets; custom GL/native widget or companion for shader-heavy work. | UI plus rendering integration and shell composition. | Viable engineering paths, but less direct for this particular visual requirement. |

Qt documents the shader build pipeline in [ShaderEffect](https://doc.qt.io/qt-6/qml-qtquick-shadereffect.html). Quickshell exposes [background-effect regions](https://quickshell.org/docs/v0.3.0/types/Quickshell.Wayland/BackgroundEffect/) and [Hyprland focus grabs](https://quickshell.org/docs/v0.2.0/types/Quickshell.Hyprland/HyprlandFocusGrab/); verify both against the exact chosen Quickshell revision. DMS's [plugin contract](https://github.com/duxlabs-dev/dux-shell/blob/master/quickshell/PLUGINS/README.md) supports QML extension surfaces but describes the API as experimental. Its old DankMaterialShell repository now redirects to Dux.

GTK is not incapable of custom shaders: [Gtk.GLArea](https://docs.gtk.org/gtk4/class.GLArea.html) provides an OpenGL integration path. However, the older [Gsk.GLShader API is deprecated](https://docs.gtk.org/gsk4/class.GLShader.html). A Waybar script module is not a general GPU widget interface. [Astal](https://aylur.github.io/astal/guide/introduction) offers useful service libraries, whereas [Wayle](https://github.com/wayle-rs/wayle) provides a more complete Rust/GTK4 shell. None removes the custom rendering work here.

## Recent projects and momentum

The six-month search window was 7 May–7 November 2026. Repository creation dates are evidence of repository age, not proof of when development began. Counts were sampled from GitHub's repository/commit/release APIs on the research date. Stars indicate interest; commit and author samples indicate activity, not stability or independent review.

| Project | Created; approximate interest | Recent evidence | Relevance |
|---|---|---|---|
| [Amane](https://github.com/MystiaFin/amane) | 4 Oct; 455 stars, 22 forks | 62 commits since 8 Oct in the retrieved sample, three author accounts; experimental 0.1.2 release on 13 Oct. | Public shader API and Rust shell framework. Maintainer explicitly pauses normal work and closes issues/PRs until mid-December. |
| [Pleamar](https://github.com/k4ditano/pleamar) | 19 Sep; 217 stars, 19 forks | At least 100 recent commits; three author accounts in latest 100; 0.7.27 released 6 Nov. | Rust/wgpu runtime, scene language and Luau, shaders, Nix/Home Manager packaging. Genuine contender. |
| [Vitreus](https://github.com/natepayn3/Vitreus) | 28 Sep; 151 stars, 14 forks | At least 100 recent commits; four author accounts in latest 100; 0.4.0 released 7 Nov. | A glass-oriented shell built on Pleamar, with real shader assets. Evidence that the framework has another consumer; not an independent renderer. |
| [Impasto](https://github.com/andreumassanet/impasto) | 25 Aug; 441 stars, 37 forks | Updated 7 Nov; active Quickshell/Hyprland shell. | Useful alternative full-shell base, but its island-centered interactions and Arch installer require adaptation. |
| [Mandara](https://github.com/theHamdiz/mandara) | 27 Oct; 72 stars | At least 100 recent commits; latest sample is maintainer plus automation; frequent releases. | Rust/GPUI shell with WASM plugins. Reviewed plugin contract describes UI nodes, not arbitrary shader resources. Too early to infer shader parity from GPU rendering claims. |
| [Capsule](https://github.com/moiCR/Capsule) | 22 Jul; 54 stars | Updated 26 Oct; Nix flake. | Rust/GPUI full shell. Worth watching, but no verified extension route that beats Amane for this job. |
| [WayVes](https://github.com/Roonil/WayVes-Shell) | 26 Oct; 9 stars | Updated 28 Oct. | Interesting GPU-sharing experiment, but insufficient adoption evidence to select as the desktop foundation. |

Established alternatives have substantially larger communities: Dux roughly 8.8k stars, Noctalia 11.9k, Caelestia 13.3k and Serpantinum 7.6k in the same API snapshot. [Caelestia](https://github.com/caelestia-dots/shell) and [Serpantinum](https://github.com/ilyamiro/serpantinum) are relevant QML shell bases, but adopting their existing interaction models does not make Liquid34 reproduction automatic. Newer QML shells such as Panacea, ChillPill and Ricelin add design choices more than a different rendering foundation. Projects requiring a replacement compositor were excluded from the main shortlist because preserving Hyprland behavior is part of the task.

## Amane: feasibility and migration plan

Source inspected at commit `544cbfc23af37f84ac237cd9bec53173981772b3`. This matters because the project is experimental. The main conclusion is **feasible with targeted extensions, not established full parity out of the box**.

| Requirement | Evidence and work needed |
|---|---|
| Liquid/glow/workspace shaders | Supported foundation. `Rectangle::shader` accepts WGSL or GLSL; `shader_values` supplies 16 vec4 rows. Port current uniforms into that layout. GLSL is compiled as version 450, so existing GLES100 source needs adaptation. |
| Alpha correctness | Amane expects straight shader output and applies blending; current j2bar shaders often output premultiplied color. Explicitly adapt this boundary and test transparent edges. |
| Exact physics | `request_frame()` allows custom motion. Preserve the fixed-step simulation in Rust; do not use Amane's ordinary duration/easing animation as a replacement. Advance once per presentation step, not once per widget/view invocation. |
| Idle behavior | Request frames only while moving. Amane treats a shader source containing `time` as continuously animated, even in comments. Supply our phase through values for effects that must settle, and inspect redraw behavior. |
| Feedback/multipass | Current shader bind-group layout contains only uniforms. No public custom texture/sampler/feedback API was found. Add bounded texture inputs and render-target passes if enabled widgets require them. This is a real renderer extension, not configuration. |
| Surface geometry | Layer namespaces, margins, layers, exclusive zones and input regions exist. Reproduce the flared silhouette with shader/path geometry and independent input/visual bounds. |
| Glass and dismissal | No `ext-background-effect` or Hyprland focus-grab implementation was found in inspected source. Add protocol support to Amane. Existing namespace support lets us retain current plugin matching while migrating. |
| Monitors | Per-monitor windows, hotplug and scaling APIs exist. Implement our single preferred-output/fallback policy; do not accidentally change to a bar on every output. Validate fractional scale and rotation. |
| Audio | Built-in service uses PulseAudio/pipewire-pulse, default sink/source and 0–100% volume. It does not replace our full PipeWire device, balance, peak-meter and over-amplification behavior. Retain or adapt the existing backend. |
| Notifications | Built-in server handles messages and actions, but documented behavior ignores sender timeouts. DND schedules, history policy, filtering, sound and inline reply require explicit work/audit. |
| Other services | Network, Bluetooth, media, applications, workspaces, battery and brightness are foundations, not evidence of complete Settings parity. Verify VPN import/secrets, pairing agent, media arbitration and privileged operations individually. |
| Tray/polkit | No full StatusNotifier/DBusMenu implementation or polkit agent was found in inspected source. Existing j2bar status controls can use services; arbitrary application tray menus, if wanted, need another backend. Use a separate polkit agent or retain the custom one during migration. |

These findings come from Amane's [shader documentation](https://github.com/MystiaFin/amane/blob/544cbfc23af37f84ac237cd9bec53173981772b3/docs/src/shaders.md), [shader pipeline](https://github.com/MystiaFin/amane/blob/544cbfc23af37f84ac237cd9bec53173981772b3/src/graphics/gpu/shader.rs), [animation API](https://github.com/MystiaFin/amane/blob/544cbfc23af37f84ac237cd9bec53173981772b3/docs/src/animation.md), [layer API](https://github.com/MystiaFin/amane/blob/544cbfc23af37f84ac237cd9bec53173981772b3/docs/src/layer_windows.md), [audio service](https://github.com/MystiaFin/amane/blob/544cbfc23af37f84ac237cd9bec53173981772b3/docs/src/audio.md) and [notifications](https://github.com/MystiaFin/amane/blob/544cbfc23af37f84ac237cd9bec53173981772b3/docs/src/notifications.md).

**Implementation sequence:**

1. Build a small Rust shell depending on a pinned Amane revision. Package the compiled shell and shader assets in Nix. Amane's supplied flake builds its development CLI, which compiles user source at runtime; production should instead start our reproducible prebuilt binary.
2. Port the workspace indicator, one liquid meter, glow and bar silhouette. Use deterministic input sequences for comparisons. Keep shaders and simulation separate from framework adapters.
3. Implement the background-effect mask and focus-grab additions as a small Amane patch series. Verify panel contrast events, outside-click dismissal and launcher recession with the extracted plugin.
4. Inventory enabled shaders. If any require feedback, prototype a two-target feedback effect before committing to the framework. Avoid promising the entire j2bar Lua/GPU API based on a successful single-pass meter.
5. Move everyday UI onto Amane while reusing only the backend behavior that its services do not cover. Preserve notification ownership and current action semantics throughout.
6. Port Settings/Assistant and their authenticated helpers separately. Keep machine configuration authoritative in Nix. Remove the old renderer once every required surface has a replacement.

The main maintenance risk is becoming the de facto maintainer of Amane's Wayland/GPU extensions during its hiatus. Keep changes isolated and upstreamable, but do not schedule the migration around upstream accepting them. If matching the bar requires a broad renderer rewrite, switch to Quickshell before porting the rest of the UI.

## Noctalia pixel streams: a real conditional option

Current Noctalia is a native C++/Wayland/GLES shell; the old QML shell is legacy. Its Luau plugins can build custom UI, but the reviewed public interface does not expose arbitrary shader programs. [Current project](https://github.com/noctalia-dev/noctalia), [plugin documentation](https://docs.noctalia.dev/noctalia/plugins/).

The documented stream uses shared-memory RGBA buffers, with one outstanding frame request, rather than compressed video. It uploads received frames and requests another when ready. The documentation asks for opaque pixels. Therefore this is a plausible route for small widgets, but transparent glow over glass is an unresolved requirement. [Protocol](https://docs.noctalia.dev/noctalia/plugins/development/declarative-ui/#companion-protocol), [upload implementation](https://github.com/noctalia-dev/noctalia/blob/main/src/scripting/ui/pixel_stream_view.cpp).

Proposed implementation: extract just our widget simulation/shaders into one companion process, with independent small streams. Keep ordinary labels, buttons and services native. Render to GPU targets, transfer results through reusable shared buffers, and respect host pacing. Use asynchronous readback where supported; never queue stale frames. Idle effects should retain their last frame without continued rendering.

Calculated example: a 104×42-pixel effect at 144 Hz contains about 2.4 MiB/s of RGBA payload per transfer direction; at 2× scale, about 9.6 MiB/s. These figures exclude synchronization, GPU readback, upload, driver overhead and redraw of the surrounding bar. They show why the idea deserves a small prototype, not that it will be fast.

Test transparent edges over both light and dark moving content first. Then measure clicks, scroll/drag and external workspace events through the complete loop. If alpha requires a small upstream patch, account for it explicitly. If it needs painting the background into every widget, full-bar streaming or unreliable overlays to hide seams, reject that approach for visual parity. Do not assume DMA-BUF or zero-copy support: the reviewed protocol uses memfd pixels.

If the prototype passes, this could beat a custom shell on maintenance: Noctalia owns general desktop services; our companion owns distinctive effects. It still requires porting custom panel layouts, checking theme/geometry limits, and separating the compositor effects and specialized game/Settings behavior.

## Alternative-specific plans

**Quickshell:** build our own bar/panels rather than theme an unrelated layout. Port shaders to Qt 6's offline `.qsb` pipeline, preserve simulation constants and stop-at-rest behavior, use focus grabs/background-effect regions, and consume existing services. Verify multipass/feedback requirements independently. Expected custom work is substantial UI code but less framework protocol work. This is the fallback if Amane's patch burden becomes too large.

**Pleamar:** first port the most parameter-heavy widget. Its [documented custom-shader ABI](https://github.com/k4ditano/pleamar/blob/main/docs/11-language-reference.md) allows eight scalar values and two colors, with no arbitrary resource bindings. Our state vectors and feedback API may exceed that. Validate exact simulation integration before using its built-in springs. Keep compositor glass in our plugin rather than assuming captured-background lens effects are equivalent. If the interface passes or needs only a small extension, build custom scenes and selectively reuse Vitreus components. Pin runtime and shell together; avoid its general desktop installer for Nix deployment.

**DMS/Dux:** prototype our workspace and liquid widgets inside its QML plugin host, then test 31 px geometry, top flare, glow overhang and popup positioning. If the host restricts these, run a separate custom Quickshell bar with Dux's bar disabled, provided the selected version supports clean role separation. Coordinate launcher state, DND, OSD and IPC. If it needs a lasting core fork across most surfaces, a standalone Quickshell UI is simpler.

**GTK family:** choose only if its ecosystem is a strong preference. Build a dedicated GLArea widget or companion for shader rendering, then integrate service libraries and style the surrounding bar/panels. Eww/Waybar scripts alone cannot supply the required arbitrary shader rendering. This adds a rendering boundary without a clear advantage over Amane or Quickshell for this project.

## Ownership after migration

Create a separate repository for the existing Hyprland plugin. This is an extraction of working effects, not a new implementation. Suggested names below are placeholders.

| Owner | Responsibilities |
|---|---|
| Nix repository | Exact package pins; Hyprland/plugin compatibility; plugin loading; monitors, binds, window/layer rules, static effect settings; PAM/polkit policy; service lifecycle; hardware permissions; game launch wrappers. |
| `desktop-effects` repository | Focus dips, title bars, glass/refraction, contrast sampling, file-chooser sheets and recession; capability protocol and compositor regression tests. |
| `desktop-shell` repository | Amane UI, shaders, animation models, panel state and display-specific interaction. Small versioned Amane patch series initially. |
| Optional `desktop-services` package | Only custom runtime behavior not covered elsewhere: game controller/libraries, privileged Settings helpers and possibly retained audio/notification adapters. Can begin as a package in the shell repo rather than creating many repositories immediately. |

Nix cannot replace a runtime game controller, shader simulation or D-Bus agent. It can configure and supervise those programs. Conversely the shell should no longer have to keep compositor policy alive merely to draw the clock.

Move `settings.effects` ownership back deliberately. Export the resolved current settings, including defaults and user overrides, then generate equivalent Lua/configuration from Nix. Static defaults alone would lose presets, rules and animation choices. Dynamic GUI editing, if retained, needs one explicit override owner and reapply behavior; do not let Nix and the shell compete for the same keys.

Initially retain the `noctalia-panel` namespace, `j2bar-launcher-backdrop` matching and contrast/capability protocol for compatibility. Later introduce neutral names with an explicit protocol migration. The plugin currently hardcodes instant handling for `noctalia-panel`; moving files does not remove that coupling.

Keep the first-frame tearing fix as the existing Hyprland source patch until an equivalent validated hook exists. Keep exact compositor build matching, private-hook tests and original license notices. Moving the plugin out of j2bar does not make its private ABI stable.

For lock/idle, [hyprlock](https://github.com/hyprwm/hyprlock) and [hypridle](https://github.com/hyprwm/hypridle) are concrete modular options, with Nix owning policy. If their visuals do not meet the existing lock-screen requirement, retain the existing locker until the custom replacement passes PAM, hotplug and suspend tests. Treat this as a separate decision from the bar framework. For notifications, a daemon such as [SwayNotificationCenter](https://github.com/ErikReider/SwayNotificationCenter) can reduce implementation work but does not imply exact visual/behavior parity.

## Delivery gates and acceptance

**Phase 0 — freeze the reference, approximately 1–2 focused days.** Record running versions, active settings/plugins, commands, assets and shader inventory. Capture the bar, all panels and interactions on the sandbox display under `/tmp/agent-media/`. Establish the required behavior checklist, including features implemented but not actually used. Preserve history/pins/wallpaper assignments and document which state can be imported.

**Phase 1 — Amane feasibility, approximately 3–5 focused days before UI expansion.** Build the three representative effects and one real panel. Prove alpha, motion, focus grabs, rounded background regions, output selection and a feedback example if required. Produce measured results and an explicit framework patch list. Stop here if it turns into a new renderer project.

**Phase 2 — effects/Nix separation, approximately 2–5 focused days plus hardware verification.** Extract plugin/package/tests; move the resolved effect policy; keep current j2bar working with the extracted plugin. This work is reusable for every candidate.

**Phase 3 — bar and core panels, approximately 1–3 weeks.** Recreate pixels and interactions, wire backend events, preserve command names through a temporary compatibility adapter, and compare all required states. Map the current `control-center` key binding explicitly: it is present in Nix while the documented j2bar panel list differs.

**Phase 4 — complete desktop parity, budget separately.** Settings, Assistant, notification policies, authentication, VPN import and game policy can dominate the work. Allow multiple additional weeks if all existing behavior must be retained; do not present a bar prototype as completion of this phase.

**Phase 5 — controlled cutover.** Build a Nix candidate generation with one owner for notifications, lock/idle, polkit, wallpaper and OSD. Preserve the old generation and state. Exercise the new session before retiring the old packages; roll back both package/configuration selection and relevant mutable state. Do not remove j2bar until the checklist is complete or a behavior change is explicitly accepted.

Proposed prototype gates, to apply consistently to every candidate:

- Compare deterministic workspace transitions, interruptions and long jumps; full-scale meter drags; launcher tint; panel accents; light/dark glass changes. Match fonts, glyph positions, geometry and alpha edges. Visual goldens need driver/font tolerances and human review.
- Test 60 Hz and the actual high-refresh display, 1×/fractional/2× scale, rotated outputs, disconnect/reconnect and fullscreen. Preserve no-animation-at-rest behavior.
- Measure end-to-end event-to-present latency and frame intervals, not just surface commits. Proposed target: no more than one refresh interval of added p95 latency versus the current implementation, no repeated stalls, and under 1% missed refresh opportunities in a controlled continuous-animation workload. Establish reliable measurement before applying a numerical gate.
- Record total CPU, PSS, scheduling activity and GPU cost for shell plus helpers plus compositor. Proposed initial budget: settled desktop under 0.5% of one CPU core for the replacement shell/helper group; active matched workloads within 20% of baseline CPU and no material GPU/power regression. These are proposed acceptance budgets, not claims about any candidate.
- Run at least a 30-minute interaction soak after warm-up; memory should plateau. Test missing services, invalid shaders/config, companion death, plugin unload/reload and shell restart. No growing queue, stale input capture or stuck lock ownership.
- Re-run on physical hardware before acceptance. Existing [performance results](../j2bar/PERFORMANCE_REPORT.md) include unmatched stimuli and known limitations; they cannot rank current Amane, Pleamar or Noctalia builds.

**Decision:** prioritize Amane because it matches the preferred implementation model and has a genuine shader interface. Accept it only after the representative visual/protocol prototype passes. Quickshell is the fallback with the fewest obvious protocol gaps. Noctalia streaming remains a worthwhile alternative if a small transparent effect proves visually and operationally equivalent. None currently has demonstrated complete parity with this desktop without custom work.
