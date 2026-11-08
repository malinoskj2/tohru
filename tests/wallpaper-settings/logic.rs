use mlua::Lua;

#[test]
fn picker_commits_once_through_owner_and_preserves_failure_and_provenance() {
    let lua = Lua::new();
    lua.load(
        r#"
        fact,text,model,handlers,modules={},{wallpaper_request_output=''}, {}, {}, {}
        function on(name, fn) handlers[name]=fn end
        function require(name) return modules[name] end
        local settings={['wallpaper.fit']={value='fill'}, ['wallpaper.enabled']={value=false},
            ['wallpaper.folder']={value='/chosen',managed=true},
            ['wallpaper.assignments']={value={OFFLINE='/chosen/old.png'},managed=true}}
        accepted=0
        tohru_settings={snapshot=function()return settings end,
            set=function(key,value)
                if refused then error('fixture write failure') end
                settings[key].value=value;settings[key].runtime_override=true;accepted=accepted+1
                return settings,'fixture durability warning'
            end}
        sys={ask=function(name,folder)
            if name=='capture.outputs' then return {{name='A',routable=true}} end
            assert(name=='wallpaper.catalog' and folder=='/chosen')
            return {'/chosen/new.png','/chosen/second.webp'}
        end}
    "#,
    )
    .exec()
    .unwrap();
    let modules: mlua::Table = lua.globals().get("modules").unwrap();
    for (name, source) in [
        (
            "wallpaper/controller",
            include_str!("../../shell/wallpaper/controller.luau"),
        ),
        (
            "wallpaper/picker",
            include_str!("../../shell/wallpaper/picker.luau"),
        ),
    ] {
        modules
            .set(name, lua.load(source).eval::<mlua::Function>().unwrap())
            .unwrap();
    }
    let factory = lua
        .load(include_str!("../../shell/settings/wallpaper.luau"))
        .eval::<mlua::Function>()
        .unwrap();
    lua.globals().set("factory", factory).unwrap();
    lua.load(
        r#"
        local wallpaper
        wallpaper=factory(function(settings) wallpaper.publish(settings) end)
        local snapshot=tohru_settings.snapshot()
        wallpaper.publish(snapshot)
        assert(not fact.wallpaper_enabled and text.wallpaper_output_0=='OFFLINE')
        assert(string.find(text.wallpaper_settings_notice,'Nix%-managed'))
        assert(wallpaper.open())
        assert(fact.wallpaper_picker_open and #model.wallpaper_picker_files==2)
        handlers.wallpaper_picker_choose(0)
        assert(accepted==1 and snapshot['wallpaper.assignments'].value.A=='/chosen/new.png')
        assert(snapshot['wallpaper.assignments'].value.OFFLINE=='/chosen/old.png')
        assert(text.settings_error=='fixture durability warning')
        refused=true; handlers.wallpaper_picker_choose(1)
        assert(accepted==1 and snapshot['wallpaper.assignments'].value.A=='/chosen/new.png')
        assert(string.find(text.wallpaper_picker_error,'fixture write failure'))
        refused=false;handlers.wallpaper_picker_clear()
        assert(accepted==2 and snapshot['wallpaper.assignments'].value.A==nil)
        assert(snapshot['wallpaper.assignments'].value.OFFLINE=='/chosen/old.png')
        wallpaper.close(); assert(not fact.wallpaper_picker_open)
    "#,
    )
    .exec()
    .unwrap();
}
