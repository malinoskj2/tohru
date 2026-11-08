use mlua::Lua;

#[test]
fn only_changed_accepted_idle_values_are_projected_and_failed_calls_retry() {
    let lua = Lua::new();
    let factory = lua
        .load(include_str!("../../shell/settings/idle.luau"))
        .eval::<mlua::Function>()
        .unwrap();
    lua.globals().set("factory", factory).unwrap();
    lua.load(
        r#"
        local keys={'lock_seconds','screen_off_unlocked_seconds','screen_off_locked_seconds',
                    'suspend_unlocked_seconds','suspend_locked_seconds','fade_milliseconds'}
        local snapshot={}
        for i,key in ipairs(keys) do snapshot['idle.'..key]={value=i} end
        snapshot['appearance.corners']={value=true}
        local calls, reject=0,true
        local project=factory({configure=function(config)
            calls=calls+1
            for i,key in ipairs(keys) do assert(config[key]==snapshot['idle.'..key].value) end
            return not reject
        end})
        assert(not project(snapshot) and calls==1)
        reject=false
        assert(project(snapshot) and calls==2, 'failed start must retry')
        assert(project(snapshot) and calls==2)
        snapshot['appearance.corners'].value=false
        assert(project(snapshot) and calls==2, 'unrelated edit cannot reset native idle')
        snapshot['idle.lock_seconds'].value=0
        assert(project(snapshot) and calls==3)
        assert(project(snapshot) and calls==3)
        -- A new VM must submit settings once so native retained owner can compare;
        -- this CPU projection test does not pretend to implement native dedup.
        local replacement=factory({configure=function(config) calls=calls+1; return true end})
        assert(replacement(snapshot) and calls==4)
    "#,
    )
    .exec()
    .unwrap();
}
