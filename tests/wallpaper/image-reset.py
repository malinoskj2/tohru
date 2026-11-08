#!/usr/bin/env python3
"""CPU check of the actual patched native cache methods, with only I/O/type fixtures.

Supply the pinned clean runtime source; no runtime build, display, image decode or GPU.
Full native text module and pixel validation remain separate allocation gates.
"""
import argparse
import pathlib
import subprocess
import tempfile


def function(source, signature):
    start = source.index(signature)
    opened = source.index("{", start)
    depth = 1
    at = opened + 1
    while depth:
        if source[at] == "{":
            depth += 1
        elif source[at] == "}":
            depth -= 1
        at += 1
    return source[start:at]


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source", type=pathlib.Path, required=True)
args = parser.parse_args()
root = pathlib.Path(__file__).resolve().parents[2]
(root / ".tmp").mkdir(exist_ok=True)
with tempfile.TemporaryDirectory(prefix="image-reset-", dir=root / ".tmp") as temporary:
    work = pathlib.Path(temporary)
    (work / "src").mkdir()
    (work / "src/text.rs").write_bytes((args.source / "src/text.rs").read_bytes())
    subprocess.run(["git", "apply", "--unsafe-paths", f"--directory={work}", str(root / "runtime/wallpaper/image-reset.patch")], check=True)
    source = (work / "src/text.rs").read_text()
    image = function(source, "pub fn image(&mut self")
    live_name = function(source, "fn live_name(name:")
    sweep = function(source, "fn sweep_pictures(&mut self")
    case = function(source, "fn cold_live_identity_never_inherits_another_outputs_picture()")
    harness = r'''
#![allow(dead_code)]
use std::collections::{HashMap,HashSet};
use std::sync::mpsc::{channel,Sender};
mod scene { #[derive(Clone)] pub struct TextId(pub u16); }
#[derive(Clone,Copy)] struct AtlasSlot { page:u32,x:u32,y:u32,width:u32,height:u32 }
#[derive(Clone)] enum ImageSource { Live(scene::TextId), File }
type LiveKey=(String,(u32,u32));
enum WorkingKey { Live(LiveKey) }
#[derive(Default)] struct WorkingSet { live:HashSet<LiveKey> }
enum Job { Live(String,(u32,u32),u32), Reload(String,(u32,u32),AtlasSlot,u32) }
struct Texts {
 sources:Vec<(ImageSource,(u32,u32))>, images:Vec<Option<AtlasSlot>>,
 last_live:HashMap<usize,(LiveKey,AtlasSlot)>,
 live:HashMap<LiveKey,Option<AtlasSlot>>,live_version:HashMap<LiveKey,String>,
 live_requested:HashSet<LiveKey>, recording:Option<Vec<WorkingKey>>,
 working:WorkingSet, rebuilding:bool,generation:u32,to_workshop:Sender<Job>,
 pending_pictures:Vec<(u32,(u32,u32),Vec<u8>)>,pictures:HashSet<u32>,dropped_pictures:Vec<u32>,
}
impl Texts {
 fn new(to_workshop:Sender<Job>) -> Self { Self {
 sources:vec![],images:vec![],last_live:HashMap::new(),live:HashMap::new(),live_version:HashMap::new(),
 live_requested:HashSet::new(),recording:None,working:WorkingSet::default(),rebuilding:false,generation:0,to_workshop,
 pending_pictures:vec![],pictures:HashSet::new(),dropped_pictures:vec![] } }
'''
    harness += image + "\n" + sweep + "\n}\n" + live_name + "\n#[test]\n" + case
    harness += r'''
#[test]
fn same_source_version_still_keeps_its_frame_while_rebuilding() {
 let (tx,_rx)=channel(); let mut t=Texts::new(tx); let size=(64,36);
 t.sources=vec![(ImageSource::Live(scene::TextId(0)),size)];
 let key=("/same.png".into(),size);
 let slot=AtlasSlot{page:7,x:0,y:0,width:64,height:36};
 t.live.insert(key.clone(),Some(slot));
 assert_eq!(t.image(0,&["/same.png?1".into()]).unwrap().page,7);
 t.live.remove(&key); t.rebuilding=true;
 assert_eq!(t.image(0,&["/same.png?2".into()]).unwrap().page,7);
 assert!(t.image(0,&["/different.png".into()]).is_none());
 t.pictures.insert(7);t.sweep_pictures();
 assert_eq!(t.dropped_pictures,vec![7]);
}
'''
    path = work / "cache.rs"
    path.write_text(harness)
    subprocess.run(["rustc", "--edition=2024", "--test", str(path), "-o", str(work / "cache-tests")], check=True)
    subprocess.run([str(work / "cache-tests")], check=True)
