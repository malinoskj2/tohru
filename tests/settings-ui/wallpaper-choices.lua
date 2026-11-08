-- Actual Settings controller at the injected atomic-wallpaper action boundary.
math.clamp=function(v,a,b)return math.max(a,math.min(b,v))end
fact,text,model,handlers={settings_selected=0,settings_sidebar=false},{settings_search=''}, {}, {}
function on(name,callback)handlers[name]=callback end
local config={['bar.mooncoil']={value=false},['appearance.corners']={value=true},['appearance.corner_size']={value=32},
    ['wallpaper.enabled']={value=true},['wallpaper.fit']={value='fill'},['capture.save']={value=true},['capture.copy']={value=true},
    ['appearance.palette']={value='mocha',managed=true},['wallpaper.change']={value='never'}}
local generic,applied_count,palette_calls,policy_calls=0,0,{},{}
tohru_settings={snapshot=function()return config end,set=function(key,value)
    generic=generic+1;assert(key~='appearance.palette'and key~='wallpaper.change','atomic choice bypass')
    config[key].value=value;return config
end}
local wallpaper={open=function()end,close=function()end}
local function accept(key,value)
    if refused then error('atomic settings refused')end
    if declined then return false end
    config[key].value=value
    config[key].runtime_override=config[key].managed or derived_temporary or false
    text.settings_ui_error=config[key].runtime_override and 'none of this derived change was saved' or ''
    return true
end
wallpaper.palette=function(value)palette_calls[#palette_calls+1]=value;return accept('appearance.palette',value)end
wallpaper.change_policy=function(value)policy_calls[#policy_calls+1]=value;return accept('wallpaper.change',value)end
local refresh=assert(loadfile(arg[1] or 'shell/settings/controller.luau'))()(function()applied_count=applied_count+1 end,wallpaper)
local function search(key)
    text.settings_search=key;handlers['text:settings_search']()
    assert(#model.settings_rows==1 and model.settings_rows[1].key==key)
    return model.settings_rows[1]
end
handlers.settings_category(2)
assert(#model.settings_rows==6,'Appearance must expose palette and wallpaper policy')
local row=search('appearance.palette')
assert(row.choice and row.editable and row.managed and row.detail:find('mocha')and row.detail:find('Nix managed'))
for _,value in ipairs({'macchiato','rose_pine','nord','gruvbox','tokyo_night','dracula','everforest','mocha'})do
    handlers['submit:settings_search']()
    assert(config['appearance.palette'].value==value and palette_calls[#palette_calls]==value)
    assert(model.settings_rows[1].temporary and model.settings_rows[1].detail:lower():find('temporary'))
    assert(text.settings_ui_error:find('none of this derived change was saved'))
end
assert(generic==0 and applied_count==0)
row=search('wallpaper.change');assert(row.detail:find('never')and row.detail:find('Saved in config.toml'))
for _,value in ipairs({'hourly','daily','login','never'})do
    handlers.settings_row(0)
    assert(config['wallpaper.change'].value==value and policy_calls[#policy_calls]==value)
    assert(model.settings_rows[1].detail:find(value)and not model.settings_rows[1].temporary)
end
-- Derived temporary values are labeled honestly even when not directly managed.
derived_temporary=true;handlers.settings_activate()
assert(config['wallpaper.change'].value=='hourly' and not model.settings_rows[1].managed and model.settings_rows[1].temporary)
assert(model.settings_rows[1].detail:find('Temporary'))
derived_temporary=false;refused=true;handlers['submit:settings_search']()
assert(config['wallpaper.change'].value=='hourly' and text.settings_ui_error:find('atomic settings refused'))
refused=false;declined=true;handlers.settings_row(0)
assert(config['wallpaper.change'].value=='hourly' and text.settings_ui_error:find('not accepted'))
declined=false;handlers.settings_row(0)
assert(config['wallpaper.change'].value=='daily' and text.settings_ui_error=='')
search('appearance.palette');wallpaper.palette=nil
local before=#policy_calls;handlers.settings_row(0)
assert(#policy_calls==before and config['appearance.palette'].value=='mocha' and text.settings_ui_error:find('unavailable'))
wallpaper.palette=function(value)palette_calls[#palette_calls+1]=value;return accept('appearance.palette',value)end
config['appearance.palette'].value='nord';config['appearance.palette'].managed=false;config['appearance.palette'].runtime_override=false
refresh(config);assert(model.settings_rows[1].detail:find('nord')and not model.settings_rows[1].managed)
handlers.settings_row(0);assert(config['appearance.palette'].value=='gruvbox' and text.settings_ui_error=='')
assert(generic==0 and applied_count==0,'atomic callbacks already publish; no second mutation/publication')
-- Existing ordinary setting and history paths retain their existing owner route.
handlers.settings_category(1);handlers.settings_back();assert(fact.settings_page==2)
search('bar.mooncoil');handlers.settings_row(0)
assert(generic==1 and applied_count==1 and config['bar.mooncoil'].value)
print('PASS searchable palette/policy cycles, atomic routes, provenance, failures and existing owner path')
