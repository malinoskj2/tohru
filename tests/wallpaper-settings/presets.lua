-- Actual S root bridges/actions + W picker/controller/timer. No PNG, native
-- backend, filesystem settings write, display or real timer is executed.
math.clamp=math.clamp or function(v,a,b)return math.max(a,math.min(b,v))end
local function copy(value)
    if type(value)~='table'then return value end
    local result={};for key,item in pairs(value)do result[key]=copy(item)end;return result
end
local function fixture(options)
    options=options or {}
    fact,text,model,handlers={}, {wallpaper_request_output='',wallpaper_randomize_outputs='',wallpaper_picker_search='',wallpaper_picker_error=''}, {}, {}
    local f={timers={},next_id=0,commits={},queries={},refused=false,
        outputs={{name='A',routable=true},{name='B',routable=true}},
        catalog={'/folder/one.png','/folder/two.webp'},native_durable=true}
    local config={['wallpaper.enabled']={value=true},['wallpaper.fit']={value='fill'},
        ['wallpaper.folder']={value=''},['wallpaper.assignments']={value={OFFLINE='/offline/missing.png'}},
        ['wallpaper.preset']={},['appearance.palette']={value='mocha'},['wallpaper.change']={value=options.change or 'never'}}
    for key,value in pairs(options.values or {})do config[key].value=copy(value)end
    for _,key in ipairs(options.managed or {})do config[key].managed=true end
    f.config=config;f.disk=copy(config)
    on=function(name,fn)handlers[name]=fn end
    after=function(ms,fn)f.next_id=f.next_id+1;f.timers[f.next_id]={ms=ms,fn=fn};return f.next_id end
    cancel=function(id)assert(f.timers[id]);f.timers[id]=nil end
    every=function()error('No polling')end
    json={encode=function(value)f.last_json=value;return 'fixture'end}
    local keys={assignments='wallpaper.assignments',preset='wallpaper.preset',palette='appearance.palette',folder='wallpaper.folder'}
    tohru_settings={snapshot=function()return copy(config)end,
        wallpaper_update=function(proposal)
            if f.refused then error('settings rename refused')end
            local temporary=false
            for field in pairs(proposal)do local key=assert(keys[field]);temporary=temporary or config[key].managed or config[key].runtime_override end
            local accepted=copy(proposal);f.commits[#f.commits+1]=accepted
            for field,value in pairs(accepted)do
                local key=keys[field];config[key].value=copy(value);config[key].runtime_override=temporary or false
                if not temporary then f.disk[key]=copy(config[key])end
            end
            return copy(config),temporary and 'Related change is temporary; none of it was saved' or nil
        end,
        set=function(key,value)
            assert(key=='wallpaper.change')
            if f.refused then error('settings rename refused')end
            config[key].value=value;return copy(config)
        end}
    sys={ask=function(name,...)
        local args=table.pack(...);f.queries[#f.queries+1]={name=name,args=args}
        if name=='capture.outputs'then return copy(f.outputs)end
        if name=='wallpaper.catalog'then return copy(f.catalog)end
        if name=='wallpaper.identities'then
            local identities={};for i,path in ipairs(args[1])do assert(not path:find('/offline/'));identities[i]=path end;return identities
        end
        assert(name=='wallpaper.preset'and args.n==2)
        if f.prepare_failed then error('native preparation refused')end
        if f.bad_result then return {path='/invalid.png'}end
        local durable=f.native_durable
        if f.durable_sequence then durable=table.remove(f.durable_sequence,1);assert(type(durable)=='boolean')end
        return {path='/mock-presets/'..args[1]..'-'..args[2]..'.png',durable=durable,cached=true}
    end}
    local sources={['wallpaper/controller']='shell/wallpaper/controller.luau',['wallpaper/picker']='shell/wallpaper/picker.luau',
        ['wallpaper/rotation']='shell/wallpaper/rotation.luau',['settings/wallpaper-actions']='shell/settings/wallpaper-actions.luau'}
    require=function(name)
        if options.false_action and name=='settings/wallpaper-actions'then return function()return {rotate=function()return false end}end end
        local path=assert(sources[name],name)
        return assert(loadfile(path))()
    end
    local make=assert(loadfile('shell/settings/wallpaper.luau'))()
    local app
    app=make(function(snapshot)app.publish(snapshot)end)
    app.publish(tohru_settings.snapshot());f.app=app
    function f.reinstall()
        f.timers={};handlers={}
        app=make(function(snapshot)app.publish(snapshot)end)
        app.publish(tohru_settings.snapshot());f.app=app
    end
    function f.count()local n=0;for _ in pairs(f.timers)do n=n+1 end;return n end
    function f.fire()
        assert(f.count()==1);local id,timer=next(f.timers);f.timers[id]=nil;timer.fn();return timer.ms
    end
    function f.preset_queries()local n=0;for _,q in ipairs(f.queries)do if q.name=='wallpaper.preset'then n=n+1 end end;return n end
    return f
end
local cases=0
local function check(name,fn)fn();cases=cases+1;print('PASS '..name)end
check('one timer tick targets native connected names as a single preset advance',function()
    local f=fixture();f.outputs[3]={name='unroutable',routable=false};assert(f.count()==0)
    handlers.wallpaper_rotate()
    local p=f.commits[1];assert(#f.commits==1 and p.preset=='dusk' and p.palette=='mocha')
    assert(p.assignments.A=='/mock-presets/dusk-mocha.png' and p.assignments.B==p.assignments.A)
    assert(p.assignments.OFFLINE=='/offline/missing.png' and not p.assignments.unroutable and f.preset_queries()==1)
    handlers.wallpaper_rotate();assert(#f.commits==2 and f.commits[2].preset=='tide' and f.preset_queries()==2)
end)
check('visible picker follows one root timer batch',function()
    local f=fixture();assert(f.app.open());handlers.wallpaper_rotate()
    assert(text.wallpaper_picker_current=='/mock-presets/dusk-mocha.png')
    assert(model.wallpaper_picker_presets[2].selected)
end)
check('zero outputs and assignment capacity fail before preparation or acceptance',function()
    local f=fixture();f.outputs={};handlers.wallpaper_rotate()
    assert(#f.commits==0 and f.preset_queries()==0 and text.wallpaper_rotation_error:find('No connected'))
    f.outputs={{name='A'}};local assignments={};for i=1,32 do assignments['OFFLINE-'..i]='/offline/'..i..'.png'end
    f.config['wallpaper.assignments'].value=assignments;handlers.wallpaper_rotate()
    assert(#f.commits==0 and f.preset_queries()==0 and text.wallpaper_rotation_error:find('capacity'))
end)
check('folder batch changes connected paths once and preserves offline entries',function()
    local f=fixture({values={['wallpaper.folder']='/folder',['wallpaper.assignments']={A='/folder/one.png',B='/folder/two.webp',OFFLINE='/offline/missing.png'}}})
    handlers.wallpaper_rotate();local p=f.commits[1]
    assert(#f.commits==1 and not p.preset and not p.palette and p.assignments.A~='/folder/one.png' and p.assignments.B~='/folder/two.webp')
    assert(p.assignments.OFFLINE=='/offline/missing.png')
    f.catalog={};handlers.wallpaper_rotate();assert(#f.commits==1 and text.wallpaper_rotation_error~='')
end)
check('per-output Solid selection and proposed palette commit together',function()
    local f=fixture();assert(f.app.open());handlers.wallpaper_picker_preset(5)
    assert(f.config['wallpaper.assignments'].value.A=='/mock-presets/solid-mocha.png')
    assert(f.commits[1].preset=='solid' and f.commits[1].palette=='mocha')
    f.native_durable=false;f.app.palette('nord');local p=f.commits[2]
    assert(text.wallpaper_picker_current=='/mock-presets/solid-nord.png','visible picker must follow root palette acceptance')
    assert(text.wallpaper_picker_warning:find('durability'))
    assert(p.palette=='nord' and p.preset=='solid' and p.assignments.A=='/mock-presets/solid-nord.png')
    assert(p.assignments.OFFLINE=='/offline/missing.png' and f.preset_queries()==3)
    assert(f.config['appearance.palette'].value=='nord')
end)
check('Solid cache success is not config acceptance; failure preserves accepted palette and paths',function()
    local f=fixture({values={['wallpaper.preset']='solid',['wallpaper.assignments']={A='/mock-presets/solid-mocha.png'}}})
    f.refused=true;assert(not pcall(f.app.palette,'nord'))
    assert(#f.commits==0 and f.preset_queries()==2 and f.config['appearance.palette'].value=='mocha')
    assert(f.config['wallpaper.assignments'].value.A=='/mock-presets/solid-mocha.png')
    f.refused=false;f.native_durable=false;f.app.palette('nord')
    assert(text.wallpaper_picker_warning:find('durability'))
    f.native_durable=true;f.app.palette('gruvbox');assert(text.wallpaper_picker_warning=='')
end)
check('Solid preparation remains unconfirmed when prior cache base was newly created',function()
    local f=fixture({values={['wallpaper.preset']='solid',['wallpaper.assignments']={A='/mock-presets/solid-mocha.png'}}})
    assert(f.app.open());f.durable_sequence={false,true};f.app.palette('nord')
    assert(#f.commits==1 and f.config['appearance.palette'].value=='nord')
    assert(text.wallpaper_picker_warning:find('durability')and #f.durable_sequence==0)
end)
check('mixed managed palette makes the whole derived operation temporary',function()
    local f=fixture({managed={'appearance.palette'},values={['wallpaper.preset']='solid',['wallpaper.assignments']={A='/mock-presets/solid-mocha.png'}}})
    f.app.palette('nord')
    assert(f.disk['appearance.palette'].value=='mocha' and f.disk['wallpaper.assignments'].value.A=='/mock-presets/solid-mocha.png')
    for _,key in ipairs({'appearance.palette','wallpaper.assignments','wallpaper.preset'})do assert(f.config[key].runtime_override)end
    assert(not f.config['wallpaper.assignments'].managed and text.settings_ui_error:find('none of it was saved'))
end)
check('managed assignment and folder lifetimes use only operation participants',function()
    local f=fixture({managed={'wallpaper.assignments'}});assert(f.app.open());handlers.wallpaper_picker_preset(5)
    assert(f.config['wallpaper.preset'].runtime_override and f.config['appearance.palette'].runtime_override)
    assert(f.disk['wallpaper.preset'].value==nil and not f.disk['wallpaper.assignments'].value.A)
    f=fixture({managed={'wallpaper.folder'}});assert(f.app.open());text.wallpaper_picker_folder='/folder';handlers.wallpaper_picker_folder_submit()
    assert(f.config['wallpaper.assignments'].runtime_override and f.disk['wallpaper.folder'].value=='' and not f.disk['wallpaper.assignments'].value.A)
    f=fixture({managed={'appearance.palette'}});assert(f.app.open());text.wallpaper_picker_folder='/folder';handlers.wallpaper_picker_folder_submit()
    assert(not f.config['wallpaper.assignments'].runtime_override and f.disk['wallpaper.folder'].value=='/folder')
end)
check('inactive Solid metadata does not replace unrelated images or include unrelated keys',function()
    local f=fixture({values={['wallpaper.preset']='solid',['wallpaper.assignments']={A='/chosen/photo.png'}}})
    f.app.palette('nord');assert(f.preset_queries()==1 and #f.commits==1)
    local p=f.commits[1];assert(p.palette=='nord' and not p.assignments and not p.preset)
    assert(f.config['wallpaper.assignments'].value.A=='/chosen/photo.png')
end)
check('folder and optional first assignment are one operation-specific proposal',function()
    local f=fixture();assert(f.app.open());text.wallpaper_picker_folder='/folder';handlers.wallpaper_picker_folder_submit()
    local p=f.commits[1];assert(#f.commits==1 and p.folder=='/folder' and p.assignments.A and not p.preset and not p.palette)
    assert(f.disk['wallpaper.folder'].value=='/folder' and f.disk['wallpaper.assignments'].value.A==p.assignments.A)
    f.catalog={};text.wallpaper_picker_folder='/empty';handlers.wallpaper_picker_folder_submit()
    assert(#f.commits==1 and f.config['wallpaper.folder'].value=='/folder')
    f.catalog={'/other/a.png'};f.refused=true;text.wallpaper_picker_folder='/other';handlers.wallpaper_picker_folder_submit()
    assert(#f.commits==1 and f.config['wallpaper.folder'].value=='/folder' and text.wallpaper_picker_error:find('refused'))
end)
check('timer uses one finite deadline and propagates false/error results',function()
    local f=fixture({change='hourly'});assert(f.count()==1 and f.fire()==3600000 and #f.commits==1 and f.count()==1)
    f.outputs={};f.fire();assert(text.wallpaper_rotation_error:find('No connected')and #f.commits==1 and f.count()==1)
    f.app.stop();assert(f.count()==0)
    local bad=fixture({change='login',false_action=true});assert(bad.fire()==1000 and text.wallpaper_rotation_error:find('rotation failed'))
end)
check('login is initial only; duplicate publication does not rearm; accepted edits cancel',function()
    local f=fixture({change='login'});local next_id=f.next_id
    f.app.publish(tohru_settings.snapshot());assert(f.count()==1 and next_id==f.next_id)
    assert(f.app.open());text.wallpaper_picker_folder='/folder';handlers.wallpaper_picker_folder_submit()
    assert(f.count()==0)
end)
check('VM replacement does not repeat login but owns one interval timer',function()
    local f=fixture({change='login'});assert(f.count()==1);f.reinstall();assert(f.count()==0)
    f=fixture({change='hourly'});assert(f.count()==1);f.reinstall();assert(f.count()==1);f.app.stop();assert(f.count()==0)
end)
check('actual Settings choices preserve atomic Solid assignments and policy callbacks',function()
    local f=fixture({values={['wallpaper.preset']='solid',['wallpaper.assignments']={A='/mock-presets/solid-mocha.png',OFFLINE='/offline/missing.png'}}})
    for key,value in pairs({['bar.mooncoil']=false,['appearance.corners']=true,['appearance.corner_size']=32,['capture.save']=true,['capture.copy']=true})do
        f.config[key]={value=value}
    end
    fact.settings_selected=0;fact.settings_sidebar=false;text.settings_search='appearance.palette'
    local make=assert(loadfile('shell/settings/controller.luau'))()
    make(function()error('atomic wallpaper hook must not trigger generic applied')end,f.app)
    assert(#model.settings_rows==1 and model.settings_rows[1].key=='appearance.palette')
    handlers['submit:settings_search']()
    local p=f.commits[1]
    assert(#f.commits==1 and p.palette=='macchiato' and p.preset=='solid')
    assert(p.assignments.A=='/mock-presets/solid-macchiato.png' and p.assignments.OFFLINE=='/offline/missing.png')
    f.refused=true;handlers.settings_row(0)
    assert(#f.commits==1 and f.config['appearance.palette'].value=='macchiato' and text.settings_ui_error:find('refused'))
    f.refused=false;text.settings_search='wallpaper.change';handlers['text:settings_search']()
    handlers['submit:settings_search']()
    assert(f.config['wallpaper.change'].value=='hourly' and f.count()==1 and #f.commits==1)
    local timer=f.next_id;f.refused=true;handlers.settings_row(0)
    assert(f.config['wallpaper.change'].value=='hourly' and f.next_id==timer and f.count()==1)
end)
check('invalid native generation and preparation errors never accept partial config',function()
    local f=fixture();f.bad_result=true;handlers.wallpaper_rotate()
    assert(#f.commits==0 and text.wallpaper_rotation_error:find('Invalid native'))
    f.bad_result=false;f.prepare_failed=true;handlers.wallpaper_rotate()
    assert(#f.commits==0 and text.wallpaper_rotation_error:find('refused'))
end)
print('Root preset integration: '..cases..' source-only cases passed')
