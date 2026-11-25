-- A single finite external scene can borrow the desktop renderer between voice appearances.
-- The caller owns the episode clock, input/idle source and semantic pose choices.
return function(puppet, host)
    local generation, active, stopping = 0, nil, false
    local skin_path, skin_rows, closeup_path = "", 0, ""
    local api = {}
    local function finite(n, lo, hi)
        return type(n) == "number" and n >= lo and n <= hi
    end

    local function stop(reason)
        local lease = active
        if not lease then
            return
        end
        stopping = true
        active = nil -- invalidate asynchronous helper, deadlines and caller before cleanup
        if lease.watch then
            cancel(lease.watch)
        end
        if lease.process then
            kill(lease.process, "term")
        end
        fact.ai_actor_open, fact.ai_actor_skin_open, fact.ai_actor_released = false, false, true
        fact.ai_actor_closeup_open, fact.ai_actor_quiet = false, false
        text.ai_actor_closeup_src, text.ai_actor_skin_src = "", ""
        fact.ai_actor_scale, fact.ai_actor_rotation = 1, 0
        fact.ai_actor_turn, fact.ai_actor_anchored = 1, false
        puppet.hold_pose(nil)
        puppet.flight(nil)
        puppet.lease_art("")
        host.close()
        local ok, err = pcall(lease.stopped, lease.token, reason)
        stopping = false
        if not ok then
            print("Desktop actor stop callback failed", err)
        end
    end

    local function can_register(token)
        if not active then
            return token == nil
        end
        return token == active.token and active.ready and active.pose == nil
    end

    function api.register_skin(path, rows, token)
        if
            not can_register(token)
            or type(path) ~= "string"
            or #path > 4096
            or path:sub(1, 1) ~= "/"
            or not finite(rows, 1, 12)
            or rows ~= math.floor(rows)
        then
            return false
        end
        skin_path, skin_rows = path, rows
        return true
    end

    function api.register_closeup(path, token)
        if not can_register(token) or type(path) ~= "string" or #path > 4096 or path:sub(1, 1) ~= "/" then
            return false
        end
        closeup_path = path
        return true
    end

    function api.acquire(ready, stopped, options)
        local quiet = type(options) == "table" and options.quiet == true
        if
            active
            or stopping
            or host.busy(quiet)
            or type(ready) ~= "function"
            or type(stopped) ~= "function"
        then
            return nil
        end
        local output = host.open()
        if output == "" then
            return nil
        end
        generation = generation + 1
        local lease =
            { token = generation, output = output, stopped = stopped, started = os.clock(), quiet = quiet }
        active = lease
        fact.ai_actor_open, fact.ai_flying, fact.ai_actor_released, fact.ai_actor_quiet =
            true, not quiet, false, quiet
        fact.ai_flight_alpha = 0.002 -- keep the mapped probe and body resident
        local function receive(raw)
            if active ~= lease then
                return
            end
            local ok, s = pcall(json.decode, raw)
            if
                not ok
                or type(s) ~= "table"
                or s.blocked
                or s.unlocked == false
                or s.session_active == false
                or s.fullscreen
                or s.inhibited
                or (s.output and s.output ~= output)
            then
                stop("scene")
                return
            end
            if lease.raw and raw ~= lease.raw then
                stop("geometry")
                return
            end
            lease.raw, lease.snapshot = raw, s
            puppet.lease_art(s.art or "")
        end
        local ok, id = pcall(spawn, "tohru_flight_scene.py", { "--actor" }, receive, function()
            if active == lease then
                lease.process = nil
                stop("observer")
            end
        end)
        if not ok then
            active = nil
            fact.ai_actor_open = false
            host.close()
            return nil
        end
        lease.process = id
        lease.watch = every(32, function()
            if active ~= lease then
                return
            end
            if host.busy(lease.quiet) then
                stop("activity")
                return
            end
            local elapsed = os.clock() - lease.started
            if elapsed > 55 or (not lease.ready and elapsed > 5) then
                stop("timeout")
                return
            end
            local w, h = host.dimensions()
            local s = lease.snapshot
            if lease.ready and (w ~= lease.w or h ~= lease.h) then
                stop("geometry")
                return
            end
            if not s or not finite(w, 1, 32768) or not finite(h, 1, 32768) then
                return
            end
            if s.output and (not finite(s.w, w - 1, w + 1) or not finite(s.h, h - 1, h + 1)) then
                stop("geometry")
                return
            end
            if lease.ready or not fact.ai_dg_ready or fact.ai_dg_warm then
                return
            end
            lease.ready = true
            lease.w, lease.h = w, h
            lease.target = s.output and s.target
            local delivered = pcall(ready, lease.token, {
                output = output,
                w = w,
                h = h,
                output_id = s.output_id,
                ox = s.ox,
                oy = s.oy,
                rotation = s.rotation,
                scale = s.scale,
                workspace = s.workspace,
                target = lease.target,
                fullscreen = s.fullscreen,
                inhibited = s.inhibited,
                session_active = s.session_active,
                unlocked = s.unlocked,
            })
            if not delivered and active == lease then
                stop("caller")
            end
        end)
        return lease.token
    end

    function api.play(token, s)
        local lease = active
        if not lease or lease.token ~= token or not lease.ready then
            return false
        end
        if host.busy(lease.quiet) then
            stop("activity")
            return false
        end
        if type(s) ~= "table" then
            stop("sample")
            return false
        end
        local scale, rotation, alpha, bank, air, turn =
            s.scale or 1, s.rotation or 0, s.opacity or 1, s.bank or 0, s.airborne or 0, s.turn or 1
        -- A center may sit beyond an edge by its own drawn half-size.
        local reach = 256 + 75 * math.abs(type(scale) == "number" and scale or 0)
        local pose = s.pose
        local skin, closeup = pose == "skin", pose == "closeup"
        if
            not finite(s.x, -reach, lease.w + reach)
            or not finite(s.y, -reach, lease.h + reach)
            or not finite(turn, -1.2, 1.2)
            or math.abs(turn) < 0.05
            or not finite(scale, 0.05, 12)
            or not finite(rotation, -360, 360)
            or not finite(alpha, 0, 1)
            or not finite(bank, -90, 90)
            or not finite(air, 0, 1)
            or (s.behind and not lease.target)
            or (closeup and closeup_path == "")
            or (
                skin
                and (
                    skin_path == ""
                    or not finite(s.skin_frame, 0, skin_rows * 8 - 1)
                    or s.skin_frame ~= math.floor(s.skin_frame)
                )
            )
        then
            stop("sample")
            return false
        end
        if lease.quiet then
            local rest = lease.rest
            local calm = skin or pose == "neutral" or pose == "nested_hover" or pose == "reclined_hover"
            if
                not calm
                or scale > 1
                or rotation ~= 0
                or bank ~= 0
                or turn ~= 1
                or air ~= 0
                or s.behind
                or not finite(s.x, 0, lease.w)
                or not finite(s.y, 0, lease.h)
                or (
                    rest
                    and (
                        s.x ~= rest.x
                        or s.y ~= rest.y
                        or scale ~= rest.scale
                        or pose ~= rest.pose
                        or s.skin_frame ~= rest.frame
                    )
                )
            then
                stop("quiet")
                return false
            end
            lease.rest = rest or { x = s.x, y = s.y, scale = scale, pose = pose, frame = s.skin_frame }
        end
        local now = os.clock()
        if pose ~= lease.pose or not lease.prepared_at then
            puppet.flight(nil)
            puppet.hold_pose(nil)
            local flying = pose == "fly-left" or pose == "fly-right" or pose == "fly-up"
            if flying then
                if
                    not puppet.flight(pose == "fly-up" and "rise" or "edge", pose == "fly-left" and -1 or 1)
                then
                    stop("pose")
                    return false
                end
            elseif
                not skin
                and not closeup
                and (type(pose) ~= "string" or not puppet.hold_pose(pose, lease.quiet))
            then
                stop("pose")
                return false
            end
            lease.pose, lease.flying, lease.beat = pose, flying, now
            -- Bind only the currently presented supplement. Removing the
            -- live path lets Pleamar reclaim it with its usual atlas policy.
            text.ai_actor_skin_src = skin and skin_path or ""
            text.ai_actor_closeup_src = closeup and closeup_path or ""
            if skin then
                fact.ai_actor_skin_rows = skin_rows
            end
            lease.anchor = nil
            if skin or closeup then
                lease.prepared_at = now + 3
            else
                lease.prepared_at = math.min(lease.prepared_at or now + 0.20, now + 0.20)
            end
        end
        fact.ai_actor_skin_open, fact.ai_actor_closeup_open = skin, closeup
        if skin then
            fact.ai_actor_skin_frame = s.skin_frame
        end
        if lease.flying then
            puppet.wingbeat((now - lease.beat) / 0.42)
        end
        fact.ai_flight_behind = s.behind == true
        fact.ai_actor_scale, fact.ai_actor_rotation = scale, rotation
        fact.ai_actor_turn, fact.ai_actor_anchored = turn, s.anchored == true
        fact.ai_flight_bank, fact.ai_flight_altitude = bank, air
        if now < lease.prepared_at then
            if not lease.anchor then
                lease.anchor = { x = s.x, y = s.y }
            end
            fact.ai_flight_x, fact.ai_flight_y = lease.anchor.x, lease.anchor.y
            fact.ai_flight_alpha = 0.002
            return true, "preparing"
        end
        lease.anchor = nil
        fact.ai_flight_x, fact.ai_flight_y = s.x, s.y
        fact.ai_flight_alpha = alpha
        return true, "ready"
    end

    function api.release(token)
        if not active or active.token ~= token then
            return false
        end
        stop("released")
        return true
    end
    return api,
        stop,
        function(quiet_only)
            return active ~= nil and (not quiet_only or active.quiet)
        end
end
