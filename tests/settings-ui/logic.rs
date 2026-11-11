use mlua::Lua;

#[test]
fn actual_settings_search_history_and_managed_edits_share_the_owner() {
    let lua = Lua::new();
    lua.load(r#"
        fact={settings_selected=0,settings_sidebar=true,settings_open=false}
        text={settings_search=''}; model={}; handlers={}; saves=0; applied_count=0
        function on(name,callback) handlers[name]=callback end
        config={['bar.mooncoil']={value=false,managed=true,runtime_override=false},
                ['bar.info_button']={value=false,managed=false,runtime_override=false},
                ['bar.about_button']={value=false,managed=false,runtime_override=false},
                ['bar.preferred_output']={value='',managed=false,runtime_override=false},
                ['appearance.corners']={value=true,managed=false,runtime_override=false},
                ['appearance.corner_size']={value=32,managed=false,runtime_override=false}, ['appearance.palette']={value='mocha',managed=false,runtime_override=false}}
        tohru_settings={snapshot=function() return config end,
          set=function(key,value)
            if denied then error('write refused') end
            local row=config[key]; row.value=value
            if row.managed then row.runtime_override=true else saves=saves+1 end
            return config
          end}
        function applied(settings,warning) applied_count=applied_count+1; applied_config=settings end
    "#).exec().unwrap();
    let callback: mlua::Function = lua.globals().get("applied").unwrap();
    lua.load(include_str!("../../shell/settings/controller.luau"))
        .eval::<mlua::Function>()
        .unwrap()
        .call::<mlua::Function>(callback)
        .unwrap();
    lua.load(
        r#"
        handlers.settings_open_request(); assert(fact.settings_open)
        assert(model.settings_rows[1].title=='Mooncoil mark')
        handlers.settings_row(0)
        assert(config['bar.mooncoil'].value and config['bar.mooncoil'].runtime_override)
        assert(saves==0 and applied_count==1 and applied_config==config)
        assert(string.find(model.settings_rows[1].detail,'temporary runtime override'))
        handlers.settings_category(2); handlers.settings_category(3)
        assert(fact.settings_page==3 and fact.settings_can_back)
        handlers.settings_back(); assert(fact.settings_page==2 and fact.settings_can_forward)
        handlers.settings_back(); assert(fact.settings_page==1)
        handlers.settings_forward(); assert(fact.settings_page==2)
        assert(model.settings_rows[1].key=='appearance.corners' and model.settings_rows[1].editable)
        handlers.settings_row(0); assert(not config['appearance.corners'].value and saves==1)
        assert(string.find(model.settings_rows[2].detail,'32 logical pixels'))
        assert(model.settings_rows[3].key=='appearance.palette')
        handlers.settings_row(2); assert(config['appearance.palette'].value=='macchiato' and saves==2)
        handlers.settings_category(1); assert(not fact.settings_can_forward)
        text.settings_search='mooncoil'; handlers['text:settings_search']()
        assert(#model.settings_rows==1 and model.settings_rows[1].editable)
        handlers['submit:settings_search']()
        assert(applied_count==4 and saves==2 and not config['bar.mooncoil'].value)
        text.settings_search='does-not-exist'; handlers['text:settings_search']()
        assert(#model.settings_rows==0)
        text.settings_search=''; handlers['text:settings_search']()
        denied=true; handlers.settings_row(0)
        assert(string.find(text.settings_ui_error,'write refused'))
        denied=false; config['bar.mooncoil'].managed=false
        handlers.settings_row(0); assert(saves==3)
        handlers.settings_close(); assert(not fact.settings_open)
    "#,
    )
    .exec()
    .unwrap();
}
