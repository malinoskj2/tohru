"""CPU contract using the pin's actual Animated/Every implementations.

This isolates native math/appointment code; it is not a compositor/role/GPU test.
Full production scene syntax is separately checked by the actual native parser.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser=argparse.ArgumentParser()
parser.add_argument('--source',type=Path,required=True)
args=parser.parse_args()
root=Path(__file__).resolve().parents[2]
source=(args.source/'src/scene.rs').read_text()
render=(args.source/'src/render.rs').read_text()
start=source.index('#[derive(Clone, Copy, Debug)]\npub struct Spring')
spring=source[start:source.index('pub enum Command',start)]
start=source.index('#[derive(Clone, Copy)]\npub struct Animated')
animated=source[start:source.index('#[cfg(test)]',start)]
start=render.index('                    Trigger::Every { between, during } => {')
end=render.index('\n                    Trigger::On(_)',start)
every=render[start:end].split('=>',1)[1].strip().removesuffix(',')
assert 'e.next = Some(Instant::now() + Duration::from_secs_f32(rng.between(between.0, between.1)))' in render
assert render.index('// ── 5. is anything still moving?') > render.index('a.step(dt)') > render.index('Trigger::Every { between, during } =>')
scene=(root/'shell/launcher/entry.plm').read_text()
assert 'every 16.666667ms while launcher_closing while launcher_seen == 0 and vel(launcher_seen) == 0' in scene
assert 'emit launcher_unmap(launcher_close_token)' in scene
assert 'launcher_closing = false\n            emit launcher_unmap' not in scene
fixture=r'''
use std::time::{Duration, Instant};
struct During(bool);
impl During {fn is_true(&self,_:())->bool{self.0}}
struct FixedRange;
impl FixedRange {fn between(&mut self,a:f32,b:f32)->f32{assert_eq!(a,b);a}}
struct Timer {next:Option<Instant>}
fn every(e:&mut Timer,during:bool,now:Instant,appointments:&mut Vec<Instant>)->bool {
 let during=During(during);let between=(1.0/60.0,1.0/60.0);let c=();let mut rng=FixedRange;
 NATIVE_EVERY
}
fn spring()->Animated {Animated::at(0.0,Spring{stiffness:170.0,damping:13.04})}
fn step(a:&mut Animated,dt:f32){a.step(dt);if a.at_rest(){a.settle();}}
#[test]
fn final_native_snap_has_followup_then_timer_cancels() {
 for hz in [60_u32,144,240] {
  let dt=1.0/(hz as f32);let start=Instant::now();let mut a=spring();a.target=1.0;
  for _ in 0..hz*3 {step(&mut a,dt);}
  assert_eq!((a.x,a.v),(1.0,0.0));a.target=0.0;
  let mut e=Timer{next:Some(start+Duration::from_secs_f32(1.0/60.0))};let mut closing=true;let mut mapped=true;let mut rest_seen=false;
  for frame in 1..hz*5 {
   let now=start+Duration::from_secs_f32((frame as f32)*dt);let mut appointments=Vec::new();
   if every(&mut e,closing,now,&mut appointments) && a.x==0.0 && a.v==0.0 {assert!(rest_seen);mapped=false;closing=false;}
   step(&mut a,dt);rest_seen|=a.x==0.0&&a.v==0.0;
   if !mapped {assert!(!closing);assert!(!every(&mut e,false,now+Duration::from_secs(10),&mut Vec::new()));break;}
   assert!(!appointments.is_empty());
  }
  assert!(!mapped);
  let mut appointments=Vec::new();every(&mut e,false,start+Duration::from_secs(30),&mut appointments);assert!(appointments.is_empty());
 }
}
#[test]
fn initial_closed_coalesced_close_reopen_and_rule_reload_keep_ownership() {
 let now=Instant::now();let mut e=Timer{next:Some(now)};let mut appointments=Vec::new();
 assert!(!every(&mut e,false,now,&mut appointments));assert!(appointments.is_empty());
 let a=spring();let due=now+Duration::from_secs(1);
 assert!(every(&mut e,true,due,&mut appointments));assert_eq!((a.x,a.v),(0.0,0.0));
 // Reopen clears closing before a stale pending check: no fire/no appointment.
 appointments.clear();assert!(!every(&mut e,false,due+Duration::from_secs(1),&mut appointments));assert!(appointments.is_empty());
 // Native scene rule rebuild initializes Every.next; closing VM/rule reload
 // cannot depend on first observing a false condition to arm its appointment.
 e=Timer{next:Some(due+Duration::from_secs_f32(1.0/60.0))};
 appointments.clear();assert!(!every(&mut e,true,due,&mut appointments));assert_eq!(appointments.len(),1);
}
'''.replace('NATIVE_EVERY',every)
folder=root/'.tmp/launcher-native-phase';folder.mkdir(exist_ok=True)
file=folder/'native.rs';file.write_text(spring+animated+fixture)
subprocess.run(['rustc','--edition','2024','--test','-Dwarnings',str(file),'-o',str(folder/'tests')],check=True)
subprocess.run([str(folder/'tests')],check=True)
(folder/'source-hashes.json').write_text(json.dumps({name:hashlib.sha256(text.encode()).hexdigest() for name,text in [('Spring',spring),('Animated',animated),('Every',every)]},indent=2)+'\n')
print('Actual pinned native spring/Every isolated phase CPU contract passed; role/GPU behavior still separate')
