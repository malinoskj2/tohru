"""Stage a native fixture in this checkout's .tmp, with the production renderer and puppet."""

from pathlib import Path
import json
import sys

root = Path(__file__).resolve().parents[2]
voice = "--voice" in sys.argv
out = root / (".tmp/flight-voice-demo" if voice else ".tmp/flight-demo")
out.mkdir(parents=True, exist_ok=True)
for name in ("assistant", "brand"):
    link = out / name
    if not link.exists():
        link.symlink_to(root / "shell" / name)
state = (out / "assistant/state.plm").read_text()
# The remaining declarations are only for the chat panel, which needs bar layout.
state = state[: state.index("    let ai_colour")].replace('file "', 'file "assistant/')
(out / "state.plm").write_text(state + "}\n")
(out / "flight.plm").write_text("""scene FlightTest {
surface { size: 1, 1; keyboard: none; reserve: -1 }
permissions { run: "tohru_flight_scene.py" }
fact notices_dnd = false
fact test_hits = 0
text test_output = ""
event test_show ->
event test_interrupt ->
include "state.plm"
include "assistant/afk-render.plm"
include "assistant/dragon.plm"
}
""")
(out / "flight.luau").write_text(
    """
local puppet=require("assistant/puppet")(require("assistant/full-body"), require("assistant/landing-frames"))
local flight=require("assistant/flight")(require("assistant/flight-path"),puppet,require("assistant/flight-actor"))
puppet.load(ARTDIR)
on("test_show",function(n)
    flight.outputs({text.test_output})
    flight.hide()
    puppet.reduced(n==5)
    local force=({"wrap","rise","right","sweep","wrap"})[n]
    flight.configure(function(k,default)
        if k=="assistant.contextual_entrances" then return true end
        if k=="assistant.entrance_variant" then return force end
        return default
    end)
    flight.show()
    fact.ai_dragon_open=true
    puppet.visible(true)
end)
on("test_interrupt",flight.interrupt)
on("assistant_dragon",function() fact.test_hits=fact.test_hits+1 end)
""".replace("ARTDIR", json.dumps(str(root / "shell/assistant/dragon")))
)
if voice:
    scene = out / "flight.plm"
    scene.write_text(
        scene.read_text().replace(
            'run: "tohru_flight_scene.py"',
            'services: "window"; run: "tohru_flight_scene.py", "tohru-flight-fixture-voice"',
        )
    )
    (out / "flight.luau").write_text("""
local puppet=require("assistant/puppet")(require("assistant/full-body"), require("assistant/landing-frames"))
local flight=require("assistant/flight")(require("assistant/flight-path"),puppet,require("assistant/flight-actor"))
sys.watch("window",flight.window)
local voice=require("assistant/voice")({
    listening=function(v) fact.ai_listening=v end, ask=function() return true end,
    remember=function() end, on_answer=function() end,
},puppet,flight,"tohru-flight-fixture-voice")
on("test_show",function(n)
    local names={}
    for name in text.test_output:gmatch("[^,]+") do names[#names+1]=name end
    flight.outputs(names)
    voice.configure({
        ["assistant.voice"]={value=true}, ["assistant.contextual_entrances"]={value=true},
        ["assistant.reduced_motion"]={value=n==5},
        ["assistant.entrance_variant"]={value=({"wrap","rise","right","sweep","wrap"})[n]},
    })
end)
""")
print(out / "flight.plm")
