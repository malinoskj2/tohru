//! CPU-only checks of the actual feature controller at its native API boundary.
use mlua::Lua;

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
