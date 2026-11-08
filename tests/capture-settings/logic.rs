use mlua::Lua;

#[test]
fn actual_result_not_prose_selects_bounded_fixed_native_notification() {
    let lua = Lua::new();
    lua.load(r#"
        json={encode=function(value) encoded=value; return '{"fixture":true}' end}
        sys={call=function(name,target,payload)
            assert(name=='say' and target=='tohru-notifications' and payload:sub(1,22)=='submit capture_result ')
            delivered=(delivered or 0)+1
        end}
    "#).exec().unwrap();
    let notify = lua
        .load(include_str!("../../shell/settings/capture-result.luau"))
        .eval::<mlua::Function>()
        .unwrap();
    lua.globals().set("notify", notify).unwrap();
    lua.load(
        r#"
        for _,case in ipairs({
            {{saved=true,copied=false,durable=true},'', 'saved'},
            {{saved=false,copied=true,durable=false},'', 'copied'},
            {{saved=true,copied=true,durable=true},'', 'saved_copied'},
            {{saved=true,copied=true,durable=false},'', 'partial'},
            {{saved=false,copied=true},'disk failed', 'partial'},
            {{saved=false,copied=false},'failure', 'error'},
        }) do
            notify('English message does not select status',case[2],case[1])
            assert(encoded.status==case[3])
        end
        notify('copied saved success!', 'native failure', nil)
        assert(encoded.status=='error')
        notify(string.rep('界',1000)..string.char(0,1,127),'',{saved=true,durable=true})
        assert(#encoded.message<=2048 and utf8.len(encoded.message)<=512)
        assert(not encoded.message:find('%z') and not encoded.message:find(string.char(1),1,true))
        notify(string.char(255),'',nil)
        assert(encoded.message=='Screenshot result contained invalid UTF-8')
        json.encode=function() return string.rep('x',3073) end
        assert(not pcall(notify,'result','',nil), 'oversized wire payload cannot be sent')
        sys.call=function() error('native notification route unavailable') end
        json.encode=function()return '{}'end
        assert(not pcall(notify,'result','',nil), 'delivery failure must reach capture status')
    "#,
    )
    .exec()
    .unwrap();
}

#[test]
fn delivered_capture_transaction_cases_execute_the_exact_production_controller() {
    let lua = Lua::new();
    lua.globals()
        .set(
            "loadfile",
            lua.create_function(|lua, path: String| {
                if path != "shell/capture/controller.luau" {
                    return Err(mlua::Error::runtime("unexpected fixture source path"));
                }
                lua.load(include_str!("../../shell/capture/controller.luau"))
                    .into_function()
            })
            .unwrap(),
        )
        .unwrap();
    lua.load(include_str!("../capture/controller.lua"))
        .exec()
        .unwrap();
}
