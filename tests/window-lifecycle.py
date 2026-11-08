#!/usr/bin/env python3
"""Compile actual candidate coordination/owner declarations with CPU test resources.

This catches the GPU-none outer gate and native-owner field/drop ordering without
claiming a typecheck of Wayland/wgpu or a compositor lifecycle acceptance test.
Run against fresh pinned source with 0001/0002/0003 and tohru_window.rs installed.
"""
import argparse
from pathlib import Path
import subprocess
import tempfile


def declaration(source, name):
    start = source.index(name)
    end = source.index("\n}", start) + 2
    return source[start:end]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    args = parser.parse_args()
    source = args.source.resolve() / "src"
    wayland = (source / "platform/wayland.rs").read_text()
    render = (source / "render.rs").read_text()
    gpu = (source / "gpu.rs").read_text()
    start = render.index("        // Conditional ordinary windows map only when requested.")
    gate = render.index("        let Some(g) = &mut gpu else {\n            // A closed window-only scene")
    assert start < gate, "first-open coordination must precede the no-GPU gate"
    coordination = render[start:gate]
    no_gpu = render[gate:render.index("        // `lens =", gate)]
    assert "resting = true;" in no_gpu and "next_appointment = appointments.iter().min().copied();" in no_gpu
    assert "sheet.id" not in wayland[wayland.index("fn retire_window"):wayland.index("fn refresh_windows")]
    assert "announced: Vec<wl_output::WlOutput>" in wayland
    assert "conditional_window: p.conditional_window.clone()" in wayland
    assert "conditional_window: self.conditional_window.clone()" in wayland
    assert "ConditionalWorker::start(window)" in wayland
    assert "let worker = conditional_worker;" in wayland
    assert "conditional_commit(conditional_window, ||" in wayland
    assert "window.life.retire();" in wayland
    assert "window.life.gpu_released();" in wayland
    assert "!window.life.ready()" in wayland
    assert "conditional_commit(conditional_window, || lock_publication(lock_face.as_ref()," in wayland
    assert "conditional_commit(self.conditional_window.as_ref(), || lock_publication(self.lock_face.as_ref()," in wayland
    assert "drop(pending);\n        drop(keep_texture);" in wayland
    assert wayland.index("MOVABLE_LAYERS.set(") < wayland.index("send(ToRender::WindowsReady)")
    assert "ToRender::WindowsReady => { windows_ready = true; }" in render
    assert "mem::forget(window)" not in wayland, "ordinary owners must retire rather than accumulate"
    placed = declaration(wayland, "struct Placed {")
    assert placed.rindex("conditional_window:") > placed.index("pending:")
    harness = r'''
#![allow(dead_code)]
use std::sync::{Arc, Mutex};
#[path = "VISIBILITY"] mod visibility;
pub struct Native;
#[derive(Clone)] struct Window(Arc<Native>);
type ConditionalWindow = Window;
type LayerSurface = ();
type WpViewport = ();
type WpFractionalScaleV1 = ();
type BackdropTarget = ();
type View = ();
mod wl_output { pub type WlOutput = (); }
mod wgpu {
    pub struct Surface<'a>(pub std::sync::Weak<super::Native>, pub std::marker::PhantomData<&'a ()>);
    impl Drop for Surface<'_> {
        fn drop(&mut self) { assert!(self.0.upgrade().is_some(), "GPU target outlived native owner"); }
    }
}
pub enum Target { Surface(wgpu::Surface<'static>) }
pub trait PlatformWindow {}
struct Guard(Window);
impl PlatformWindow for Guard {}
ROLE
PLACED
NEWSHEET
fn placed() -> Placed {
    let window = Window(Arc::new(Native));
    Placed { id: 1, which: 0, role: Role::Window(window.clone()), output: (),
        viewport: None, _fractional_scale: None, scale: 1., size: (0,0),
        pending: Some((wgpu::Surface(Arc::downgrade(&window.0), std::marker::PhantomData), String::new(), 0)),
        backdrop: None, conditional_window: Some(window) }
}
fn configure(p: &mut Placed) -> NewSheet {
    let (target, name, mhz) = p.pending.take().unwrap();
    NewSheet { id: p.id, target: Target::Surface(target),
        window: Box::new(Guard(p.conditional_window.clone().unwrap())),
        scale: 1., size: (800,560), mhz, name, view: () }
}
#[test] fn pending_state_exit_drops_target_before_final_native_owner() { drop(placed()); }
#[test] fn queued_sheet_survives_output_removal_or_disconnect() {
    let mut p = placed(); let queued = configure(&mut p);
    drop(p); drop(queued);
}
#[test] fn queued_sheet_can_be_discarded_before_native_state() {
    let mut p = placed(); let queued = configure(&mut p);
    drop(queued); drop(p);
}
#[test] fn retired_sheet_and_state_can_drop_in_either_order() {
    let mut p = placed(); let queued = configure(&mut p); let retired = vec![p];
    drop(retired); drop(queued);
}
struct Surface { window: Option<()>, lock_screen: bool, open: Option<bool> }
struct Scene { surfaces: Vec<Surface> }
static REQUESTS: Mutex<Vec<(usize,bool)>> = Mutex::new(Vec::new());
mod platform { pub fn window_open(which: usize, wanted: bool) { super::REQUESTS.lock().unwrap().push((which,wanted)); } }
fn coordinate(scene: &Scene, window_intents: &mut Vec<visibility::Intent>, requested: bool, windows_ready: bool) {
    let open = |_: usize| requested;
COORDINATION
}
#[test] fn actual_coordination_first_open_needs_no_sheet_or_gpu() {
    let scene = Scene { surfaces: vec![Surface { window: Some(()), lock_screen: false, open: Some(false) }] };
    let mut intents = Vec::new();
    coordinate(&scene, &mut intents, true, false); // renderer before native transport
    assert!(REQUESTS.lock().unwrap().is_empty());
    coordinate(&scene, &mut intents, false, true);
    coordinate(&scene, &mut intents, true, true);
    coordinate(&scene, &mut intents, true, true);
    intents[0].closed();
    coordinate(&scene, &mut intents, true, true); // WM close must not immediately reopen.
    coordinate(&scene, &mut intents, false, true);
    coordinate(&scene, &mut intents, true, true); // explicit reopen without output change.
    assert_eq!(*REQUESTS.lock().unwrap(), vec![(0,false),(0,true),(0,true)]);
    REQUESTS.lock().unwrap().clear();
    let mut initial_true = Vec::new();
    coordinate(&scene, &mut initial_true, true, false);
    coordinate(&scene, &mut initial_true, true, true);
    coordinate(&scene, &mut initial_true, true, true);
    assert_eq!(*REQUESTS.lock().unwrap(), vec![(0,true)]);
}
'''
    for key, value in {
        "VISIBILITY": str(source / "tohru_window.rs"),
        "ROLE": declaration(wayland, "enum Role {"),
        "PLACED": placed,
        "NEWSHEET": declaration(gpu, "pub struct NewSheet {"),
        "COORDINATION": coordination,
    }.items():
        harness = harness.replace(key, value)
    root = Path(__file__).resolve().parents[1]
    (root / ".tmp").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="window-cpu-", dir=root / ".tmp") as work:
        rust = Path(work) / "check.rs"
        binary = Path(work) / "check"
        rust.write_text(harness)
        subprocess.run(["rustc", "--edition", "2024", "--test", "-Dwarnings", str(rust), "-o", str(binary)], check=True)
        subprocess.run([str(binary)], check=True)


if __name__ == "__main__":
    main()
