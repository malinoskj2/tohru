# Optional launcher visibility adapter (proposal v0)

The existing plugin consumes native Hyprland `layer.opened`/`layer.closed`
events. `plugin:hyprrecede:namespace` is a `std::regex` matched against the
layer-shell namespace. The component default stays `^j2bar-launcher-backdrop$`.
The old shell's authored config broadens this to
`^j2bar-(launcher-backdrop|polkit)$`; extraction does not select polkit recession.

Each matching layer's monitor owns an independent recession. Multiple matching
layers keep that output recessed until its last layer closes. Closing animates
back; destroying/removing an output cleans up its state. The plugin listens for
future opens; loading it while a matching layer is already open does not scan
that layer into state. Load/configure the candidate plugin before opening the
launcher. A config change while open needs a close/reopen to re-evaluate matches.

Proposed narrow mapping for F, relayed through the coordinator:

| Shell visibility | Optional compositor input |
| --- | --- |
| false → true | Map a launcher-owned layer on the launcher's output, namespace `tohru-launcher-backdrop` |
| true → false | Unmap/destroy that same layer |
| crash/exit/output removed | Wayland surface teardown supplies the close/output lifecycle |
| plugin missing / Sway | Continue launcher behavior normally; no capability request is required for opening |

Nix configures the exact regex `^tohru-launcher-backdrop$`. Surface ownership,
namespace and monitor must follow the actual launcher lifecycle. An existing
backdrop surface is preferable; do not add polling, a helper daemon, global
Hyprland dispatches or a new IPC server. The shell must not depend on internal
plugin types, settings writes or the capability command. F verified the pinned runtime's `wayland.rs` hardcodes the layer namespace to
`pleamar`; `app_id` applies only to windows (coordinator message
`msg_91dcb9a91aa7`). Therefore this mapping is blocked pending a user gap decision
on a native layer namespace property patch. No patch, custom workaround or reduced
behavior is authorized. The mapping is not implemented. Do not match the generic
`pleamar` namespace: that would recess windows for unrelated shell layers.

The plugin's distinct read-only `desktop-effects-capabilities` query supports
operator diagnostics; it is not a shell transport. Setting a visibility boolean
alone cannot synthesize the required output-scoped Wayland lifecycle.
