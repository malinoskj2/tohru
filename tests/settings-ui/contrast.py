#!/usr/bin/env python3
"""Settings row fill and text contrast regression; no native services."""
from pathlib import Path

source = (Path(__file__).resolve().parents[2] / 'shell/settings/window.plm').read_text()
rows = source.split('for setting in settings_rows {', 1)[1].split('on press hit', 1)[0]
assert rows.count('size: 556, 58; corner: 10;') == 1, 'no second full-row fill overlay'
assert 'color: if(setting.index == settings_selected and not settings_sidebar, #3b3d53, #313244)' in rows

def luminance(color):
    rgb = [int(color[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    rgb = [v / 12.92 if v <= .04045 else ((v + .055) / 1.055) ** 2.4 for v in rgb]
    return sum(v * w for v, w in zip(rgb, (.2126, .7152, .0722)))

for text in ('cdd6f4', 'a6adc8'):
    assert '#' + text in rows
    for fill in ('313244', '3b3d53'):
        ratio = (luminance(text) + .05) / (luminance(fill) + .05)
        assert ratio >= 4.5, (text, fill, ratio)
print('PASS one dark row fill; title/detail contrast >= 4.5:1 in both states')
