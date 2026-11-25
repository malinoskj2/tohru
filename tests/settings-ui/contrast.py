#!/usr/bin/env python3
"""Settings row fill and text contrast regression; no native services."""
import re
from pathlib import Path

root = Path(__file__).resolve().parents[2]
source = (root / 'shell/settings/window.plm').read_text()
rows = source.split('for setting in settings_rows {', 1)[1].split('on press hit', 1)[0]
assert rows.count('size: settings_width, 58; corner: 10;') == 1, 'no second full-row fill overlay'
assert 'color: if(setting.index == settings_selected and not settings_sidebar, ink_card_selected, ink_card)' in rows
assert 'text setting.title' in rows and 'color: ink_text }' in rows
assert 'text setting.detail' in rows and 'color: ink_sub0 }' in rows

ink = (root / 'shell/panel-ink.plm').read_text()

def palette(name):
    match = re.search(r'let ' + name + r' = (?:mix\()?pick\(palette_id, ([^)]*)\)', ink)
    assert match, name
    colors = [c.strip().lstrip('#') for c in match.group(1).split(',')]
    assert len(colors) == 8, (name, colors)
    return colors

def luminance(color):
    rgb = [int(color[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    rgb = [v / 12.92 if v <= .04045 else ((v + .055) / 1.055) ** 2.4 for v in rgb]
    return sum(v * w for v, w in zip(rgb, (.2126, .7152, .0722)))

MOCHA = 0
assert palette('ink_card')[MOCHA] == '313244' and palette('ink_card_selected')[MOCHA] == '3b3d53'
for text in ('ink_text', 'ink_sub0'):
    color = palette(text)[MOCHA]
    for fill in ('ink_card', 'ink_card_selected'):
        ratio = (luminance(color) + .05) / (luminance(palette(fill)[MOCHA]) + .05)
        assert ratio >= 4.5, (text, fill, ratio)
print('PASS one row fill; Mocha title/detail contrast >= 4.5:1 in both states')
