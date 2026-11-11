//! CPU-only checks of the actual feature controller at its native API boundary.
use mlua::Lua;

#[path = "../osd/logic.rs"]
mod osd;


#[path = "../../runtime/pleamar/bindings/workspace_luau.rs"]
mod actual_workspace_binding;

fn controller() -> Lua {
    let lua = Lua::new();
    lua.load(r#"
        fact = {['audio.volume']=-1, ['audio.muted']=false, ['sound_outputs.count']=0, sound_open=false}
        text, model, handlers, timers, calls = {}, {}, {}, {}, {}
        function on(name, callback) handlers[name]=callback end
        function after(ms, callback) assert(ms==50); timers[1]=callback; return 1 end
        function cancel(id) timers[id]=nil end
        sys = {watch=function(name, callback) assert(name=='audio'); audio_watch=callback;return true end,call=function(name, value, extra)
            if denied then error('sound server refused') end
            table.insert(calls, {name=name, value=value, extra=extra})
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
fn native_input_apps_and_output_selection_use_typed_current_ids_only() {
    let lua = controller();
    lua.load(r#"
        fact['audio.volume']=0.3;fact['sound_outputs.count']=1;fact['sound_outputs.0.id']=0
        fact['audio.input']=0.5;fact['audio.input_muted']=false
        fact['sound_inputs.count']=1;fact['sound_inputs.0.id']=0;fact['sound_inputs.0.default']=true
        fact['sound_apps.count']=1;fact['sound_apps.0.id']=7;fact['sound_apps.0.volume']=0.98
        text['sound_apps.0.name']=string.rep('界',200);handlers.audio_report()
        assert(last_status.outputs[1].id==0 and last_status.inputs[1].id==0 and last_status.apps[1].id==7)
        assert(#last_status.apps[1].name<=256 and utf8.len(last_status.apps[1].name))
        assert(last_status.input_selection=='unavailable_native_id_collision')
        assert(last_status.input_available and last_status.input_muted==false and last_status.input_volume==0.5)
        handlers.audio_input_up();assert(calls[1].name=='audio.input' and calls[1].value==0.55)
        handlers.audio_input_mute();assert(calls[2].name=='audio.input_mute' and calls[2].value==nil)
        assert(handlers.audio_input_select==nil, 'colliding input IDs never route audio.default')
        handlers.audio_output_select(0);assert(calls[3].name=='audio.default' and calls[3].value==0)
        handlers.audio_app_up(7)
        assert(calls[4].name=='audio.app_volume' and calls[4].value==7 and calls[4].extra==1)
        handlers.audio_app_mute(7);assert(calls[5].name=='audio.app_mute' and calls[5].value==7)
        assert(fact['sound_apps.0.volume']==0.98 and fact['audio.input']==0.5, 'ack does not overwrite observations')
        fact['sound_apps.0.id']=8
        handlers.audio_app_down(7);assert(#calls==5 and last_status.error~='')
        for _,id in {-1,0.5,4294967296,0/0,math.huge} do handlers.audio_output_select(id);handlers.audio_app_mute(id) end
        assert(#calls==5)
        fact['sound_inputs.0.default']=false;handlers.audio_report()
        handlers.audio_input_down();handlers.audio_input_mute()
        assert(#calls==5 and not last_status.input_available and last_status.input_volume==nil)
        handlers.sound_details_toggle();assert(fact.sound_details)
        handlers.sound_tab(1);assert(fact.sound_tab==1 and fact.sound_list_page==0)
        fact['sound_inputs.count']=5;handlers.sound_list_page(1);assert(fact.sound_list_page==1)
        handlers.sound_list_page(1);assert(fact.sound_list_page==1)
        handlers.sound_tab(2);assert(fact.sound_list_page==0 and fact.sound_row_id==8)
        fact['sound_apps.0.id']=9;handlers.audio_report();assert(#calls==5 and fact.sound_row_id==9, 'replacement reselects outlined row without issuing a request')
        fact['sound_apps.0.id']=8;handlers.audio_report()
        fact['sound_inputs.0.default']=true;handlers.audio_report()
        handlers.audio_set(0.2);handlers.audio_input_set(0.7);handlers.audio_input_set(0.8)
        assert(#calls==6 and calls[6].name=='audio.volume' and calls[6].value==0.2)
        fact.sound_input_dragging=true;handlers.panel_close()
        assert(#calls==7 and calls[7].name=='audio.input' and calls[7].value==0.8 and not fact.sound_input_dragging and timers[1]==nil)
        handlers.sound_details_step(1);assert(calls[8].name=='audio.app_volume' and calls[8].value==8)
        handlers.sound_details_activate();assert(calls[9].name=='audio.app_mute' and calls[9].value==8)
        handlers.sound_tab(0);handlers.sound_details_activate();assert(calls[10].name=='audio.default' and calls[10].value==0)
        handlers.sound_tab(1);handlers.sound_details_step(-1);assert(calls[11].name=='audio.input' and calls[11].value==0.45)
        handlers.sound_details_activate();assert(calls[12].name=='audio.input_mute')
        handlers.sound_tab(2);fact['sound_apps.count']=0;fact.sound_list_page=8;handlers.audio_report()
        assert(fact.sound_list_page==0)
    "#).exec().unwrap();
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
    assert!(view.contains("Audio request failed"));
    assert!(view.contains("show: audio_failed"));
    assert!(view.contains("116 + sound_details * 316 + audio_failed * 62"));
    assert!(root.contains("116 + sound_details * 316 + audio_failed * 62"));
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
        local model_rows={}
        model=setmetatable({}, {__index=function(_,key)return model_rows[key]end,
            __newindex=function(_,name,rows)
                model_rows[name]=rows;fact[name..'.count']=#rows
                for i,row in ipairs(rows) do for key,value in pairs(row) do
                    local path=name..'.'..(i-1)..'.'..key
                    if type(value)=='string' then text[path]=value else fact[path]=value end
                end end
            end})
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
                if name=='audio' then audio_watch=callback
                elseif name=='clock' then callback({year=2026,month=11,day=7,weekday=6,hour=12,minute=0})
                else callback({}) end
                return true
             end}
        json={encode=function(value) return 'snapshot' end}
        info_reads=0
        tohru_info={snapshot=function()
            info_reads=info_reads+1
            return {system={os_name='Fixture Linux'},accounts={},disks={},mounts={},generations={},issues={}}
        end}
        tohru_settings={snapshot=function() return {
            ['bar.mooncoil']={value=false},['appearance.corners']={value=true},['appearance.corner_size']={value=32},
            ['bar.preferred_output']={value='legacy'},
            ['appearance.palette']={value='mocha'}
        } end}
    "#).exec().unwrap();
    let modules: mlua::Table = lua.globals().get("modules").unwrap();
    for (name, source) in [
        (
            "session-route",
            include_str!("../../shell/session-route.luau"),
        ),
        ("bar/bell", include_str!("../../shell/bar/bell.luau")),
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
        assert(fact.corners and fact.corner_size==32)
        local asked; for _,call in ipairs(calls) do if call.name=='say' and call.value=='tohru-notifications' then asked=true end end
        assert(asked, 'the bar asks the notification daemon for its state')
        dispatch('notices_state',3); assert(fact.notices_unread and fact.notices_dnd)
        dispatch('notices_state',0); assert(not fact.notices_unread and not fact.notices_dnd)
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
        assert(info_reads==1 and model.info_rows[1].value=='Unknown')
        fact['bt.pairing_id']=0; fact['bt.pairing']='none'
        dispatch('about_open'); assert(fact.about_visible and not fact.info_visible and info_reads==2)
        dispatch('about_more')
        assert(fact.info_visible and not fact.about_visible and fact.info_page==0 and info_reads==3)
        dispatch('network_open'); dispatch('panel_close')
        assert(not fact.net_open and not fact.info_visible and not fact.bt_open and info_reads==3)
        tohru_settings.set=function(key,value) last_setting_key=key;last_setting_value=value;return tohru_settings.snapshot() end
        dispatch('setting_palette',3); assert(last_setting_key=='appearance.palette' and last_setting_value=='nord')

    "#,
    )
    .exec()
    .unwrap();
}
