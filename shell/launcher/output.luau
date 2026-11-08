-- Native per-output bootstrap names plus native rescreen text; no compositor IPC.
return function()
    local focused = ""
    local function names()
        local list,seen = {},{}
        local limit = fact.launcher_outputs_known and math.max(0,math.min(8,fact.launcher_output_count or 0)) or 8
        for index=0,limit-1 do
            local name=text["screen."..index..".name"]
            if type(name)=="string" and name~="" and not name:find("[,%s]") and not seen[name] then list[#list+1]=name;seen[name]=true end
        end
        return list,seen
    end
    local function choose(fresh)
        local list,present=names()
        if not fresh and present[text.launcher_outputs] then return end
        text.launcher_outputs=present[focused] and focused or list[1] or ""
    end
    local function reconcile() if fact.launcher_visible then choose(false) end end
    local function open() fact.launcher_closing=false;choose(true) end
    local function close() fact.launcher_closing=true end
    on("launcher_unmap",function()
        if fact.launcher_visible then return end -- stale native event after reopen
        fact.launcher_closing=false;text.launcher_outputs=""
    end)
    on("launcher_outputs_changed",reconcile)
    for index=0,7 do on("text:screen."..index..".name",reconcile) end
    sys.watch("window",function(value)focused=value.monitor or "";reconcile()end)
    if fact.launcher_visible then choose(false)
    elseif not fact.launcher_closing then text.launcher_outputs="" end
    return {open=open,close=close}
end
