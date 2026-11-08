//! CPU-only checks of the actual feature controller at its native API boundary.
use mlua::Lua;

#[path = "../osd/logic.rs"]
mod osd;

#[path = "../idle-settings/logic.rs"]
mod idle_settings;

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
             ask=function(name)
                assert(name=='capture.outputs'); output_queries=(output_queries or 0)+1
                return {{name='PRIVATE-1',width=1280,height=720}}
             end,
             watch=function(name,callback)
                if name=='clock' then callback({year=2026,month=11,day=7,weekday=6,hour=12,minute=0})
                else callback({}) end
                return true
             end}
        json={encode=function(value) return 'snapshot' end}
        info_reads=0
        tohru_info={snapshot=function()
            info_reads=info_reads+1
            return {system={os_name='Fixture Linux'},accounts={},disks={},mounts={},generations={},issues={}}
        end}
        tohru_idle={start=function(config)
            assert(config.lock_seconds==1800 and config.screen_off_unlocked_seconds==0)
            assert(config.screen_off_locked_seconds==60 and config.suspend_unlocked_seconds==7200)
            assert(config.suspend_locked_seconds==5400 and config.fade_milliseconds==2000)
            idle_configured=true; idle_configure_count=(idle_configure_count or 0)+1
        end}
        tohru_settings={snapshot=function() return {
            ['bar.mooncoil']={value=false},['appearance.corners']={value=true},['appearance.corner_size']={value=32},
            ['bar.preferred_output']={value='legacy'},
            ['idle.lock_seconds']={value=1800}, ['idle.screen_off_unlocked_seconds']={value=0},
            ['idle.screen_off_locked_seconds']={value=60}, ['idle.suspend_unlocked_seconds']={value=7200},
            ['idle.suspend_locked_seconds']={value=5400}, ['idle.fade_milliseconds']={value=2000}
        } end}
    "#).exec().unwrap();
    let modules: mlua::Table = lua.globals().get("modules").unwrap();
    for (name, source) in [
        (
            "network/controller",
            include_str!("../../shell/network/controller.luau"),
        ),
        (
            "bluetooth/controller",
            include_str!("../../shell/bluetooth/controller.luau"),
        ),
        (
            "info/controller",
            include_str!("../../shell/info/controller.luau"),
        ),
        (
            "about/controller",
            include_str!("../../shell/about/controller.luau"),
        ),
        ("info/power", include_str!("../../shell/info/power.luau")),
        (
            "osd/controller",
            include_str!("../../shell/osd/controller.luau"),
        ),
        (
            "settings/idle",
            include_str!("../../shell/settings/idle.luau"),
        ),
        (
            "session/idle",
            include_str!("../../shell/session/idle.luau"),
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
            .set(name, lua.load(source).eval::<mlua::Value>().unwrap())
            .unwrap();
    }
    lua.load(include_str!("../../shell/bar.luau"))
        .exec()
        .unwrap();
    lua.load(
        r#"
        assert(fact.corners and fact.corner_size==32 and idle_configured)
        dispatch('config_reload'); assert(idle_configure_count==1, 'unchanged projection cannot resume idle')
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
        assert(info_reads==0, 'closed information pages cannot read snapshots')
        dispatch('network_open'); assert(fact.net_open and not fact.sound_open and not fact.cal_open)
        fact.net_joining=true; text.net_password='fixture secret'
        dispatch('bluetooth_open')
        assert(fact.bt_open and not fact.net_open and text.net_password=='')
        fact['bt.pairing_id']=7; fact['bt.pairing']='pin'
        dispatch('bluetooth_question'); assert(fact.bt_open)
        fact.bt_scan_owned=true; text.bt_answer='private answer'
        dispatch('info_open',5)
        assert(fact.info_visible and not fact.bt_open and not fact.bt_scan_owned and text.bt_answer=='')
        assert(info_reads==1)
        dispatch('info_open',1)
        assert(info_reads==1 and output_queries==1 and model.info_rows[1].value=='PRIVATE-1')
        assert(model.info_rows[2].value=='1280 × 720' and model.info_rows[3].value=='Unknown')
        fact['bt.pairing_id']=0; fact['bt.pairing']='none'
        dispatch('about_open'); assert(fact.about_visible and not fact.info_visible and info_reads==2)
        dispatch('about_more')
        assert(fact.info_visible and not fact.about_visible and fact.info_page==0 and info_reads==3)
        dispatch('network_open'); dispatch('settings_open_request')
        assert(fact.settings_open and not fact.net_open and not fact.info_visible and not fact.bt_open)
        dispatch('panel_close'); assert(info_reads==3)
    "#,
    )
    .exec()
    .unwrap();
}
