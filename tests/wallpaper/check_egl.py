#!/usr/bin/env python3
"""ALLOCATED SLOT ONLY: compile actual EGL/MPV boundary against pinned native graph, never start GL/decoder/display."""
import pathlib
import subprocess

root=pathlib.Path(__file__).resolve().parents[2]
work=root / ".tmp/video-egl-check"
(work / "src").mkdir(parents=True,exist_ok=True)
evidence=root / ".tmp/check-results"
evidence.mkdir(parents=True,exist_ok=True)
source=(root / "tests/wallpaper/egl_fixture.rs.in").read_text()
for token,file in [("WAKE_SOURCE","wake.rs"),("MPV_SOURCE","mpv.rs"),("EGL_SOURCE","egl.rs")]:
    source=source.replace(token,str(root / "runtime/video" / file))
(work / "src/lib.rs").write_text(source)
(work / "Cargo.toml").write_text('''[package]
name="tohru-video-egl-check"
version="0.0.0"
edition="2024"
rust-version="1.88"
[dependencies]
thiserror="=2.0.20"
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
    result=subprocess.run(command,cwd=root,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (evidence / name).write_text("COMMAND: "+repr(command)+"\nEXIT: "+str(result.returncode)+"\n"+result.stdout)
    print(f"{name}: exit{result.returncode}",flush=True)
    if result.returncode:
        print(result.stdout,flush=True)
    result.check_returncode()
check(["pkg-config","--modversion","wayland-client","wayland-egl","xkbcommon"],"video-egl-sdk.txt")
check(["cargo","generate-lockfile","--offline","--manifest-path",str(work / "Cargo.toml")],"video-egl-lock.txt")
base=["--offline","--manifest-path",str(work / "Cargo.toml"),"--target-dir",str(root / ".tmp/wallpaper-cpu/target"),"--locked","--lib","--jobs","1"]
check(["cargo","check",*base],"video-egl-check.txt")
check(["cargo","clippy",*base,"--","-D","warnings"],"video-egl-clippy.txt")
print("Compile-only dependency graph checked; no display/EGL context/libmpv/decoder/role was started",flush=True)
