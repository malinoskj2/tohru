math.clamp=function(v,a,b)return math.max(a,math.min(b,v))end
fact,text,model,handlers={settings_selected=0,settings_sidebar=false},{settings_search=''},{},{}
local listeners, probes, values = {}, {}, {}
local publications = 0
model = setmetatable({}, {
    __index = values,
    __newindex = function(_, key, value)
        values[key] = value
        if key == "settings_rows" then publications = publications + 1 end
    end,
})
function on(name, callback)
    listeners[name] = listeners[name] or {}
    table.insert(listeners[name], callback)
    handlers[name] = function(...)
        for _, handle in ipairs(listeners[name]) do handle(...) end
    end
end
function run(program, args, done)
    assert(program == "tohru-voice" and args[1] == "--devices")
    table.insert(probes, {source = args[3], done = done})
end
json = {decode = function(source) return {sources={source}, selected=source, aec="unknown"} end}
local config={['bar.mooncoil']={value=false},['bar.info_button']={value=false},['bar.about_button']={value=false},['bar.preferred_output']={value=''},['appearance.corners']={value=true},['appearance.corner_size']={value=32},
    ['appearance.palette']={value='mocha',managed=true},['lock.style']={value='flooded-monolith'},
    ['notifications.position']={value='top-right',managed=true},
    ['assistant.reduced_motion']={value=false},['assistant.voice_source']={value=''}}
local reloads=0
local reload_error, next_config
tohru_settings={snapshot=function()return config end,
    reload=function()
        reloads=reloads+1
        if reload_error then error(reload_error) end
        config=next_config or config
        return config
    end,
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
search('appearance.corners');local before=publications;handlers.settings_row(0)
assert(publications==before+1)
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
said={};search('notifications.position');handlers.settings_row(0)
local forwarded=false;for _,command in ipairs(said)do forwarded=forwarded or command=='emit setting_notifications_position 1' end
assert(config['notifications.position'].value=='bottom-right' and said[1]=='emit config_reload' and forwarded)
print('PASS Settings process writes through its owner, reloads the bar and forwards runtime overrides')

handlers.settings_category(3)
search("Echo cancellation")
handlers.settings_close()
next_config={}
for key,setting in pairs(config) do next_config[key]=setting end
next_config["assistant.voice_source"]={value="new microphone"}
next_config["appearance.corner_size"]={value=81}
before=publications
local older=probes[#probes]
handlers.settings_open_request()
assert(fact.settings_open and fact.settings_page==3 and text.settings_search=="Echo cancellation")
assert(publications==before+1 and text.settings_corner_size_edit=="81")
local latest=probes[#probes]
assert(older.source=="" and latest.source=="new microphone" and text.settings_ui_error=="")
latest.done(latest.source,0)
before=publications
older.done("obsolete microphone",0)
assert(publications==before and model.settings_rows[1].detail=="unknown · new microphone")
handlers.settings_close()
reload_error="unreadable settings"
local probe_count=#probes
handlers.settings_open_request()
assert(fact.settings_open and text.settings_ui_error:find(reload_error,1,true))
assert(publications==before and #probes==probe_count and model.settings_rows[1].detail=="unknown · new microphone")

reload_error=nil
for _,group in ipairs({'models','voice_models','semantic_models'}) do
    for _,provider in ipairs({'codex','claude'}) do
        for _,field in ipairs({'model','effort'}) do
            config['assistant.'..group..'.'..provider..'.'..field]={value=''}
        end
    end
end
handlers.settings_category(4)
assert(#model.settings_rows==8)
handlers.settings_row(3) -- spoken model
assert(fact.settings_model_editing)
text.settings_model_edit='gpt-6.2-sol'
handlers['submit:settings_model_edit']()
assert(config['assistant.voice_models.codex.model'].value=='gpt-6.2-sol' and not fact.settings_model_editing)
handlers.settings_row(4) -- spoken effort
assert(config['assistant.voice_models.codex.effort'].value=='low')
handlers.settings_row(5) -- semantic model
text.settings_model_edit='gpt-6.2-astra'
handlers['submit:settings_model_edit']()
assert(config['assistant.semantic_models.codex.model'].value=='gpt-6.2-astra')
assert(config['assistant.models.codex.model'].value=='')
handlers.settings_row(3)
text.settings_model_edit=''
handlers['submit:settings_model_edit']()
assert(config['assistant.voice_models.codex.model'].value=='')
handlers.settings_row(0) -- edit Claude without changing the active provider
handlers.settings_row(1)
assert(text.settings_model_edit=='')
handlers.settings_model_cancel()
assert(not fact.settings_model_editing)
print('PASS Assistant scenario model editing, effort cycling, inheritance reset and provider selection')
