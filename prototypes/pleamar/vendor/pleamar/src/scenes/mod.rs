pub mod face;
pub mod from_file;
pub mod island;
pub mod marea;
pub mod showcase;
pub mod swarm;

/// The program does not start again when it changes on disk (`stay_on_update`).
pub(crate) static STAY_ON_UPDATE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
