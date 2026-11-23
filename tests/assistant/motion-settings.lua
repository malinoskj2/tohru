-- The separate Settings process must forward managed temporary changes to the
-- bar's process owner, while persisted edits need only the normal reload.
_G.fact, _G.text = {}, {}
local commands, applied = {}, nil
_G.sys = {call = function(verb, target, command)
    assert(verb=='say' and target=='tohru')
    commands[#commands+1]=command
end}
_G.tohru_settings={snapshot=function() return {} end}
_G.on=function() end
_G.require=function(name)
    if name=='corner-size' then return function() return {publish=function() end} end end
    assert(name=='controller')
    return function(callback) applied=callback;return function() end end
end
assert(loadfile('shell/settings/settings.luau'))()
for key,event in pairs({['assistant.reduced_motion']='setting_assistant_reduced_motion', ['assistant.contextual_entrances']='setting_assistant_entrances'}) do
    for _,value in ipairs({true,false}) do
        commands={}
        applied({[key]={value=value,managed=true,runtime_override=true}},nil)
        assert(#commands==2 and commands[1]=='emit config_reload')
        assert(commands[2]=='emit '..event..' '..(value and '1' or '0'))
    end
end
commands={}
applied({['assistant.reduced_motion']={value=true,managed=false,runtime_override=false}},nil)
assert(#commands==1 and commands[1]=='emit config_reload')
print('Managed motion preferences reach the bar owner; persisted preferences reload normally')
