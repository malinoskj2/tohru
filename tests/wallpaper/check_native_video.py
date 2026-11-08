#!/usr/bin/env python3
"""SEPARATE ALLOCATED SLOT ONLY: compile full actual video actor with pinned native graph; no link/run/native library loading."""
import pathlib
import subprocess
import os
import hashlib
root=pathlib.Path(__file__).resolve().parents[2]
work=root / ".tmp/video-owner-check"
(work / "src").mkdir(parents=True,exist_ok=True)
evidence=root / ".tmp/check-results"
evidence.mkdir(parents=True,exist_ok=True)
# Enforced for the future allocated check; constructing this runner is not a grant.
environment=os.environ.copy()
cargo_home=environment.get("CARGO_HOME",str(pathlib.Path.home() / ".cargo"))
for name in ("DISPLAY","WAYLAND_DISPLAY","WAYLAND_SOCKET","DBUS_SESSION_BUS_ADDRESS","DBUS_SYSTEM_BUS_ADDRESS","PLEAMAR_SOCKETS"):
    environment.pop(name,None)
private=root / ".tmp/video-owner-environment"
for name in ("HOME","XDG_CONFIG_HOME","XDG_CACHE_HOME","XDG_DATA_HOME","XDG_RUNTIME_DIR"):
    directory=private / name.lower()
    directory.mkdir(parents=True,exist_ok=True,mode=0o700)
    environment[name]=str(directory)
environment["CARGO_HOME"]=cargo_home
inputs=sorted((root / "runtime/video").glob("*.rs"))+[root / "tests/wallpaper/native_fixture.rs.in",pathlib.Path(__file__)]
(evidence / "video-owner-source-hashes.txt").write_text("".join(hashlib.sha256(path.read_bytes()).hexdigest()+"  "+str(path)+"\n" for path in inputs))
source=(root / "tests/wallpaper/native_fixture.rs.in").read_text().replace("POLICY_SOURCE",str(root / "runtime/video/mod.rs")).replace("NATIVE_SOURCE",str(root / "runtime/video/native.rs"))
(work / "src/lib.rs").write_text(source)
(work / "Cargo.toml").write_text('''[package]
name="tohru-video-owner-check"
version="0.0.0"
edition="2024"
rust-version="1.88"
[dependencies]
thiserror="=2.0.20"
anyhow="=1.0.103"
libc="=0.2.189"
khronos-egl={version="=6.0.0",features=["dynamic"]}
smithay-client-toolkit="=0.21.1"
wayland-client={version="=0.31.15",features=["system"]}
wayland-protocols={version="=0.32.13",features=["client","staging","unstable"]}
wayland-egl="=0.32.11"
[lints.rust]
unsafe_code="deny"
[lints.clippy]
pedantic={level="warn",priority=-1}
undocumented_unsafe_blocks="warn"
multiple_unsafe_ops_per_block="warn"
allow_attributes_without_reason="warn"
''')
def check(command,name):
    result=subprocess.run(command,cwd=root,env=environment,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (evidence / name).write_text("COMMAND: "+repr(command)+"\nEXIT: "+str(result.returncode)+"\n"+result.stdout)
    print(f"{name}: exit{result.returncode}",flush=True)
    if result.returncode:print(result.stdout,flush=True)
    result.check_returncode()
check(["cargo","generate-lockfile","--offline","--manifest-path",str(work / "Cargo.toml")],"video-owner-lock.txt")
base=["--offline","--manifest-path",str(work / "Cargo.toml"),"--target-dir",str(root / ".tmp/wallpaper-cpu/target"),"--locked","--lib","--jobs","1"]
check(["cargo","check",*base],"video-owner-check.txt")
check(["cargo","clippy",*base,"--","-D","warnings"],"video-owner-clippy.txt")
print("Actual actor typecheck only; framework types remain fixtures, no linked/executed libmpv/EGL/Wayland/native role",flush=True)
