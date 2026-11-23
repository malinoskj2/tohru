import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "bin"))
from tohru_flight_scene import flight_output, snapshot

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
landscape = monitor | dict(id=9, name="DP-1", transform=0, x=0)
actor = {"pid": 211, "namespace": "tohru-dragon-flight"}


def layers(*surfaces):
    return {"levels": {"1": list(surfaces), "3": []}}


assert (
    flight_output({"DP-4": layers(actor), "DP-1": layers(actor | {"pid": 12})}, 211)
    == "DP-4"
)
assert flight_output({"DP-4": layers(actor), "DP-1": layers(actor)}, 211) is None
assert flight_output({"DP-1": layers()}, 211) is None
twin = monitor | dict(id=9, name="DP-1", x=0)
assert "target" not in snapshot([monitor, twin], window, [window], "DP-1")
assert snapshot([twin], window, [window], "DP-4") == {}
assert not snapshot([monitor, twin], window | {"fullscreen": 2}, [], "DP-1")["blocked"]
scene = snapshot([landscape, monitor], window, [window], "DP-4")
assert (scene["w"], scene["h"], scene["blocked"]) == (720, 1280, False)
assert scene["target"] == dict(id="0xab", x=120, y=160, w=420, h=610)
assert snapshot([monitor, monitor], window, [window], "DP-4") == {}
for change in (
    {"hidden": True},
    {"mapped": False},
    {"workspace": {"id": 2}},
    {"monitor": 3},
):
    assert "target" not in snapshot([landscape, monitor], window | change, [], "DP-4")
assert snapshot([landscape, monitor], window | {"fullscreen": 2}, [], "DP-4")["blocked"]
assert "target" not in snapshot([landscape, monitor], {}, [], "DP-4")
print("PASS rotated fractional output, global origin, identity and visibility")

cover = window | {"address": "0xcd", "at": [-300, 1300], "size": [200, 240]}
assert "target" not in snapshot([landscape, monitor], window, [window, cover], "DP-4")
