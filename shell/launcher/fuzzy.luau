local MATCH, MISMATCH, OPEN, EXTEND = 12, 6, 5, 1
local PREFIX, DELIMITER, CAPITAL, CASE, EXACT = 12, 4, 4, 4, 8
local function lower(b) return b >= 97 and b <= 122 end
local function upper(b) return b >= 65 and b <= 90 end
local function delimiter(b) return b < 128 and not (lower(b) or upper(b) or (b >= 48 and b <= 57)) end

-- frizbee 0.13.0 as j2bar runs it (CaseMatching::Ignore) on an AVX-512 CPU: its horizontal gap
-- propagation is per SIMD chunk, so scores depend on the lane count (64 u8 lanes for needles of
-- up to 13 bytes, else 32 u16 lanes). The needle is taken as bytes, so a non-ASCII one is scored
-- like UnicodeMatching::Ignore.
return function(needle, typos)
    local n = #needle
    if n == 0 then return function() return 0 end end
    local want, flip = {}, {}
    for i = 1, n do
        local b = needle:byte(i)
        want[i], flip[i] = b, lower(b) and b - 32 or upper(b) and b + 32 or b
    end
    local least, lanes = (utf8.len(needle) or n) - typos, n <= 13 and 64 or 32

    local function prefilter(text, len)
        local function find(i, from, to)
            for p = from, to do
                local b = text:byte(p)
                if b == want[i] or b == flip[i] then return p end
            end
        end
        local function window(first, k)
            for p = len, 1, -1 do
                local b = text:byte(p)
                for i = n - k, n do if b == want[i] or b == flip[i] then return first, p end end
            end
            return first, len
        end
        if typos == 0 then
            local first = find(1, 1, len)
            local p = first
            for i = 2, n do p = p and find(i, p + 1, len) end
            if not p then return nil end
            return window(first, 0)
        end
        if n <= typos then return 1, len end
        local paths, at, first = typos + 1, {}, math.huge
        for start = 1, len, 64 do
            local stop, from = math.min(start + 63, len), {}
            if typos <= 2 then
                -- One path per allowed typo, path j having skipped j - 1 needle bytes, each
                -- searching on from its own position in the chunk.
                if start == 1 then for j = 1, paths do at[j] = j end end
                for j = 1, paths do from[j] = start end
                local advanced = true
                while advanced do
                    for j = 2, paths do
                        local next = at[j - 1] + 1
                        if next > at[j] then
                            if next > n then return window(first, typos) end
                            at[j], from[j] = next, from[j - 1]
                        elseif next == at[j] and from[j - 1] < from[j] then
                            from[j] = from[j - 1]
                        end
                    end
                    advanced = false
                    for j = 1, paths do
                        local p = find(at[j], from[j], stop)
                        if p then
                            first, at[j], from[j], advanced = math.min(first, p), at[j] + 1, p + 1, true
                            if at[j] > n then return window(first, typos) end
                        end
                    end
                end
            else
                if start == 1 then for j = 1, paths do at[j] = 1 end end
                local p = start
                while true do
                    for j = 2, paths do
                        local next = at[j - 1] + 1
                        if next > at[j] then
                            if next > n then return window(first, typos) end
                            at[j] = next
                        end
                    end
                    local hit
                    while p <= stop and not hit do
                        local b = text:byte(p)
                        for j = 1, paths do if b == want[at[j]] or b == flip[at[j]] then hit = b end end
                        p = p + 1
                    end
                    if not hit then break end
                    first = math.min(first, p - 1)
                    for j = 1, paths do
                        if hit == want[at[j]] or hit == flip[at[j]] then
                            at[j] = at[j] + 1
                            if at[j] > n then return window(first, typos) end
                        end
                    end
                end
            end
        end
        return nil
    end

    local function greedy(text, from, len, prefix)
        if n > len then return 0 end
        local score, h, enabled, was_lower, was_delimiter = 0, 0, false, false, false
        for i = 1, n do
            local start, found = h, false
            while not found and h <= len - n + i - 1 do
                local b = text:byte(from + h)
                local is_lower, is_delimiter = lower(b), delimiter(b)
                if not is_delimiter then enabled = true end
                if b == want[i] or b == flip[i] then
                    score = score + MATCH
                    if h ~= start and i ~= 1 then score = math.max(0, score - OPEN - EXTEND * (h - start - 1)) end
                    if b == want[i] then score = score + CASE end
                    if upper(b) and was_lower then score = score + CAPITAL end
                    if prefix and h == 0 then score = score + PREFIX end
                    if was_delimiter and not is_delimiter then score = score + DELIMITER end
                    found = true
                end
                was_delimiter, was_lower, h = enabled and is_delimiter, is_lower, h + 1
            end
            if not found then return 0 end
        end
        return score
    end

    local function smith_waterman(text, from, len, prefix)
        if len > 1024 then return greedy(text, from, len, prefix) end
        local width, bytes, bonus = math.ceil(len / lanes) * lanes, {}, {}
        for p = 1, width do bytes[p] = p <= len and text:byte(from + p - 1) or 0 end
        for p = 1, width do
            local b, before = bytes[p], bytes[p - 1]
            bonus[p] = MATCH + MISMATCH + (p == 1 and prefix and PREFIX or 0)
                + (before and delimiter(before) and not delimiter(b) and DELIMITER or 0)
                + (before and upper(b) and lower(before) and CAPITAL or 0)
        end
        local above, matched_above = {}, {}
        for p = 1, width do above[p], matched_above[p] = 0, false end
        for i = 1, n do
            local row, matched = {}, {}
            for p = 1, width do
                local b = bytes[p]
                local is_match = b == want[i] or b == flip[i]
                local diagonal = math.max(0, (above[p - 1] or 0) + (is_match and bonus[p] or 0) - MISMATCH)
                    + (b == want[i] and CASE or 0)
                row[p] = math.max(diagonal, above[p] - (matched_above[p] and OPEN or EXTEND))
                matched[p] = is_match
            end
            -- Each lane takes the best of the lanes 1, 2, 4… to its left, paying a gap that opens
            -- only where the lane it shifts from matched; lanes left of the chunk are final.
            for chunk = 0, width - 1, lanes do
                local shift = 1
                while shift < lanes do
                    for p = chunk + lanes, chunk + 1 + math.max(0, shift - chunk), -1 do
                        local q = p - shift
                        row[p] = math.max(row[p], row[q] - EXTEND * shift - (matched[q] and OPEN - EXTEND or 0))
                    end
                    shift = shift * 2
                end
            end
            above, matched_above = row, matched
        end
        local best = 0
        for p = 1, width do best = math.max(best, above[p]) end
        return best
    end

    return function(text)
        local len = #text
        if len < least then return nil end
        local first, last = prefilter(text, len)
        if not first then return nil end
        local from = math.max(first - 1, 1)
        local score = smith_waterman(text, from, last - from + 1, from == 1)
        if from == 1 and last == len and text == needle then score = score + EXACT end
        return score
    end
end
