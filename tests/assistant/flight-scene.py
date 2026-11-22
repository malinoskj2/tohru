import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "bin"))
from tohru_flight_scene import snapshot

monitor = dict(
    id=7,
    name="DP-4",
    x=-900,
    y=220,
    width=1920,
    height=1080,
    scale=1.5,
    transform=1,
    activeWorkspace={"id": 3},
    solitary="0",
)
window = dict(
    address="0xab",
    workspace={"id": 3},
    mapped=True,
    hidden=False,
    monitor=7,
    at=[-780, 380],
    size=[420, 610],
)
scene = snapshot([monitor], window, [window])
assert (scene["w"], scene["h"], scene["blocked"]) == (720, 1280, False)
assert scene["target"] == dict(id="0xab", x=120, y=160, w=420, h=610)
assert snapshot([monitor, monitor], window, [window]) == {}
for change in (
    {"hidden": True},
    {"mapped": False},
    {"workspace": {"id": 2}},
    {"monitor": 3},
):
    assert "target" not in snapshot([monitor], window | change, [])
assert snapshot([monitor], window | {"fullscreen": 2}, [])["blocked"]
assert "target" not in snapshot([monitor], {}, [])
print("PASS rotated fractional output, global origin, identity and visibility")

cover = window | {"address": "0xcd", "at": [-300, 1300], "size": [200, 240]}
assert "target" not in snapshot([monitor], window, [window, cover])
