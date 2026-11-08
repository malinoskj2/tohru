//! CPU-only checks of the actual feature controller at its native API boundary.
use mlua::Lua;

#[path = "../osd/logic.rs"]
mod osd;

#[path = "../../runtime/workspace_luau.rs"]
mod actual_workspace_binding;

fn controller() -> Lua {
    let lua = Lua::new();
    lua.load(r#"
        fact = {['audio.volume']=-1, ['audio.muted']=false, ['sound_outputs.count']=0, sound_open=false}
        text, handlers, timers, calls = {}, {}, {}, {}
        function on(name, callback) handlers[name]=callback end
        function after(ms, callback) assert(ms==50); timers[1]=callback; return 1 end
        function cancel(id) timers[id]=nil end
        sys = {call=function(name, value)
            if denied then error('sound server refused') end
            table.insert(calls, {name=name, value=value})
        end}
        json = {encode=function(status) last_status=status; return 'snapshot' end}
    "#).exec().unwrap();
    lua.load(include_str!("../../shell/sound/controller.luau"))
        .eval::<mlua::Function>()
        .unwrap()
        .call::<()>(())
        .unwrap();
    lua
}

#[test]
fn observation_keeps_missing_and_unmuted_distinct() {
    let lua = controller();
    lua.load(
        r#"
        assert(last_status.observation=='awaiting_report')
        assert(last_status.volume==nil and last_status.muted==nil)
        fact['audio.volume']=0; fact['audio.muted']=false; fact['sound_outputs.count']=1
        handlers.audio_report()
        assert(last_status.volume==0 and last_status.muted==false and last_status.output_present)
        handlers.audio_step(1)
        assert(calls[1].name=='audio.step' and calls[1].value==0.05)
        assert(fact['audio.volume']==0, 'native observations remain authoritative')
        handlers.audio_mute()
        assert(calls[2].name=='audio.mute' and calls[2].value==nil)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn slider_is_bounded_coalesces_and_flushes_final_value() {
    let lua = controller();
    lua.load(
        r#"
        handlers.audio_set(0.2); handlers.audio_set(0.4); handlers.audio_set(0.9)
        assert(#calls==0 and timers[1])
        handlers.audio_flush()
        assert(#calls==1 and calls[1].name=='audio.volume' and calls[1].value==0.9)
        assert(timers[1]==nil)
        handlers.audio_set(0); timers[1]()
        assert(#calls==2 and calls[2].value==0)
        handlers.audio_set(1.01); handlers.audio_set(-0.1); handlers.audio_set(0/0)
        assert(#calls==2 and last_status.error~='')
        handlers.audio_set(0.5); handlers.panel_close()
        assert(#calls==3 and calls[3].value==0.5 and not fact.sound_open)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn failures_are_reported_and_later_native_success_clears_them() {
    let lua = controller();
    lua.load(
        r#"
        denied=true; handlers.audio_mute()
        assert(string.find(last_status.error, 'sound server refused'))
        assert(last_status.volume==nil)
        denied=false; handlers.audio_step(-1)
        assert(last_status.error=='' and calls[1].value==-0.05)
        handlers.sound_toggle(); assert(fact.sound_open)
        handlers.sound_toggle(); assert(not fact.sound_open)
        handlers.sound_open_request(); assert(fact.sound_open and fact.sound_focus==1)
        handlers.panel_close(); assert(not fact.sound_open and not fact.sound_dragging)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn native_dismissal_toggle_and_reload_clean_drag_state_idempotently() {
    let lua = controller();
    lua.load(
        r#"
        fact['audio.volume']=0.2; fact['sound_outputs.count']=1
        fact.sound_open=true; fact.sound_dragging=true; fact.sound_requested=0.8
        handlers.audio_set(0.8)
        fact.sound_open=false
        handlers['fact:sound_open'](); handlers['fact:sound_open']()
        assert(#calls==1 and calls[1].value==0.8 and timers[1]==nil)
        assert(not fact.sound_dragging)
        handlers.sound_open_request()
        assert(not fact.sound_dragging and fact['audio.volume']==0.2)
        fact.sound_dragging=true; handlers.audio_set(0.7); handlers.sound_toggle()
        assert(not fact.sound_open and not fact.sound_dragging and calls[2].value==0.7)
        fact.sound_open=true; fact.sound_dragging=true; fact.sound_requested=0.9
        handlers.audio_set(0.9)
        -- Native VM replacement owns cancellation of old VM timer callbacks.
        timers={}
    "#,
    )
    .exec()
    .unwrap();
    lua.load(include_str!("../../shell/sound/controller.luau"))
        .eval::<mlua::Function>()
        .unwrap()
        .call::<()>(())
        .unwrap();
    lua.load(
        r#"
        assert(not fact.sound_dragging and #calls==2 and timers[1]==nil)
        handlers.panel_close()
        assert(#calls==2 and not fact.sound_open)
        denied=true; handlers.audio_mute()
        assert(fact.audio_failed and text.audio_error~='')
        denied=false; handlers.audio_mute()
        assert(not fact.audio_failed and text.audio_error=='')
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn command_error_has_a_visible_panel_boundary() {
    let view = include_str!("../../shell/sound/panel.plm");
    let root = include_str!("../../shell/bar.plm");
    assert!(view.contains("text audio_error"));
    assert!(view.contains("Could not change volume"));
    assert!(view.contains("show: audio_failed"));
    assert!(view.contains("116 + audio_failed * 62"));
    assert!(root.contains("116 + audio_failed * 62"));
}

#[test]
fn actual_root_composition_closes_opposite_panels_without_emit_loopback() {
    let _guard = actual_workspace_binding::TEST_LOCK.lock().unwrap();
    let lua = Lua::new();
    actual_workspace_binding::install(&lua).unwrap();
    lua.load("tohru_workspace.reset('workspace'); tohru_workspace.reset('calendar')")
        .exec()
        .unwrap();
    lua.load(r#"
        fact={['audio.volume']=0.2, ['audio.muted']=false, ['sound_outputs.count']=1,
              sound_open=false, sound_dragging=false, cal_open=false}
        text,model,handlers,timers,calls,modules={},{},{},{},{},{}
        function on(name, callback)
            handlers[name]=handlers[name] or {}; table.insert(handlers[name],callback)
        end
        function dispatch(name,value)
            for _,callback in ipairs(handlers[name] or {}) do callback(value) end
        end
        function after(ms,callback) timers[1]=callback; return 1 end
        function every(ms,callback) assert(ms==1000/60); timers[2]=callback; return 2 end
        function cancel(id) timers[id]=nil end
        -- Match native direction: emit and fact writes do NOT call Lua handlers.
        function emit(name) table.insert(calls,{render_only=name}) end
        function require(name) return modules[name] end
        sys={call=function(name,value) table.insert(calls,{name=name,value=value}) end,
             watch=function(name,callback)
                if name=='clock' then callback({year=2026,month=11,day=7,weekday=6,hour=12,minute=0})
                else callback({}) end
                return true
             end}
        json={encode=function(value) return 'snapshot' end}
        tohru_settings={snapshot=function() return {
            ['bar.mooncoil']={value=false},['appearance.corners']={value=true},['appearance.corner_size']={value=32},
            ['bar.preferred_output']={value='legacy'}
        } end}
    "#).exec().unwrap();
    let modules: mlua::Table = lua.globals().get("modules").unwrap();
    for (name, source) in [
        (
            "osd/controller",
            include_str!("../../shell/osd/controller.luau"),
        ),
        (
            "sound/controller",
            include_str!("../../shell/sound/controller.luau"),
        ),
        (
            "bar/controller",
            include_str!("../../shell/bar/controller.luau"),
        ),
        (
            "calendar/controller",
            include_str!("../../shell/calendar/controller.luau"),
        ),
        (
            "calendar/rail-publisher",
            include_str!("../../shell/calendar/rail-publisher.luau"),
        ),
        (
            "bar/workspace",
            include_str!("../../shell/bar/workspace.luau"),
        ),
        (
            "settings/controller",
            include_str!("../../shell/settings/controller.luau"),
        ),
    ] {
        modules
            .set(name, lua.load(source).eval::<mlua::Function>().unwrap())
            .unwrap();
    }
    lua.load(include_str!("../../shell/bar.luau"))
        .exec()
        .unwrap();
    lua.load(
        r#"
        assert(fact.corners and fact.corner_size==32)
        dispatch('audio_report'); assert(not fact.osd_open)
        dispatch('audio_step',1); assert(not fact.osd_open, 'ack is not an observation')
        fact['audio.volume']=0.4; dispatch('audio_report')
        assert(fact.osd_open and text.osd_label=='40')
        dispatch('sound_open_request'); assert(not fact.osd_open)
        dispatch('panel_close')
        dispatch('calendar_open'); assert(fact.cal_open and not fact.sound_open)
        assert(text.cal_rail_error=='' and not fact.cal_rail_moving and fact.cal_rail_present)
        dispatch('calendar_next'); assert(fact.cal_rail_moving and timers[2])
        dispatch('calendar_close'); assert(timers[2]==nil)
        dispatch('calendar_open'); assert(not fact.cal_rail_moving and text.cal_rail_error=='')
        dispatch('sound_open_request'); assert(fact.sound_open and not fact.cal_open)
        fact.sound_dragging=true; dispatch('audio_set',0.8)
        dispatch('calendar_open')
        assert(fact.cal_open and not fact.sound_open and not fact.sound_dragging)
        assert(timers[1]==nil and calls[#calls].name=='audio.volume')
        dispatch('panel_close'); dispatch('panel_close')
        assert(not fact.cal_open and not fact.sound_open)
        dispatch('sound_toggle'); assert(fact.sound_open and not fact.cal_open)
        dispatch('calendar_toggle'); assert(fact.cal_open and not fact.sound_open)
        dispatch('calendar_close'); assert(not fact.cal_open)
        dispatch('sound_open_request'); fact.sound_dragging=true
        dispatch('audio_set',0.6); fact.sound_open=false
        dispatch('fact:sound_open',false)
        assert(not fact.sound_dragging and timers[1]==nil)
    "#,
    )
    .exec()
    .unwrap();
}
