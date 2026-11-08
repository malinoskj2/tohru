-- Root-owned batch policy. The W picker keeps its independent per-output actions.
return function(wallpaper, read, commit, connected_outputs, prepare)
    local presets={"spectrum","dusk","tide","ember","moss","solid"}
    local palettes={mocha=true,macchiato=true,rose_pine=true,nord=true,gruvbox=true,tokyo_night=true,dracula=true,everforest=true}
    local function snapshot()
        local value=read()
        local assignments,count={},0
        for output,path in pairs(value.assignments)do
            wallpaper.validate(output,path);assignments[output]=path;count=count+1
            if count>32 then error("Wallpaper assignment capacity is32 outputs")end
        end
        return value,assignments,count
    end
    local function outputs()
        local records=connected_outputs()
        if type(records)~="table" or #records>32 then error("Native connected output information is unavailable")end
        local names,seen={},{}
        for _,record in ipairs(records)do
            if record.routable~=false then
                wallpaper.validate(record.name,"/validation.png")
                if seen[record.name]then error("Duplicate native output name")end
                seen[record.name]=true;names[#names+1]=record.name
            end
        end
        if #names==0 then error("No connected routable wallpaper output")end
        table.sort(names)
        return names
    end
    local function generated(preset,palette)
        local result=prepare(preset,palette)
        if type(result)~="table" or type(result.path)~="string" or type(result.durable)~="boolean" or type(result.cached)~="boolean"
            or wallpaper.validate("preset",result.path)~="image" or result.path:sub(-4)~=".png" then error("Invalid native fixed-preset result")end
        return result
    end
    local function rotate()
        text.wallpaper_picker_warning=""
        local names=outputs()
        local value,assignments,count=snapshot()
        for _,name in ipairs(names)do if not assignments[name]then count=count+1 end end
        if count>32 then error("Wallpaper assignment capacity is32 outputs; nothing was changed")end
        if value.folder~="" then
            local files=sys.ask("wallpaper.catalog",value.folder)
            if type(files)~="table" or #files==0 or #files>4096 then error("Configured wallpaper folder has no bounded media catalog")end
            for _,path in ipairs(files)do wallpaper.validate("catalog",path)end
            for i=#files,2,-1 do local j=math.random(i);files[i],files[j]=files[j],files[i]end
            -- Resolve current connected assignments only. Offline entries remain
            -- literal identities without filesystem reads or mutation.
            local paths,identities={},{}
            for _,path in pairs(assignments)do identities[path]=path end
            for _,path in ipairs(files)do identities[path]=path end
            for _,name in ipairs(names)do if assignments[name]then paths[#paths+1]=assignments[name]end end
            if #paths>0 then
                local keys=sys.ask("wallpaper.identities",paths)
                if type(keys)~="table" or #keys~=#paths then error("Canonical wallpaper identities are unavailable")end
                for i,path in ipairs(paths)do identities[path]=keys[i]end
            end
            commit({assignments=wallpaper.randomize(assignments,names,files,function(path)return identities[path]end)})
        else
            local index=1
            if value.preset then
                index=nil;for i,name in ipairs(presets)do if name==value.preset then index=i;break end end
                if not index then error("Invalid accepted wallpaper preset")end
            end
            local preset=presets[index%#presets+1]
            local result=generated(preset,value.palette)
            for _,name in ipairs(names)do assignments[name]=result.path end
            commit({assignments=assignments,preset=preset,palette=value.palette})
            if not result.durable then text.wallpaper_picker_warning="Preset ready; disk durability could not be confirmed"end
        end
        return true
    end
    local function palette(proposed)
        text.wallpaper_picker_warning=""
        if not palettes[proposed]then error("Choose a canonical retained palette")end
        local value,assignments=snapshot()
        if value.preset=="solid" and value.palette~=proposed then
            local previous=generated("solid",value.palette)
            local active=false
            for _,path in pairs(assignments)do if path==previous.path then active=true;break end end
            if not active then commit({palette=proposed});return true end
            local result=generated("solid",proposed)
            for output,path in pairs(assignments)do if path==previous.path then assignments[output]=result.path end end
            commit({assignments=assignments,preset="solid",palette=proposed})
            if not previous.durable or not result.durable then text.wallpaper_picker_warning="Preset ready; disk durability could not be confirmed"end
        else commit({palette=proposed})end
        return true
    end
    return {rotate=rotate,palette=palette}
end
