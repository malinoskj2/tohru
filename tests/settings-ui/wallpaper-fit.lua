-- Actual S-owned controllers/root; no role, service or native binary is started.
math.clamp=function(value,low,high)return math.max(low,math.min(high,value))end
fact,text,model,handlers={settings_selected=0},{settings_search=''}, {}, {}
function on(name,fn)handlers[name]=fn end
local config={}
for _,key in ipairs({'bar.mooncoil','appearance.corners','wallpaper.enabled','capture.save','capture.copy'}) do
    config[key]={value=false,managed=false}
end
config['appearance.corner_size']={value=32}
config['wallpaper.fit']={value='fill',managed=true}
config['wallpaper.folder']={value=''}
config['wallpaper.preset']={}
config['appearance.palette']={value='mocha'}
config['wallpaper.change']={value='never'}
config['wallpaper.assignments']={value={OFFLINE='/image.png'}}
config['bar.preferred_output']={value=''}
local changed=0
local function set(key,value)
    if key=='appearance.corner_size' then
        assert(type(value)=='number' and value>=1 and value<=100)
    else
        assert(key=='wallpaper.fit' and type(value)=='string')
        assert(value=='fill' or value=='fit' or value=='centre' or value=='tile')
    end
    if refused then error('fixture write refusal')end
    config[key].value=value; config[key].runtime_override=true;changed=changed+1
    return config,'durability warning'
end
tohru_settings={snapshot=function()return config end,set=set}
local refreshed=0
local refresh=assert(loadfile('shell/settings/controller.luau'))()(function(settings,warning)
    assert(settings==config and warning=='durability warning');refreshed=refreshed+1
end)
handlers.settings_category(2)
local index
for i,row in ipairs(model.settings_rows)do if row.key=='wallpaper.fit' then index=i-1;assert(row.choice and row.editable and row.managed)end end
assert(index)
for _,mode in ipairs({'fit','centre','tile','fill'})do
    handlers.settings_row(index)
    assert(config['wallpaper.fit'].value==mode and text.settings_ui_error=='durability warning')
end
assert(changed==4 and refreshed==4)
text.settings_search='wallpaper.fit';handlers['text:settings_search']()
assert(#model.settings_rows==1 and model.settings_rows[1].choice)
handlers['submit:settings_search']();assert(config['wallpaper.fit'].value=='fit')
refused=true;handlers.settings_row(0)
assert(config['wallpaper.fit'].value=='fit' and text.settings_ui_error:find('fixture write refusal'))
refused=false

-- The actual bridge forwards the accepted value and marks fit-only managed edits.
local projected
local modules={
    ['wallpaper/controller']=function()return {publish=function(enabled,assignments,fit)
        assert(enabled==false and assignments==config['wallpaper.assignments'].value)
        projected=fit;return {}
    end}end,
    ['wallpaper/picker']=function()return {open=function()end}end,
    ['settings/wallpaper-actions']=assert(loadfile('shell/settings/wallpaper-actions.luau'))(),
    ['wallpaper/rotation']=assert(loadfile('shell/wallpaper/rotation.luau'))(),
}
function require(name)assert(modules[name],name);return modules[name]end
local bridge=assert(loadfile('shell/settings/wallpaper.luau'))()(function()end)
for _,mode in ipairs({'fill','fit','centre','tile'})do
    config['wallpaper.fit'].value=mode;bridge.publish(config);assert(projected==mode)
    assert(text.wallpaper_settings_notice:find('Nix%-managed'))
end

-- Actual root numeric event mapping. Other feature boundaries are inert fixtures;
-- Lua emit/fact writes do not redispatch to handlers.
handlers={}; local status
json={encode=function(value)status=value;return 'fixture'end}
local noop=function()end
local panel={open=noop,close=noop,toggle=noop,observe=noop,hide=noop,publish=noop}
setmetatable(modules,{__index=function(_,name)
    if name=='settings/controller' then return function()return noop end end
    return function()return panel end
end})
modules['settings/wallpaper']=function()return bridge end
modules['settings/corner-size']=assert(loadfile('shell/settings/corner-size.luau'))()
tohru_workspace={};tohru_info={snapshot=noop}
assert(loadfile('shell/bar.luau'))()
assert(status.schema==7)
local before=changed
for index,mode in ipairs({'fill','fit','centre','tile'})do
    handlers.setting_wallpaper_fit(index-1)
    assert(config['wallpaper.fit'].value==mode and projected==mode)
end
assert(changed==before+4)
for _,invalid in ipairs({-1,4,.5,'fit',math.huge})do
    handlers.setting_wallpaper_fit(invalid)
    assert(changed==before+4 and text.settings_error:find('Wallpaper fit must be'))
end
assert(text.settings_corner_size_edit=='32')
text.settings_corner_size_edit='64';handlers['submit:settings_corner_size_edit']()
assert(config['appearance.corner_size'].value==64 and text.settings_corner_size_edit=='64')
local last_policy,last_palette
bridge.change_policy=function(value)last_policy=value end
bridge.palette=function(value)last_palette=value end
for index,value in ipairs({'never','hourly','daily','login'})do
    handlers.setting_wallpaper_change(index-1);assert(last_policy==value)
end
for index,value in ipairs({'mocha','macchiato','rose_pine','nord','gruvbox','tokyo_night','dracula','everforest'})do
    handlers.setting_palette(index-1);assert(last_palette==value)
end
handlers.setting_wallpaper_change(4);assert(last_policy=='login' and text.settings_error:find('Invalid'))
handlers.setting_palette(8);assert(last_palette=='everforest' and text.settings_error:find('Invalid'))
print('PASS wallpaper fit/corner size and root canonical palette/change action routes')
