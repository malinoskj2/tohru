-- Pure episode policy. Only a maintained desktop idle source may call idle().
-- No timer infers input inactivity; a failed/ineligible roll consumes the episode.
return function(scenes)
    local settings = {enabled=false, probability=50, seed=1, force="auto", quiet=false}
    local generation, episode, idle, last = nil, 0, false, nil
    local active, observations, decision = nil, {}, nil
    local function decided(value)decision=value;return value end

    local function blocked(s)
        -- Unknown session/lock/inhibition/output state is unsafe. Conversation
        -- flags are supplied by the one actor arbiter, never chat timestamps.
        return s.unlocked ~= true or s.session_active ~= true or s.output_ready ~= true
            or s.inhibited ~= false or s.listening or s.speaking or s.pending
            or s.text_entry or s.dnd or s.entrance or s.puppet_busy
    end
    local function stop(reason)
        if not active then return nil end
        local old = active
        active = nil
        return {kind="stop", scene=old.scene, generation=generation, episode=old.episode,
            immediate=reason=="lock" or reason=="session" or reason=="output" or reason=="source",
            reason=reason}
    end
    local function eligible()
        local result = {}
        if blocked(observations) then return result end
        for _, scene in ipairs(scenes) do
            if settings.scenes[scene.id] ~= false
                and observations.supported and observations.supported[scene.id] == true
                and (not scene.quiet or settings.quiet and observations.reduced == true)
                and (not scene.broad or observations.fullscreen == false)
                and (not scene.window or observations.window_ready == true)
                and (not scene.occlusion or observations.occlusion_ready == true)
                and (not observations.reduced or settings.quiet and scene.quiet) then
                result[#result+1] = scene.id
            end
        end
        return result
    end
    local function xor(a,b)
        if bit32 then return bit32.bxor(a,b) end
        -- Lua 5.4 policy fixtures lack Luau's built-in bit32. This bounded
        -- arithmetic equivalent operates only once per idle decision.
        local result,place=0,1
        while a>0 or b>0 do
            if a%2~=b%2 then result=result+place end
            a,b,place=math.floor(a/2),math.floor(b/2),place*2
        end
        return result
    end
    -- Products remain exact in Luau's doubles. XOR folding prevents the keyed
    -- episode counter from becoming an arithmetic progression of chance rolls.
    -- Domain-separated streams keep chance independent of eligible-list length.
    local function random(stream)
        local n = (settings.seed + episode * 104729 + stream * 13007) % 2147483646 + 1
        for _=1,4 do
            n=xor(n,math.floor(n/65536))
            n=n*48271%2147483647
            n=xor(n,math.floor(n/128))
        end
        return n / 2147483647
    end
    local function observe(s)
        observations = s
        if not active then return nil end
        if s.unlocked ~= true then return stop("lock") end
        if s.session_active ~= true then return stop("session") end
        if s.output_ready ~= true or s.output ~= active.output then return stop("output") end
        if blocked(s) then return stop("interaction") end
        local choices = eligible()
        for _, id in ipairs(choices) do if id == active.scene then return nil end end
        return stop("eligibility")
    end
    return {
        configure = function(value)
            local changed = stop("config")
            settings = {enabled=value.enabled == true, probability=value.probability or 50,
                seed=value.seed or 1, force=value.force or "auto", quiet=value.quiet == true,
                scenes=value.scenes or {}}
            assert(settings.probability >= 0 and settings.probability <= 100)
            assert(settings.seed >= 0 and settings.seed <= 2147483646 and settings.seed % 1 == 0)
            return changed
        end,
        source = function(token)
            if token == generation then return nil end
            local changed = stop("source")
            generation = token
            -- Reconnecting a source does not imply activity or reset replay order.
            return changed
        end,
        observe = observe,
        idle = function(token,sequence)
            if generation == nil or token ~= generation then return nil end
            if sequence~=nil then
                if type(sequence)~="number" or sequence%1~=0 or sequence<=episode or sequence>2147483646 then return nil end
                episode=sequence
            else
                if idle then return nil end
                episode=episode+1
            end
            idle = true
            if not settings.enabled then return decided({kind="skip",reason="disabled",episode=episode}) end
            local choices = eligible()
            if #choices == 0 then return decided({kind="skip", reason="ineligible", episode=episode}) end
            if settings.force ~= "auto" then
                local found = false
                for _, id in ipairs(choices) do if id == settings.force then found=true end end
                if not found then return decided({kind="skip",reason="forced-ineligible",episode=episode}) end
                choices = {settings.force}
            elseif #choices > 1 then
                local alternatives = {}
                for _, id in ipairs(choices) do if id ~= last then alternatives[#alternatives+1]=id end end
                choices = alternatives
            end
            local roll = random(1) * 100
            -- Force selects a scene but never bypasses probability or safety.
            if roll >= settings.probability then return decided({kind="skip",reason="chance",roll=roll,episode=episode,seed=settings.seed}) end
            local id = choices[math.floor(random(2) * #choices) + 1]
            active = {kind="start",scene=id,output=observations.output,episode=episode,
                generation=generation,seed=settings.seed,roll=roll}
            last = id
            return decided(active)
        end,
        resume = function(token)
            if generation == nil or token ~= generation then return nil end
            idle = false
            return stop("input")
        end,
        finished = function(token, sequence)
            if token == generation and active and active.episode == sequence then active=nil end
        end,
        state = function() return {episode=episode,idle=idle,last=last,active=active,generation=generation,decision=decision} end,
        eligible = eligible,
    }
end
