-- Candidate private-compositor configuration. Load only desktop-effects first.
-- These are the authored j2bar selected-effect defaults, not observed live settings.
-- Nix owns generating/installing the final compositor configuration.
hl.config({ plugin = {
    hyprfocus = {
        enable = true, animate_floating = true, only_on_monitor_change = false,
        keyboard_focus_animation = "shrink", mouse_focus_animation = "shrink",
        fade_opacity = 0.8, shrink_percentage = 0.99, slide_height = 20,
        class = "^(?!xdg-desktop-portal-gtk$).*$",
    },
    hyprbars = {
        enabled = true, bar_color = "rgba(11111b8c)", ["col.text"] = "rgba(ffffffff)",
        inactive_button_color = "rgba(00000000)", bar_height = 12,
        bar_text_size = 10, bar_text_weight = 400, bar_title_enabled = false,
        bar_blur = false, bar_text_font = "Sans", bar_text_align = "center",
        bar_part_of_window = true, bar_precedence_over_border = true,
        bar_buttons_alignment = "right", bar_padding = 7, bar_button_padding = 5,
        icon_on_hover = false, on_double_click = "tohru:maximize",
    },
    hyprsheet = {
        class = "^(xdg-desktop-portal-gtk)$", parent_share = 0.75,
        min_size = { 700, 450 }, dip = 0.99,
    },
    hyprrecede = {
        -- Optional proposed shell adapter namespace; see LAUNCHER-CONTRACT.md.
        namespace = "^tohru-launcher-backdrop$",
        angle = 16, scale = 0.86, perspective = 1400, shade = 0.62, blur = 1,
    },
} })

-- Preserve the configured-application whitelist without the old managed-rules API.
-- Compositor rules use RE2 (negative: prefix), not focus/sheet's std::regex syntax.
hl.window_rule({ name = "desktop-effects-titlebars",
    match = { class = "negative:^(Alacritty|mpv)$" }, ["hyprbars:no_bar"] = true })

hl.curve("desktopEffectsDip", { type = "bezier", points = { { 0.25, 1 }, { 0.5, 1 } } })
hl.curve("desktopEffectsLand", { type = "spring", stiffness = 625, dampening = 42.5, mass = 1 })
hl.curve("desktopEffectsSettle", { type = "bezier", points = { { 0.3, 1.25 }, { 0.4, 1 } } })
hl.curve("desktopEffectsEase", { type = "bezier", points = { { 0.25, 0.1 }, { 0.25, 1 } } })
-- The leaves exist only once the plugin is loaded, which at startup is after this file has run,
-- and an unknown leaf is an error that would end the configuration here.
local function plugin_animations()
    hl.animation({ leaf = "hyprfocusIn", enabled = true, speed = 1.5, bezier = "desktopEffectsDip" })
    hl.animation({ leaf = "hyprfocusOut", enabled = true, speed = 4, bezier = "desktopEffectsDip" })
    hl.animation({ leaf = "hyprsheetIn", enabled = true, speed = 7, spring = "desktopEffectsLand" })
    hl.animation({ leaf = "hyprsheetOut", enabled = true, speed = 7, spring = "desktopEffectsLand" })
    hl.animation({ leaf = "hyprrecedeTilt", enabled = true, speed = 6.2, bezier = "desktopEffectsSettle" })
    hl.animation({ leaf = "hyprrecedeShade", enabled = true, speed = 5, bezier = "desktopEffectsEase" })
    hl.animation({ leaf = "hyprrecedeBlur", enabled = true, speed = 2.4, bezier = "desktopEffectsEase" })
end
if not pcall(plugin_animations) then
    hl.on("hyprland.start", function()
        hl.timer(function() pcall(plugin_animations) end, { timeout = 1, type = "oneshot" })
    end)
end
