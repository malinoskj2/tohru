//! Process-owned shell state; presentation and services stay in Pleamar.
pub mod settings;

#[path = "../runtime/idle/mod.rs"]
pub mod idle;

#[path = "../runtime/workspace/mod.rs"]
pub mod workspace;
