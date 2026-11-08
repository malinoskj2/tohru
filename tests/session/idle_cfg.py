#!/usr/bin/env python3
"""Compile actual idle patch callbacks with/without Luau; authority is a stub."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import time

ROOT=Path(__file__).resolve().parents[2]

def function(source,name):
    start=source.index('pub(crate) fn '+name+'(')
    opening=source.index('{',start);depth=1;end=opening+1
    while depth:
        if source[end]=='{':depth+=1
        elif source[end]=='}':depth-=1
        end+=1
    return source[start:end]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--security-source',type=Path,required=True)
    args=parser.parse_args()
    evidence=ROOT/'.tmp'/('idle-cfg-'+str(time.time_ns()));evidence.mkdir()
    for name in ['mod.rs','wayland.rs']:
        target=evidence/'source/src/platform'/name;target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copy2(args.security_source/'src/platform'/name,target)
    patch=ROOT/'runtime/idle/0004-idle-native.patch'
    subprocess.run(['git','apply','--check','--directory='+str((evidence/'source').relative_to(ROOT)),str(patch)],cwd=ROOT,check=True)
    subprocess.run(['git','apply','--directory='+str((evidence/'source').relative_to(ROOT)),str(patch)],cwd=ROOT,check=True)
    source=(evidence/'source/src/platform/wayland.rs').read_text()
    ack=function(source,'acknowledge_idle_sleep')
    wrapper=function((evidence/'source/src/platform/mod.rs').read_text(),'acknowledge_idle_sleep')
    match=re.search(r'e\.cycle\.confirm\(\);\s*(#\[cfg\(feature = "luau"\)\]\s*crate::tohru_idle::native_held\(\);)',source)
    assert match,'production held callback lost feature guard'
    fixture='''
#[cfg(feature="luau")]
mod tohru_idle {
 pub static CALLS:std::sync::atomic::AtomicUsize=std::sync::atomic::AtomicUsize::new(0);
 pub fn native_held(){CALLS.fetch_add(1,std::sync::atomic::Ordering::SeqCst);}
}
mod wayland {
 #[cfg(feature="luau")] use std::sync::{OnceLock,Mutex};
 #[cfg(feature="luau")] struct Cycle;
 #[cfg(feature="luau")] impl Cycle {fn held(&self)->bool{true}}
 #[cfg(feature="luau")] struct Lock;
 #[cfg(feature="luau")] impl Lock {fn is_locked(&self)->bool{true}}
 #[cfg(feature="luau")] struct Engaged {cycle:Cycle,lock:Lock}
 #[cfg(feature="luau")] struct Context {engaged:Mutex<Option<Engaged>>}
 #[cfg(feature="luau")] static LOCKS:OnceLock<Context>=OnceLock::new();
 ACK
 pub fn held_callback(){HELD}
 #[cfg(feature="luau")] pub fn acquire(){assert!(LOCKS.set(Context{engaged:Mutex::new(Some(Engaged{cycle:Cycle,lock:Lock}))}).is_ok());}
}
WRAPPER
fn main(){
 #[cfg(not(feature="luau"))] {assert!(!acknowledge_idle_sleep());wayland::held_callback();}
 #[cfg(feature="luau")] {assert!(!acknowledge_idle_sleep());wayland::acquire();assert!(acknowledge_idle_sleep());wayland::held_callback();assert_eq!(tohru_idle::CALLS.load(std::sync::atomic::Ordering::SeqCst),2);}
}
'''.replace('ACK',ack).replace('HELD',match[1]).replace('WRAPPER',wrapper)
    path=evidence/'fixture.rs';path.write_text(fixture)
    results=[]
    for enabled in [False,True]:
        executable=evidence/('luau' if enabled else 'no-luau')
        command=['rustc','--edition=2024',str(path),'-o',str(executable)]
        if enabled:command+=['--cfg','feature="luau"']
        subprocess.run(command,check=True,capture_output=True,text=True)
        subprocess.run([str(executable)],check=True,timeout=5)
        results.append({'feature_luau':enabled,'compiled':True,'ran':True})
    (evidence/'proof.json').write_text(json.dumps({'patch_sha256':hashlib.sha256(patch.read_bytes()).hexdigest(),'actual_patch_functions':True,'native_authority':'explicit std-only stub; no native runtime/display/bus/PAM','results':results},indent=2)+'\n')
    print(evidence)

if __name__=='__main__':main()
