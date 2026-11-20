local events, calls = {}, {}
_G.fact = {net_open = false, net_unfolded = false, net_has_request = false, net_page = 0, net_selected = -1, ["net_rows.count"] = 3, ["net.wifi"] = true, ["net.kind"] = "wifi", ["net.online"] = true}
_G.text = {net_password = "", net_target = "", net_request = "", ["net.name"] = "Home"}
_G.on = function(name, handler) events[name] = handler end
local failure
_G.sys = {call = function(name, ...) if failure then error(failure) end; calls[#calls+1] = {name,...} end}
local network = assert(loadfile("shell/network/controller.luau"))()()
local function row(index, name, secure, known, active)
    text["net_rows."..index..".ssid"] = name
    for field, value in pairs({secure=secure, known=known, active=active}) do fact["net_rows."..index.."."..field] = value end
end
row(0,"Home",true,true,true);row(1,"Cafe",false,false,false);row(2,"Secret",true,false,false)
network.open(); assert(fact.net_open and not fact.net_unfolded and fact.net_selected == -1 and not fact.net_has_request)
events.network_fold_toggle(); assert(fact.net_unfolded)
events.network_fold_toggle(); assert(not fact.net_unfolded)
events.network_select(-1); events.network_select(3); events.network_select(0.5); assert(#calls==0)
events.network_select(1); assert(calls[#calls][1]=="network.connect" and calls[#calls][2]=="Cafe" and not fact.net_joining)
events.network_select(2); assert(fact.net_joining and fact.net_unfolded and text.net_target=="Secret")
local count=#calls
events.network_join(); assert(#calls==count and text.net_request:find("password") and fact.net_has_request)
text.net_password="private value"
events["submit:net_password"](); assert(calls[#calls][2]=="Secret" and calls[#calls][3]=="private value" and text.net_password=="" and not fact.net_joining)
assert(text.net_request:find("Request sent",1,true))
events.network_select(0); events.network_disconnect(); assert(calls[#calls][1]=="network.disconnect")
events.network_disconnect_active(); assert(calls[#calls][1]=="network.disconnect")
text["net.name"]="New connection"; count=#calls
events.network_disconnect(); assert(#calls==count) -- Never disconnect a replacement network.
events.network_forget(); assert(calls[#calls][1]=="network.forget" and calls[#calls][2]=="Home")
events.network_forget_row(0); assert(calls[#calls][1]=="network.forget" and calls[#calls][2]=="Home")
events.network_forget_row(1); assert(#calls==count+2) -- Never forget an unknown network.
events.network_power(); assert(calls[#calls][1]=="network.wifi" and calls[#calls][2]==false)
failure="Native unavailable";events.network_scan();assert(text.net_request:find(failure,1,true) and fact.net_has_request);failure=nil
events.network_select(2);text.net_password="do not retain";events.network_fold_toggle();assert(not fact.net_unfolded and not fact.net_joining and text.net_password=="")
events.network_select(2);text.net_password="do not retain";network.close();assert(not fact.net_open and not fact.net_unfolded and not fact.net_joining and text.net_password=="")
network.open();events.network_move(1);assert(fact.net_selected==0 and fact.net_unfolded);events.network_move(99);assert(fact.net_selected==2 and fact.net_page==0)
events.network_page(1);assert(fact.net_page==0)
print("Production Wi-Fi native command, password, fold, stale status, error and lifecycle fixtures passed")
