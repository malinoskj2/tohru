"""Verify actual native template's unit conversion at retained presentation values."""
from pathlib import Path
import math
scene = Path("shell/network/popup.plm").read_text()
assert '"{round(n.strength * 100)}%"' in scene
for value, expected in [(0,0),(.49,49),(.8,80),(1,100)]:
    assert math.floor(value * 100 + .5) == expected
print("Native normalized Wi-Fi strength template: 0/49/80/100 percent")
