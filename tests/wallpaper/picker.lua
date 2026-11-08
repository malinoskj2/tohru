local make_wallpaper=assert(loadfile("shell/wallpaper/controller.luau"))()
local make_picker=assert(loadfile("shell/wallpaper/picker.luau"))()
local count=0
local function check(name, body)
    local ok, err=pcall(body)
    assert(ok,name .. ": " .. tostring(err))
    count=count+1
end
local function fixture(configure)
    text={wallpaper_request_output="",wallpaper_picker_search="",wallpaper_randomize_outputs=""}
    fact,model={},{wallpaper_picker_files={}}
    local s={handlers={},calls={},accepted=0,folder_saves=0,value={folder="/chosen",assignments={A="/chosen/old.png"}},
        connected={{name="A",routable=true},{name="B",routable=true}},catalog={},identities={}}
    for i=1,19 do s.catalog[i]="/chosen/file" .. i .. ".png" end
    on=function(name,handler) s.handlers[name]=handler end
    sys={ask=function(name,args)
        s.calls[#s.calls+1]={name=name,args=args}
        if name == "wallpaper.catalog" then
            if s.catalog_failure then error("catalog failed") end
            local copy={}; for i,path in ipairs(s.catalog) do copy[i]=path end; return copy
        end
        if name == "wallpaper.identities" then
            local result={};for i,path in ipairs(args) do result[i]=s.identities[path] or path end; return result
        end
        error("unexpected native query: " .. name)
    end}
    json={decode=function() error("unexpected JSON in fixture") end}
    if configure then configure(s) end
    s.wallpaper=make_wallpaper()
    s.picker=make_picker(s.wallpaper,function() return s.value end,function(map)
        s.proposed=map
        if s.commit_failure then error("settings persist failed") end
        s.value.assignments=map;s.accepted=s.accepted+1
    end,function(folder)
        if s.commit_failure then error("folder persist failed") end
        s.value.folder=folder;s.folder_saves=s.folder_saves+1
    end,function() return s.connected end)
    return s
end
check("page and search publish at most8 files and choose correct global index",function()
    local s=fixture()
    assert(s.picker.open() and fact.wallpaper_picker_open and #model.wallpaper_picker_files==8)
    s.handlers.wallpaper_picker_page(1)
    assert(model.wallpaper_picker_files[1].name=="file9.png")
    assert(s.picker.choose(2) and s.value.assignments.A=="/chosen/file11.png" and s.accepted==1)
    text.wallpaper_picker_search="file19"
    s.handlers["text:wallpaper_picker_search"]()
    assert(#model.wallpaper_picker_files==1 and model.wallpaper_picker_files[1].name=="file19.png")
end)
check("failed commit never mutates returned owner snapshot",function()
    local s=fixture(function(v) v.commit_failure=true end)
    assert(s.picker.open())
    local original=s.value.assignments
    assert(not s.picker.choose(0) and s.accepted==0 and s.value.assignments==original and original.A=="/chosen/old.png")
    assert(text.wallpaper_picker_error:find("persist failed"))
end)
check("disconnected assignment remains selectable and clear commits one map",function()
    local s=fixture(function(v) v.value.assignments.Z="/chosen/offline.mp4" end)
    assert(s.picker.open())
    s.handlers.wallpaper_picker_output(-1)
    assert(text.wallpaper_picker_output=="Z · disconnected")
    s.handlers.wallpaper_picker_clear()
    assert(s.accepted==1 and not s.value.assignments.Z and s.value.assignments.A=="/chosen/old.png")
end)
check("folder save waits for successful catalog and persistence",function()
    local s=fixture()
    assert(s.picker.open())
    s.catalog_failure=true;text.wallpaper_picker_folder="/other"
    s.handlers.wallpaper_picker_folder_submit()
    assert(s.folder_saves==0 and s.value.folder=="/chosen")
    s.catalog_failure=false;s.commit_failure=true
    s.handlers.wallpaper_picker_folder_submit()
    assert(s.folder_saves==0 and s.value.folder=="/chosen")
    s.commit_failure=false;s.handlers.wallpaper_picker_folder_submit()
    assert(s.folder_saves==1 and s.value.folder=="/other")
end)
check("randomization uses actual canonical aliases and one atomic map commit",function()
    local s=fixture(function(v)
        v.catalog={"/chosen/one.png","/chosen/two.png","/chosen/three.png"}
        v.value.assignments={A="/aliases/one.png",B="/chosen/three.png",Z="/elsewhere/retained.png"}
        v.identities["/aliases/one.png"]="/chosen/one.png"
    end)
    assert(s.picker.open())
    math.randomseed(7)
    assert(s.picker.randomize({"A"}) and s.accepted==1)
    assert(s.value.assignments.A=="/chosen/two.png" and s.value.assignments.B=="/chosen/three.png" and s.value.assignments.Z=="/elsewhere/retained.png")
    local identity_calls=0;for _,call in ipairs(s.calls) do if call.name=="wallpaper.identities" then identity_calls=identity_calls+1 end end
    assert(identity_calls==1)
end)
check("empty target selection rerolls canonical catalog member while retaining unrelated image",function()
    local s=fixture(function(v)
        v.catalog={"/chosen/one.png","/chosen/two.png"}
        v.value.assignments={A="/alias/one.png",Z="/outside/retained.png"}
        v.identities["/alias/one.png"]="/chosen/one.png"
    end)
    assert(s.picker.randomize({}) and s.accepted==1 and s.value.assignments.A=="/chosen/two.png" and s.value.assignments.Z=="/outside/retained.png")
end)
check("invalid targets and catalog overflow reject without commit",function()
    local s=fixture()
    assert(not s.picker.randomize({wrong="A"}) and s.accepted==0 and #s.calls==0)
    s.catalog={};for i=1,4097 do s.catalog[i]="/chosen/" .. i .. ".png" end
    assert(not s.picker.open() and #model.wallpaper_picker_files==0)
    assert(not s.picker.randomize({"A"}) and s.accepted==0)
end)
check("empty folder avoids native identity request and bounded4096 catalog stays paged",function()
    local s=fixture(function(v) v.value.folder="";v.catalog={} end)
    assert(s.picker.open() and #model.wallpaper_picker_files==0 and #s.calls==0)
    assert(s.picker.randomize({}) and s.accepted==1 and #s.calls==0)
    s=fixture(function(v) for i=1,4096 do v.catalog[i]="/chosen/" .. i .. ".png" end end)
    assert(s.picker.open() and #model.wallpaper_picker_files==8 and text.wallpaper_picker_count:find("4096"))
end)
check("navigation clamps pages; requested output and current no-op preserve state",function()
    local s=fixture(function(v) text.wallpaper_request_output="B";v.value.assignments.B=v.catalog[1] end)
    assert(s.picker.open() and text.wallpaper_picker_output=="B")
    assert(s.picker.choose(0) and s.accepted==0)
    s.handlers.wallpaper_picker_page(-1)
    assert(not fact.wallpaper_picker_previous)
    s.handlers.wallpaper_picker_move(1)
    assert(fact.wallpaper_picker_selected==1)
    s.handlers.wallpaper_picker_close()
    assert(not fact.wallpaper_picker_open)
end)
check("no outputs cannot assign/randomize selected output and malformed JSON cannot reroll all",function()
    local s=fixture(function(v) v.connected={};v.value.assignments={} end)
    assert(s.picker.open())
    assert(not s.picker.choose(0) and s.accepted==0)
    s.handlers.wallpaper_picker_randomize()
    assert(s.accepted==0 and text.wallpaper_picker_error:find("Choose an output"))
    text.wallpaper_randomize_outputs="{}"
    s.handlers.wallpaper_randomize()
    assert(s.accepted==0 and text.wallpaper_picker_error:find("JSON array"))
end)
check("new output cannot exceed32 retained assignments before commit",function()
    local s=fixture(function(v)
        v.value.assignments={}
        for i=1,32 do v.value.assignments["OFFLINE" .. i]="/chosen/old.png" end
        text.wallpaper_request_output="A"
    end)
    assert(s.picker.open() and text.wallpaper_picker_output=="A")
    assert(not s.picker.choose(0) and s.accepted==0 and text.wallpaper_picker_error:find("capacity"))
end)
print("wallpaper production picker: " .. count .. " cases passed")
