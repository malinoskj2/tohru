-- Composition seam for the single native actor; never writes ai_flight facts.
-- actor: supported(), observe(), prepare(sample,ready), play(sample), release(stop)
-- source: start(callback)->handle, stop(handle). The systemd-owned source reports
-- {kind=ready|idle|resume|lost, generation=..., seed=...}; ready is NOT current idle.
return function(choreography, make_policy, actor, source, clock)
    local policy = make_policy(choreography.catalog)
    local handle, subscription, scene, timer, generation, source_seed
    local serial, elapsed, last_time, beat = 0, 0, 0, nil
    local ready, preparing, started = false, false, false
    local prepare_at, pose_generation = 0, 0
    local configuration

    local function stop(reason)
        serial = serial + 1
        if timer then cancel(timer);timer=nil end
        local previous=scene
        scene, beat, ready, preparing, started = nil, nil, false, false, false
        if previous then actor.release(reason) end
    end
    local function observations()
        local state = actor.observe()
        state.supported = state.supported or choreography.supported(actor.supported())
        return state
    end
    local function apply(event)
        if event and event.kind == "stop" then stop(event) end
    end
    local function update()
        if not scene then return end
        apply(policy.observe(observations()))
        if not scene then return end
        local now = clock()
        if preparing and now - prepare_at > 5 then
            -- A failed/cold pose consumes the episode rather than floating a
            -- static drawing or retrying the same scene indefinitely.
            policy.finished(generation, scene.episode)
            stop({reason="pose-timeout",immediate=true})
            return
        end
        if ready then elapsed = elapsed + math.max(0,now-last_time) end
        last_time = now
        local sample = choreography.sample(scene.scene,elapsed)
        if beat ~= sample.beat then
            beat, ready, preparing, started = sample.beat, false, true, false
            prepare_at, pose_generation = now, pose_generation + 1
            local mine = serial
            local requested_pose = pose_generation
            actor.prepare(sample,function(ok)
                if mine ~= serial or requested_pose ~= pose_generation or not scene or not preparing then return end
                preparing = false
                if not ok then
                    policy.finished(generation,scene.episode)
                    stop({reason="pose-unavailable",immediate=true})
                    return
                end
                -- Even synchronous readiness gets one staged frame before travel.
                ready, last_time = true, clock()
            end)
            return
        end
        if not ready then return end
        if not started then started=true;last_time=now;return end
        actor.play(sample)
        if sample.finished then
            policy.finished(generation,scene.episode)
            stop({reason="finished",immediate=false})
        end
    end
    local function receive(event)
        if event.kind == "ready" then
            if event.generation == generation then return end
            generation = event.generation
            source_seed = event.seed
            apply(policy.source(generation))
            local values = {}
            for key,value in pairs(configuration) do values[key]=value end
            values.seed = values.seed or event.seed
            policy.configure(values)
        elseif event.kind == "seed" then
            if event.generation~=generation then return end
            if source_seed==event.seed then return end
            source_seed=event.seed
            local values={}
            for key,value in pairs(configuration) do values[key]=value end
            values.seed=configuration.seed or source_seed
            apply(policy.configure(values))
        elseif event.kind == "lost" then
            if event.generation ~= generation then return end
            apply(policy.source(nil))
            generation = nil
        elseif event.kind == "resume" then
            apply(policy.resume(event.generation))
        elseif event.kind == "idle" then
            apply(policy.observe(observations()))
            local decision = policy.idle(event.generation,event.episode)
            if decision and decision.kind == "start" then
                scene, elapsed, last_time = decision, 0, clock()
                serial = serial + 1
                update()
                if scene then timer=every(16,update) end
            end
        end
    end
    return {
        configure=function(values)
            local previous=configuration
            configuration = values
            stop({reason="config",immediate=true})
            local policy_values={}
            for key,value in pairs(values) do policy_values[key]=value end
            policy_values.seed=values.seed or source_seed or generation
            policy.configure(policy_values)
            local keep_source=previous and previous.enabled and values.enabled
                and previous.source_signature==values.source_signature
            if keep_source then return end
            subscription=(subscription or 0)+1
            if handle then source.stop(handle);handle=nil end
            generation=nil
            source_seed=nil
            policy.source(nil)
            if values.enabled then
                -- A separate source generation guards helper callbacks across
                -- scene cancellation as well as configuration/reload teardown.
                local source_generation=subscription
                handle=source.start(function(event)
                    if source_generation==subscription then receive(event) end
                end)
            end
        end,
        changed=function() apply(policy.observe(observations())) end,
        close=function()
            subscription=(subscription or 0)+1
            if handle then source.stop(handle);handle=nil end
            apply(policy.source(nil))
            stop({reason="source",immediate=true})
        end,
        state=policy.state,
    }
end
