#![expect(
    unsafe_code,
    reason = "Command::pre_exec registers prctl(PR_SET_PDEATHSIG) for the child"
)]

use std::{os::unix::process::CommandExt, process::Command};

/// The reservation must end if the voice service dies without stopping it.
pub(super) fn die_with_parent(command: &mut Command) {
    // SAFETY: the hook only calls prctl, which is async-signal-safe and touches no memory
    // shared with the parent.
    unsafe {
        command.pre_exec(|| {
            rustix::process::set_parent_process_death_signal(Some(rustix::process::Signal::TERM))
                .map_err(std::io::Error::from)
        });
    }
}
