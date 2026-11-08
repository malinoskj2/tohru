local events, calls = {}, {}
_G.fact = {bt_open=false,bt_page=0,bt_selected=-1,["bt_devices.count"]=2,["bt.present"]=true,["bt.powered"]=true,["bt.discovering"]=false,["bt.pairing_id"]=0,["bt.pairing"]="none"}
_G.text = {bt_answer="",bt_target="",bt_request=""}
_G.on = function(name,handler) events[name]=handler end
local failure
_G.sys = {call=function(name,...) if failure then error(failure) end;calls[#calls+1]={name,...} end}
local bluetooth=assert(loadfile("shell/bluetooth/controller.luau"))()()
for index,address in ipairs({"AA:BB:CC:DD:EE:01","AA:BB:CC:DD:EE:02"}) do
 local prefix="bt_devices."..(index-1)..".";text[prefix.."address"]=address;text[prefix.."name"]="Device"..index;fact[prefix.."connected"]=index==1
end
bluetooth.open();events.bluetooth_select(-1);events.bluetooth_select(2);assert(not fact.bt_has_target)
events.bluetooth_select(0);events.bluetooth_connect();assert(calls[#calls][1]=="bluetooth.disconnect")
events.bluetooth_select(1);events.bluetooth_connect();assert(calls[#calls][1]=="bluetooth.connect" and calls[#calls][2]=="AA:BB:CC:DD:EE:02")
text["bt_devices.1.address"]="replacement";local count=#calls;events.bluetooth_connect();assert(#calls==count)
events.bluetooth_scan();assert(calls[#calls][1]=="bluetooth.scan" and calls[#calls][2]==true)
bluetooth.close();assert(calls[#calls][1]=="bluetooth.scan" and calls[#calls][2]==false and not fact.bt_open)
for _,kind in ipairs({"pin","passkey","confirm","authorize","display"}) do
 fact["bt.pairing_id"]=10;fact["bt.pairing"]=kind;events.bluetooth_question();assert(fact.bt_open and text.bt_answer=="")
 text.bt_answer="000042";count=#calls;events.bluetooth_answer()
 if kind=="display" then assert(#calls==count)
 elseif kind=="pin" or kind=="passkey" then assert(calls[#calls][1]=="bluetooth.answer" and calls[#calls][2]==10 and calls[#calls][3]=="000042" and text.bt_answer=="")
 else assert(calls[#calls][1]=="bluetooth.confirm" and calls[#calls][2]==10) end
end
fact["bt.pairing_id"]=11;fact["bt.pairing"]="confirm";count=#calls
events.bluetooth_answer();assert(#calls==count and text.bt_request:find("changed",1,true))
events.bluetooth_question();events.bluetooth_answer();assert(calls[#calls][2]==11)
events.bluetooth_cancel();assert(calls[#calls][1]=="bluetooth.cancel" and calls[#calls][2]==11)
fact["bt.pairing_id"]=0;fact["bt.pairing"]="none";events.bluetooth_question();count=#calls;events.bluetooth_answer();assert(#calls==count)
failure="No adapter";events.bluetooth_power();assert(text.bt_request:find(failure,1,true))
print("Production Bluetooth stale device/question, leading-zero code, confirmation/display/cancel and owned discovery fixtures passed")
