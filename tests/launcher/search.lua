local fuzzy = assert(loadfile("shell/launcher/fuzzy.luau"))()
local search = assert(loadfile("shell/launcher/search.luau"))()(fuzzy)

-- Scores printed by frizbee 0.13.0 itself (CaseMatching::Ignore, AVX-512).
local long = "start " .. string.rep("x", 2000) .. "end"
local miss = "a" .. string.rep("z", 1100) .. "b"
local front = string.rep("0123456789", 6) .. "0123foo"
local split = "f" .. string.rep("0123456789", 6) .. "0123oo"
local description = "a terminal emulator for wayland, with a long description to pass sixty-four bytes"
for _, case in ipairs({
    {"fire", "firefox", 0, 76},
    {"ffx", "firefox", 0, 48},
    {"code", "visual studio code", 0, 68},
    {"code", "unicodes", 0, 64},
    {"vim", "vim", 0, 68},
    {"vim", "vimdiff", 0, 60},
    {"xyz", "firefox", 0, nil},
    {"xof", "firefox", 0, nil},
    {"firefoxes", "firefox", 0, nil},
    {"f", "", 0, nil},
    {"chromuim", "chromium", 0, nil},
    {"chromuim", "chromium", 2, 114},
    {"dolhpin", "dolphin", 1, 98},
    {"vmi", "vim", 0, nil},
    {"chromxyz", "chromium", 2, nil},
    {"start", long, 0, 92},
    {"end", long, 0, 48},
    {"ends", long, 0, nil},
    {"se", long, 0, 4},
    {"abc", miss, 1, 0},
    {"a", "abc", 0, 28},
    {"b", "abc", 0, 16},
    {"a", "babc", 0, 16},
    {"foo", "fooBar", 0, 60},
    {"bar", "fooBar", 0, 48},
    {"bar", "foo_bar", 0, 52},
    {"fb", "foo-bar", 0, 41},
    {"test", "Utooooeoooosoooot", 0, 40},
    {"test", "Utooooooeoooooosoooooot", 0, 34},
    {"foo", "Ufooo", 1, 48},
    {"foo", "Ufo", 1, 27},
    {"abc", "xaxbxcx", 0, 38},
    {"foo", "f_______________o_______________o", 0, 30},
    {"aa", "a", 0, nil},
    {"ab", "a", 1, 23},
    {"abcdefghijklmn", "abcdefghijklmn", 0, 244},
    {"abcdefghijklm", "abcdefghijklm", 0, 228},
    {"foo", front, 0, 48},
    {"foo", split, 0, 32},
    {"terminal emulator", description, 0, 280},
    {"terminl emulatr", description, 3, 238},
    {"gogle chrom", "google chrome", 2, 187},
    {"pavucontrl", "pavucontrol", 2, 167},
    {"cran", "écran", 0, 64},
}) do
    local got = fuzzy(case[1], case[3])(case[2])
    assert(got == case[4], ("%s in %s with %d typos: %s, not %s"):format(case[1], case[2]:sub(1, 40), case[3], tostring(got), tostring(case[4])))
end
-- frizbee scores a non-ASCII needle by codepoint (54 here); the port scores its bytes.
assert(fuzzy("éan", 0)("écran") == 70)

