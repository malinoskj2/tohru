#!/usr/bin/env python3
"""CPU-only hostile inherited-environment regression; never probes any endpoint."""
from pathlib import Path
from native import private_environment, assert_private_ipc

evidence = Path('/private/evidence')
runtime = Path('/private/runtime')
hostile = {key: '/outside/do-not-probe' for key in ('PLEAMAR_SOCKETS', 'WAYLAND_DISPLAY', 'SWAYSOCK', 'DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE', 'TOHRU_RUNTIME', 'TOHRU_SCENE', 'TOHRU_MANAGED_CONFIG', 'XDG_RUNTIME_DIR', 'HOME')}
env = private_environment(hostile, evidence, runtime)
assert env['PLEAMAR_SOCKETS'] == '/private/runtime/ipc'
assert env['XDG_RUNTIME_DIR'] == '/private/runtime'
assert env['HOME'] == '/private/evidence/home'
for key in ('WAYLAND_DISPLAY', 'SWAYSOCK', 'DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE', 'TOHRU_RUNTIME', 'TOHRU_SCENE', 'TOHRU_MANAGED_CONFIG'):
    assert key not in env, key
assert hostile['PLEAMAR_SOCKETS'] == '/outside/do-not-probe'
print('Hostile inherited selectors overridden; no endpoints probed')

# Reject an overridden route and a symlink endpoint without contacting either.
import tempfile
root = Path(__file__).resolve().parents[2]
(root / '.tmp').mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix='b-isolation-', dir=root / '.tmp') as temporary:
    private = Path(temporary)
    ipc = private / 'ipc'
    ipc.mkdir()
    env = private_environment(hostile, evidence, private)
    assert_private_ipc(env, private)
    for replacement in ('/outside/do-not-probe', str(private / 'other')):
        try:
            assert_private_ipc({**env, 'PLEAMAR_SOCKETS': replacement}, private)
        except AssertionError:
            pass
        else:
            raise AssertionError('outside socket directory accepted')
    (ipc / 'hostile.sock').symlink_to('/outside/do-not-probe.sock')
    try:
        assert_private_ipc(env, private)
    except AssertionError:
        pass
    else:
        raise AssertionError('outside socket endpoint accepted')
print('Outside directory and symlink endpoints rejected before invocation')
