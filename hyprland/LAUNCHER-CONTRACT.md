# Launcher and recession

The recession effect and the launcher do not talk to each other. They meet at a layer-shell namespace.

- The launcher (`shell/launcher/entry.plm`) maps a full-screen overlay layer named `tohru-launcher-backdrop` on the output it opens on, through Pleamar patch 0005's `layer_namespace` property. It unmaps that layer once its closing animation has settled. Its 1 px bootstrap surface keeps the default `pleamar` namespace.
- The plugin listens for Hyprland's `layer.opened` and `layer.closed` events and matches the namespace against `plugin:hyprrecede:namespace`, a `std::regex`. Set it to `^tohru-launcher-backdrop$`, as `examples/effects.lua` does. The plugin's own default is still j2bar's `^j2bar-launcher-backdrop$`.

Do not match `pleamar`: every other Tohru layer uses it, and the windows would recede whenever the bar or a corner maps.

Each monitor recedes on its own, and stays receded until the last matching layer on it closes. Closing animates back; removing an output drops its state. The plugin only reacts to layers that open after it is loaded, so a launcher already open when the plugin loads, or when the regex changes, needs closing and reopening.

Without the plugin, or on another compositor, the launcher works the same with no recession.
