-- Semantic choreography consumed by the single native actor. Anchors are resolved
-- by its existing output/window adapter; this module owns no surface or geometry.
local scenes = {
    {id="screen_window", broad=true, beats={
        {3.6,"flight_forward","home","depth_near"},
        {5.4,"window_gaze","depth_near",nil,"curious_gaze"},
        {3.4,"flight_back","depth_near","home"}}},
    {id="corner_nap", broad=true, beats={
        {2.8,"flight_circle","home","bottom_corner"},
        {2.0,"curl_down","bottom_corner"}, {12,"curled_sleep","bottom_corner",nil,"breathing"},
        {1.6,"sleep_wake","bottom_corner"}, {2.6,"flight_home","bottom_corner","home"}}},
    {id="glass_inspection", broad=true, beats={
        {2.6,"flight_forward","home","glass"}, {1.5,"head_tilt","glass"},
        {3.5,"glass_paw","glass",nil,"breath_fog"}, {1.4,"glass_withdraw","glass"},
        {2.8,"flight_home","glass","home"}}},
    {id="upside_down_peek", broad=true, beats={
        {2.6,"flight_up","home","top_exit"}, {1.8,"inverted_peek","top_hidden","top_peek"},
        {3.6,"inverted_look","top_peek",nil,"look_both_ways"},
        {1.1,"inverted_withdraw","top_peek","top_hidden"}, {3.0,"flight_home","top_exit","home"}}},
    {id="window_perch", broad=true, window=true, beats={
        {2.8,"flight_land","home","window_top"}, {1.4,"wings_fold","window_top"},
        {5.0,"perch_tail_dangle","window_top",nil,"tail_sway"},
        {1.0,"wings_unfold","window_top"}, {2.8,"flight_home","window_top","home"}}},
    {id="tail_chase", broad=true, beats={
        {2.0,"flight_out","home","air"}, {2.2,"tail_follow_left","air",nil,"tail_circle_one"},
        {2.2,"tail_follow_right","air",nil,"tail_circle_two"},
        {1.8,"tail_catch_recover","air"}, {1.6,"settle_hover","air"},
        {2.5,"flight_home","air","home"}}},
    {id="sleepy_drift", broad=true, beats={
        {2.4,"flight_out","home","air"}, {2.2,"sleepy_nod","air"},
        {2.4,"sleepy_fall","air","lower_air"}, {1.3,"startled_wingbeat","lower_air","air"},
        {2.8,"flight_home","air","home"}}},
    {id="distant_flight", broad=true, beats={
        {3.0,"flight_back","home","depth_far"}, {5.0,"flight_depth_loop","depth_far",nil,"broad_depth_loop"},
        {1.8,"flight_bank","depth_far","depth_return"}, {3.2,"flight_forward","depth_return","home"}}},
    {id="hide_peek", broad=true, window=true, occlusion=true, beats={
        {2.2,"flight_behind","home","window_hidden"},
        {0.7,"peek_wait","window_hidden",nil,"behind"},
        {2.3,"peek_head","edge_right"},
        {1.7,"peek_look","edge_right"},
        {0.7,"peek_hide","edge_right"},
        {0.85,"peek_wait","window_hidden",nil,"behind"},
        {2.2,"peek_reach","edge_left"},
        {4.2,"peek_pull","edge_left"},
        {2.2,"peek_wave","edge_left_wave"},
        {2.9,"flight_home","edge_left_wave","home"}}},
    {id="wing_grooming", broad=true, beats={
        {2.0,"flight_out","home","air"}, {1.4,"settle_hover","air"},
        {2.8,"wing_straighten","air",nil,"groom_wing"},
        {2.6,"horn_polish","air",nil,"polish_horn"}, {1.8,"groom_stretch","air"},
        {2.5,"flight_home","air","home"}}},
    {id="waiting_for_you", broad=true, beats={
        {2.8,"flight_down","home","lower_edge"}, {2.0,"chin_on_paws","lower_edge"},
        {3.4,"hopeful_glances","lower_edge",nil,"look_both_ways"},
        {1.6,"hopeful_wave","lower_edge"}, {2.8,"flight_home","lower_edge","home"}}},
    -- Reduced motion has a separate, bounded, small resting pose. This is not
    -- counted as one of the eleven scenes and has no translation or zoom.
    {id="quiet_rest", quiet=true, beats={{10,"quiet_rest","home"}}},
}
local by_id = {}
for _, scene in ipairs(scenes) do
    scene.duration = 0
    scene.poses = {}
    for _, beat in ipairs(scene.beats) do
        scene.duration = scene.duration + beat[1]
        scene.poses[beat[2]] = true
    end
    by_id[scene.id] = scene
end
return {
    catalog=scenes,
    get=function(id) return by_id[id] end,
    supported=function(poses)
        local result = {}
        for _, scene in ipairs(scenes) do
            local ok = true
            for pose in pairs(scene.poses) do if poses[pose] ~= true then ok=false end end
            result[scene.id] = ok
        end
        return result
    end,
    sample=function(id, elapsed)
        local scene = assert(by_id[id], "unknown AFK scene")
        local start = 0
        for index, beat in ipairs(scene.beats) do
            if elapsed < start + beat[1] or index == #scene.beats then
                local p = math.max(0, math.min(1, (elapsed-start)/beat[1]))
                return {scene=id, beat=index, pose=beat[2], from=beat[3], to=beat[4] or beat[3],
                    effect=beat[5], progress=p, ease=p*p*(3-2*p),
                    moving=beat[4] ~= nil, finished=elapsed>=scene.duration,
                    -- Actor must stage this authored pose and wait for readiness
                    -- before advancing root motion; a cold asset is not a timer.
                    requires_pose_ready=true}
            end
            start = start + beat[1]
        end
    end,
}
