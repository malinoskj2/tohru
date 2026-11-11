# 0005: layer namespace

Stock Pleamar gives every layer surface the layer-shell namespace `pleamar`. This patch adds an optional surface property that names it instead:

```
surface launcher_layer {
    layer_namespace: "tohru-launcher-backdrop"
    ...
}
```

The value is static: it is read when the scene is compiled, so changing it needs a process restart. The compiler accepts 1 to 128 ASCII letters, digits, `.`, `_` or `-`, and rejects the property on windows and lock surfaces. Surfaces without it keep the `pleamar` namespace.

The launcher uses it so the Hyprland plugin's recession effect can match its layer (`plugin:hyprrecede:namespace = "^tohru-launcher-backdrop$"`) without matching every other Pleamar layer. Only the launcher's full-screen layer is named; its 1 px bootstrap surface keeps `pleamar`.

## Changes

| File | Change |
|---|---|
| `src/scene.rs` | `Surface` gains `layer_namespace: Option<String>` |
| `src/language/vocabulary.rs` | `layer_namespace` is a known surface property |
| `src/language/compiler.rs` | Parses and validates it |
| `src/platform/wayland.rs` | Passes it to `create_layer_surface`, and stores it in `BackdropKind::Layer` |
| `src/platform/hyprland.rs` | `layer_position` takes the namespace to look for |

The last two matter for glass. On Hyprland, Pleamar finds where a layer really is by asking Hyprland for the layers on its monitor and matching this process's layer by namespace and size. Before the patch that lookup only matched `pleamar`, so it missed the launcher's layer and fell back to placing it by its anchor. With the bar's 31 px exclusive zone pushing the layer down, the launcher's glass then captured the wrong part of the screen.

## Checking it

```sh
python3 tests/launcher/namespace.py --runtime "$PWD/.tmp/pleamar-runtime/target/release/pleamar"
```

This runs `pleamar --check` on small scenes: no namespace, a valid one, the longest valid one, and the invalid cases (empty, too long, a space, non-ASCII, on a window, on a lock surface). It does not start a compositor.

To see the namespace on a running session, open the launcher and look for `tohru-launcher-backdrop` in `hyprctl layers`.
