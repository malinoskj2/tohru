#!/usr/bin/env python3
"""The foundation crate builds from the runtime vendor graph; every registry pin must exist."""
from pathlib import Path
import tomllib

ROOT = Path(__file__).resolve().parents[1]
foundation = tomllib.loads((ROOT / 'runtime/Cargo.lock').read_text())['package']
runtime = tomllib.loads((ROOT / 'runtime/pleamar/Cargo.lock').read_text())['package']
keys = {(p['name'], p['version'], p.get('source'), p.get('checksum')) for p in runtime}
for package in foundation:
    key = (package['name'], package['version'], package.get('source'), package.get('checksum'))
    assert key in keys, f'foundation package absent from runtime vendor lock: {key}'
recipe = (ROOT / 'nix/default.nix').read_text()
assert 'cargoLock.lockFile = ../runtime/pleamar/Cargo.lock' in recipe
print(f'PASS all {len(foundation)} foundation package identities exist in runtime vendor lock')
