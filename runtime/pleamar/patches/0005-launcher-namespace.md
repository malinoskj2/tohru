# Optional launcher namespace

0005-launcher-namespace.patch is against exact clean pin dc902feb0ee9058b7db7c2bc2c9d59d9f655fd86.
S applies once after other shared patches and builds the existing package/runtime.
It adds only optional STATIC layer_namespace quoted property to Surface, native
language vocabulary/compiler and existing create_layer_surface argument. Unset
still creates namespace pleamar; window/lock reject it;1..128ASCII ._- identifiers.
No generic Pleamar matching, runtime service, renderer, helper or IPC extension.
Changing namespace on an existing role requires process restart; the feature
constant never changes dynamically. Default native scenes are unaffected.

Launcher named fullscreen layer uses EXACT tohru-launcher-backdrop per E effects
LAUNCHER-CONTRACT.md. Basic launcher has no plugin/compositor dependency. Feature
code supplies focused/first native output on open and clears its screens text
only once native closing spring is settled; existing native rescreen removes the
role (open:false alone merely paints empty and does not unmap). Bootstrap stays
transparent default namespace with empty input/no reserve and no rest animation.
No separate layer lifetime patch, generic pleamar match or catcher.

Source-only git apply --check passed against fresh clean-pin copies in root.tmp;
no source-reference edits or heavy/native build. S owns prep/shared patch list.
After shared build run actual parser matrix:

    python3 tests/launcher/namespace.py --runtime ABS_RUNTIME

Then separately allocated private native workload must verify actual role create/
namespace/map-unmap, focused/fallback/hotplug, settled close/rapid reopen/reload,
no permanent recession and no recurring frames at rest. Parser/source evidence
is not this native lifecycle or plugin/physical/performance acceptance.
