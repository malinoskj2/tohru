#!/usr/bin/env python3
"""CPU-only capture scene/compiler and actual Lua controller checks; never starts roles."""
import argparse
import os
import pathlib
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--pleamar", required=True, type=pathlib.Path)
parser.add_argument("--lua", required=True, type=pathlib.Path)
args = parser.parse_args()
root = pathlib.Path(__file__).resolve().parents[2]
evidence = root / ".tmp/check-results"
evidence.mkdir(parents=True, exist_ok=True)
env = os.environ.copy()
for key in ["DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS", "PLEAMAR_SOCKETS"]:
    env.pop(key, None)
for key, directory in [("HOME", "home"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_RUNTIME_DIR", "runtime")]:
    path = root / ".tmp/capture-ui-check" / directory
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    env[key] = str(path)
for command, name in [
    ([str(args.lua), "tests/capture/controller.lua"], "capture-controller.txt"),
    ([str(args.pleamar), "--check", "shell/capture/entry.plm"], "capture-scene-check.txt"),
    ([str(args.pleamar), "--luau-types", "shell/capture/entry.plm"], "capture-scene-types.d.luau"),
]:
    result = subprocess.run(command, cwd=root, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (evidence / name).write_text(result.stdout)
    print(f"{name}: exit {result.returncode}", flush=True)
    if result.returncode:
        print(result.stdout, flush=True)
    result.check_returncode()
print("No native display/protocol role was started; type generation does not analyze or execute Lua", flush=True)
