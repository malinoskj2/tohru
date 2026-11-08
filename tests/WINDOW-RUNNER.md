# Private ordinary-window, sound and Settings workload

Run only with the coordinator's exclusive GPU allocation. No production session,
authentication role, physical radio, browser or host audio endpoint is started.

The runner owns a private bus, Sway display, Pulse null sink, IPC directory and
persistent virtual pointer. Every app/client inherits the same forced private
endpoints; inherited Wayland FDs, alternate input sockets, bus markers and Pulse
runtime/config selectors cannot bypass them. Diagnostics failures still attempt
cleanup/reap of all five owners. Raw logs and screenshots remain under root .tmp.

CPU readiness checks:

```sh
python3 tests/isolation.py
python3 tests/window-output-route.py .tmp/s-window-output-correction
source runtime/dev-env.sh
mkdir -p .tmp/s-pointer-build
wayland-scanner client-header tests/bar/virtual-pointer.xml .tmp/s-pointer-build/virtual-pointer.h
wayland-scanner private-code tests/bar/virtual-pointer.xml .tmp/s-pointer-build/virtual-pointer.c
cc -std=c11 -Wall -Wextra -Werror $(pkg-config --cflags wayland-client) -I.tmp/s-pointer-build tests/bar/pointer.c .tmp/s-pointer-build/virtual-pointer.c $(pkg-config --libs wayland-client) -o .tmp/s-pointer-build/pointer
```

Supply an installed wayland-scanner path if it is absent from PATH. The current
sandbox has `/nix/store/2a96lbjxyq7yiaf5zb013kshf8p8nigx-wayland-scanner-1.25.0-bin/bin/wayland-scanner`.

After allocation, with the corrected typed artifact (frozen53 lacks the final
output-removal barrier):

```sh
tests/native-settings.sh \
  --runtime "$PWD/.tmp/s-native-artifact-27b16e4/bin/pleamar" \
  --windows --sound --pointer "$PWD/.tmp/s-pointer-build/pointer" \
  --wtype /nix/store/4hifjyka5k29srmd515w323habr7g8kf-wtype-0.4/bin/wtype
```

The window-only scene has no other surface/GPU dependency. Checks cover initial
unmapped/no-GPU state, explicit first open, WM-close fact acknowledgement/reopen,
resize, output removal/reconnect with changed native role identity, repeated
retirement and active shutdown. The same private compositor then runs actual root
click/grab/outside-dismissal, CLI Escape, private output volume/mute/error paths,
Settings search Return/provenance/WM-close/reopen, visible-calendar VM reload and
retained timer rest, writable/managed settings reload and process restart.
Ctrl+F evidence types the query through the virtual keyboard and checks the live
input text before Return; no IPC search setter can bypass a failed focus action.
The evidence-only Settings clone adds a second real native input. Focus starts on
that canary (it persists across virtual keyboard device changes), so native
first-field autofocus cannot make a broken Ctrl+F pass. The canary must remain
unchanged and is hidden before screenshots. Production Settings files stay intact.
Intentional window/root quit must return exit status zero; its status is recorded
in CLI evidence before clearing process ownership. Crashes/timeouts remain failures.

This is driver/lifecycle/input evidence, not performance, exact pixel parity or
physical-session acceptance. Both application and compositor hardware adapters
must be present in their logs. Failed assertions remain failures with raw evidence;
no successful source/CPU check substitutes for the native run.
