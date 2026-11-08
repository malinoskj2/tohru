//! Actual native-observation OSD controller: no renderer/event-loop simulation.
use mlua::Lua;

#[test]
fn observed_changes_hold_cancel_and_reload_without_polling_or_commands() {
    let lua = Lua::new();
    lua.load(
        r#"
        fact={['audio.volume']=-1, ['audio.muted']=false, ['sound_outputs.count']=0,
              sound_open=false, osd_open=false, osd_known=false}
        text, handlers, timers = {}, {}, {}
        local next_id=0
        function on(name, callback) handlers[name]=callback end
        function after(ms, callback)
            assert(ms==1600); next_id=next_id+1; timers[next_id]=callback; return next_id
        end
        function cancel(id) assert(timers[id]); timers[id]=nil end
        function count() local n=0; for _ in pairs(timers) do n=n+1 end; return n end
        sys={call=function() error('OSD cannot command audio') end,
             watch=function() error('OSD cannot add an observer') end}
        function every() error('OSD cannot poll') end
    "#,
    )
    .exec()
    .unwrap();
    let controller = lua
        .load(include_str!("../../shell/osd/controller.luau"))
        .eval::<mlua::Function>()
        .unwrap();
    lua.globals()
        .set("osd", controller.call::<mlua::Table>(()).unwrap())
        .unwrap();
    lua.load(
        r#"
        osd.observe(); assert(not fact.osd_open and not fact.osd_known and count()==0)
        fact['audio.volume']=0.2; fact['sound_outputs.count']=1
        osd.observe(); assert(fact.osd_known and not fact.osd_open and count()==0)
        fact['audio.volume']=0.4; osd.observe()
        assert(fact.osd_open and count()==1 and fact.osd_level==0.4 and text.osd_label=='40')
        fact['audio.volume']=0.8; osd.observe()
        assert(count()==1 and timers[1]==nil and timers[2])
        osd.observe(); assert(count()==1 and timers[2], 'unchanged report cannot extend hold')
        handlers.osd_enter(); assert(count()==0 and fact.osd_open)
        fact['audio.muted']=true; osd.observe()
        assert(count()==0 and text.osd_label=='Mute' and fact.osd_level==0)
        handlers.osd_leave(); assert(count()==1)
        timers[3](); timers[3]=nil
        assert(not fact.osd_open and count()==0)
        fact['audio.muted']=false; osd.observe(); assert(fact.osd_open and count()==1)
        fact.sound_open=true; handlers['fact:sound_open']()
        assert(not fact.osd_open and count()==0)
        fact['audio.volume']=0.5; osd.observe(); assert(not fact.osd_open)
        fact.sound_open=false; osd.observe(); assert(not fact.osd_open)
        fact['audio.volume']=1.5; osd.observe()
        assert(fact.osd_open and fact.osd_level==1 and text.osd_label=='150')
        fact['sound_outputs.count']=0; osd.observe()
        assert(not fact.osd_known and not fact.osd_open and count()==0)
        fact['sound_outputs.count']=1; osd.observe(); assert(not fact.osd_open)
        fact['audio.volume']=0/0; osd.observe(); assert(not fact.osd_known)
        fact['audio.volume']=0.6; osd.observe(); assert(not fact.osd_open)
        fact['audio.volume']=0.7; osd.observe(); assert(fact.osd_open)
        -- Native VM teardown cancels old callbacks but scene facts survive.
        timers={}; fact.osd_hover=true
    "#,
    )
    .exec()
    .unwrap();
    lua.globals()
        .set("osd", controller.call::<mlua::Table>(()).unwrap())
        .unwrap();
    lua.load(
        r#"
        assert(fact.osd_open and not fact.osd_hover and count()==1)
        osd.observe(); assert(count()==1)
        for id, callback in pairs(timers) do callback(); timers[id]=nil end
        assert(not fact.osd_open and count()==0)
    "#,
    )
    .exec()
    .unwrap();
}
