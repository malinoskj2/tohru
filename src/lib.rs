//! Process-owned shell state; presentation and services stay in Pleamar.
pub mod settings;

#[path = "../runtime/workspace/mod.rs"]
pub mod workspace;

#[path = "../runtime/info/mod.rs"]
pub mod info;
