local search = assert(loadfile("shell/launcher/search.luau"))()
local apps = {
    {id = "subsequence", name = "Fast Image Reader"},
    {id = "substring", name = "Open Firefox"},
    {id = "prefix-a", name = "Firefox"},
    {id = "prefix-b", name = "Firefox Beta"},
    {id = "metadata", name = "Browser", keywords = "fire;web;"},
    {id = "missing", name = "Calculator"},
}
local result = search(" FIRE ", apps)
assert(#result == 5)
assert(result[1].id == "prefix-a" and result[2].id == "prefix-b" and result[3].id == "metadata")
assert(result[4].id == "substring" and result[5].id == "subsequence")
assert(#search("doesnotmatch", apps) == 0)
assert(#search("fier", {{name = "fire"}}) == 0) -- no typo/transposition framework
assert(search("   ", apps) == apps)
assert(#search("éan", {{name = "écran"}}) == 1) -- whole UTF-8 codepoints
local many = {}
for i = 1, 100 do many[i] = {name = "Test " .. i, id = tostring(i)} end
result = search("test", many)
assert(#result == 50 and result[1].id == "1" and result[50].id == "50")
print("Simple native-catalog fuzzy prefix/substring/subsequence/stable-bound fixtures passed")
