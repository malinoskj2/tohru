//! Process-owned shell state; presentation and services stay in Pleamar.
pub mod settings;

#[path = "../runtime/idle/mod.rs"]
pub mod idle;

#[path = "../runtime/workspace/mod.rs"]
pub mod workspace;

#[path = "../runtime/wallpaper/mod.rs"]
pub mod wallpaper;

#[path = "../runtime/capture/mod.rs"]
pub mod capture;

#[path = "../runtime/info/mod.rs"]
pub mod info;

#[path = "../runtime/video/mod.rs"]
pub mod video;
