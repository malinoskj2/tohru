local events,reads={},0
_G.fact={info_visible=false,info_page=0,info_offset=0,info_cache_valid=false}
_G.text={}
local records={}
_G.model=setmetatable({},{__index=records,__newindex=function(_,name,list)
 records[name]=list;fact[name..".count"]=#list
 for index,row in ipairs(list) do for key,item in pairs(row) do text[name.."."..(index-1).."."..key]=item end end
end})
_G.on=function(name,fn)events[name]=fn end
local stops,resets={},0
local rail={observe=function(slots,slot)assert(slots==7);stops[#stops+1]=slot end,reset=function()resets=resets+1 end}
local data={system={os_name="NixOS",os_version="26.05 (Yarara)",hostname="home",kernel="6.12.14",processor="AMD Ryzen 9 7950X 16-Core Processor",
 memory_total_bytes=64*1073741824,memory_available_bytes=48.2*1073741824,current_uid=1000,current_generation=412,system_target="/nix/store/system-412"},
 issues={},accounts={{name="jesse",uid=1000,description="",home="/home/jesse",shell="/bin/zsh"}},
 disks={{name="nvme0n1",capacity_bytes=2e12,model="Samsung 990 PRO 2TB",removable=false}},
 mounts={{path="/",source="/dev/nvme0n1p2",filesystem="ext4",read_only=false},{path="/boot",source="/dev/nvme0n1p1",filesystem="vfat",read_only=false}},
 generations={{number=412,target="/nix/store/system-412",current=true},{number=411,target="/nix/store/system-411",current=false}}}
local function snapshot()reads=reads+1;return data end
local info=assert(loadfile("shell/info/controller.luau"))()(snapshot,nil,rail)

-- Opening without a page shows the folded overview, reads once and fills every tile.
events.info_open()
assert(fact.info_visible and not fact.info_unfolded and fact.info_page==0 and reads==1)
assert(text["info_big.0"]=="64" and text["info_unit.0"]=="GiB" and text.info_caption=="Ryzen 9 · 6.12")
assert(text["info_note.0"]=="48.2 GiB available · NixOS 26.05 (Yarara)")
assert(text["info_big.4"]=="2" and text["info_unit.4"]=="TB" and text.info_storage_caption=="1 disk · 2 mounts")
assert(text["info_note.4"]=="1 block device · 2 mounts")
assert(text.info_platform_title=="Generations" and text["info_big.5"]=="412" and text.info_generation_caption=="2 generations" and text["info_note.5"]=="2 generations · active")
assert(text["info_big.3"]=="1" and text["info_unit.3"]=="account" and text["info_note.3"]=="Current jesse · UID 1000")
assert(not fact.info_partial and stops[1]==1)

-- Left and Right choose a stop on the rail while folded, clamped to the seven pages, without reading.
events.info_step(1);events.info_step(1)
assert(fact.info_page==2 and not fact.info_unfolded and reads==1 and stops[#stops]==3)
events.info_step(-5);assert(fact.info_page==1)
events.info_step(-1);events.info_step(-1);assert(fact.info_page==0 and stops[#stops]==1)
for _=1,9 do events.info_step(1) end
assert(fact.info_page==6 and stops[#stops]==7)

-- Return or Tab unfolds the chosen page; Power is drawn live, so it reads nothing.
events.info_unfold()
assert(fact.info_unfolded and fact.info_page==6 and reads==1 and fact.info_total==0)
-- Unfolded, the arrows turn the page and the rail follows; a data page reads its snapshot.
events.info_step(-1)
assert(fact.info_unfolded and fact.info_page==5 and reads==2 and stops[#stops]==6)
assert(model.info_rows[1].label=="Generation 412 · active")
events.info_step(-1);assert(fact.info_page==4 and reads==3 and model.info_rows[1].label=="nvme0n1")
-- Tab folds back to the overview on the same page, with nothing read.
events.info_fold_toggle()
assert(not fact.info_unfolded and fact.info_page==4 and reads==3 and stops[#stops]==5)
-- Unfolding the page already cached does not read again.
events.info_fold_toggle();assert(fact.info_unfolded and fact.info_page==4 and reads==3)
events.info_fold();events.info_fold();assert(not fact.info_unfolded)
-- A tile or a stop unfolds its own page directly.
events.info_page(0);assert(fact.info_unfolded and fact.info_page==0 and reads==4 and model.info_rows[3].value=="home")
events.info_fold();events.info_page(2);assert(fact.info_unfolded and fact.info_page==2 and reads==4 and model.info_rows[1].label=="Timezone")
events.info_page(9);assert(fact.info_page==2)

-- Displays and its hero come from the outputs function when one is given.
info=assert(loadfile("shell/info/controller.luau"))()(snapshot,function()return {{name="DP-1",width=2560,height=1440}} end,rail)
events.info_page(1);assert(text["info_big.1"]=="1" and text["info_unit.1"]=="output" and text["info_note.1"]=="DP-1 · 2560 × 1440")

-- Closing folds and drops the bead; opening on a page unfolds it straight away.
events.info_close();assert(not fact.info_visible and not fact.info_unfolded and resets==1)
events.info_open(5);assert(fact.info_visible and fact.info_unfolded and fact.info_page==5)
events.info_toggle();assert(not fact.info_visible and resets==2)
events.info_toggle();assert(fact.info_visible and not fact.info_unfolded and fact.info_page==5)
info.close()
-- Stepping does nothing while closed.
events.info_step(1);assert(fact.info_page==5 and not fact.info_visible)

-- Missing facts read as unknown and a partial snapshot is flagged on the overview.
data={system={},issues={{source="missing",kind="Io"}}}
events.info_open()
assert(text["info_big.0"]=="—" and text["info_big.4"]=="—" and text.info_caption=="Unknown" and fact.info_partial)
-- Without a NixOS system target the fifth tile is Platform, which still names the kernel family.
assert(text.info_platform_title=="Platform" and text["info_big.5"]=="Linux" and text["info_note.5"]=="System information")
print("Information overview folding, page selection, rail stops, summaries and lifecycle passed")

-- The scene binds the keyboard to those events.
local popup=assert(io.open("shell/info/popup.plm")):read("a")
for _,rule in ipairs({
 "on key Tab while info_visible { emit info_fold_toggle }",
 "on key Left while info_visible { emit info_step(-1) }",
 "on key Right while info_visible { emit info_step(1) }",
 "on key Return while info_visible and not info_unfolded { emit info_unfold }",
 "on key Down while info_visible and info_unfolded { emit info_scroll(1) }",
 "on key Escape while info_visible { emit info_close }",
}) do assert(popup:find(rule,1,true),rule) end
print("Information keyboard rules passed")

-- More Info opens the overview folded, with the page it names chosen.
info.close();fact.info_page=3
local about=assert(loadfile("shell/about/controller.luau"))()(snapshot,function(page)info.overview(page)end)
about.open();assert(text.about_short=="—")
events.about_more();assert(fact.info_visible and not fact.info_unfolded and fact.info_page==0 and not fact.about_visible)
data.system={os_name="NixOS",os_version="26.05 (Yarara)"};about.open();assert(text.about_short=="26.05" and text.about_version=="26.05 (Yarara)")
print("About's More Info opens the folded overview on System")
