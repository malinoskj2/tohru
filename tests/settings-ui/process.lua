math.clamp=function(v,a,b)return math.max(a,math.min(b,v))end
fact,text,model,handlers={settings_selected=0,settings_sidebar=false},{settings_search=''},{},{}
local listeners, probes, values, shortcut_queries = {}, {}, {}, {}
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
    if program == "tohru" then
        assert(args[1] == "shortcuts")
        table.insert(shortcut_queries, {operation = args[2], done = done})
        return
    end
    assert(program == "tohru-voice" and args[1] == "--devices")
    table.insert(probes, {source = args[3], done = done})
end
json = {decode = function(source) return {sources={source}, selected=source, aec="unknown"} end}
local config={['bar.mooncoil']={value=false},['bar.info_button']={value=false},['bar.about_button']={value=false},['bar.preferred_output']={value=''},['appearance.corners']={value=true},['appearance.corner_size']={value=32},
    ['appearance.palette']={value='mocha',managed=true},['lock.style']={value='flooded-monolith'},
    ['notifications.position']={value='top-right',managed=true},
    ['assistant.reduced_motion']={value=false},['assistant.voice_source']={value=''}}
config['appearance.font_text']={value='Noto Sans'}
config['appearance.font_display']={value='Noto Sans',managed=true}
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
local queries_before = #shortcut_queries
handlers.settings_open_request()
assert(#shortcut_queries == queries_before + 1 and shortcut_queries[#shortcut_queries].operation == "status")
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
queries_before = #shortcut_queries
handlers.settings_open_request()
assert(fact.settings_open and text.settings_ui_error:find(reload_error,1,true))
assert(publications==before and #probes==probe_count and model.settings_rows[1].detail=="unknown · new microphone")
assert(#shortcut_queries == queries_before)
reload_error, next_config = nil, nil

local font=search('appearance.font_text'); assert(font.editable)
handlers.settings_row(0); assert(config['appearance.font_text'].value=='DejaVu Sans')
local managedfont=search('appearance.font_display'); assert(not managedfont.editable)
handlers.settings_row(0); assert(config['appearance.font_display'].value=='Noto Sans')
local bindings=search('read-only'); assert(not bindings.editable and bindings.detail:find('read%-only'))
print('PASS native fonts, managed read-only font and unavailable shortcut ownership UI')

-- Portal ownership is distinct from compositor-managed rows and uses real owner state.
config['shortcuts.mode']={value='portal'}
local operations={}
local portal={owner='Tohru actions via desktop portal',state='not-configured',registration='portal-session',portal_version=2,
 bindings={},setup_available=true,configure_available=false}
json={decode=function(value)return value end}
run=function(program,args,callback)
 if program=='tohru-voice' then callback({sources={}},0);return end
 assert(program=='tohru' and args[1]=='shortcuts')
 operations[#operations+1]=args[2];callback(portal,0)
end
search('Refresh shortcut snapshot');handlers.settings_row(0)
local setup=search('Set up Tohru action shortcuts');assert(setup.editable)
handlers.settings_row(0);assert(operations[#operations]=='setup')
portal.state='granted';portal.configure_available=true
portal.bindings={{shortcut='Backend granted key',action='launcher',arguments='',submap='',flags='portal grant'}}
search('Refresh shortcut snapshot');handlers.settings_row(0)
local granted=search('Backend granted key');assert(not granted.editable)
local count=#operations;handlers.settings_row(0);assert(#operations==count)
local configure=search('Configure granted shortcuts');assert(configure.editable)
handlers.settings_row(0);assert(operations[#operations]=='configure')
portal.portal_version=1;portal.configure_available=false;search('Refresh shortcut snapshot');handlers.settings_row(0)
assert(not search('Configure granted shortcuts').editable)
config['shortcuts.mode'].managed=true
assert(not search('Shortcut ownership mode').editable)
print('PASS portal setup/configure actions, backend-confirmed read-only triggers, v1 limitations and managed mode')
