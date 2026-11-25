-- AFK-specific choreography in the existing actor's output-local coordinates.
-- Geometry is a verified scene from flight.actor, never a second provider.
return function(choreography)
    local rows={glass_paw=0,head_tilt=1,wing_begin=2,wing_straighten=3,horn_low=4,horn_polish=5,
        chin_on_paws=6,hopeful_wave=7,curled_sleep=8,perch_tail_dangle=9,tail_follow_left=10,tail_follow_right=11}
    -- Registered eye scale between the full-body closeup and the bust.
    local BODY=1.9245097100019937
    -- Generated FL2VA in-betweens follow the 12 authored rows, 16 frames each.
    local MOTION={turn=96,arrive_right=112,arrive_left=128,curl=144,perch=160,bank=176,
        edge_reach=192,edge_pull=240,edge_head=304}
    -- The horned pose the pull-out ends on, then the horned front hover,
    -- centred like the canonical cells.
    local EDGE_WAVE=352
    local LAST=15
    -- Source frame (of 124) behind each edge-clip cell: cells are picked half
    -- by motion and half by time, and playback follows the generated timing.
    local EDGE_PICKS={edge_reach={0,3,6,9,11,15,17,20,22,25,27,31,34,37,41,43,47,48,51,54,57,59,62,65,67,69,71,73,75,76,78,80,82,84,87,89,91,93,96,98,100,103,107,109,113,117,120,123},edge_pull={0,3,6,9,13,16,18,21,24,28,30,33,36,38,41,43,46,50,52,55,57,59,62,64,67,69,71,74,75,77,78,79,80,81,82,83,84,85,86,87,89,90,91,92,93,94,96,97,98,99,100,101,102,103,105,106,108,109,111,113,115,118,120,123},edge_head={0,3,6,9,12,16,18,22,25,28,31,34,36,39,42,43,46,48,50,52,55,57,59,61,64,67,69,72,74,76,78,81,83,85,87,89,91,94,96,98,100,102,106,109,113,117,120,123}}
    -- Last source frame of the glance that opens the pull clip, before its dip.
    local EDGE_GLANCE=14
    local TURN,ARRIVE=.8,.6
    -- One generated bust gesture loop and one closeup wingbeat, 16 frames each.
    local BUST_LOOP,CLOSEUP_FRAMES=5.2,16
    local function clamp(v,a,b)return math.max(a,math.min(b,v)) end
    local function lerp(a,b,p)return a+(b-a)*p end
    local function smooth(p)p=clamp(p,0,1);return p*p*(3-2*p) end
    local function smoother(p)p=clamp(p,0,1);return p*p*p*(p*(p*6-15)+10) end
    local function back(p)
        p=clamp(p,0,1)
        local c=1.4
        return 1+(c+1)*(p-1)^3+c*(p-1)^2
    end
    local function dimensions(s)
        local scale=s.scale or 1
        -- Camera approach fills the output vertically and intentionally crops
        -- only the lower body. Glass uses its own head-dominant paw source.
        local camera=s.h*1.15
        local glass=math.min(s.w*.92,s.h)
        return scale,camera,glass
    end
    local bust_scenes={glass_inspection=true,upside_down_peek=true}
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
            for id in pairs(bust_scenes) do
                result[id]=false
                reasons[id]="1254px bust source cannot provide required face coverage at native physical resolution"
            end
        end
        local t=s.target
        local top=t and math.max(75,t.x+75)
        local right=t and math.min(s.w-75,t.x+t.w-75)
        if not t or t.w<180 or t.h<160 or t.y<80 or t.y>s.h-120 or top>right then
            result.window_perch=false;reasons.window_perch="no supported visible upper window edge"
        end
        if not t or t.x<120 or t.x+t.w>s.w-80 or t.y<80 or t.y+t.h>s.h-60 or t.h<200 then
            result.hide_peek=false;reasons.hide_peek="window needs visible left/right edges for genuine occluded peeks"
        end
        return result,reasons
    end
    -- Which way the perch and nap poses face: the generated landing starts
    -- facing left, so a rightward arrival mirrors the whole sequence.
    local function side(from,to)return to.x>from.x and -1 or 1 end
    local function anchors(s,id)
        local _,camera,glass=dimensions(s)
        local home={x=s.w-123,y=s.h-133,scale=1}
        local target=s.target
        local bust=glass/150
        local a={home=home,
            air={x=s.w*.58,y=s.h*.43,scale=1},lower_air={x=s.w*.58,y=s.h*.43+55,scale=1},
            depth_far={x=s.w*.40,y=s.h*.36,scale=.18},depth_return={x=s.w*.65,y=s.h*.30,scale=.32},
            depth_near={x=s.w*.50,y=s.h*.56,scale=camera/150},
            glass={x=s.w*.50,y=s.h*.50,scale=bust},
            bottom_corner={x=s.w-113,y=s.h-73,scale=1},
            upper_air={x=s.w*.54,y=s.h*.20,scale=1},lower_edge={x=s.w*.55,y=s.h+5,scale=1}}
        if id=="screen_window" then
            -- Out into the room first, then a swooping approach to the glass.
            a.approach_via={x=s.w*.24,y=s.h*.18,scale=.36}
            a.retreat_via={x=s.w*.78,y=s.h*.16,scale=.34}
        end
        if bust_scenes[id] then
            -- Eye-registered full-body → bust handoff. The full body flies
            -- with the registered body transform at its normal size.
            home.scale=1/BODY
            home.y=home.y-26.2
        end
        if id=="upside_down_peek" then
            -- The bust hangs from the top edge, rotated: its chest crop stays
            -- above the output while the head fills the upper screen.
            a.top_exit={x=s.w*.47,y=-140,scale=1/BODY}
            a.top_hidden={x=s.w*.47,y=-75*bust-24,scale=bust}
            a.top_peek={x=s.w*.47,y=33*bust,scale=bust}
        end
        if target then
            a.window_top={x=math.max(75,math.min(s.w-75,target.x+target.w*.62)),y=target.y,scale=1}
            a.window_hidden={x=target.x+target.w*.50,y=target.y+target.h*.50,scale=1}
            a.window_left={x=target.x-22,y=target.y+target.h*.40,scale=1}
            a.window_left_shy={x=target.x+14,y=target.y+target.h*.36,scale=1}
            a.window_right={x=target.x+target.w+24,y=target.y+target.h*.58,scale=1}
        end
        if id~="screen_window" and not bust_scenes[id] then
            local normal=math.min(1,236/(150*(s.scale or 1)))
            for _,anchor in pairs(a) do anchor.scale=anchor.scale*normal end
        end
        if target then
            -- Edge clips: the cell center sits 185 canvas px (at 432 canvas
            -- px per cell) outside the stand-in edge; the wave key sits 30
            -- cell px right of the clip's crop.
            local k=a.window_hidden.scale
            local off,y=185*150*k/432,target.y+math.max(target.h*.36,75*k)
            a.edge_left={x=target.x-off,y=y,scale=k}
            a.edge_right={x=target.x+target.w+off,y=y,scale=k}
            a.edge_left_wave={x=a.edge_left.x+30*150*k/240,y=y,scale=k}
        end
        -- She arrives at the corner flying right, so the nap curls mirrored.
        a.nap_turn=-1
        if a.window_top then
            a.perch_turn=side(home,a.window_top);a.leave_turn=side(a.window_top,home)
            -- The perch frame's seat sits 21 px below the cell center; the
            -- body rests on the edge and the tail drops behind the window.
            a.window_top.y=target.y-21*a.window_top.scale+1
        end
        return a
    end
    local function travel(from,to,via,p)
        local e=smoother(p)
        local dx,dy=to.x-from.x,to.y-from.y
        if not via then
            local lift=clamp(math.sqrt(dx*dx+dy*dy)*.2,24,150)
            via={x=(from.x+to.x)/2,y=math.min(from.y,to.y)-lift,
                scale=2/(1/from.scale+1/to.scale)}
        end
        local u=1-e
        local x=u*u*from.x+2*u*e*via.x+e*e*to.x
        local y=u*u*from.y+2*u*e*via.y+e*e*to.y
        -- Depth moves linearly in 1/scale, so growth reads as perspective.
        local z=u*u/from.scale+2*u*e/via.scale+e*e/to.scale
        return x,y,1/z
    end
    local function flight_state(from,to,via,p,duration)
        local x,y,scale=travel(from,to,via,p)
        local h=.01
        local x0,y0=travel(from,to,via,math.max(0,p-h))
        local x1,y1=travel(from,to,via,math.min(1,p+h))
        local span=(math.min(1,p+h)-math.max(0,p-h))*duration
        return x,y,scale,(x1-x0)/span,(y1-y0)/span
    end
    -- Whole wing cycles per beat, so the flap rests level on arrival.
    local function wing(progress,duration,period)
        local cycles=math.max(1,math.floor(duration/period+.5))
        return progress*cycles
    end
    -- Plays a generated sequence at q, crossfading between neighbouring frames.
    local function play(sample,name,q,reverse)
        q=clamp(q,0,1)
        if reverse then q=1-q end
        local f=q*LAST;local i=math.min(LAST,math.floor(f))
        sample.pose="skin";sample.generated=true
        sample.skin_frame=MOTION[name]+i
        if i<LAST then sample.skin_next=sample.skin_frame+1;sample.skin_mix=f-i end
        return sample
    end
    -- Plays an edge clip at q (0..1) through generated source frames a..b.
    -- Half follows the generated timing and half the evenly spaced motion,
    -- so held moments still breathe and a fast swing is not a jump cut.
    local function clip(sample,name,a,b,q,blend)
        local picks=EDGE_PICKS[name]
        local function cell_at(f)
            local i=1
            while i<#picks-1 and picks[i+1]<=f do i=i+1 end
            return i-1+clamp((f-picks[i])/(picks[i+1]-picks[i]),0,1)
        end
        local from,to=cell_at(a),cell_at(b)
        local timed=cell_at(lerp(a,b,clamp(q,0,1)))
        local even=lerp(from,to,clamp(q,0,1))
        local last=#picks-1
        local f=clamp((timed+even)/2,0,last)
        -- Edge clips are dense enough to step from cell to cell; blending
        -- neighbours would double her outline during quick moves. A slow
        -- stretch may blend the middle of each step instead.
        sample.pose="skin";sample.generated=true
        if blend then
            local i=math.min(last,math.floor(f))
            sample.skin_frame=MOTION[name]+i
            if i<last then sample.skin_next=sample.skin_frame+1;sample.skin_mix=smooth((f-i-.3)/.4) end
        else
            sample.skin_frame=MOTION[name]+math.min(last,math.floor(f+.5))
        end
        return sample
    end
    local function cycle(sample,frames,position)
        local f=(position%1)*frames;local i=math.floor(f)%frames
        sample.closeup_frame=i;sample.closeup_next=(i+1)%frames;sample.closeup_mix=f-math.floor(f)
    end
    local function raw(id,s,elapsed)
        local semantic=choreography.sample(id,elapsed)
        local scene=choreography.get(id)
        local a=anchors(s,id)
        local from,to=assert(a[semantic.from]),assert(a[semantic.to])
        local p=semantic.ease
        local duration=scene.beats[semantic.beat][1]
        local sample={x=lerp(from.x,to.x,p),y=lerp(from.y,to.y,p),scale=lerp(from.scale,to.scale,p),
            opacity=1,airborne=1,bank=0,rotation=0,turn=1,pose="nested_hover",finished=semantic.finished,
            beat=semantic.beat,action=0,art_kind=0,phase=elapsed/.7,fog=0}
        local pose=semantic.pose
        local flying=semantic.moving and (pose:sub(1,6)=="flight" or pose=="startled_wingbeat")
        local vx,vy=0,0
        if flying then
            local via
            if pose=="flight_forward" and a.approach_via then via=a.approach_via
            elseif pose=="flight_back" and a.retreat_via then via=a.retreat_via
            elseif pose=="flight_land" and a.window_top then
                via={x=lerp(from.x,to.x,.7),y=to.y-170,scale=to.scale}
            end
            sample.x,sample.y,sample.scale,vx,vy=flight_state(from,to,via,semantic.progress,duration)
        end
        if id=="screen_window" or bust_scenes[id] then
            -- One dedicated source through the complete approach and retreat
            -- avoids a cold low-res→closeup swap and duplicate faces.
            sample.pose="closeup"
            sample.art_kind=bust_scenes[id] and 1 or 0
            if flying then
                local beats=wing(semantic.progress,duration,.34)
                sample.phase=beats
                -- Downstroke lifts; the bob fades as she flares to a stop.
                local size=150*sample.scale*(id=="screen_window" and 1 or BODY)
                local settle=1-smooth((semantic.progress-.82)/.18)
                sample.y=sample.y-.035*size*math.sin(2*math.pi*beats)*settle
                local lateral=vx/math.max(size,1)
                sample.bank=clamp(lateral*9,-16,16)*math.sin(math.pi*semantic.progress)
            end
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
                if sample.bust_mix>0 then
                    -- Bob only downward: the crop edge never lifts off the
                    -- bottom of the output.
                    sample.anchored=true
                    sample.y=sample.y+.012*150*sample.scale*(1-math.cos(2*math.pi*elapsed/1.7))/2*sample.bust_mix
                end
            elseif id=="upside_down_peek" then
                local inverted=pose:sub(1,8)=="inverted"
                sample.bust_mix=inverted and 1 or 0
                if inverted then
                    sample.rotation=180
                    sample.anchored=true
                    if pose=="inverted_peek" then
                        local e=back(semantic.progress)
                        sample.y=lerp(from.y,to.y,e)
                    elseif pose=="inverted_withdraw" then
                        local e=semantic.progress^2
                        sample.y=lerp(from.y,to.y,e)
                    end
                    local sway=math.sin(2*math.pi*elapsed/2.4)
                    if pose=="inverted_look" then
                        -- Hanging from her tail: a slow pendulum toward each side.
                        local look=math.sin(math.pi*2*semantic.progress)
                        sample.rotation=180+7*look+1.5*sway
                        sample.x=sample.x-.06*150*sample.scale*look
                    else
                        sample.rotation=180+1.5*sway
                    end
                    sample.y=sample.y-.01*150*sample.scale*(1-math.cos(2*math.pi*elapsed/1.9))/2
                end
            end
            if flying then
                -- Generated wingbeats: one 16-frame loop holds two beats.
                cycle(sample,CLOSEUP_FRAMES,sample.phase/2)
                sample.body_frame=sample.closeup_frame;sample.body_next=sample.closeup_next;sample.body_mix=sample.closeup_mix
                if bust_scenes[id] then sample.closeup_frame,sample.closeup_next,sample.closeup_mix=0,nil,nil end
            elseif bust_scenes[id] then
                -- The generated bust gesture loop: glances, a blink, a paw tap.
                cycle(sample,CLOSEUP_FRAMES,elapsed/BUST_LOOP)
            else
                -- Hovering at the glass, she keeps a slower generated wingbeat.
                cycle(sample,CLOSEUP_FRAMES,elapsed/1.1)
            end
            if flying then sample.action=bust_scenes[id] and 1 or 0
            elseif pose=="glass_paw" then sample.action=3
            else sample.action=bust_scenes[id] and 0 or 2 end
            if pose=="glass_paw" then sample.fog=math.sin(math.pi*semantic.progress)^4 end
            if not flying then sample.phase=elapsed/.7 end
            return sample
        end
        if rows[pose] then
            sample.pose="skin";sample.skin_frame=rows[pose]*8+math.floor(elapsed/.14)%8
            if pose=="curled_sleep" then sample.turn=a.nap_turn;sample.facing=-a.nap_turn
            elseif pose=="perch_tail_dangle" then sample.turn=a.perch_turn;sample.facing=0 end
        elseif pose=="curl_down" or pose=="sleep_wake" then
            play(sample,"curl",semantic.progress,pose=="sleep_wake")
            sample.turn=a.nap_turn;sample.facing=-a.nap_turn
        elseif pose=="wings_fold" then
            sample.pose="skin";sample.skin_frame=9*8;sample.turn=a.perch_turn;sample.facing=0
        elseif pose=="wings_unfold" then
            sample.pose="skin";sample.skin_frame=9*8;sample.turn=a.leave_turn;sample.facing=0
        elseif pose=="hopeful_glances" then
            sample.pose="skin";sample.skin_frame=6*8+math.floor(elapsed/.2)%8
        elseif pose=="tail_catch_recover" then
            sample.pose="skin";sample.skin_frame=11*8+math.floor(elapsed/.14)%8
        elseif pose=="sleepy_nod" or pose=="sleepy_fall" then
            sample.pose="skin";sample.skin_frame=8*8+math.floor(elapsed/.2)%8
            sample.rotation=4*math.sin(math.pi*semantic.progress)
        elseif pose=="groom_stretch" then sample.pose="skin";sample.skin_frame=15
        elseif pose=="settle_hover" then sample.pose="nested_hover"
        elseif pose=="quiet_rest" then
            sample.pose="nested_hover";sample.airborne=0;sample.phase=0
        elseif pose:sub(1,4)=="peek" then
            -- Generated edge clips keep the stand-in window's occlusion: the
            -- hidden body is absent and the gripping paw is drawn over the
            -- real edge, so they play in front of the window, edge on edge.
            -- The right edge mirrors them; she comes out at the left, where
            -- the pull ends on the canonical wave her turn home starts from.
            local q=semantic.progress
            local beat_time=q*duration
            sample.facing=0;sample.anchored=true
            sample.turn=semantic.from:find("right") and -1 or 1
            if pose=="peek_wait" then
                -- Fully hidden. Pleamar can move her layer a frame before her
                -- art and position, so a visible wait would flash in front.
                sample.pose="skin";sample.skin_frame=8;sample.behind=true;sample.opacity=0
            elseif pose=="peek_head" then
                -- Head first; then she pauses and her paw rises from under
                -- her chin and eases its fingers over the edge into a grip.
                local f=q<.45 and .563*q/.45 or .563+.437*smooth((q-.45)/.55)
                clip(sample,"edge_head",20,123,f,true)
            elseif pose=="peek_reach" then
                -- Her paw pokes out first and grips; her head follows.
                clip(sample,"edge_reach",14,123,q)
            elseif pose=="peek_look" then
                -- The pull clip opens on a glance with the paw held still:
                -- a turn out along it, a curious little nod while she looks,
                -- and a turn back.
                local look=smooth(math.min(1,1.8*math.sin(math.pi*q)))
                clip(sample,"edge_pull",0,EDGE_GLANCE,look*(1-.3*math.sin(math.pi*smooth((q-.25)/.5))))
            elseif pose=="peek_hide" then
                -- Her paw lets go of the edge and slips back, then her head
                -- ducks after it, blended like the way out.
                local src=q<.15 and 123-23*q/.15 or q<.55 and 100-12*(q-.15)/.4 or 88-68*smooth((q-.55)/.45)
                clip(sample,"edge_head",20,123,(src-20)/103,true)
            elseif pose=="peek_pull" then
                -- Her slide out from behind the edge (source 50-86) runs at
                -- its generated pace; the glance and grip before it and the
                -- glide after run brisker.
                local src=q<.3 and 50*q/.3 or q<.65 and 50+36*(q-.3)/.35 or 86+37*(q-.65)/.35
                clip(sample,"edge_pull",0,123,src/123)
            elseif pose=="peek_wave" then
                -- The pull ends on the horned wave key: two happy hops
                -- with a tilt and the paw wave, all back at rest before she
                -- settles into the front hover the turn home starts on.
                local waving=duration-.9
                local w=math.min(beat_time,waving)/waving
                local hop=math.abs(math.sin(2*math.pi*w))
                sample.pose="skin";sample.skin_frame=EDGE_WAVE
                sample.turn=1-.05*hop
                sample.rotation=-6*math.sin(3*math.pi*w)
                sample.wave_phase=2*w
                sample.y=sample.y-9*sample.scale*hop
                -- Paws first, on the horned front hover, then only her horns
                -- change into the crest of the shared hover, all before the
                -- turn home starts on it.
                local settle=smooth((beat_time-waving)/.3)
                local crest=smooth((beat_time-waving-.3)/.3)
                if crest>0 then
                    sample.skin_frame=EDGE_WAVE+1;sample.skin_next=8;sample.skin_mix=crest
                elseif settle>0 then
                    sample.skin_next=EDGE_WAVE+1;sample.skin_mix=settle
                end
            end
        elseif flying then
            local dx=to.x-from.x
            local beats=wing(semantic.progress,duration,.5)
            local up=pose=="flight_up" or pose=="startled_wingbeat" or (math.abs(dx)<60 and to.y<from.y-60)
            local facing=dx<0 and -1 or 1
            if math.abs(dx)<30 and not up then facing=0 end
            sample.facing=facing
            sample.pose="skin"
            if up then
                sample.skin_frame=4*8+math.floor(beats*8)%8
                -- The climbing art faces right; mirror it for a leftward climb.
                if dx<0 then sample.turn=-1;sample.facing=-1 else sample.facing=1 end
            elseif facing==0 then
                sample.skin_frame=8
            else
                sample.skin_frame=(facing<0 and 0 or 2)*8+math.floor(beats*8)%8
            end
            local settle=1-smooth((semantic.progress-.8)/.2)
            sample.y=sample.y-3*sample.scale*math.sin(2*math.pi*beats)*settle
            local speed=math.sqrt(vx*vx+vy*vy)
            if speed>1 and facing~=0 then
                local pitch=math.deg(math.atan(vy/math.max(math.abs(vx),.001)))*.45
                sample.bank=clamp(pitch,-22,22)*(sample.facing or 1)
                if up then sample.bank=clamp(vx/math.max(speed,1)*14,-14,14) end
            end
            -- Generated bank into a climb: side flight pitches nose-up through
            -- these frames as the arc rises, and a climb opens with them.
            local mirror=sample.facing<0 and -1 or 1
            if up and semantic.progress<.35 then
                play(sample,"bank",semantic.progress/.35);sample.turn=mirror
                sample.bank=sample.bank*smooth(semantic.progress/.35)
            elseif not up and facing~=0 and speed>1 then
                local climb=clamp((-math.deg(math.atan(vy/math.max(math.abs(vx),.001)))-10)/35,0,1)
                if climb>0 then
                    play(sample,"bank",clamp(climb*.55+.04*climb*math.sin(2*math.pi*beats),0,1));sample.turn=mirror
                    sample.bank=sample.bank*(1-.6*climb)
                end
            end
            -- Climb frames are flight variants, so a body turn may still override them.
            sample.generated=nil
            if id=="window_perch" and pose=="flight_land" and semantic.progress>.5 then
                -- Generated landing: upright, a braking flap, onto the edge.
                local q=(semantic.progress-.5)/.5
                play(sample,"perch",q);sample.turn=a.perch_turn;sample.facing=-a.perch_turn
                sample.bank=sample.bank*(1-smooth(q/.4))
            elseif id=="window_perch" and pose=="flight_home" and semantic.progress<.4 then
                play(sample,"perch",semantic.progress/.4,true);sample.turn=a.leave_turn;sample.facing=-a.leave_turn
                sample.bank=sample.bank*smooth((semantic.progress-.2)/.2)
            elseif id=="corner_nap" and pose=="flight_home" and semantic.progress<.3 then
                -- Awake and airborne, she swings round to face the room.
                play(sample,"arrive_right",semantic.progress/.3);sample.facing=0
            end
            if pose=="flight_home" and semantic.progress>.78 and not sample.generated then
                -- Hover in front-on for the last stretch, matching her rest pose.
                sample.skin_frame=8;sample.facing=0;sample.turn=1
                sample.bank=sample.bank*(1-smooth((semantic.progress-.78)/.22))
            end
        elseif semantic.moving then
            local dx=to.x-from.x
            sample.pose=dx<0 and "fly-left" or "fly-right"
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
        if id=="hide_peek" and (pose=="flight_behind" or pose=="peek_wait") then sample.behind=true end
        if id=="window_perch" and (pose=="wings_fold" or pose=="perch_tail_dangle" or pose=="wings_unfold"
            or (pose=="flight_land" and semantic.progress>.7)
            or (pose=="flight_home" and semantic.progress<.25)) then
            -- She settles onto the back of the top edge, so anything below
            -- it, the hanging tail included, is behind the window.
            sample.behind=true
        end
        -- Keep the selected normal-size scene in one finite skin binding:
        -- exact canonical flight frames share this atlas with authored AFK
        -- actions, so a landing never blanks for another texture warm-up.
        if sample.pose=="fly-left" or sample.pose=="fly-right" or sample.pose=="fly-up" then
            local row=sample.pose=="fly-left" and 0 or sample.pose=="fly-right" and 2 or 4
            sample.facing=row==0 and -1 or 1
            sample.pose="skin";sample.skin_frame=row*8+math.floor(elapsed/.07)%8
        elseif sample.pose=="nested_hover" then sample.pose="skin";sample.skin_frame=8 end
        if sample.pose=="skin" then
            sample.art_kind=2;sample.phase=elapsed/1.12
            local row=math.floor(sample.skin_frame/8)
            if sample.facing==nil then sample.facing=row==8 and -1 or 0 end
            local actions={[3]=4,[5]=5,[7]=6,[8]=7,[9]=8,[10]=9,[11]=10}
            if actions[row] then
                sample.action=actions[row]
                sample.skin_frame=row*8
            elseif row==1 and sample.skin_frame~=15 then
                sample.action=11;sample.skin_frame=8
            end
            if id=="quiet_rest" then sample.action=0;sample.phase=0 end
            if sample.wave_phase then sample.phase=sample.wave_phase;sample.wave_phase=nil end
        end
        return sample
    end
    -- A facing change plays a generated body turn, centered on the authored
    -- switch: side to side through the front view, or side to front hover.
    local function turned(id,s,elapsed,sample)
        if sample.pose~="skin" or id=="quiet_rest" or sample.generated then return sample end
        local half=TURN/2
        local function at(t)return raw(id,s,math.max(0,t)) end
        local b,f=at(elapsed-half),at(elapsed+half)
        if b.generated or f.generated then return sample end
        local before,after=b.facing,f.facing
        if before==nil or after==nil or before==after then return sample end
        local lo,hi=elapsed-half,elapsed+half
        for _=1,12 do
            local mid=(lo+hi)/2
            if at(mid).facing==before then lo=mid else hi=mid end
        end
        local switch=(lo+hi)/2
        local span=(before~=0 and after~=0) and TURN or ARRIVE
        local q=(elapsed-switch)/span+.5
        if q<=0 or q>=1 then return sample end
        local bank=sample.bank
        if before~=0 and after~=0 then
            play(sample,"turn",q,before>0)
            sample.bank=bank*math.abs(1-2*q)
        elseif after==0 then
            play(sample,before<0 and "arrive_left" or "arrive_right",q)
            sample.bank=bank*(1-q)
        else
            play(sample,after<0 and "arrive_left" or "arrive_right",q,true)
            sample.bank=bank*q
        end
        sample.turn=1;sample.action=0
        return sample
    end
    return {
        availability=availability,
        poses=function()
            local supported={}
            for _,scene in ipairs(choreography.catalog) do for pose in pairs(scene.poses) do supported[pose]=true end end
            return supported
        end,
        sample=function(id,s,elapsed)
            local sample=turned(id,s,elapsed,raw(id,s,elapsed))
            sample.facing=nil;sample.generated=nil
            if sample.beat==1 then
                local semantic=choreography.sample(id,elapsed)
                sample.opacity=math.min(1,semantic.progress*8)
            end
            if sample.finished then sample.opacity=0 end
            return sample
        end,
    }
end
