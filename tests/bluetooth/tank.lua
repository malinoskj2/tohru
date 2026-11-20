local events, calls, timers, watched = {}, {}, {}, nil
local next_timer = 0
_G.fact = {bt_open=false,bt_unfolded=false,bt_page=0,bt_selected=-1,["bt_devices.count"]=0,["bt.present"]=true,["bt.powered"]=true,["bt.discovering"]=false,["bt.pairing_id"]=0,["bt.pairing"]="none"}
_G.text = {bt_answer="",bt_target="",bt_request=""}
_G.on = function(name,handler) events[name]=handler end
_G.after = function(ms,callback) next_timer=next_timer+1; timers[next_timer]={ms=ms,run=callback}; return next_timer end
_G.cancel = function(id) timers[id]=nil end
local failure
_G.sys = {call=function(name,...) if failure then error(failure) end; calls[#calls+1]={name,...} end,
          watch=function(name,callback) assert(name=="bluetooth"); watched=callback; return true end}
local moves = {}
local rail = {goal=8}
for _,name in ipairs({"place","glide","chase","switch"}) do
 rail[name]=function(goal) rail.goal=goal; moves[#moves+1]={name,goal} end
end
rail.target=function() return rail.goal end
local function fire(ms)
 for id,timer in pairs(timers) do if timer.ms==ms then timers[id]=nil; timer.run(); return true end end
end
local bluetooth=assert(loadfile("shell/bluetooth/controller.luau"))()(rail)
assert(watched, "the controller watches the service itself")

local function device(name,address,icon,paired,connected,battery) return {name=name,address=address,icon=icon,paired=paired,connected=connected,battery=battery} end
local list = {
 device("Keychron Q1","AA:00:00:00:00:01","input-keyboard",true,true,-1),
 device("Arctis Nova 7","AA:00:00:00:00:02","audio-headset",true,true,0.84),
 device("MX Master 3S","AA:00:00:00:00:03","input-mouse",true,false,0.19),
 device("WH-1000XM5","AA:00:00:00:00:04","audio-headphones",true,false,-1),
 device("DualSense","AA:00:00:00:00:05","input-gaming",true,false,-1),
 device("AirPods Pro","AA:00:00:00:00:06","",false,false,-1),
}
local function report(powered,discovering)
 fact["bt.powered"]=powered;fact["bt.discovering"]=discovering;fact["bt_devices.count"]=#list
 for index,d in ipairs(list) do
  local prefix="bt_devices."..(index-1).."."
  text[prefix.."name"]=d.name;text[prefix.."address"]=d.address;fact[prefix.."paired"]=d.paired;fact[prefix.."connected"]=d.connected
 end
 watched({present=true,powered=powered,discovering=discovering,devices=list})
end
report(true,false)
-- The device with a battery leads the tank even when another connected device is listed first.
assert(fact.bt_active==1 and text.bt_active_name=="Arctis Nova 7" and fact.bt_active_battery==0.84)
assert(fact.bt_connected==2 and fact.bt_paired==5)
assert(fact["bt_kind.0"]==2 and fact["bt_kind.1"]==1 and fact["bt_kind.2"]==3 and fact["bt_kind.4"]==5 and fact["bt_kind.5"]==0)
assert(fact["bt_mini.0"]==1 and fact["bt_mini.1"]==0 and fact["bt_mini.2"]==2 and fact["bt_mini.3"]==3 and fact.bt_minis_right==3)
assert(fact["bt_mini_on.1"]==true and fact["bt_mini_on.2"]==false and fact["bt_mini_level.2"]==0.19 and fact["bt_mini_level.1"]==-1)
assert(fact.bt_stop==1)
print("Bluetooth tank: active device, counts, kinds and minis passed")

-- Folding and unfolding: the rail parks at its left end and leaves it once it has faded in.
bluetooth.open();assert(fact.bt_open and not fact.bt_unfolded and moves[#moves-1][1]=="place" and moves[#moves-1][2]==8 and rail.goal==54)
events.bluetooth_fold_toggle();assert(fact.bt_unfolded and rail.goal==8)
assert(fire(180) and moves[#moves][1]=="switch" and rail.goal==86)
events.bluetooth_fold_toggle();assert(not fact.bt_unfolded)
events.bluetooth_fold_toggle();fire(180);assert(rail.goal==86)

-- The rail asks for a stop; the bead holds it until the adapter reports it, then follows the report.
local count=#calls
events.bluetooth_rail(2);assert(calls[#calls][1]=="bluetooth.scan" and calls[#calls][2]==true and fact.bt_scan_owned)
assert(fact.bt_stop==2 and rail.goal==164)
report(true,false);assert(fact.bt_stop==2 and rail.goal==164, "held while the adapter has not caught up")
report(true,true);assert(fact.bt_stop==2)
events.bluetooth_rail(1);assert(calls[#calls][1]=="bluetooth.scan" and calls[#calls][2]==false and not fact.bt_scan_owned and rail.goal==86)
report(true,false)
-- Slide to Off and let go: the nearest stop is chosen.
events.bluetooth_rail_press(80);events.bluetooth_rail_drag(40);events.bluetooth_rail_drag(-30)
assert(moves[#moves][1]=="chase" and rail.goal==8)
report(true,false);assert(rail.goal==8, "a report does not pull the bead out of a slide")
events.bluetooth_rail_release();assert(calls[#calls][1]=="bluetooth.power" and calls[#calls][2]==false and fact.bt_stop==0)
report(false,false);assert(fact.bt_stop==0 and rail.goal==8)
assert(fact.bt_active==-1 and text.bt_active_name=="Bluetooth" and fact.bt_connected==0, "an adapter that is off has no active device")
-- A held stop the adapter never reaches is let go.
count=#calls;events.bluetooth_rail(1);assert(calls[#calls][2]==true and fact.bt_stop==1)
assert(fire(4000) and fact.bt_stop==0 and rail.goal==8)
-- Scan while off powers on first and scans once the adapter reports power.
count=#calls;events.bluetooth_rail(2);assert(#calls==count+1 and calls[#calls][1]=="bluetooth.power")
report(true,false);assert(#calls==count+2 and calls[#calls][1]=="bluetooth.scan" and calls[#calls][2]==true and fact.bt_scan_owned)
report(true,true)
-- Discovery started elsewhere is not stopped.
fact.bt_scan_owned=false;count=#calls;events.bluetooth_rail(1);assert(#calls==count and text.bt_request:find("elsewhere",1,true))
report(true,false)
print("Bluetooth rail: fold, stops, slide, hold and scan-after-power passed")

-- Folded with the adapter off: a click anywhere on the power rail turns it on; a short slide does not.
events.bluetooth_fold_toggle();assert(not fact.bt_unfolded)
report(false,false);assert(rail.goal==8)
count=#calls;events.bluetooth_rail_press(-150);events.bluetooth_rail_release()
assert(calls[#calls][1]=="bluetooth.power" and calls[#calls][2]==true and rail.goal==54)
fire(4000);report(false,false)
count=#calls;events.bluetooth_rail_press(10);events.bluetooth_rail_drag(20);events.bluetooth_rail_release();assert(#calls==count and rail.goal==8)
events.bluetooth_rail_press(10);events.bluetooth_rail_drag(50);events.bluetooth_rail_release();assert(calls[#calls][2]==true)
failure="org.bluez.Error.Blocked";fire(4000);report(false,false)
events.bluetooth_rail_press(10);events.bluetooth_rail_release();assert(fact.bt_failed and text.bt_request:find("Blocked",1,true) and fact.bt_stop==0)
failure=nil
report(true,false)
print("Bluetooth folded power rail passed")

-- Connecting: the row is marked until the device reports connected.
events.bluetooth_row(2);assert(calls[#calls][1]=="bluetooth.connect" and calls[#calls][2]=="AA:00:00:00:00:03" and fact.bt_connecting==2)
count=#calls;events.bluetooth_row(2);assert(#calls==count and text.bt_request:find("Already",1,true))
report(true,false);assert(fact.bt_connecting==2)
list[3].connected=true;table.insert(list,1,table.remove(list,3));report(true,false)
assert(fact.bt_connecting==-1 and fact.bt_connected==3)
assert(fact.bt_selected==0 and text.bt_target=="AA:00:00:00:00:03", "the selection follows its device")
-- Its row moves with a reordered list, and a connect that never lands is given up.
events.bluetooth_row(3);assert(calls[#calls][2]=="AA:00:00:00:00:04" and fact.bt_connecting==3)
table.insert(list,1,table.remove(list,6));report(true,false);assert(fact.bt_connecting==4)
fact["bt.pairing"]="confirm";fact["bt.pairing_id"]=7;assert(fire(30000) and fact.bt_connecting==4, "a pairing question keeps it alive")
fact["bt.pairing"]="none";fact["bt.pairing_id"]=0;assert(fire(30000) and fact.bt_connecting==-1)
-- A failed request marks nothing; forgetting or powering off ends the attempt.
failure="busy";events.bluetooth_row(4);assert(fact.bt_connecting==-1 and fact.bt_failed);failure=nil
events.bluetooth_row(4);assert(fact.bt_connecting==4);events.bluetooth_forget_row(4);assert(calls[#calls][1]=="bluetooth.forget" and fact.bt_connecting==-1)
events.bluetooth_row(4);report(false,false);assert(fact.bt_connecting==-1)
events.bluetooth_select(4);local gone=table.remove(list,5);report(true,false)
assert(fact.bt_selected==-1 and text.bt_target=="" and not fact.bt_has_target, "a device that goes takes the selection");table.insert(list,5,gone)
-- Nearby devices cannot be forgotten.
report(true,false);count=#calls;events.bluetooth_forget_row(0);assert(#calls==count)
print("Bluetooth connecting state passed")

-- The pairing code is grouped as the mock shows it, and closing folds the panel.
fact["bt.pairing_id"]=9;fact["bt.pairing"]="confirm";text["bt.pairing_code"]="482910";events.bluetooth_question();assert(text.bt_code=="482 910")
text["bt.pairing_code"]="1234";events.bluetooth_question();assert(text.bt_code=="1234")
fact.bt_unfolded=true;bluetooth.close();assert(not fact.bt_open and not fact.bt_unfolded and calls[#calls][1]=="bluetooth.cancel")
fact["bt.pairing_id"]=0;fact["bt.pairing"]="none"
bluetooth.open();events.bluetooth_move(1);assert(fact.bt_unfolded and fact.bt_selected==0)
print("Bluetooth pairing code and fold lifecycle passed")
