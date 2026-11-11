math.clamp=function(v,a,b)return math.max(a,math.min(b,v))end
fact,text,model,handlers={settings_selected=0,settings_sidebar=false},{settings_search=''},{},{}
function on(name,callback)handlers[name]=callback end
local config={['bar.mooncoil']={value=false},['bar.preferred_output']={value=''},['appearance.corners']={value=true},['appearance.corner_size']={value=32},
    ['appearance.palette']={value='mocha',managed=true}}
local reloads=0
tohru_settings={snapshot=function()return config end,
    reload=function()reloads=reloads+1;return config end,
    set=function(key,value)
        config[key].value=value;config[key].runtime_override=config[key].managed or false
        return config
    end}
local said,refuse={},false
sys={call=function(service,scene,command)
    assert(service=='say' and scene=='tohru')
    if refuse then error('no bar')end
    said[#said+1]=command
end}
function require(name)return assert(loadfile('shell/settings/'..name..'.luau'))()end
assert(loadfile('shell/settings/settings.luau'))()
assert(fact.settings_open and text.settings_corner_size_edit=='32')
local function search(key)text.settings_search=key;handlers['text:settings_search']();return model.settings_rows[1]end
search('appearance.corners');handlers.settings_row(0)
assert(config['appearance.corners'].value==false and #said==1 and said[1]=='emit config_reload')
said={};search('appearance.palette');handlers.settings_row(0)
assert(config['appearance.palette'].value=='macchiato' and config['appearance.palette'].runtime_override)
assert(#said==2 and said[1]=='emit config_reload' and said[2]=='emit setting_palette 1')
said={};text.settings_corner_size_edit='64';handlers['submit:settings_corner_size_edit']()
assert(config['appearance.corner_size'].value==64 and said[1]=='emit config_reload' and said[2]=='emit setting_palette 1')
refuse=true;search('bar.mooncoil');handlers.settings_row(0)
assert(config['bar.mooncoil'].value and text.settings_ui_error:find('did not take'))
refuse=false;handlers.settings_open_request();assert(reloads==1 and fact.settings_open)
handlers.settings_close();assert(not fact.settings_open)
print('PASS Settings process writes through its owner, reloads the bar and forwards runtime overrides')
