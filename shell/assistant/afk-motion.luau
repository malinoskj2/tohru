-- AFK-specific choreography in the existing actor's output-local coordinates.
-- Geometry is a verified scene from flight.actor, never a second provider.
return function(choreography)
    local rows={glass_paw=0,head_tilt=1,wing_begin=2,wing_straighten=3,horn_low=4,horn_polish=5,
        chin_on_paws=6,hopeful_wave=7,curled_sleep=8,perch_tail_dangle=9,tail_follow_left=10,tail_follow_right=11}
    local function lerp(a,b,p)return a+(b-a)*p end
    local function dimensions(s)
        local scale=s.scale or 1
        -- Camera approach fills the output vertically and intentionally crops
        -- only the lower body. Glass uses its own head-dominant paw source.
        local camera=s.h*1.15
        local glass=math.min(s.w*.92,s.h)
        return scale,camera,glass
    end
    local function availability(s)
        local result,reasons={},{}
        for _,scene in ipairs(choreography.catalog) do result[scene.id]=true end
        if type(s.scale)~="number" or s.scale<.25 or s.scale>8 then
            for _,scene in ipairs(choreography.catalog) do
                result[scene.id]=false;reasons[scene.id]="physical output scale is unknown"
            end
            return result,reasons
        end
        local scale,camera,glass=dimensions(s)
        -- Conservative authored occupied head bounds (not transparent canvas):
        -- fullbody 580x610; bust 1020x1000. No magnification beyond native texels.
        local short=math.min(s.w,s.h)*scale
        if camera*scale>1254 or camera*scale*580/1254 < short*.45 then
            result.screen_window=false
            reasons.screen_window="1254px camera source cannot provide required face coverage at native physical resolution"
        end
        if glass*scale>1254 or glass*scale*1020/1254 < short*.65 then
            result.glass_inspection=false
            reasons.glass_inspection="1254px glass-paw source cannot provide required face coverage at native physical resolution"
        end
        local t=s.target
        local top=t and math.max(75,t.x+75)
        local right=t and math.min(s.w-75,t.x+t.w-75)
        if not t or t.w<180 or t.h<160 or t.y<80 or t.y>s.h-120 or top>right then
            result.window_perch=false;reasons.window_perch="no supported visible upper window edge"
        end
        if not t or t.x<80 or t.x+t.w>s.w-80 or t.y<80 or t.y+t.h>s.h-60 then
            result.hide_peek=false;reasons.hide_peek="window needs visible left/right edges for genuine occluded peeks"
        end
        return result,reasons
    end
    local function anchors(s,id)
        local _,camera,glass=dimensions(s)
        local home={x=s.w-123,y=s.h-133,scale=1}
        local target=s.target
        local a={home=home,
            air={x=s.w*.58,y=s.h*.43,scale=1},lower_air={x=s.w*.58,y=s.h*.43+55,scale=1},
            depth_far={x=s.w*.40,y=s.h*.36,scale=.18},depth_return={x=s.w*.65,y=s.h*.30,scale=.32},
            depth_near={x=s.w*.50,y=s.h*.56,scale=camera/150},
            glass={x=s.w*.50,y=s.h*.50,scale=glass/150},
            bottom_corner={x=s.w-113,y=s.h-73,scale=1},
            top_hidden={x=s.w*.47,y=-95,scale=1},top_peek={x=s.w*.47,y=-20,scale=1},
            upper_air={x=s.w*.54,y=s.h*.20,scale=1},lower_edge={x=s.w*.55,y=s.h+5,scale=1}}
        if id=="screen_window" then a.home={x=s.w-123,y=s.h-133,scale=1} end
        if id=="glass_inspection" then
            -- Eye-registered full-body → bust handoff. The full body flies
            -- first; both eye centers coincide through the small crossfade.
            a.home.scale=1/1.9245097100019937
            a.home.y=a.home.y-26.2
        end
        if target then
            a.window_top={x=math.max(75,math.min(s.w-75,target.x+target.w*.62)),y=target.y-14,scale=1}
            a.window_hidden={x=target.x+target.w*.50,y=target.y+target.h*.50,scale=1}
            a.window_left={x=target.x-20,y=target.y+target.h*.40,scale=1}
            a.window_right={x=target.x+target.w+20,y=target.y+target.h*.64,scale=1}
        end
        if id~="screen_window" and id~="glass_inspection" then
            local normal=math.min(1,236/(150*(s.scale or 1)))
            for _,anchor in pairs(a) do anchor.scale=anchor.scale*normal end
        end
        return a
    end
    return {
        availability=availability,
        poses=function()
            local supported={}
            for _,scene in ipairs(choreography.catalog) do for pose in pairs(scene.poses) do supported[pose]=true end end
            return supported
        end,
        sample=function(id,s,elapsed)
            local semantic=choreography.sample(id,elapsed)
            local a=anchors(s,id)
            local from,to=assert(a[semantic.from]),assert(a[semantic.to])
            local p=semantic.ease
            local sample={x=lerp(from.x,to.x,p),y=lerp(from.y,to.y,p),scale=lerp(from.scale,to.scale,p),
                opacity=1,airborne=1,bank=0,rotation=0,pose="nested_hover",finished=semantic.finished,
                beat=semantic.beat,action=0,art_kind=0,phase=elapsed/.7,fog=0}
            local pose=semantic.pose
            if id=="screen_window" or id=="glass_inspection" then
                -- One dedicated source through the complete approach and retreat
                -- avoids a cold low-res→closeup swap and duplicate faces.
                sample.pose="closeup"
                sample.art_kind=id=="glass_inspection" and 1 or 0
                if id=="glass_inspection" then
                    local physical=150*sample.scale*(s.scale or 1)
                    local blend=math.max(0,math.min(1,(physical-350)/(651.59-350)))
                    sample.bust_mix=blend*blend*(3-2*blend)
                    -- Move the camera framing onto the lower output edge
                    -- before the bust contributes. Its intentional chest crop
                    -- then stays outside the desktop, rather than floating as
                    -- a rectangular cutout during the handoff.
                    local framing=math.max(0,math.min(1,(physical-150)/200))
                    framing=framing*framing*(3-2*framing)
                    local edge=s.h+4-75*sample.scale
                    sample.y=lerp(sample.y,math.max(sample.y,edge),framing)
                end
                sample.action=semantic.moving and 1 or (pose=="glass_paw" and 3 or 2)
                if pose=="glass_paw" then sample.fog=math.sin(math.pi*semantic.progress)^4 end
            elseif rows[pose] then
                sample.pose="skin";sample.skin_frame=rows[pose]*8+math.floor(elapsed/.14)%8
            elseif pose=="curl_down" or pose=="sleep_wake" then
                sample.pose="skin";sample.skin_frame=(pose=="curl_down" and 8 or 1)*8
            elseif pose=="wings_fold" or pose=="wings_unfold" then
                sample.pose="skin";sample.skin_frame=9*8
            elseif pose=="hopeful_glances" then
                sample.pose="skin";sample.skin_frame=6*8+math.floor(elapsed/.2)%8
            elseif pose=="tail_catch_recover" then
                sample.pose="skin";sample.skin_frame=11*8+math.floor(elapsed/.14)%8
            elseif pose=="sleepy_nod" or pose=="sleepy_fall" then
                sample.pose="skin";sample.skin_frame=8*8+math.floor(elapsed/.2)%8
                sample.rotation=4*math.sin(math.pi*semantic.progress)
            elseif pose=="inverted_peek" or pose=="inverted_look" then
                sample.pose="skin";sample.skin_frame=1*8+math.floor(elapsed/.2)%7
                sample.rotation=180
            elseif pose=="controlled_tumble" then
                sample.pose="skin";sample.skin_frame=1*8+math.floor(elapsed/.14)%7
                sample.rotation=180+180*p
            elseif pose=="groom_stretch" then sample.pose="skin";sample.skin_frame=15
            elseif pose=="settle_hover" then sample.pose="nested_hover"
            elseif pose=="quiet_rest" then
                sample.pose="nested_hover";sample.airborne=0;sample.phase=0
            elseif pose:sub(1,4)=="peek" then
                sample.pose="skin";sample.skin_frame=1*8+math.floor(elapsed/.2)%7
            elseif semantic.moving or pose:sub(1,6)=="flight" or pose=="startled_wingbeat" then
                local dx=to.x-from.x
                sample.pose=(pose=="flight_up" or pose=="startled_wingbeat") and "fly-up" or (dx<0 and "fly-left" or "fly-right")
                sample.bank=math.sin(math.pi*semantic.progress)*(dx<0 and -8 or 8)
            end
            if semantic.effect=="tail_circle_one" or semantic.effect=="tail_circle_two" then
                local theta=semantic.progress*math.pi*2
                local side=semantic.effect=="tail_circle_one" and 1 or -1
                sample.x=sample.x+32*math.sin(theta)*side
                sample.y=sample.y+20*(1-math.cos(theta))
                sample.bank=16*math.sin(theta)*side
            elseif semantic.effect=="broad_depth_loop" then
                local theta=semantic.progress*math.pi*2
                sample.x=sample.x+s.w*.22*math.sin(theta)
                sample.y=sample.y+s.h*.12*(1-math.cos(theta))
                sample.scale=.18+.18*(1-math.cos(theta))/2
                sample.pose=math.cos(theta)<0 and "fly-left" or "fly-right"
                sample.bank=18*math.sin(theta)
            end
            if pose=="flight_circle" then
                local theta=semantic.progress*math.pi*2
                sample.x=sample.x+38*math.sin(theta)
                sample.y=sample.y-28*(1-math.cos(theta))
                sample.pose=math.cos(theta)<0 and "fly-left" or "fly-right"
                sample.bank=12*math.sin(theta)
            end
            if id=="hide_peek" then sample.behind=true end
            -- Keep the selected normal-size scene in one finite skin binding:
            -- exact canonical flight frames share this atlas with authored AFK
            -- actions, so a landing never blanks for another texture warm-up.
            if sample.pose=="fly-left" or sample.pose=="fly-right" or sample.pose=="fly-up" then
                local row=sample.pose=="fly-left" and 0 or sample.pose=="fly-right" and 2 or 4
                sample.pose="skin";sample.skin_frame=row*8+math.floor(elapsed/.07)%8
            elseif sample.pose=="nested_hover" then sample.pose="skin";sample.skin_frame=8 end
            if sample.pose=="skin" then
                sample.art_kind=2;sample.phase=elapsed/1.12
                local row=math.floor(sample.skin_frame/8)
                local actions={[3]=4,[5]=5,[7]=6,[8]=7,[9]=8,[10]=9,[11]=10}
                if actions[row] then
                    sample.action=actions[row]
                    sample.skin_frame=row*8
                elseif row==1 and sample.skin_frame~=15 then
                    sample.action=11;sample.skin_frame=8
                end
                if id=="quiet_rest" then sample.action=0;sample.phase=0 end
            end
            if semantic.beat==1 then sample.opacity=math.min(1,semantic.progress*8) end
            if semantic.finished then sample.opacity=0 end
            return sample
        end,
    }
end
