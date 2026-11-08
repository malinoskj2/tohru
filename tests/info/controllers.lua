local events,reads={},0
_G.fact={info_visible=false,info_page=0,info_offset=0,info_cache_valid=false}
_G.text={}
local records={}
_G.model=setmetatable({},{__index=records,__newindex=function(_,name,list)
 records[name]=list;fact[name..".count"]=#list
 for index,row in ipairs(list) do for key,item in pairs(row) do text[name.."."..(index-1).."."..key]=item end end
end})
_G.on=function(name,fn)events[name]=fn end
local data={system={os_name="Fixture Linux",hostname="harbor",memory_total_bytes=8*1073741824,current_uid=1000,current_generation=nil},issues={{source="missing",kind="Io"}},accounts={{name="root",uid=0,description="Root",home="/root",shell="/bin/sh"},{name="alice",uid=1000,description="Alice",home="/home/alice",shell="/bin/zsh"}},disks={{name="sda",capacity_bytes=1024,removable=false}},mounts={{path="/",source="/dev/sda",filesystem="ext4",read_only=true}},generations={}}
for number=30,1,-1 do data.generations[#data.generations+1]={number=number,target="/nix/store/system"..number,current=false} end
local function snapshot()reads=reads+1;return data end
local function outputs()return {{name="DP-1",width=1920,height=1080}} end
local info=assert(loadfile("shell/info/controller.luau"))()(snapshot,outputs)
assert(reads==0)
info.open(0);assert(reads==1 and fact.info_visible and #model.info_rows==8 and text.info_error:find("partial",1,true))
events.info_page(1);assert(reads==1 and model.info_rows[1].value=="DP-1" and model.info_rows[2].value=="1920 × 1080" and model.info_rows[3].value=="Unknown")
events.info_page(3);assert(reads==2 and model.info_rows[1].label=="alice" and text.info_current_user=="alice · UID 1000")
events.info_page(4);assert(model.info_rows[2].value:find("read only",1,true))
events.info_page(5);assert(text.info_generation=="Unknown" and fact.info_total==30 and #model.info_rows==8)
events.info_scroll(8);assert(fact.info_offset==8 and model.info_rows[1].label=="Generation 22")
local prior=reads
events={};info=assert(loadfile("shell/info/controller.luau"))()(snapshot,outputs)
assert(reads==prior and #model.info_rows==8 and model.info_rows[1].label=="Generation 22")
events.info_scroll(999);assert(fact.info_offset==22 and model.info_rows[8].label=="Generation 1")
events.info_refresh();assert(reads==prior+1 and fact.info_offset==0)
events.info_page(2);events.info_refresh();events.info_page(6);events.info_refresh();assert(reads==prior+1)
info.close();assert(not fact.info_visible)
-- Bounded complete account presentation preserves the current account first.
data.accounts={};for index=1,256 do data.accounts[index]={name="user"..index,uid=index,description="",home="/home/user",shell="/bin/sh"} end
data.system.current_uid=256;info.open(3);assert(fact.info_total==256 and model.info_rows[1].label=="user256")
local about=assert(loadfile("shell/about/controller.luau"))()(snapshot,function(page)info.open(page)end)
prior=reads;about.open();assert(reads==prior+1 and #model.about_specs==6 and text.about_name=="Fixture Linux")
assert(model.about_specs[4].value=="8.0 GiB" and model.about_specs[5].value:find("sda",1,true))
events.about_more();assert(not fact.about_visible and fact.info_visible)
about=assert(loadfile("shell/about/controller.luau"))()(function()error("Read unavailable")end,function()error("Role unavailable")end)
about.open();assert(text.about_name=="Unknown" and text.about_error:find("Read unavailable",1,true))
events.about_more();assert(fact.about_visible and text.about_error:find("Role unavailable",1,true))
print("Production information/About open-only reads, cached reload, unknowns, full account paging and direct lifecycle fixtures passed")

assert(events.about_repository == nil)
print("About repository remains selectable text without an unsupported launch command")
