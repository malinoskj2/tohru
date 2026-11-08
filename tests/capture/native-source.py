#!/usr/bin/env python3
"""Compile actual patched native capture permission/cache methods with explicit I/O/type fixtures."""
import argparse
import pathlib
import shutil
import subprocess
import tempfile


def function(source, signature):
    start = source.index(signature)
    at = source.index("{", start) + 1
    depth = 1
    while depth:
        depth += (source[at] == "{") - (source[at] == "}")
        at += 1
    return source[start:at]


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source", required=True, type=pathlib.Path)
args = parser.parse_args()
root = pathlib.Path(__file__).resolve().parents[2]
(root / ".tmp").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix="capture-native-source-", dir=root / ".tmp") as temp:
    temp = pathlib.Path(temp)
    (temp / "src/platform").mkdir(parents=True)
    for relative in ["src/text.rs", "src/scene.rs", "src/render.rs", "src/logic_luau.rs", "src/platform/capture.rs", "src/platform/wayland.rs"]:
        shutil.copyfile(args.source / relative, temp / relative)
    for patch in ["runtime/wallpaper/image-reset.patch", "runtime/capture/native.patch", "runtime/capture/image-lifecycle.patch", "runtime/capture/output-events.patch", "runtime/capture/layout-link.patch"]:
        subprocess.run(["git", "apply", "--unsafe-paths", f"--directory={temp}", str(root / patch)], check=True)
    wayland = (temp / "src/platform/wayland.rs").read_text()
    handlers = wayland[wayland.index("impl OutputHandler for State"):]
    for method in ["new_output", "update_output", "output_destroyed"]:
        body = function(handlers, "fn " + method + "(")
        assert body.split("{", 1)[1].lstrip().startswith("let _ = self.to_render.send(ToRender::CaptureOutputChanged);")
    renderer = (temp / "src/render.rs").read_text()
    assert 'scene.signals.iter().any(|&(name, outgoing)| name == "capture_output_changed" && outgoing)' in renderer
    print("Native topology hooks cover add/update/remove before early returns; scene subscription guarded", flush=True)
    capture = (temp / "src/platform/capture.rs").read_text()
    assert "require_copy" not in function(capture, "fn connect(")
    for method in ["pixel", "freeze"]:
        body = function(capture, "fn " + method + "(")
        assert "self.require_copy()?;" in body and body.index("self.require_copy()?;") < body.index("self.output(")
    print("Readonly topology connects without capture protocol; pixel/freeze require copy before protocol use", flush=True)
    logic = (temp / "src/logic_luau.rs").read_text()
    assert 'check_service_permission(&c, &name, capture_query_mutates(&name))?' in logic
    permission = r'''
use std::sync::Mutex;
mod mlua {pub type Result<T> = std::result::Result<T,String>;}
struct Permissions {services:Vec<String>}
struct Shared {permissions:Permissions}
fn denied(_: &Mutex<Shared>,_:&str,_:&str)->String{"denied".into()}
'''
    permission += "\nstruct CaptureState{shm:Option<()>,wlr:Option<()>,sources:Option<()>,copies:Option<()>}\nstruct Link{state:CaptureState}\nenum Asked{No(String)}\nimpl Link{\n"
    permission += function(capture, "fn require_copy(") + "\n}\n"
    permission += r'''
#[test]fn copy_protocol_guard_rejects_missing_shm_or_both_copy_backends(){
 for bits in 0..16 {
  let yes=|bit| if bits & bit != 0 {Some(())}else{None};
  let link=Link{state:CaptureState{shm:yes(1),wlr:yes(2),sources:yes(4),copies:yes(8)}};
  let expected=bits&1!=0&&(bits&2!=0||(bits&4!=0&&bits&8!=0));
  match link.require_copy(){Ok(())=>assert!(expected),Err(Asked::No(why))=>{assert!(!expected);assert!(why.contains("copy the screen"));}}
 }
}
'''
    permission += function(logic, "fn capture_query_mutates(") + "\n" + function(logic, "fn check_service_permission(")
    permission += r'''
fn can(declared:&[&str],name:&str)->bool{
 let c=Mutex::new(Shared{permissions:Permissions{services:declared.iter().map(|s|s.to_string()).collect()}});
 check_service_permission(&c,name,capture_query_mutates(name)).is_ok()
}
#[test]fn readonly_capture_grant_does_not_gain_save_copy_or_release(){
 assert!(can(&["capture"],"capture.outputs"));assert!(can(&["capture"],"capture.freeze"));
 assert!(!can(&["capture"],"capture.region"));assert!(!can(&["capture"],"capture.release"));
 assert!(can(&["capture.region"],"capture.region"));assert!(can(&["capture.release"],"capture.release"));
 assert!(can(&["capture.*"],"capture.region"));assert!(can(&["capture.*"],"capture.outputs"));
}
#[test]fn catalog_query_uses_native_base_read_permission(){
 assert!(!can(&["wallpaper.catalog"],"wallpaper.catalog"));assert!(can(&["wallpaper"],"wallpaper.catalog"));
}
'''
    (temp / "permission.rs").write_text(permission)
    subprocess.run(["rustc", "--edition=2024", "--test", str(temp / "permission.rs"), "-o", str(temp / "permission-tests")], check=True)
    subprocess.run([str(temp / "permission-tests")], check=True)

    source = (temp / "src/text.rs").read_text()
    cache = r'''
#![allow(dead_code)]
use std::collections::{HashMap,HashSet};
use std::sync::{Arc,Mutex,mpsc::{channel,Sender}};
mod scene {#[derive(Clone)]pub struct TextId(pub u16);}
#[derive(Clone,Copy,Debug)]struct AtlasSlot{x:u32,y:u32,width:u32,height:u32,page:u32}
#[derive(Clone)]enum ImageSource{Live(scene::TextId),File}
type LiveKey=(String,(u32,u32));
enum WorkingKey{Live(LiveKey)}
#[derive(Clone,Default,PartialEq,Eq)]struct WorkingSet{live:HashSet<LiveKey>}
enum Job{Live(String,(u32,u32),u32),Reload(String,(u32,u32),AtlasSlot,u32)}
mod platform{
 use super::*;
 pub struct ThumbnailFrame{pub version:u64,pub width:u32,pub height:u32,pub pixels:Vec<u8>,pub opaque:bool}
 pub static FRAMES:Mutex<Option<HashMap<String,Arc<ThumbnailFrame>>>>=Mutex::new(None);
 pub fn thumbnail_frame(name:&str)->Option<Arc<ThumbnailFrame>>{FRAMES.lock().unwrap().as_ref()?.get(name).cloned()}
}
struct Snapshot{layouts:HashMap<String,Arc<()>>,images:Vec<Option<AtlasSlot>>,animations:Vec<Option<()>>,live:HashMap<LiveKey,(Option<AtlasSlot>,String)>}
enum Delivery{Layout{key:String,layout:Arc<()>,generation:u32},Images{slots:Vec<Option<AtlasSlot>>,animations:Vec<Option<()>>,generation:u32},
 Live{name:String,size:(u32,u32),slot:Option<AtlasSlot>,generation:u32},Rebuilt{snapshot:Snapshot,working:WorkingSet,generation:u32}}
struct Parcel{delivery:Delivery,pending_upload:Vec<()>,pending_fit:Vec<()>,pending_pictures:Vec<(u32,(u32,u32),Vec<u8>)>,exhausted:bool,height:u32,most:u32}
struct Shelves{most:u32}
struct Typesetter{last_picture:u32,pending_pictures:Vec<(u32,(u32,u32),Vec<u8>)>,shelves:Shelves}
impl Typesetter{
'''
    for signature in ["fn picture(&mut self", "fn capture_picture(&mut self"]:
        cache += function(source, signature) + "\n"
    cache += "}\n" + function(source, "fn fit_frame(") + "\n" + function(source, "fn fit_within(") + "\n" + function(source, "fn live_name(")
    cache += r'''
const ATLAS_SIZE:u32=2048;
struct Texts{sources:Vec<(ImageSource,(u32,u32))>,images:Vec<Option<AtlasSlot>>,animations:Vec<Option<()>>,
 last_live:HashMap<usize,(LiveKey,AtlasSlot)>,live:HashMap<LiveKey,Option<AtlasSlot>>,live_version:HashMap<LiveKey,String>,live_requested:HashSet<LiveKey>,
 recording:Option<Vec<WorkingKey>>,working:WorkingSet,rebuilding:bool,generation:u32,to_workshop:Sender<Job>,
 pending_upload:Vec<()>,pending_fit:Vec<()>,pending_pictures:Vec<(u32,(u32,u32),Vec<u8>)>,pictures:HashSet<u32>,dropped_pictures:Vec<u32>,
 unseen:HashMap<LiveKey,u32>,height:u32,exhausted:bool,layouts:HashMap<String,Arc<()>>,requested:HashSet<String>,last:HashMap<String,Arc<()>>,most:u32,too_large:Option<WorkingSet>}
impl Texts{
 fn new(to_workshop:Sender<Job>)->Self{Self{sources:vec![],images:vec![],animations:vec![],last_live:HashMap::new(),live:HashMap::new(),live_version:HashMap::new(),live_requested:HashSet::new(),
 recording:None,working:WorkingSet::default(),rebuilding:false,generation:1,to_workshop,pending_upload:vec![],pending_fit:vec![],pending_pictures:vec![],pictures:HashSet::new(),dropped_pictures:vec![],unseen:HashMap::new(),height:2048,exhausted:false,layouts:HashMap::new(),requested:HashSet::new(),last:HashMap::new(),most:8192,too_large:None}}
'''
    for signature in ["pub fn image(&mut self", "pub fn release_capture(&mut self", "fn sweep_pictures(&mut self", "pub fn receive(&mut self"]:
        cache += function(source, signature) + "\n"
    cache += "}\n#[test]\n" + function(source, "fn capture_release_drops_pending_picture_and_last_live_references()")
    cache += r'''
#[test]fn immutable_capture_is_cold_on_reuse_and_uses_its_own_upright_size(){
 let token="capture:NEW-SNAPSHOT?8";
 let frame=platform::ThumbnailFrame{version:8,width:2,height:1,pixels:vec![0,0,255,0,0,255,0,0],opaque:true};
 platform::FRAMES.lock().unwrap().get_or_insert_with(HashMap::new).insert(token.into(),Arc::new(frame));
 assert_eq!(live_name(token),(token,""));assert_eq!(live_name("/same.png?2"),("/same.png","2"));
 let(tx,_rx)=channel();let mut t=Texts::new(tx);t.sources=vec![(ImageSource::Live(scene::TextId(0)),(0,0))];
 let old=AtlasSlot{x:0,y:0,width:2,height:1,page:9};
 t.live.insert(("capture:NEW-SNAPSHOT?7".into(),(0,0)),Some(old));
 t.last_live.insert(0,(("capture:NEW-SNAPSHOT?7".into(),(0,0)),old));
 assert!(t.image(0,&[token.into()]).is_none());assert!(t.last_live.is_empty());
 let mut workshop=Typesetter{last_picture:10,pending_pictures:vec![],shelves:Shelves{most:8192}};
 let slot=workshop.capture_picture(token,None).unwrap();assert_eq!((slot.width,slot.height),(2,1));
 assert_eq!(workshop.pending_pictures[0].2,vec![255,0,0,255,0,255,0,255]);
 platform::FRAMES.lock().unwrap().as_mut().unwrap().remove(token);
 assert!(workshop.capture_picture(token,None).is_none());
}
#[test]fn released_capture_late_delivery_cannot_resurrect_pixels(){
 let(tx,_)=channel();let mut t=Texts::new(tx);
 let name="capture:ALREADY-RELEASED?77";let slot=AtlasSlot{x:0,y:0,width:2,height:1,page:22};
 t.receive(Parcel{delivery:Delivery::Live{name:name.into(),size:(0,0),slot:Some(slot),generation:1},
 pending_upload:vec![],pending_fit:vec![],pending_pictures:vec![(22,(2,1),vec![255;8])],exhausted:false,height:2048,most:8192});
 assert!(t.live.is_empty()&&t.pending_pictures.is_empty()&&t.pictures.is_empty());
}
#[test]fn late_rebuild_keeps_regular_images_but_discards_released_capture(){
 let(tx,_)=channel();let mut t=Texts::new(tx);
 let capture=AtlasSlot{x:0,y:0,width:2,height:1,page:22};let regular=AtlasSlot{page:23,..capture};
 let mut live=HashMap::new();live.insert(("capture:REBUILD-RELEASED?78".into(),(0,0)),(Some(capture),String::new()));
 live.insert(("/still.png".into(),(0,0)),(Some(regular),String::new()));
 let snapshot=Snapshot{layouts:HashMap::new(),images:vec![],animations:vec![],live};
 t.receive(Parcel{delivery:Delivery::Rebuilt{snapshot,working:WorkingSet::default(),generation:1},pending_upload:vec![],pending_fit:vec![],
 pending_pictures:vec![(22,(2,1),vec![255;8]),(23,(2,1),vec![255;8])],exhausted:false,height:2048,most:8192});
 assert_eq!(t.live.len(),1);assert_eq!(t.pending_pictures.len(),1);assert_eq!(t.pending_pictures[0].0,23);
 assert_eq!(t.dropped_pictures,vec![22]);
}
'''
    (temp / "Cargo.toml").write_text('[package]\nname="capture-native-source-check"\nversion="0.0.0"\nedition="2024"\nrust-version="1.88"\n[dependencies]\nimage={version="=0.25.10",default-features=false,features=["png"]}\n')
    (temp / "src/lib.rs").write_text(cache)
    subprocess.run(["cargo", "test", "--offline", "--manifest-path", str(temp / "Cargo.toml"), "--target-dir", str(root / ".tmp/wallpaper-cpu/target"), "--lib", "--jobs", "1"], check=True)
