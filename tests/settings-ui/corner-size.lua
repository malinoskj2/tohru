-- Actual numeric preference module; all operations use the existing owner boundary.
text,handlers={},{}
function on(name,callback)assert(not handlers[name]);handlers[name]=callback end
local setting={value=32,managed=false,runtime_override=false}
local settings={['appearance.corner_size']=setting}
local calls,published=0,0
local bridge
local function publish(snapshot)
    assert(snapshot==settings);published=published+1;bridge.publish(snapshot)
end
tohru_settings={set=function(key,value)
    assert(key=='appearance.corner_size' and type(value)=='number' and value%1==0 and value>=1 and value<=100)
    calls=calls+1
    if refused then error('write refused')end
    setting.value=value;setting.runtime_override=setting.managed
    return settings,warning
end}
bridge=assert(loadfile('shell/settings/corner-size.luau'))()(publish)
bridge.publish(settings)
assert(text.settings_corner_size_edit=='32' and text.settings_corner_size_notice:find('saved'))
-- An unrelated snapshot must not clobber the input draft.
text.settings_corner_size_edit='75';bridge.publish(settings)
assert(text.settings_corner_size_edit=='75')
for _,value in ipairs({'1','32','99','100'})do
    text.settings_corner_size_edit=value;handlers['submit:settings_corner_size_edit']()
    assert(setting.value==tonumber(value) and text.settings_corner_size_edit==value)
end
assert(calls==4 and published==4)
for _,invalid in ipairs({'0','101','-1','+1','01',' 1','1 ','1.5','1e2','0x20','',string.rep('9',4096),'32\nemit quit'})do
    text.settings_corner_size_edit=invalid;handlers['submit:settings_corner_size_edit']()
    assert(calls==4 and published==4 and setting.value==100)
    assert(text.settings_corner_size_edit==invalid and text.settings_ui_error:find('1 to 100'))
end
refused=true;text.settings_corner_size_edit='40';handlers['submit:settings_corner_size_edit']()
assert(calls==5 and published==4 and setting.value==100)
assert(text.settings_corner_size_edit=='40' and text.settings_ui_error:find('write refused'))
refused=false;warning='committed but durability unconfirmed';setting.managed=true
text.settings_corner_size_edit='40';handlers['submit:settings_corner_size_edit']()
assert(setting.value==40 and setting.runtime_override and published==5)
assert(text.settings_ui_error==warning and text.settings_error==warning)
assert(text.settings_corner_size_notice:find('temporary runtime override'))
warning=nil;text.settings_corner_size_edit='41';handlers['submit:settings_corner_size_edit']()
assert(text.settings_ui_error=='' and text.settings_error=='' and setting.value==41)
-- Accepted external changes replace the stale draft and refresh provenance.
setting.value=32;setting.runtime_override=false;bridge.publish(settings)
assert(text.settings_corner_size_edit=='32' and text.settings_corner_size_notice:find('until the shell exits'))
print('PASS actual corner-size input bounds/drafts/errors/provenance/owner mapping')
