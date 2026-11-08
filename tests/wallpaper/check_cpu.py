#!/usr/bin/env python3
"""Actual scoped wallpaper catalog/identity adapters and fixed-sibling probe, CPU only."""
import json
import pathlib
import subprocess
import tempfile

root = pathlib.Path(__file__).resolve().parents[2]
work = root / ".tmp/wallpaper-picker-cpu"
(work / "src").mkdir(parents=True, exist_ok=True)
evidence = root / ".tmp/check-results"
evidence.mkdir(parents=True, exist_ok=True)
(work / "src/lib.rs").write_text('''extern crate self as tohru_shell;
#[derive(Debug)] pub enum SysValue {Text(String),List(Vec<SysValue>)}
pub mod platform {pub use crate::SysValue;}
#[path=WALLPAPER] pub mod wallpaper;
#[path=NATIVE] mod native;
#[expect(clippy::missing_errors_doc,reason="ephemeral actual adapter entry")]
pub fn query(name:&str,args:&[SysValue])->Result<SysValue,String>{native::query(name,args)}
'''.replace("WALLPAPER", json.dumps(str(root / "runtime/wallpaper/mod.rs"))).replace("NATIVE", json.dumps(str(root / "runtime/wallpaper/native.rs"))))
(work / "src/query.rs").write_text('''use tohru_shell::{SysValue,query};
fn main(){
 let args:Vec<String>=std::env::args().skip(1).collect();
 let (name,values)=if args.first().is_some_and(|s|s=="--identities") {
  ("wallpaper.identities",vec![SysValue::List(args.into_iter().skip(1).map(SysValue::Text).collect())])
 } else {("wallpaper.catalog",args.into_iter().map(SysValue::Text).collect())};
 match query(name,&values){
  Ok(SysValue::List(values))=>{let strings:Vec<_>=values.into_iter().map(|v|match v{SysValue::Text(s)=>s,_=>panic!("text result")}).collect();println!("{}",serde_json::to_string(&strings).unwrap());},
  other=>{eprintln!("{other:?}");std::process::exit(1);}
 }
}
''')
(work / "Cargo.toml").write_text('''[package]
name="tohru-wallpaper-cpu-check"
version="0.0.0"
edition="2024"
rust-version="1.85"
[lib]
name="tohru_shell"
[dependencies]
thiserror="=2.0.20"
anyhow="=1.0.103"
serde_json="=1.0.151"
libc="=0.2.189"
[lints.rust]
unsafe_code="deny"
unreachable_pub="warn"
[lints.clippy]
pedantic={level="warn",priority=-1}
undocumented_unsafe_blocks="warn"
multiple_unsafe_ops_per_block="warn"
allow_attributes_without_reason="warn"
[[bin]]
name="tohru-wallpaper-probe"
path=PROBE
[[bin]]
name="wallpaper-query-fixture"
path="src/query.rs"
'''.replace("PROBE", json.dumps(str(root / "runtime/wallpaper/probe.rs"))))
target = root / ".tmp/wallpaper-cpu/target"
def check(command, name):
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    (evidence / name).write_text(result.stdout)
    print(f"{name}: exit {result.returncode}", flush=True)
    if result.returncode:
        print(result.stdout, flush=True)
    result.check_returncode()
base=["--offline", "--manifest-path", str(work / "Cargo.toml"), "--target-dir", str(target)]
check(["cargo", "generate-lockfile", "--offline", "--manifest-path", str(work / "Cargo.toml")], "wallpaper-cpu-lock.txt")
check(["cargo", "test", *base, "--locked", "--lib", "--jobs", "1", "--", "--test-threads=1"], "wallpaper-cpu-tests.txt")
check(["cargo", "clippy", *base, "--locked", "--lib", "--bin", "tohru-wallpaper-probe", "--jobs", "1", "--", "-D", "warnings"], "wallpaper-cpu-clippy.txt")
check(["cargo", "build", *base, "--locked", "--bins", "--jobs", "1"], "wallpaper-cpu-build.txt")
with tempfile.TemporaryDirectory(prefix="wallpaper-query-", dir=root / ".tmp") as folder:
    folder=pathlib.Path(folder)
    file=folder / "a.png"
    file.write_bytes(b"metadata fixture")
    alias=folder / "alias.png"
    alias.symlink_to(file)
    exe=str(target / "debug/wallpaper-query-fixture")
    catalog=subprocess.run([exe,str(folder)], capture_output=True, text=True, check=True)
    assert json.loads(catalog.stdout)==[str(file)], catalog
    identities=subprocess.run([exe,"--identities",str(alias),str(file)], capture_output=True, text=True, check=True)
    assert json.loads(identities.stdout)==[str(file),str(file)], identities
    missing=subprocess.run([exe,"--identities",str(folder / "missing.png")],capture_output=True,text=True)
    assert missing.returncode != 0 and not missing.stdout and missing.stderr, missing
    (evidence / "wallpaper-sibling-query.txt").write_text(catalog.stdout+identities.stdout+missing.stderr)
print("Actual fixed-sibling query→probe catalog/identity/missing-file checks passed; no display/decoder/role started",flush=True)
