#!/usr/bin/env python3
"""CPU-only actual capture adapters/helper/parser checks; never starts a display or clipboard owner."""
import argparse
import json
import os
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source", type=pathlib.Path, help="also apply/check native patches against this pinned source")
args = parser.parse_args()
work = ROOT / ".tmp/capture-cpu"
(work / "src").mkdir(parents=True, exist_ok=True)
evidence = ROOT / ".tmp/check-results"
evidence.mkdir(parents=True, exist_ok=True)

BODY = (ROOT / "tests/capture/native_fixture.rs.in").read_text()
BODY = BODY.replace("ROOT_CAPTURE_SOURCE", str(ROOT / "runtime/capture/mod.rs"))
BODY = BODY.replace("NATIVE_CAPTURE_SOURCE", str(ROOT / "runtime/capture/native.rs"))
(work / "src/lib.rs").write_text(BODY)
manifest = """[package]
name="tohru-capture-cpu-check"
version="0.0.0"
edition="2024"
rust-version="1.88"
[lib]
name="tohru_shell"
[dependencies]
thiserror="=2.0.20"
anyhow="=1.0.103"
chrono={version="=0.4.45",default-features=false,features=["clock"]}
libc="=0.2.189"
image={version="=0.25.10",default-features=false,features=["png"]}
wl-clipboard-rs="=0.9.3"
[lints.rust]
unsafe_code="deny"
unreachable_pub="warn"
[lints.clippy]
pedantic={level="warn",priority=-1}
undocumented_unsafe_blocks="warn"
multiple_unsafe_ops_per_block="warn"
allow_attributes_without_reason="warn"
[[bin]]
name="tohru-capture-clipboard"
path=CLIPBOARD_SOURCE
""".replace("CLIPBOARD_SOURCE", json.dumps(str(ROOT / "runtime/capture/clipboard.rs")))
(work / "Cargo.toml").write_text(manifest)

def check(command, name):
    result = subprocess.run(command, cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (evidence / name).write_text(result.stdout)
    print(f"{name}: exit {result.returncode}", flush=True)
    if result.returncode:
        print(result.stdout, flush=True)
    result.check_returncode()

base = ["--offline", "--manifest-path", str(work / "Cargo.toml"), "--target-dir", str(ROOT / ".tmp/wallpaper-cpu/target")]
check(["cargo", "generate-lockfile", "--offline", "--manifest-path", str(work / "Cargo.toml")], "capture-cpu-lock.txt")
check(["cargo", "test", *base, "--locked", "--lib", "--jobs", "1"], "capture-cpu-tests.txt")
check(["cargo", "clippy", *base, "--locked", "--lib", "--tests", "--bin", "tohru-capture-clipboard", "--jobs", "1", "--", "-D", "warnings"], "capture-cpu-clippy.txt")
check(["cargo", "build", *base, "--locked", "--bin", "tohru-capture-clipboard", "--jobs", "1"], "capture-clipboard-build.txt")
# Invalid PNG and arguments fail before the helper reaches any Wayland operation.
env = os.environ.copy()
for key in ["DISPLAY", "WAYLAND_DISPLAY", "WAYLAND_SOCKET", "DBUS_SESSION_BUS_ADDRESS", "DBUS_SYSTEM_BUS_ADDRESS", "PLEAMAR_SOCKETS"]:
    env.pop(key, None)
for key, folder in [("HOME", "home"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_RUNTIME_DIR", "runtime")]:
    target = work / folder
    target.mkdir(exist_ok=True, mode=0o700)
    env[key] = str(target)
helper = ROOT / ".tmp/wallpaper-cpu/target/debug/tohru-capture-clipboard"
for extra in [[], ["unexpected"]]:
    result = subprocess.run([str(helper), *extra], input=b"bad PNG", env=env, capture_output=True, timeout=3)
    assert result.returncode != 0 and b"READY" not in result.stdout and result.stderr, result
print("Actual clipboard helper parser rejects invalid PNG/arguments before native connection", flush=True)
if args.source:
    check(["python3", str(ROOT / "tests/capture/native-source.py"), "--source", str(args.source.resolve())], "capture-native-source.txt")
print("CPU checks complete; native protocol/clipboard/GPU checks remain unverified", flush=True)