-- j2bar's Catalog::search over these entries in its name order, with Pleamar's Exec: j2bar matched
-- Exec with its field codes, so "f", "fire ", "chromuim" and "%u" differ from it there.
local apps = {
    {id = "blender", name = "Blender", local_name = "Blender", generic_name = "3D modeler", keywords = "3d;cg;modeling;animation;painting;sculpting;texturing;video editing;video tracking;rendering;render engine;cycles;python;", categories = "Graphics;3DGraphics;", exec = "/nix/store/0h3y8k2mzqv7x1cn5w9a4lr6p2fb8dsj-blender-4.5.3/bin/blender"},
    {id = "btop", name = "btop++", local_name = "btop++", generic_name = "System Monitor", keywords = "system;process;task;", categories = "System;Monitor;ConsoleOnly;", exec = "btop"},
    {id = "chromium-browser", name = "Chromium", local_name = "Chromium", generic_name = "Web Browser", keywords = "", categories = "Network;WebBrowser;", exec = "chromium"},
    {id = "jetbrains-datagrip", name = "DataGrip", local_name = "DataGrip", generic_name = "SQL IDE", keywords = "sql;database;jetbrains;", categories = "Development;IDE;", exec = "datagrip"},
    {id = "org.gnome.Nautilus", name = "Files", local_name = "Dateien", generic_name = "File Manager", keywords = "folder;manager;explore;disk;filesystem;", categories = "GNOME;GTK;Utility;Core;FileManager;", exec = "nautilus --new-window"},
    {id = "firefox", name = "Firefox", local_name = "Firefox", generic_name = "Web Browser", keywords = "internet;www;browser;web;explorer;", categories = "Network;WebBrowser;", exec = "firefox --name firefox"},
    {id = "foot", name = "Foot", local_name = "Foot", generic_name = "Terminal", keywords = "shell;prompt;command;commandline;", categories = "System;TerminalEmulator;", exec = "foot"},
    {id = "footclient", name = "Foot Client", local_name = "Foot Client", generic_name = "Terminal", keywords = "shell;prompt;command;commandline;", categories = "System;TerminalEmulator;", exec = "footclient"},
    {id = "footy", name = "Footy Terminal", local_name = "Footy Terminal", generic_name = "Terminal emulator", keywords = "shell;prompt;command;commandline;cmd;", categories = "System;TerminalEmulator;Utility;", exec = "footy --working-directory=%k"},
    {id = "ghidra", name = "Ghidra", local_name = "Ghidra", generic_name = "Ghidra Software Reverse Engineering Suite", keywords = "reverse;engineering;disassembler;decompiler;", categories = "Development;Debugger;", exec = "ghidra"},
    {id = "google-chrome", name = "Google Chrome", local_name = "Google Chrome", generic_name = "Web Browser", keywords = "", categories = "Network;WebBrowser;", exec = "/nix/store/7rq1nzxw2cl8f0mbq9vj4h6ayk3gdp5s-google-chrome-142.0.7444.59/bin/google-chrome-stable"},
    {id = "htop", name = "Htop", local_name = "Htop", generic_name = "Process Viewer", keywords = "system;process;task;", categories = "System;Monitor;ConsoleOnly;", exec = "htop"},
    {id = "mpv", name = "mpv Media Player", local_name = "mpv Media Player", generic_name = "Multimedia player", keywords = "mpv;media;player;video;audio;tv;", categories = "AudioVideo;Audio;Video;Player;TV;", exec = "mpv --player-operation-mode=pseudo-gui --"},
    {id = "nvim", name = "Neovim wrapper", local_name = "Neovim wrapper", generic_name = "Text Editor", keywords = "text;editor;", categories = "Utility;TextEditor;", exec = "nvim"},
    {id = "nixos-manual", name = "NixOS Manual", local_name = "NixOS Manual", generic_name = "System Manual", keywords = "nixos;manual;help;documentation;", categories = "System;", exec = "nixos-help"},
    {id = "com.obsproject.Studio", name = "OBS Studio", local_name = "OBS Studio", generic_name = "Streaming/Recording Software", keywords = "streaming;recording;screencast;capture;", categories = "AudioVideo;Recorder;", exec = "obs"},
    {id = "orca", name = "Orca", local_name = "Orca", generic_name = "Agent IDE", keywords = "agents;worktrees;code;", categories = "Development;IDE;", exec = "orca --enable-features=UseOzonePlatform --ozone-platform=wayland"},
    {id = "pavucontrol", name = "pavucontrol", local_name = "pavucontrol", generic_name = "Volume Control", keywords = "pavucontrol;microphone;volume;fade;balance;headset;speakers;headphones;audio;mixer;output;input;devices;playback;recording;system settings;sound;sound card;settings;preferences;", categories = "AudioVideo;Audio;Mixer;GTK;Settings;", exec = "pavucontrol"},
    {id = "steam", name = "Steam", local_name = "Steam", generic_name = "", keywords = "", categories = "Network;FileTransfer;Game;", exec = "steam"},
    {id = "vim", name = "Vim", local_name = "Vim", generic_name = "Text Editor", keywords = "text;editor;", categories = "Utility;TextEditor;", exec = "vim"},
    {id = "code", name = "Visual Studio Code", local_name = "Visual Studio Code", generic_name = "Text Editor", keywords = "vscode;", categories = "Utility;TextEditor;Development;IDE;", exec = "code"},
}
local expected = {
    ["f"] = {{"firefox", 1966197.5}, {"foot", 1966190}, {"footclient", 1966190}, {"footy", 1966190}, {"org.gnome.Nautilus", 655490}, {"ghidra", 32}, {"com.obsproject.Studio", 32}, {"pavucontrol", 22.400000000000002}, {"steam", 20.9}, {"orca", 20}, {"blender", 16}, {"google-chrome", 16}},
    ["fi"] = {{"firefox", 1966277.5}, {"footclient", 1310870}, {"footy", 1310855}, {"org.gnome.Nautilus", 655570}, {"ghidra", 32}, {"steam", 25.7}, {"blender", 16}, {"google-chrome", 16}},
    ["fir"] = {{"firefox", 1966357.5}, {"org.gnome.Nautilus", 94}, {"ghidra", 50}, {"footy", 40}, {"steam", 28.4}, {"blender", 20}, {"google-chrome", 17}},
    ["fire"] = {{"firefox", 1966437.5}, {"ghidra", 82}, {"footy", 56}, {"steam", 30.8}, {"google-chrome", 27}},
    ["chr"] = {{"chromium-browser", 1966350}, {"google-chrome", 1310960}},
    ["chrom"] = {{"chromium-browser", 1966510}, {"google-chrome", 1311120}},
    ["chromuim"] = {{"chromium-browser", 1311270}, {"google-chrome", 77}},
    ["code"] = {{"code", 1311040}, {"com.obsproject.Studio", 88}, {"orca", 67.2}, {"foot", 48.800000000000004}, {"footclient", 48.800000000000004}, {"footy", 48.800000000000004}},
    ["vim"] = {{"vim", 1966390}, {"nvim", 1310940}, {"mpv", 29}, {"google-chrome", 16}},
    ["ob"] = {{"com.obsproject.Studio", 1966270}, {"chromium-browser", 40.5}, {"orca", 31}, {"google-chrome", 22}, {"blender", 20}},
    ["st"] = {{"steam", 1966282.5}, {"com.obsproject.Studio", 1310880}, {"code", 1310880}, {"btop", 76}, {"nixos-manual", 76}, {"ghidra", 60}, {"jetbrains-datagrip", 37.5}, {"blender", 36}, {"google-chrome", 36}, {"htop", 32.900000000000006}, {"pavucontrol", 31.200000000000003}, {"org.gnome.Nautilus", 25.6}, {"orca", 19}, {"foot", 11.4}, {"footclient", 11.4}, {"footy", 11.4}},
    ["term"] = {{"footy", 1311040}, {"foot", 152}, {"footclient", 152}, {"orca", 41}, {"google-chrome", 36}},
    ["web"] = {{"firefox", 127.5}, {"chromium-browser", 120}, {"google-chrome", 120}, {"blender", 24}},
    ["zzqx"] = {{"blender", 42}},
    ["e"] = {{"steam", 1310792.5}, {"firefox", 1310787.5}, {"blender", 1310780}, {"org.gnome.Nautilus", 1310780}, {"footclient", 1310780}, {"footy", 1310780}, {"google-chrome", 1310780}, {"mpv", 1310780}, {"nvim", 1310780}, {"code", 1310780}, {"ghidra", 40}, {"vim", 40}, {"htop", 34.5}, {"btop", 32}, {"chromium-browser", 32}, {"jetbrains-datagrip", 32}, {"foot", 32}, {"nixos-manual", 32}, {"com.obsproject.Studio", 32}, {"orca", 32}, {"pavucontrol", 32}},
    ["o"] = {{"com.obsproject.Studio", 1966190}, {"orca", 1966190}, {"firefox", 1310787.5}, {"htop", 1310782.5}, {"btop", 1310780}, {"chromium-browser", 1310780}, {"foot", 1310780}, {"footclient", 1310780}, {"footy", 1310780}, {"google-chrome", 1310780}, {"nvim", 1310780}, {"nixos-manual", 1310780}, {"pavucontrol", 1310780}, {"code", 1310780}, {"org.gnome.Nautilus", 42}, {"blender", 32}, {"ghidra", 32}, {"vim", 32}, {"mpv", 20}, {"steam", 17.3}, {"jetbrains-datagrip", 4.8}},
    ["files"] = {{"org.gnome.Nautilus", 655850}, {"google-chrome", 46}, {"steam", 37.7}},
    ["dat"] = {{"jetbrains-datagrip", 1966350}, {"org.gnome.Nautilus", 1966350}, {"ghidra", 70}, {"nixos-manual", 39.2}},
    ["browser"] = {{"firefox", 239.5}, {"chromium-browser", 232}, {"google-chrome", 232}},
    ["media"] = {{"mpv", 1311120}, {"com.obsproject.Studio", 104}},
    ["nvim"] = {{"nvim", 1311050}, {"google-chrome", 26}},
    ["htop"] = {{"htop", 1966472.5}},
    ["manual"] = {{"nixos-manual", 1311200}},
    ["mpv"] = {{"mpv", 1966350}},
    [" fire"] = {{"ghidra", 102}, {"firefox", 91.5}},
    ["fire "] = {{"firefox", 92.5}},
    ["web browser"] = {{"firefox", 407.5}, {"chromium-browser", 400}, {"google-chrome", 400}},
    ["terminal emulator"] = {{"footy", 592}},
    ["visual studio"] = {{"code", 1967170}},
    ["frefox"] = {{"firefox", 1311222.5}},
    ["blendr"] = {{"blender", 1311215}},
    ["gogle chrom"] = {{"google-chrome", 1311635}},
    ["pavucontrl"] = {{"pavucontrol", 1311535}},
    ["terminl emulatr"] = {{"footy", 492}},
    ["audio"] = {{"code", 1311060}, {"mpv", 80}, {"pavucontrol", 80}, {"com.obsproject.Studio", 27.599999999999998}},
    ["sys"] = {{"btop", 120}, {"nixos-manual", 120}, {"htop", 50.5}, {"pavucontrol", 48}, {"org.gnome.Nautilus", 38.400000000000006}, {"google-chrome", 22}, {"blender", 20}, {"foot", 18}, {"footclient", 18}, {"footy", 18}},
    ["editor"] = {{"nvim", 200}, {"vim", 200}, {"code", 200}},
    ["monitor"] = {{"btop", 232}, {"htop", 42.1}},
    ["%u"] = {},
    ["stream"] = {{"com.obsproject.Studio", 216}, {"google-chrome", 59}, {"orca", 50}},
    ["x"] = {{"firefox", 1310787.5}, {"nixos-manual", 1310780}, {"nvim", 32}, {"vim", 32}, {"code", 32}, {"blender", 16}, {"google-chrome", 16}, {"org.gnome.Nautilus", 12.8}, {"pavucontrol", 12.8}},
    ["studio code"] = {{"code", 1311620}},
    ["reverse engineering"] = {{"ghidra", 624}},
    ["dev"] = {{"ghidra", 64}, {"pavucontrol", 48}, {"jetbrains-datagrip", 18}, {"orca", 18}, {"code", 18}},
    ["nix"] = {{"nixos-manual", 1966350}, {"blender", 52}, {"google-chrome", 52}, {"firefox", 42.5}},
    ["bin"] = {{"jetbrains-datagrip", 63}, {"blender", 52}, {"google-chrome", 52}},
}
local counts = {firefox = 3, steam = 9, htop = 1}
for query, want in pairs(expected) do
    local got, scores = search(query, apps, counts)
    assert(#got == #want, query)
    for index, result in ipairs(want) do
        assert(got[index].id == result[1] and scores[index] == result[2],
            ("%q #%d: %s %s, not %s %s"):format(query, index, got[index].id, scores[index], result[1], result[2]))
    end
end

assert(#search("", apps) == 0)
assert(search("FIRE", apps)[1].id == "firefox")
local many = {}
for i = 1, 100 do many[i] = {name = "Test " .. i, id = tostring(i)} end
local result = search("test", many)
assert(#result == 50 and result[1].id == "1" and result[50].id == "50")
result = search("test", many, {["70"] = 1})
assert(result[1].id == "70" and result[2].id == "1")
local translated = {
    {id = "mpv", name = "mpv Media Player", local_name = "", names = {"Lecteur multimédia mpv", "mpv Media Player"}, exec = "mpv --", exec_line = "mpv -- %U"},
    {id = "other", name = "Other", local_name = "", names = {}, exec = "other"},
}
result = search("lecteur", translated)
assert(#result == 1 and result[1].id == "mpv")
result = search("%u", translated)
assert(#result == 1 and result[1].id == "mpv")
print("frizbee scores and j2bar's search ranking reproduced")
