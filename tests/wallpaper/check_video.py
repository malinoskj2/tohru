#!/usr/bin/env python3
"""Compile-only libmpv SDK ABI and actual wrapper tests using fake foreign symbols; no native/decoder role."""
import argparse
import pathlib
import subprocess

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("--headers",required=True,type=pathlib.Path,help="selected libmpv0.41 include directory")
args=parser.parse_args()
root=pathlib.Path(__file__).resolve().parents[2]
work=root / ".tmp/video-cpu"
(work / "src").mkdir(parents=True,exist_ok=True)
evidence=root / ".tmp/check-results"
evidence.mkdir(parents=True,exist_ok=True)
(work / "Cargo.toml").write_text('''[package]
name="tohru-video-cpu-check"
version="0.0.0"
edition="2024"
rust-version="1.88"
[dependencies]
thiserror="=2.0.20"
[lints.rust]
unsafe_code="deny"
[lints.clippy]
pedantic={level="warn",priority=-1}
undocumented_unsafe_blocks="warn"
multiple_unsafe_ops_per_block="warn"
allow_attributes_without_reason="warn"
''')
(work / "src/lib.rs").write_text('''#![expect(dead_code,reason="actual native owner methods await outer EGL/Wayland integration")]
#[path="'''+str(root / "runtime/video/wake.rs")+'''"] mod wake;
use wake::Wake;
#[path="'''+str(root / "runtime/video/mpv.rs")+'''"] mod mpv;
''')
def check(command,name):
    result=subprocess.run(command,cwd=root,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    (evidence / name).write_text(result.stdout)
    print(f"{name}: exit{result.returncode}",flush=True)
    if result.returncode:
        print(result.stdout,flush=True)
    result.check_returncode()
check(["cc","-std=c11","-Wall","-Wextra","-Werror","-fsyntax-only","-I"+str(args.headers),"tests/wallpaper/mpv-abi.c"],"video-mpv-header-abi.txt")
base=["--offline","--manifest-path",str(work / "Cargo.toml"),"--target-dir",str(root / ".tmp/wallpaper-cpu/target"),"--jobs","1"]
check(["cargo","generate-lockfile","--offline","--manifest-path",str(work / "Cargo.toml")],"video-mpv-cpu-lock.txt")
check(["cargo","test",*base,"--locked","--lib"],"video-mpv-cpu-tests.txt")
check(["cargo","clippy",*base,"--locked","--tests","--","-D","warnings"],"video-mpv-cpu-clippy.txt")
print("Actual wrapper uses fake SDK symbols; selected real headers are compile-checked only, no libmpv/GL role started",flush=True)
