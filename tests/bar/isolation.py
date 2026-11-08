#!/usr/bin/env python3
"""CPU-only hostile inherited-environment regression; never probes any endpoint."""
import sys
sys.dont_write_bytecode = True
from pathlib import Path
from native import private_environment, assert_private_ipc, bootstrap_private_bus

evidence = Path('/private/evidence')
runtime = Path('/private/runtime')
hostile = {key: '/outside/do-not-probe' for key in ('PLEAMAR_SOCKETS', 'WAYLAND_DISPLAY', 'SWAYSOCK', 'DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE', 'TOHRU_RUNTIME', 'TOHRU_SCENE', 'TOHRU_MANAGED_CONFIG', 'XDG_RUNTIME_DIR', 'HOME', 'WAYLAND_SOCKET', 'CUA_INJECT_SOCKET', 'DBUS_STARTER_ADDRESS', 'DBUS_STARTER_BUS_TYPE')}
env = private_environment(hostile, evidence, runtime)
assert env['PLEAMAR_SOCKETS'] == '/private/runtime/ipc'
assert env['XDG_RUNTIME_DIR'] == '/private/runtime'
assert env['HOME'] == '/private/evidence/home'
for key in ('WAYLAND_DISPLAY', 'SWAYSOCK', 'DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE', 'TOHRU_RUNTIME', 'TOHRU_SCENE', 'TOHRU_MANAGED_CONFIG', 'WAYLAND_SOCKET', 'CUA_INJECT_SOCKET', 'TOHRU_B_PRIVATE_BUS', 'DBUS_STARTER_ADDRESS', 'DBUS_STARTER_BUS_TYPE'):
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

# Exercise the actual bootstrap before any spawn. An inherited marker can never
# bypass creation, and the new child must match its private address+token.
spawns = []
def capture(command, **options):
    spawns.append((command, options))
    assert options['close_fds']
    assert 'DBUS_SESSION_BUS_ADDRESS' not in options['env']
    assert 'TOHRU_B_PRIVATE_BUS' not in options['env']
    assert 'WAYLAND_SOCKET' not in options['env']
    assert 'CUA_INJECT_SOCKET' not in options['env']
    directory = Path(command[command.index('--bus-child') + 1])
    token = command[command.index('--bus-token') + 1]
    private_address = 'unix:path=' + str(directory / 'bus') + ',guid=0123456789abcdef'
    assert not bootstrap_private_bus(directory, token, {'DBUS_SESSION_BUS_ADDRESS': private_address}, [], run=capture)
    for bad in ({'DBUS_SESSION_BUS_ADDRESS': 'unix:path=/outside/do-not-probe'}, {'DBUS_SESSION_BUS_ADDRESS': private_address + ';unix:path=/outside/do-not-probe'}, {'DBUS_SESSION_BUS_ADDRESS': 'unix:path=/outside/do-not-probe,' + private_address}):
        try:
            bootstrap_private_bus(directory, token, bad, [], run=capture)
        except AssertionError:
            pass
        else:
            raise AssertionError('outside session bus accepted')
    try:
        bootstrap_private_bus(directory, 'wrong-token', {'DBUS_SESSION_BUS_ADDRESS': private_address}, [], run=capture)
    except AssertionError:
        pass
    else:
        raise AssertionError('inherited provenance accepted')

assert bootstrap_private_bus(None, None, {**hostile, 'TOHRU_B_PRIVATE_BUS': '1', 'DBUS_SESSION_BUS_ADDRESS': 'unix:path=/outside/do-not-probe', 'WAYLAND_SOCKET': '9'}, ['--runtime', '/not-executed'], run=capture)
assert len(spawns) == 1
print('Actual bootstrap rejects inherited marker and outside bus/FD/injection selectors before spawn')
