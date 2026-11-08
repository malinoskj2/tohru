# Focused native review workload

Requires coordinator GPU allocation. This is a targeted check after Q-B1/2/3 and
same-root composition changes; it does not rerun broad bar/calendar acceptance.
Private bus/bootstrap/IPC/Wayland selectors use the independently reviewed
777022a runner helpers. Native payload CPU fixtures separately cover empty
window payload and native Identity/alias boundaries.

The persistent virtual pointer fixture is test-only, using the standard
wlr-virtual-pointer protocol (embedded MIT notice), from the previously frozen
reference wlrctl protocol. One-shot wlrctl appeared/disappeared before the
headless app had bound wl_pointer: logs show seat.get_pointer after device
removal, so no button event/serial arrived. Keep one fixture alive, wait for
wl_pointer binding, then issue absolute motion and clicks over stdin. This
changes input stimulus ownership only; it adds no production popup workaround
and uses the same private WAYLAND_DISPLAY as every test child. No CUA injection,
host display, new backend, runtime patch or custom shell input path is used.

Prepare the fixture (small C compilation, no native runtime build):

```sh
mkdir -p .tmp/pointer-build
# use installed wayland-scanner; sandbox currently has it in the Nix store
wayland-scanner client-header tests/bar/virtual-pointer.xml .tmp/pointer-build/virtual-pointer.h
wayland-scanner private-code tests/bar/virtual-pointer.xml .tmp/pointer-build/virtual-pointer.c
source runtime/dev-env.sh
cc -std=c11 -Wall -Wextra -Werror $(pkg-config --cflags wayland-client) -I.tmp/pointer-build tests/bar/pointer.c .tmp/pointer-build/virtual-pointer.c $(pkg-config --libs wayland-client) -o .tmp/pointer-build/pointer
```

After allocation, with existing pinned runtime and driver libraries:

```sh
python3 tests/bar/native.py --review-only --runtime /absolute/path/to/pleamar
```

Workload: six short/long/empty title × icon/no-icon captures and native width
measurements, same-root real Wayland clock-button click/popup grab/outside click,
four actual VM reloads with thread-name/count and FD snapshots, and one shared
native clock worker for both controllers. Empty-title captures explicitly set
private presentation text after opening a titled native window (Pleamar's test
window substitutes its default title for empty surface title); those captures
are not a claim of native service omission behavior.

The runner checks NVIDIA RTX5090 Vulkan/GLES2 app/compositor logs. Finally it
stops/waits/kills all owned apps/input fixture/Sway and the parent-owned private
bus. Media/logs are in tree-root .tmp with absolute printed host path. It is
intermediate correctness evidence, not visual/performance/physical acceptance.
