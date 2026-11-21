//! The greeter controller: style selection, optional user avatars, multi-output routing,
//! multi-step PAM handling, custom user entry, and two-click power actions.
use mlua::Lua;

const STUBS: &str = r#"
    fact = {
        typed=0, style=-1, wrong=false, unlocking=false, checking=false, awake=false,
        is_greeter=false, has_avatar=false, has_shot=false, pk=0, allow_power=true,
        user_count=0, session_count=0, prompt_visible=false, prompt_info=false,
        custom_user_open=false, restart_armed=false, poweroff_armed=false,
        has_kb_layout=false, caps_lock=false
    }
    text, handlers, timers, logs, ran = {}, {}, {}, {}, {}
    focused = nil
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
        ask = function(what, a)
            if what == 'env' then return env[a] end
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

fn lua_with_greeter(init_lua: &str) -> (Lua, mlua::Function) {
    let lua = Lua::new();
    lua.load(STUBS).exec().unwrap();
    lua.load(format!(
        r#"
        calls = {{ users = {{}}, sessions = {{}}, visible = {{}}, cancelled = 0, finished = 0 }}
        tohru_greeter = {{
            init = function() return ({init_lua}) end,
            select_user = function(u) calls.users[#calls.users + 1] = u end,
            select_session = function(s) calls.sessions[#calls.sessions + 1] = s end,
            submit_visible = function(v) calls.visible[#calls.visible + 1] = v == nil and '__NIL__' or v end,
            cancel = function() calls.cancelled = calls.cancelled + 1 end,
            finish = function() calls.finished = calls.finished + 1 end,
        }}
        "#
    ))
    .exec()
    .unwrap();
    let controller = lua
        .load(include_str!("../../shell/greeter/controller.luau"))
        .eval::<mlua::Function>()
        .unwrap();
    (lua, controller)
}

#[test]
fn every_style_and_plain_is_selected_with_safe_fallback() {
    for (i, style) in tohru_shell::settings::LockStyle::ALL.iter().enumerate() {
        let init = format!(
            "{{ config = {{ style = {:?}, allow_power_actions = true, warnings = {{}} }}, users = {{}}, sessions = {{}}, selected_user = '', selected_session = '', host = 'home' }}",
            style.name()
        );
        let (lua, controller) = lua_with_greeter(&init);
        controller.call::<()>(true).unwrap();
        let chosen: i64 = lua.load("return fact.style").eval().unwrap();
        assert_eq!(chosen, i as i64 + 1, "{}", style.name());
    }

    let (lua, controller) = lua_with_greeter(
        "{ config = { style = 'plain', warnings = {} }, users = {}, sessions = {}, selected_user = '', selected_session = '', host = 'home' }",
    );
    controller.call::<()>(true).unwrap();
    assert_eq!(lua.load("return fact.style").eval::<i64>().unwrap(), 0);

    let (lua, controller) = lua_with_greeter(
        "{ config = { style = 'bogus', warnings = {} }, users = {}, sessions = {}, selected_user = '', selected_session = '', host = 'home' }",
    );
    controller.call::<()>(true).unwrap();
    assert_eq!(lua.load("return fact.style").eval::<i64>().unwrap(), 3);
    assert!(
        lua.load("return logged(\"unknown greeter.style 'bogus'\")")
            .eval::<bool>()
            .unwrap()
    );

    // Plain fallback scene forces style 0 even when config asks for a shader style.
    let (lua, controller) = lua_with_greeter(
        "{ config = { style = 'horizon-meniscus', wallpaper = '/nix/store/bg.png', warnings = {} }, users = {}, sessions = {}, selected_user = '', selected_session = '', host = 'home' }",
    );
    controller.call::<()>(false).unwrap();
    assert_eq!(lua.load("return fact.style").eval::<i64>().unwrap(), 0);
    assert!(!lua.load("return fact.has_shot").eval::<bool>().unwrap());
}

#[test]
fn optional_avatar_switches_cleanly_between_users_and_custom_user() {
    let init = r#"{
        config = { style = 'flooded-monolith', allow_power_actions = true, warnings = {} },
        users = {
            { name = 'jesse', display_name = 'Jesse', avatar = '/var/lib/AccountsService/icons/jesse' },
            { name = 'guest', display_name = 'Guest', avatar = nil },
        },
        sessions = {
            { id = 'hyprland-uwsm', name = 'Hyprland (uwsm-managed)', kind = 'wayland' },
            { id = 'hyprland', name = 'Hyprland', kind = 'wayland' },
        },
        selected_user = 'jesse',
        selected_session = 'hyprland-uwsm',
        host = 'home',
    }"#;
    let (lua, controller) = lua_with_greeter(init);
    controller.call::<()>(true).unwrap();
    lua.load(
        r#"
        assert(fact.is_greeter)
        assert(not fact.has_shot and fact.pk == 0)
        assert(text.user == 'jesse' and text.user_display == 'Jesse')
        assert(fact.has_avatar and text.avatar_path == '/var/lib/AccountsService/icons/jesse')
        assert(text.session_name == 'Hyprland (uwsm-managed)')
        assert(focused == 'password')

        -- Switch to guest (no avatar -> avatar hidden cleanly)
        handlers.greeter_next_user()
        assert(text.user == 'guest' and not fact.has_avatar and text.avatar_path == '')

        -- Switch session
        handlers.greeter_next_session()
        assert(text.session_name == 'Hyprland')
        assert(calls.sessions[#calls.sessions] == 'hyprland')

        -- Custom username modal via Ctrl+u
        handlers.key('Ctrl+u')
        assert(fact.custom_user_open and focused == 'custom_user_input')
        handlers['submit:custom_user_input']('admin')
        assert(not fact.custom_user_open and text.user == 'admin' and not fact.has_avatar)
        assert(calls.users[#calls.users] == 'admin' and focused == 'password')
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn multi_output_routing_prefers_configured_output_and_falls_back_on_unplug() {
    let init = r#"{
        config = { style = 'flooded-monolith', preferred_output = 'DP-2', warnings = {} },
        users = {}, sessions = {}, selected_user = '', selected_session = '', host = 'home',
    }"#;
    let (lua, controller) = lua_with_greeter(init);
    controller.call::<()>(true).unwrap();
    lua.load(
        r#"
        text['screen.0.name'] = 'DP-1'
        text['screen.1.name'] = 'DP-2'
        handlers['text:screen.0.name']('DP-1')
        handlers['text:screen.1.name']('DP-2')
        assert(text.greeter_primary_outputs == 'DP-2')
        assert(text.greeter_secondary_outputs == 'DP-1')

        -- Unplugging DP-2 promotes DP-1 so login stays reachable.
        text['screen.1.name'] = ''
        handlers['text:screen.1.name']('')
        assert(text.greeter_primary_outputs == 'DP-1')
        assert(text.greeter_secondary_outputs == '')
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn multistep_pam_auth_verdicts_and_power_actions() {
    let init = r#"{
        config = { style = 'flooded-monolith', allow_power_actions = true, warnings = {} },
        users = {{ name = 'jesse', display_name = 'Jesse' }},
        sessions = {{ id = 'hyprland', name = 'Hyprland', kind = 'wayland' }},
        selected_user = 'jesse', selected_session = 'hyprland', host = 'home',
    }"#;
    let (lua, controller) = lua_with_greeter(init);
    controller.call::<()>(true).unwrap();
    lua.load(
        r#"
        handlers['text:password']('•••••')
        assert(fact.typed == 5 and fact.awake)
        handlers['fact:greeter_auth'](1); assert(fact.checking)

        -- Rejection surfaces greetd error
        text.greeter_error = 'Wrong password'
        handlers['fact:greeter_auth'](3)
        assert(fact.wrong and text.status == 'Wrong password' and focused == 'password')

        -- Visible PAM prompt (e.g. OTP)
        text.greeter_prompt = 'Verification code:'
        handlers['fact:greeter_auth'](5)
        assert(fact.prompt_visible and focused == 'visible_answer')
        handlers['submit:visible_answer']('123456')
        assert(calls.visible[#calls.visible] == '123456')

        -- Info PAM prompt (e.g. U2F touch)
        handlers['fact:greeter_auth'](6)
        assert(fact.prompt_info)
        handlers.key('Return')
        assert(calls.visible[#calls.visible] == '__NIL__')

        -- Escape cancels active prompt
        handlers['fact:greeter_auth'](6)
        handlers.key('Escape')
        assert(not fact.prompt_info and calls.cancelled == 1)

        -- Power actions: Sleep immediate, Restart & Poweroff require two presses
        handlers.lock_sleep()
        assert(ran[#ran] == 'systemctl suspend')
        local before_ran = #ran
        handlers.lock_restart()
        assert(fact.restart_armed and #ran == before_ran)
        handlers.lock_restart()
        assert(ran[#ran] == 'systemctl reboot')
        handlers.greeter_poweroff()
        assert(fact.poweroff_armed)
        handlers.greeter_poweroff()
        assert(ran[#ran] == 'systemctl poweroff')

        -- Accepted transitions and calls native finish()
        handlers['fact:greeter_auth'](2)
        assert(fact.unlocking and not fact.wrong)
        fire_all()
        assert(calls.finished == 1)
    "#,
    )
    .exec()
    .unwrap();
}
