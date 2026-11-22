# Standalone portability implementation map

Supported: Linux, systemd/logind and a Wayland compositor with layer-shell.
This independent branch owns the following incremental changes:

1. Separate native output routing from optional compositor workspace/window data;
   retain Hyprland IPC and expose generic workspace names/IDs and capabilities.
2. Add native settings for fonts, wallpaper ownership/backend, idle ownership and
   voice environment. Implement session-scoped wallpaper and idle lifecycles.
3. Ship one shared user-service specification for standalone and Home Manager,
   opt-in PipeWire AEC generation and a read-only, bounded `tohru doctor`.
4. Expose font and shortcut ownership in Settings; gate Nix-only information.
5. Ship greetd templates/validation and complete non-Nix setup/dependency docs.
6. Verify policy/lifecycle/config/packaging with focused tests; record live-protocol,
   PAM, audio and GPU limitations separately from source-level checks.

No host activation, privileged setup, model downloads or real Discord requests.
Heavy checks require resource admission; this environment currently lacks both
`/etc/resource-scheduler/mode` and `resource-run`. See the final verification report
for checks performed and runnable commands awaiting that gate.
