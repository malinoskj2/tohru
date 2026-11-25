-- Composition caller for flight.actor. Owns no surface or geometry observer.
return function(shared, options)
    options=options or {}
    local choreography=require("assistant/afk-scenes")
    local motion=require("assistant/afk-motion")(choreography)
    local core, deliver, lease, geometry, prepare_timer, watchdog
    local epoch, source_token, threshold, heartbeat = 0, nil, nil, 0
    local enabled, real_idle, session_safe, idle_attempted = false,false,false,false
    local settings, last_signature
    local controls,controlling={},false
    local stop_requested=false
    local pending_decision=false
    local consume_pending
    local last_availability
    local source_episode
    local function publish(reason)
        text.ai_afk_status=json.encode({state=reason,source=source_token~=nil,
            session_safe=session_safe,idle=real_idle,episode=core and core.state().episode or 0,
            availability=last_availability})
    end
    local function clear_art()
        fact.ai_afk_action,fact.ai_afk_phase,fact.ai_afk_fog=0,0,0
        fact.ai_afk_active=false
        fact.ai_afk_body_open,fact.ai_afk_preparing=false,false
        text.ai_afk_body_src=""
    end
    local function release()
        epoch=epoch+1
        if prepare_timer then cancel(prepare_timer);prepare_timer=nil end
        local old=lease
        lease,geometry=nil,nil
        clear_art()
        if old then shared.release(old) end
    end
    local function observations()
        local s=geometry or {}
        local supported={}
        if geometry then supported=motion.availability(s) end
        return {unlocked=session_safe and s.unlocked~=false,
            session_active=session_safe and s.session_active~=false,
            output_ready=geometry~=nil and options.art~=nil,output=s.output,
            inhibited=not real_idle,
            fullscreen=s.fullscreen,window_ready=s.target~=nil,occlusion_ready=s.target~=nil and s.output_id~=nil,
            listening=fact.ai_listening or fact.ai_voice_conversation,
            speaking=fact.ai_speaking,pending=fact.ai_streaming or fact.ai_voice_paused,
            text_entry=fact.ai_open or fact.ai_menu_open,dnd=fact.notices_dnd,
            entrance=fact.ai_dragon_open and not fact.ai_actor_open,
            reduced=fact.ai_dg_reduced,supported=supported}
    end
    local function scene_time(sample)
        local scene=choreography.get(sample.scene)
        local elapsed=0
        for i=1,sample.beat-1 do elapsed=elapsed+scene.beats[i][1] end
        return elapsed+sample.progress*scene.beats[sample.beat][1]
    end
    local function native_sample(sample,hidden)
        local value=motion.sample(sample.scene,geometry,scene_time(sample))
        if hidden then value.opacity=0 end
        fact.ai_afk_art_kind=value.art_kind
        fact.ai_afk_action=value.action
        fact.ai_afk_phase=value.phase
        fact.ai_afk_fog=value.fog
        fact.ai_afk_preparing=hidden==true
        fact.ai_afk_bust_mix=value.bust_mix or 1
        fact.ai_afk_body_frame=value.body_frame or 0
        fact.ai_afk_body_next=value.body_next or value.body_frame or 0
        fact.ai_afk_body_mix=value.body_next and (value.body_mix or 0) or 0
        return value
    end
    local actor={
        supported=motion.poses,
        observe=observations,
        prepare=function(sample,ready)
            if not lease or not geometry then ready(false);return end
            local token,mine=lease,epoch
            -- Only a cold binding stages hidden; a warm scene keeps drawing
            -- through beat changes instead of blinking for one frame.
            local cold=not fact.ai_afk_active
            if cold then
                local base=options.art or ""
                local ok
                local bust=sample.scene=="glass_inspection" or sample.scene=="upside_down_peek"
                if sample.scene=="screen_window" or bust then
                    -- 4x4 generated motion grids: the bust gesture loop and the closeup wingbeat.
                    ok=shared.register_closeup(base..(bust and "/afk-bust-motion.webp" or "/afk-closeup-motion.webp"),token,4)
                    if bust then
                        text.ai_afk_body_src=base.."/afk-closeup-motion.webp"
                        fact.ai_afk_body_open=true
                    end
                else ok=shared.register_skin(base.."/afk.webp",34,token) end
                if not ok then ready(false);return end
                fact.ai_afk_active=true
            end
            local function poll()
                if mine~=epoch or lease~=token or not geometry then return end
                local ok,state=shared.play(token,native_sample(sample,cold))
                if not ok or state=="ready" then
                    if prepare_timer then cancel(prepare_timer);prepare_timer=nil end
                    ready(ok==true)
                end
            end
            prepare_timer=every(16,poll)
            poll()
        end,
        play=function(sample)
            if not lease or not geometry then return end
            if not shared.play(lease,native_sample(sample,false)) then
                release();if core then core.changed() end
            end
        end,
        release=function(reason)
            publish(reason.reason or "Stopped")
            release()
        end,
    }
    local function native_control(action,callback)
        if options.control then return options.control(action,callback) end
        local args={"control",action}
        if action=="start" then
            args={"control",action,"--idle-seconds",tostring(settings.idle_seconds),
                "--source",settings.idle_source,"--owner",settings.owner}
        end
        controls[#controls+1]={args=args,callback=callback}
        local function pump()
            if controlling or #controls==0 then return end
            controlling=true
            local request=table.remove(controls,1)
            run("tohru_afk_idle.py",request.args,function(_,code)
                controlling=false
                if request.callback then request.callback(code==0) end
                pump()
            end)
        end
        pump()
    end
    local source={
        start=function(callback)
            deliver=callback
            source_token,threshold,session_safe,real_idle,idle_attempted=nil,nil,false,false,false
            native_control("start",function(ok)if not ok and deliver==callback then publish("Idle helper unavailable") end end)
            watchdog=every(500,function()
                if source_token and os.clock()-heartbeat>5 then
                    local old=source_token
                    consume_pending()
                    source_token,session_safe,real_idle=nil,false,false
                    release();callback({kind="lost",generation=old});publish("Idle source unknown")
                end
                core.changed()
            end)
            return callback
        end,
        stop=function(callback)
            if deliver~=callback then return end
            deliver=nil
            if watchdog then cancel(watchdog);watchdog=nil end
            release();native_control("stop")
        end,
    }
    core=require("assistant/afk-controller")(choreography,require("assistant/afk-policy"),actor,source,os.clock)
    consume_pending=function()
        if pending_decision and deliver and source_token then
            pending_decision=false
            deliver({kind="idle",generation=source_token,episode=source_episode})
        end
    end
    local function source_ready(token)
        if not enabled then
            if not stop_requested then stop_requested=true;native_control("stop") end
            return
        end
        if not deliver then return end
        heartbeat=os.clock()
        if source_token~=token then
            consume_pending()
            release()
            source_token,threshold,session_safe,real_idle,idle_attempted=token,nil,false,false,false
            source_episode=nil
            deliver({kind="ready",generation=token,seed=token})
        end
    end
    on("afk_source",source_ready)
    on("afk_seed",function(seed)
        if deliver and source_token then deliver({kind="seed",generation=source_token,seed=seed}) end
    end)
    on("afk_episode",function(n)source_episode=n end)
    on("afk_threshold",function(n)threshold=n end)
    on("afk_safe",function(token)
        if token~=source_token then return end
        session_safe=true
        if not fact.ai_afk_active then publish("Waiting for compositor idle") end
    end)
    on("afk_unsafe",function(token)
        if token~=source_token then return end
        session_safe=false;core.changed();release();publish("Session locked, inactive or unknown")
        consume_pending()
    end)
    on("afk_resume",function(token)
        if token~=source_token or not deliver then return end
        consume_pending()
        real_idle,idle_attempted=false,false
        release();deliver({kind="resume",generation=token});publish("Activity resumed")
    end)
    on("afk_lost",function(token)
        if token~=source_token or not deliver then return end
        session_safe,real_idle=false,false
        consume_pending()
        release();deliver({kind="lost",generation=token});source_token=nil;publish("Idle source lost")
    end)
    on("afk_idle",function(token)
        if token~=source_token or not deliver or (idle_attempted and (not source_episode or source_episode<=core.state().episode)) then return end
        idle_attempted,real_idle=true,true
        pending_decision=true
        local mine=epoch
        local function choose()
            pending_decision=false
            deliver({kind="idle",generation=token,episode=source_episode})
            local state=core.state()
            text.ai_afk_trace=json.encode({decision=state.decision,episode=state.episode,
                selected=state.active and state.active.scene,geometry=geometry})
            publish(state.active and "Playing "..state.active.scene or "Idle episode consumed")
            if not state.active then release() end
        end
        if threshold~=settings.idle_seconds or not session_safe then choose();return end
        local acquired=shared.acquire(function(token_lease,scene)
            if mine~=epoch or not real_idle or not session_safe or token~=source_token then
                shared.release(token_lease);return
            end
            lease,geometry=token_lease,scene
            last_availability=select(2,motion.availability(scene))
            choose()
        end,function(token_lease,reason)
            if lease~=token_lease then return end
            lease,geometry=nil,nil
            epoch=epoch+1
            if pending_decision then choose() end
            clear_art();core.changed();publish("Actor preempted: "..tostring(reason))
        end,{quiet=fact.ai_dg_reduced==true})
        if acquired then lease=acquired else choose() end
    end)
    return {
        configure=function(values)
            local function value(key,default)local row=values["assistant.afk_"..key];return row and row.value or default end
            local signature=json.encode({value("enabled",false),value("idle_seconds",300),value("probability_percent",50),
                value("seed",""),value("scene","auto"),value("scenes","all"),value("quiet_rest",false),value("idle_source","passive"),fact.ai_dg_reduced,
                values["idle.owner"] and values["idle.owner"].value or "external"})
            if signature==last_signature then return end
            consume_pending()
            last_signature=signature
            settings={enabled=value("enabled",false),idle_seconds=value("idle_seconds",300),
                probability=value("probability_percent",50),seed=tonumber(value("seed","")),
                force=value("scene","auto"),quiet=value("quiet_rest",false),scenes={}}
            settings.idle_source=value("idle_source","passive")
            settings.owner=values["idle.owner"] and values["idle.owner"].value or "external"
            settings.source_signature=json.encode({settings.idle_seconds,settings.idle_source,settings.owner})
            local raw=value("scenes","all")
            for _,scene in ipairs(choreography.catalog) do
                settings.scenes[scene.id]=raw=="all" or (","..raw..","):find(","..scene.id..",",1,true)~=nil
            end
            enabled=settings.enabled
            stop_requested=false
            if enabled and not options.art then
                run("tohru_afk_idle.py",{"assets"},function(out,code)
                    local ok,value=pcall(json.decode,out)
                    if code==0 and ok and type(value)=="table" and type(value.art)=="string" then
                        options.art=value.art
                    else publish("AFK artwork unavailable") end
                end)
            end
            core.configure(settings)
            publish(enabled and "Waiting for maintained desktop idle source" or "Disabled")
        end,
        changed=function()core.changed() end,
        close=function()consume_pending();enabled=false;core.close();release() end,
        state=core.state,
    }
end
