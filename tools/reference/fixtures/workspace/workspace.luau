-- Rust owns the exact 240 Hz simulation; Luau publishes state to the scene.
local target, timer = 1, nil
local function update()
    local values, moving = workspace_tick(target)
    for i = 1, 24 do fact["v" .. i] = values[i] or 0 end
    fact.target = target
    if not moving and timer then cancel(timer); timer = nil end
end
local function select(i)
    target = math.clamp(i, 1, 5)
    update()
    if not timer then
        local _, moving = workspace_tick(target)
        if moving then timer = every(1000 / 60, update) end
    end
end
update()
on("go", function(i)
    if workspace_mode == "live" then
        if workspace_sway() then workspace_select(i) else sys.call("workspaces.focus", i) end
    else select(i) end
end)
if workspace_mode == "replay" or workspace_mode == "active" then
    timer = every(1000 / 60, update)
elseif workspace_mode == "live" then
    if not workspace_sway() then
        sys.watch("workspaces", function(w) select(w.active) end)
    else
        -- Query the bounded native Sway IPC adapter every 100ms.
        every(100, function()
            local active = workspace_external()
            if active ~= target then select(active) end
        end)
    end
end
