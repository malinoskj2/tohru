-- Explicit owner-approved task updates only. Never feed provider commentary here.
-- Clock and speech effects are injected so scheduling has no wall-clock or model dependency.
return function(clock, speak, hush)
    local chat, request, epoch = nil, nil, 0
    local tasks, count, queue, active = {}, 0, {}, nil
    local enabled, seed, last_topic, chatter_count = false, 1, nil, 0
    local next_chatter, last_speech = 0, -60000
    local topics = {
        "While we wait, a tiny question: would you rather explore a forest or a library?",
        "A little design thought: leaving room for silence can be a useful feature.",
        "Something to ponder: what would you build if it only had one button?",
        "A small creative challenge: imagine a useful tool that fits in a pocket.",
    }
    local priority = {ack = 2, progress = 1, done = 3, failed = 3, cancelled = 3, chatter = 0}
    local function random(n)
        seed = (seed * 48271) % 2147483647
        return seed % n
    end
    local function clear()
        queue = {}
        if active then hush(); active = nil end
    end
    local function interrupt()
        epoch = epoch + 1
        chat, request = nil, nil
        tasks, count = {}, 0
        clear()
    end
    local function enqueue(item)
        -- Latest progress supersedes waiting progress; terminal events retire all
        -- queued speech for that task. The queue is bounded independently of tasks.
        for i = #queue, 1, -1 do
            if queue[i].task == item.task and (item.priority == 3 or queue[i].kind == item.kind) then
                table.remove(queue, i)
            end
        end
        if active and (item.priority == 3 or active.kind == "chatter") then
            hush(); active = nil
        end
        if #queue >= 8 then
            local worst = #queue
            for i = 1, #queue do if queue[i].priority < queue[worst].priority then worst = i end end
            if queue[worst].priority > item.priority then return end
            table.remove(queue, worst)
        end
        item.epoch, item.at = epoch, clock()
        queue[#queue + 1] = item
    end
    local function tick(visible, busy)
        local now = clock()
        if not visible then interrupt(); return end
        if active and now - active.at >= 30000 then hush(); active = nil end
        if busy then return end
        if active then return end
        local running = false
        for _, task in pairs(tasks) do
            if not task.terminal and now - task.at < 600000 then running = true end
        end
        if #queue == 0 and running and enabled and chatter_count < 2 and now >= next_chatter and now - last_speech >= 45000 then
            next_chatter = now + 45000 + random(30001)
            -- Silence is an explicit equally weighted outcome; never retry this tick.
            local pick = random(#topics + 1)
            if pick > 0 then
                if pick == last_topic then pick = pick % #topics + 1 end
                last_topic = pick
                chatter_count = chatter_count + 1
                enqueue({kind = "chatter", priority = 0, text = topics[pick]})
            end
        end
        local best
        for i = #queue, 1, -1 do
            local item = queue[i]
            if item.epoch ~= epoch or now - item.at > (item.priority == 3 and 30000 or 15000) then
                table.remove(queue, i)
            end
        end
        for i, item in ipairs(queue) do
            if not best or item.priority > queue[best].priority then best = i end
        end
        if best then
            active = table.remove(queue, best)
            active.at = now
            last_speech = now
            active.generation = speak(active.text, chat, active.kind)
        end
    end
    return {
        begin = function(cid, rid)
            interrupt()
            chat, request = cid, rid
            next_chatter, chatter_count = clock() + 45000 + random(30001), 0
        end,
        configure = function(on, replay_seed)
            enabled = on == true
            local value = tonumber(replay_seed) or 1
            seed = math.max(1, math.min(2147483646, math.floor(value)))
            if not enabled then
                for i = #queue, 1, -1 do if queue[i].kind == "chatter" then table.remove(queue, i) end end
                if active and active.kind == "chatter" then hush(); active = nil end
            end
        end,
        update = function(event)
            if not chat or not request or request == "" or event.chat ~= chat or event.request ~= request then return false, "origin_mismatch" end
            local rank = priority[event.kind]
            if not rank or event.kind == "chatter" or type(event.task) ~= "string" or event.task == "" or #event.task > 128
                or type(event.sequence) ~= "number" or event.sequence < 1 or event.sequence > 2147483646 or event.sequence % 1 ~= 0
                or type(event.text) ~= "string" or event.text == "" or #event.text > 320 or event.text:find("[%c]") then return false, "invalid" end
            local task = tasks[event.task]
            if not task then
                if event.kind ~= "ack" or count >= 8 then return false, "unknown_or_full" end
                task = {sequence = 0, at = clock()}
                tasks[event.task], count = task, count + 1
            end
            if task.terminal or event.sequence <= task.sequence then return false, "replay" end
            if event.kind == "ack" and task.sequence > 0 then return false, "replay" end
            task.sequence = event.sequence
            task.terminal = rank == 3
            if event.kind == "cancelled" then
                for i = #queue, 1, -1 do if queue[i].task == event.task then table.remove(queue, i) end end
                if active and (active.task == event.task or active.kind == "chatter") then hush(); active = nil end
                return true
            end
            if task.last_text == event.text then return true end
            task.last_text = event.text
            enqueue({task = event.task, kind = event.kind, priority = rank, text = event.text})
            return true
        end,
        finished = function(generation)
            if active and active.generation == generation then active = nil end
        end,
        interrupt = interrupt,
        tick = tick,
    }
end
