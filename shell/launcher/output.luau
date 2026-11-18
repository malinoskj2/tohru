-- Native per-output bootstrap names plus native rescreen text; no compositor IPC.
return function()
    local routes = {}
    local focused = text.launcher_focused_output or ""
    local function white(code)
        return (code >= 9 and code <= 13) or code == 32 or code == 133 or code == 160 or code == 5760 or (code >= 8192 and code <= 8202) or code == 8232 or code == 8233 or code == 8239 or code == 8287 or code == 12288
    end
    local function routable(name)
        if type(name) ~= "string" or name == "" or name:find(",",1,true) then return false end
        -- Native monitor_names uses Rust Unicode str::trim, preserving interiors.
        return not white(utf8.codepoint(name,1)) and not white(utf8.codepoint(name,utf8.offset(name,-1)))
    end
    local function names()
        local list,seen = {},{}
        local limit = fact.launcher_outputs_known and math.max(0,math.min(8,fact.launcher_output_count or 0)) or 8
        for index=0,limit-1 do
            local name=text["screen."..index..".name"]
            if routable(name) and not seen[name] then list[#list+1]=name;seen[name]=true end
        end
        return list,seen
    end
    local function choose(fresh,backdrop)
        local list,present=names()
        if not fresh and present[text.launcher_outputs] then
            if backdrop then text.launcher_backdrop_outputs=text.launcher_outputs end
            return
        end
        local name=present[focused] and focused or list[1] or ""
        -- The scrim's layer goes first, so the compositor stacks the card's above it.
        if backdrop then text.launcher_backdrop_outputs=name end
        text.launcher_outputs=name
    end
    -- A layer whose `open:` turns false is only drawn empty; the backdrop's own outputs going empty
    -- is what unmaps it as closing starts, which is when the Hyprland plugin brings the windows back.
    local function reconcile() if fact.launcher_visible then choose(false,true) end end
    local function open() fact.launcher_closing=false;choose(true,true) end
    local function close()
        -- Event payloads are native f32; keep exact integer cycle IDs in24 bits.
        fact.launcher_close_token = ((fact.launcher_close_token or 0) + 1) % 16777216
        fact.launcher_closing=true;text.launcher_backdrop_outputs=""
    end
    on("launcher_unmap",function(token)
        if fact.launcher_visible or token ~= fact.launcher_close_token then return end -- stale native event after reopen
        fact.launcher_closing=false;text.launcher_outputs=""
        if routes.unmapped then routes.unmapped() end
    end)
    on("launcher_outputs_changed",reconcile)
    for index=0,7 do on("text:screen."..index..".name",reconcile) end
    sys.watch("window",function(value)focused=value.monitor or "";text.launcher_focused_output=focused;reconcile()end)
    if fact.launcher_visible then choose(false,true)
    else
        text.launcher_backdrop_outputs=""
        if not fact.launcher_closing then text.launcher_outputs="" end
    end
    routes.open,routes.close=open,close
    return routes
end
