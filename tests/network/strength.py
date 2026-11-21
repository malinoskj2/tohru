"""Signal shows only as the leading arc glyph: no percentage on the tiles or the rows."""
from pathlib import Path
import re
scene = Path("shell/network/popup.plm").read_text()
assert "* 100" not in scene and not re.search(r'"[^"]*%"', scene), "no percentage is drawn"
for icon, threshold in [("net_wifi_full", "n.strength >= 0.6"), ("net_wifi_high", "n.strength >= 0.35"),
                        ("net_wifi_low", "n.strength >= 0.15"), ("net_wifi_zero", "n.strength < 0.15")]:
    assert re.search(rf"image {icon} \{{ at: net_ox \+ 13, n_y \+ 6;.*{re.escape(threshold)}", scene), icon
print("Wi-Fi strength: arc glyphs at 0.15/0.35/0.6, no percentage")
