//! The lock controller: which style it picks and what it falls back to, and that nothing it does
//! lets the lock go without the native authority's verdict.
use mlua::Lua;

const STUBS: &str = r#"
    fact = {typed=0, style=-1, engaged=false, released=false, wrong=false, unlocking=false, checking=false}
    text, handlers, timers, logs, asked, ran = {}, {}, {}, {}, {}, {}
    local next_id = 0
    function on(name, callback) handlers[name] = callback end
    function after(ms, callback) next_id = next_id + 1; timers[next_id] = {ms=ms, f=callback}; return next_id end
    function cancel(id) timers[id] = nil end
    function focus(name) focused = name end
    function log(line) logs[#logs + 1] = line end
    function run(cmd, args, done) ran[#ran + 1] = cmd .. ' ' .. table.concat(args, ' ') end
    env = {}
    watchers = {}
    sys = {
        ask = function(what, a, b, c)
            asked[#asked + 1] = what
            if what == 'env' then return env[a] end
            if what == 'capture.freeze' then return capture_name or error('no capture') end
            if what == 'capture.pixel' then return '#202020' end
            error('unexpected ask ' .. what)
        end,
        watch = function(what, callback) watchers[what] = callback end,
    }
    function logged(fragment)
        for _, line in ipairs(logs) do if string.find(line, fragment, 1, true) then return true end end
        return false
    end
    function fire_all()
        local due = timers; timers = {}
        for _, t in pairs(due) do t.f() end
    end
"#;

fn lua_with(settings: &str) -> (Lua, mlua::Function) {
    let lua = Lua::new();
    lua.load(STUBS).exec().unwrap();
    lua.load(settings).exec().unwrap();
    let controller = lua
        .load(include_str!("../../shell/lock/controller.luau"))
        .eval::<mlua::Function>()
        .unwrap();
    (lua, controller)
}

fn style_for(settings: &str, env_style: Option<&str>) -> (i64, Lua) {
    let (lua, controller) = lua_with(settings);
    if let Some(s) = env_style {
        lua.load(format!("env.TOHRU_LOCK_STYLE = {s:?}")).exec().unwrap();
    }
    controller.call::<()>(true).unwrap();
    let style = lua.load("return fact.style").eval::<i64>().unwrap();
    (style, lua)
}

fn configured(value: &str) -> String {
    format!(
        "tohru_settings = {{ snapshot = function() return {{['lock.style'] = {{value = {value:?}}}}} end, warnings = function() return {{}} end }}"
    )
}

#[test]
fn every_configured_style_is_chosen_by_its_name() {
    for (i, style) in tohru_shell::settings::LockStyle::ALL.iter().enumerate() {
        let (index, _) = style_for(&configured(style.name()), None);
        assert_eq!(index, i as i64 + 1, "{}", style.name());
    }
}

#[test]
fn unknown_missing_or_broken_settings_fall_back_to_flooded_monolith() {
    let (index, lua) = style_for(&configured("aurora"), None);
    assert_eq!(index, 3);
    assert!(lua.load("return logged(\"unknown lock.style 'aurora'\")").eval::<bool>().unwrap());

    let (index, _) = style_for("tohru_settings = nil", None);
    assert_eq!(index, 3);

    let (index, lua) = style_for("tohru_settings = { snapshot = function() error('config unreadable') end }", None);
    assert_eq!(index, 3);
    assert!(lua.load("return logged('settings are unavailable')").eval::<bool>().unwrap());

    let (index, lua) = style_for(
        "tohru_settings = { snapshot = function() return {} end, warnings = function() return {'config.toml: unknown lock.style \"x\"'} end }",
        None,
    );
    assert_eq!(index, 3);
    assert!(lua.load("return logged('warning: config.toml')").eval::<bool>().unwrap());
}

#[test]
fn the_environment_overrides_the_setting_and_an_unknown_one_falls_back() {
    let (index, _) = style_for(&configured("still-glass"), Some("corner-anchor"));
    assert_eq!(index, 9);
    let (index, lua) = style_for(&configured("still-glass"), Some("nonsense"));
    assert_eq!(index, 3);
    assert!(lua.load("return logged(\"TOHRU_LOCK_STYLE 'nonsense'\")").eval::<bool>().unwrap());
}

#[test]
fn the_plain_fallback_takes_style_zero_and_never_captures() {
    let (lua, controller) = lua_with(&configured("ribbon-key"));
    lua.load("capture_name = 'shot'").exec().unwrap();
    controller.call::<()>(false).unwrap();
    lua.load(
        r#"
        assert(fact.style == 0)
        assert(watchers.workspaces == nil and watchers.media == nil)
        for _, what in ipairs(asked) do assert(string.sub(what, 1, 8) ~= 'capture.', what) end
        -- With nothing to wait for it engages at once.
        assert(fact.engaged)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn a_failed_capture_still_engages_the_lock() {
    let (index, lua) = style_for(&configured("flooded-monolith"), None);
    assert_eq!(index, 3);
    lua.load(
        r#"
        assert(not fact.engaged)
        watchers.workspaces({active = 1, list = {{id = 1, monitor = 'DP-2'}}})
        assert(fact.engaged and not fact.has_shot)
        assert(logged('no capture of DP-2'))
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn only_an_accepted_verdict_lets_the_lock_go() {
    let (_, lua) = style_for(&configured("flooded-monolith"), None);
    lua.load(
        r#"
        fire_all()
        assert(fact.engaged)
        -- What the field holds arrives as bullets; its length is all the controller keeps.
        handlers['text:password']('•••••')
        assert(fact.typed == 5)
        handlers['text:password']('')
        handlers['submit:password']('')
        assert(fact.typed == 5)
        handlers['fact:lock_auth'](1); assert(fact.checking)
        handlers['fact:lock_auth'](3)
        assert(fact.wrong and text.status == 'Incorrect password' and fact.engaged and not fact.released)
        fire_all()
        assert(fact.engaged and not fact.released)
        handlers['fact:lock_auth'](4)
        assert(text.status == 'Authentication unavailable' and fact.engaged and not fact.released)
        handlers.key('Escape')
        assert(fact.typed == 0 and not fact.wrong)
        handlers['fact:lock_auth'](2)
        assert(fact.unlocking and fact.engaged and not fact.released)
        fire_all()
        assert(fact.released and not fact.engaged)
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn restart_needs_a_second_press_and_sleep_suspends() {
    let (_, lua) = style_for(&configured("unfolding-glass-capsule"), None);
    lua.load(
        r#"
        local before = #ran
        handlers.lock_restart()
        assert(fact.restart_armed and #ran == before)
        fire_all()
        assert(not fact.restart_armed and #ran == before)
        handlers.lock_restart(); handlers.lock_restart()
        assert(ran[#ran] == 'systemctl reboot')
        handlers.lock_sleep()
        assert(ran[#ran] == 'systemctl suspend')
    "#,
    )
    .exec()
    .unwrap();
}
