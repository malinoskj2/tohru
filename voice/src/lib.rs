//! Native acoustic and speech worker; desktop routing remains in the Luau controller.
pub mod acoustic;
mod config;
pub mod evaluate;
pub mod frontend;
pub mod latency;
pub mod policy;
pub mod process;
pub mod profile;
pub mod protocol;
pub mod service;
pub mod speaker;
pub mod synthesis;
pub use config::{Args, ModelCheck};
