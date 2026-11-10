#!/usr/bin/env python3
"""Source/API installation assertions only; never compile or query a native service."""
import argparse
from pathlib import Path
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--source', type=Path, required=True)
source = parser.parse_args().source
prepare = (ROOT / 'runtime/prepare.sh').read_text()
nix = (ROOT / 'nix/default.nix').read_text()
patches = re.findall(r'"\$root/([^"\n]+\.patch)"', prepare)
nix_patches = re.findall(r'\.\./([^\s\]]+\.patch)', re.search(r'    patches = \[(.*?)\];', nix, re.S).group(1))
assert patches == nix_patches and len(patches) == 21
at = patches.index('runtime/capture/image-lifecycle.patch')
assert patches[at + 1:at + 3] == ['runtime/wallpaper/raster-limits.patch', 'runtime/wallpaper/presets.patch']
copies = {
    'runtime/wallpaper/native.rs': 'src/tohru_wallpaper/mod.rs',
    'runtime/wallpaper/presets.rs': 'src/tohru_wallpaper/presets.rs',
    'runtime/wallpaper/raster.rs': 'src/tohru_wallpaper/raster.rs',
    'runtime/wallpaper/preset.rs': 'foundation/runtime/wallpaper/preset.rs',
    'runtime/wallpaper/J2BAR-LICENSE': 'foundation/runtime/wallpaper/J2BAR-LICENSE',
    'tests/wallpaper/preset.rs': 'foundation/tests/wallpaper/preset.rs',
    'tests/wallpaper/presets.rs': 'tests/wallpaper/presets.rs',
    'tests/wallpaper/raster.rs': 'tests/wallpaper/raster.rs',
    'src/settings.rs': 'foundation/src/settings.rs',
    'tests/settings.rs': 'foundation/tests/settings.rs',
    'runtime/settings_luau.rs': 'src/tohru_settings.rs',
}
for original, installed in copies.items():
    assert (ROOT / original).read_bytes() == (source / installed).read_bytes(), installed
assert not (source / 'src/tohru_wallpaper.rs').exists()
assert 'Permission is hereby granted' in (source / 'foundation/runtime/wallpaper/J2BAR-LICENSE').read_text()
assert 'J2BAR-WALLPAPER-LICENSE' in nix and 'lib.licenses.bsd3 lib.licenses.mit' in nix
manifest = tomllib.loads((source / 'Cargo.toml').read_text())
lock = tomllib.loads((source / 'Cargo.lock').read_text())
assert manifest['dependencies']['thiserror'] == '=2.0.20'
assert 'thiserror 2.0.20' in next(p for p in lock['package'] if p['name'] == 'pleamar')['dependencies']
logic = (source / 'src/logic_luau.rs').read_text()
assert 'capture_query_mutates(name) || name == "wallpaper.preset"' in logic
assert 'check_service_permission(&c, &name, query_mutates(&name))?' in logic
permission = logic[logic.index('fn check_service_permission('):logic.index('pub(crate) fn say_allowed(')]
assert 'if commands {' in permission and 'has(name) || has(&format!("{service}.*"))' in permission
assert permission.index('if commands {') < permission.index('if has(service)')
bar = (ROOT / 'shell/bar.plm').read_text()
assert '"wallpaper", "wallpaper.preset"' in bar and '"wallpaper.*"' not in bar
assert 'size: 800, 800; keyboard: exclusive' in (ROOT / 'shell/settings/settings.plm').read_text()
assert 'schema = 7' in (ROOT / 'shell/bar.luau').read_text()
# Import provenance, not a recompilation of the reviewed W domain/cache code.
paths = set()
for commit in ['6f26c7d', 'ee51122', '5205be7', '9e60dc1']:
    paths.update(subprocess.check_output(['git', 'diff-tree', '--no-commit-id', '--name-only', '-r', commit], cwd=ROOT, text=True).splitlines())
for name in paths:
    assert (ROOT / name).read_bytes() == subprocess.check_output(['git', 'show', '9e60dc1:' + name], cwd=ROOT), name
print(f'PASS {len(patches)} explicit patches, {len(copies)} copies, module directory, canonical dependency/permission hooks, exact {len(paths)} W files')
print('Source assertions only: no Rust/permission/runtime/cache/PNG execution.')
