#!/usr/bin/env python3
"""Exercise actual conditional output-removal/retirement/callback routes on CPU.

Only foreign Wayland/GPU resources and the output enumerator are replaced. The
actual placement eligibility gate, remove/retire/refresh, worker guard, native
callbacks and State Drop come from the supplied composed source.
"""
import argparse
from pathlib import Path
import subprocess
import tempfile


def item(source, needle):
    start = source.index(needle)
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    args = parser.parse_args()
    source = args.source.resolve() / "src"
    native = (source / "platform/wayland.rs").read_text()
    gpu = (source / "gpu.rs").read_text()
    gate = item(native, "if self.wanted[which].window.is_some() {")
    rust = r'''
#![allow(dead_code)]
use std::sync::{Arc, Weak, Mutex, OnceLock, LazyLock, mpsc};
use std::collections::{BTreeMap, HashSet};
#[path="VISIBILITY"] mod tohru_window;
pub struct Native;
#[derive(Clone)] struct Window(Arc<Native>);
impl Window { fn wl_surface(&self) -> &u32 { &0 } }
#[derive(Clone)] struct LayerSurface;
impl LayerSurface { fn wl_surface(&self) -> &u32 { &0 } }
type WpViewport = ();
type WpFractionalScaleV1 = ();
type BackdropTarget = ();
type View = ();
mod wl_output { pub type WlOutput = u32; }
mod wl_surface { pub type WlSurface = u32; }
mod wgpu {
    pub struct Surface<'a>(pub std::sync::Weak<super::Native>, pub std::marker::PhantomData<&'a ()>);
    impl Drop for Surface<'_> {
        fn drop(&mut self) { assert!(self.0.upgrade().is_some(), "GPU dropped after native owner"); }
    }
}
pub enum Target { Surface(wgpu::Surface<'static>) }
pub trait PlatformWindow {}
struct Guard(ConditionalWindow);
impl PlatformWindow for Guard {}
CONDITIONAL
WORKER
WORKER_IMPL
WORKER_DROP
ROLE
ROLE_IMPL
PLACED
NEWSHEET
struct QueueHandle<T>(std::marker::PhantomData<fn() -> T>);
struct Connection;
impl Connection {
    fn display(&self) -> &Self { self }
    fn sync<T>(&self, _: &QueueHandle<State>, _: T) {}
    fn flush(&self) -> Result<(), ()> { Ok(()) }
}
mod wayland_client { pub mod protocol { pub mod wl_callback {
    pub struct WlCallback;
    pub enum Event { Done { callback_data: u32 }, Other }
}}}
trait Dispatch2<T, S> {
    fn event(&self, state: &mut S, callback: &T, event: wayland_client::protocol::wl_callback::Event,
             connection: &Connection, qh: &QueueHandle<S>);
}
struct WindowRefresh;
REFRESH_CALLBACK
struct WindowReleased(u32);
RELEASE_CALLBACK
enum ToRender { SheetGone(u32) }
struct Locks { monitors: Mutex<Vec<(u32, (), ())>> }
struct MovableLayers {
    connection: Connection, qh: QueueHandle<State>,
    placed: Mutex<Vec<(usize, LayerSurface, (), (), ())>>,
}
static OUTPUTS: LazyLock<Mutex<BTreeMap<String,u32>>> = LazyLock::new(Mutex::default);
static LOCKS: OnceLock<Locks> = OnceLock::new();
static MOVABLE_LAYERS: OnceLock<MovableLayers> = OnceLock::new();
static RETIRING_WINDOWS: LazyLock<Mutex<HashSet<u32>>> = LazyLock::new(Mutex::default);
fn lock_face_for_gone_monitor(_: &u32) -> Vec<u32> { vec![] }
fn forget_parent(_: &u32) {}
fn outputs_changed(_: &mpsc::Sender<ToRender>) {}
struct Said { window_open: Option<bool> }
fn said_of(_: usize) -> Said { Said { window_open: Some(true) } }
struct Surface { window: Option<()>, open: Option<bool> }
struct State {
    placed: Vec<Placed>, retired_windows: Vec<Placed>, to_render: mpsc::Sender<ToRender>,
    wanted: Vec<Surface>, announced: Vec<u32>, created: usize,
}
impl State {
    RETIRE
    REMOVE
    REFRESH
    OUTPUT_DESTROYED
    fn place_on_all(&mut self, _: &QueueHandle<State>, except: Option<&u32>) {
        // Foreign output enumeration is substituted; eligibility is native code.
        if !self.announced.iter().any(|output| Some(output) != except) { return; }
        for which in 0..self.wanted.len() {
            PLACEMENT_GATE
            if self.placed.iter().any(|p| p.which == which) { continue; }
            self.created += 1;
            self.placed.push(placed(self.announced[0]));
        }
    }
}
STATE_DROP
fn placed(output: u32) -> Placed {
    let window = Window(Arc::new(Native));
    Placed { id: output, which: 0, role: Role::Window(window.clone()), output,
        viewport: None, _fractional_scale: None, scale: 1., size: (0,0),
        pending: Some((wgpu::Surface(Arc::downgrade(&window.0), std::marker::PhantomData), String::new(), 0)),
        backdrop: None, conditional_window: Some(ConditionalWindow { _window: window, life: Arc::default() }) }
}
fn configured(p: &mut Placed) -> NewSheet {
    let (target, name, mhz) = p.pending.take().unwrap();
    NewSheet { id:p.id, target:Target::Surface(target), window:Box::new(Guard(p.conditional_window.clone().unwrap())),
               scale:1.,size:(400,200),mhz,name,view:() }
}
fn refresh(state: &mut State, qh: &QueueHandle<State>) {
    WindowRefresh.event(state, &wayland_client::protocol::wl_callback::WlCallback,
        wayland_client::protocol::wl_callback::Event::Done {callback_data:0}, &Connection, qh);
}
fn acknowledge(state: &mut State, qh: &QueueHandle<State>) {
    WindowReleased(1).event(state, &wayland_client::protocol::wl_callback::WlCallback,
        wayland_client::protocol::wl_callback::Event::Done {callback_data:0}, &Connection, qh);
}
#[test] fn actual_output_remove_waits_for_delayed_worker_in_both_completion_orders() {
    for gpu_first in [true,false] {
        let qh = QueueHandle(std::marker::PhantomData);
        let (tx,rx) = mpsc::channel();
        let mut p = placed(1);
        let mut queued = Some(configured(&mut p));
        let life = p.conditional_window.as_ref().unwrap().life.clone();
        let worker = ConditionalWorker::start(p.conditional_window.clone().unwrap()).unwrap();
        let (resume, delayed) = mpsc::channel();
        let worker_life = life.clone();
        let thread = std::thread::spawn(move || {
            delayed.recv().unwrap();
            worker_life.if_current(|| panic!("retired worker published obsolete frame"));
            drop(worker); // actual production counted-worker/native-owner Drop
        });
        let mut state = State { placed:vec![p], retired_windows:vec![], to_render:tx,
            wanted:vec![Surface{window:Some(()),open:Some(true)}],announced:vec![1,2],created:0 };
        state.output_destroyed(&Connection, &qh, 1); // actual callback+remove route
        assert!(!life.current());
        assert_eq!(state.retired_windows.len(),1);
        assert_eq!(state.created,0,"replacement before barrier");
        assert!(matches!(rx.recv().unwrap(),ToRender::SheetGone(1)));
        if gpu_first { drop(queued.take()); acknowledge(&mut state,&qh); assert_eq!(state.created,0); }
        resume.send(()).unwrap(); thread.join().unwrap();
        refresh(&mut state,&qh);
        if !gpu_first { assert_eq!(state.created,0); drop(queued.take()); acknowledge(&mut state,&qh); }
        assert!(state.retired_windows.is_empty());
        assert_eq!(state.created,1,"exactly one replacement after both completions");
    }
}
#[test] fn all_outputs_gone_and_state_exit_preserve_the_same_lifetime() {
    let qh=QueueHandle(std::marker::PhantomData); let (tx,_) = mpsc::channel();
    let mut p=placed(1); let queued=configured(&mut p);
    let life=p.conditional_window.as_ref().unwrap().life.clone();
    let worker=ConditionalWorker::start(p.conditional_window.clone().unwrap()).unwrap();
    let mut state=State{placed:vec![p],retired_windows:vec![],to_render:tx,
        wanted:vec![Surface{window:Some(()),open:Some(true)}],announced:vec![1],created:0};
    state.output_destroyed(&Connection,&qh,1);
    assert_eq!(state.created,0); assert_eq!(state.retired_windows.len(),1);
    state.announced.push(3); state.place_on_all(&qh,None); assert_eq!(state.created,0);
    drop(queued); acknowledge(&mut state,&qh); assert_eq!(state.created,0);
    drop(worker); refresh(&mut state,&qh); assert_eq!(state.created,1);
    let new_life=state.placed[0].conditional_window.as_ref().unwrap().life.clone();
    drop(state); assert!(!new_life.current()); assert!(life.ready());
}
'''
    substitutions = {
        "VISIBILITY": str(source / "tohru_window.rs"),
        "CONDITIONAL": "#[derive(Clone)]\n" + item(native, "struct ConditionalWindow {"),
        "WORKER": "struct ConditionalWorker(Option<ConditionalWindow>);",
        "WORKER_IMPL": item(native, "impl ConditionalWorker {"),
        "WORKER_DROP": item(native, "impl Drop for ConditionalWorker {"),
        "ROLE": "#[derive(Clone)]\n" + item(native, "enum Role {"),
        "ROLE_IMPL": item(native, "impl Role {"),
        "PLACED": item(native, "struct Placed {"),
        "NEWSHEET": item(gpu, "pub struct NewSheet {"),
        "REFRESH_CALLBACK": item(native, "impl Dispatch2<wayland_client::protocol::wl_callback::WlCallback, State> for WindowRefresh {"),
        "RELEASE_CALLBACK": item(native, "impl Dispatch2<wayland_client::protocol::wl_callback::WlCallback, State> for WindowReleased {"),
        "RETIRE": item(native, "fn retire_window("),
        "REMOVE": item(native, "fn remove_from("),
        "REFRESH": item(native, "fn refresh_windows("),
        "OUTPUT_DESTROYED": item(native, "fn output_destroyed("),
        "PLACEMENT_GATE": gate,
        "STATE_DROP": item(native, "impl Drop for State {"),
    }
    # Longest first avoids replacing WORKER inside WORKER_DROP tokens.
    for key in sorted(substitutions, key=len, reverse=True):
        rust = rust.replace(key, substitutions[key])
    root = Path(__file__).resolve().parents[1]
    (root / ".tmp").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="window-route-", dir=root / ".tmp") as work:
        fixture = Path(work) / "route.rs"
        binary = Path(work) / "route"
        fixture.write_text(rust.replace("Arc, Weak, Mutex", "Arc, Mutex"))
        subprocess.run(["rustc", "--edition", "2024", "--test", "-Dwarnings", str(fixture), "-o", str(binary)], check=True)
        subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
