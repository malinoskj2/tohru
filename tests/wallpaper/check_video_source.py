#!/usr/bin/env python3
"""Cheap cached-dependency rustc checks of actual policy/startup/report/wrapper; no Cargo or native graph/display/libmpv."""
import pathlib
import subprocess

root=pathlib.Path(__file__).resolve().parents[2]
deps=root / ".tmp/wallpaper-cpu/target/debug/deps"
evidence=root / ".tmp/check-results"
evidence.mkdir(parents=True,exist_ok=True)
def cached(crate,version):
    for dependency in sorted(deps.glob(crate+"-*.d")):
        library=dependency.with_name("lib"+dependency.stem+".rlib")
        if library.is_file() and "/"+crate+"-"+version+"/" in dependency.read_text():
            return library
    raise SystemExit(f"Missing cached {crate}{version} rlib; do not start dependency compilation without allocation")
thiserror=cached("thiserror","2.0.20")
anyhow=cached("anyhow","1.0.103")
def item(source,signature):
    start=source.index(signature)
    at=source.index("{",start)+1
    depth=1
    while depth:
        depth+=(source[at]=="{")-(source[at]=="}")
        at+=1
    return source[start:at]
def path_module(name,file,public=False):
    return '#[path="'+str(root / "runtime/video" / file)+'"] '+('pub ' if public else '')+'mod '+name+';\n'
source=(root / "runtime/video/native.rs").read_text()
report='''#![deny(unsafe_code)]
#![expect(dead_code,reason="report fixture omits renderer/actor lifetime calls")]
use std::collections::BTreeMap;
use std::sync::{Arc,Mutex,mpsc::Sender,atomic::{AtomicBool,AtomicUsize,Ordering}};
use anyhow::{Result,Context,bail};
use std::{path::PathBuf,ffi::OsString,thread,time::{Duration,Instant}};
#[derive(Debug)] enum ToRender{ExternalSignal(&'static str,Option<f32>),Quit}
#[derive(Debug)] enum SysValue{Text(String),Bool(bool),Num(f64),Map(Vec<(String,SysValue)>)}
'''+path_module("video","mod.rs",True)+"use video::{Snapshot,Protection,PauseReason,Backoff};\n"+path_module("startup","startup.rs")+path_module("wake","wake.rs")+"use wake::{Wake,Receiver};\n"
for signature in ["enum Phase {","enum PlayerState {"]:
    report+="#[derive(Clone,Copy,Debug,Default,PartialEq,Eq)]\n"+item(source,signature)+"\n"
for signature in ["struct Report {","struct OutputReport {"]:
    report+="#[derive(Clone,Debug,Default,PartialEq,Eq)]\n"+item(source,signature)+"\n"
report+=item(source,"struct Shared {")+"\n"+item(source,"impl Shared {")+"\n"
report+="struct Controller{shared:Arc<Shared>}\n"+item(source,"enum OwnerState {")+"\n"
start=source.index("static CONTROLLER:");report+=source[start:source.index(";",start)+1]+"\n"
for signature in ["pub(crate) fn query(","fn map(","fn number(","fn protection(","fn display_path(","fn phase(","fn player_state(","fn pause_reason(","fn message(","fn inactive_report(","fn supervise("]:
    report+=item(source,signature)+"\n"
report+=(root / "tests/wallpaper/video_report.rs").read_text()
checks={
    "policy":path_module("video","mod.rs",True),
    "startup":path_module("startup","startup.rs",True),
    "report":report,
    "mpv":'#![expect(dead_code,reason="actual wrapper methods before full native graph check")]\n'+path_module("wake","wake.rs")+"use wake::Wake;\n"+path_module("mpv","mpv.rs"),
}
for name,body in checks.items():
    fixture=root / ".tmp" / ("video-"+name+"-source.rs")
    binary=root / ".tmp" / ("video-"+name+"-source-tests")
    fixture.write_text(body)
    commands=[["rustc","--edition=2024","--test",str(fixture),"--extern","thiserror="+str(thiserror),"--extern","anyhow="+str(anyhow),"-L","dependency="+str(deps),"-C","codegen-units=1","-o",str(binary)],[str(binary)]]
    commands.append(["clippy-driver","--edition=2024","--test",str(fixture),"--emit=metadata","--extern","thiserror="+str(thiserror),"--extern","anyhow="+str(anyhow),"-L","dependency="+str(deps),"-C","codegen-units=1","-o",str(binary.with_suffix(".rmeta")),"-Wclippy::pedantic","-Wclippy::undocumented_unsafe_blocks","-Wclippy::multiple_unsafe_ops_per_block","-Wclippy::allow_attributes_without_reason","-Dwarnings"])
    for index,command in enumerate(commands):
        result=subprocess.run(command,cwd=root,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        (evidence / ("video-"+name+"-source-"+str(index)+".txt")).write_text("COMMAND: "+repr(command)+"\nEXIT: "+str(result.returncode)+"\n"+result.stdout)
        print(f"video-{name}-source-{index}: exit{result.returncode}",flush=True)
        if result.returncode:
            print(result.stdout,flush=True)
        result.check_returncode()
print("Cached-dependency source checks only; report uses explicit controller/renderer type fixtures, no Wayland/EGL/libmpv runtime",flush=True)
