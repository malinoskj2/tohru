//! One coalesced nonblocking wake for root updates, libmpv callbacks and shutdown.
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct Signal {
    socket: UnixStream,
    pending: AtomicBool,
}

#[derive(Clone)]
pub(super) struct Wake(Arc<Signal>);
pub(super) struct Receiver {
    socket: UnixStream,
    wake: Wake,
}

impl Wake {
    pub(super) fn pair() -> io::Result<(Self, Receiver)> {
        let (write, read) = UnixStream::pair()?;
        write.set_nonblocking(true)?;
        read.set_nonblocking(true)?;
        let wake = Self(Arc::new(Signal {
            socket: write,
            pending: AtomicBool::new(false),
        }));
        Ok((wake.clone(), Receiver { socket: read, wake }))
    }

    pub(super) fn notify(&self) {
        if !self.0.pending.swap(true, Ordering::AcqRel) {
            // No allocations, locks, API calls or blocking waits on a foreign callback.
            loop {
                match (&self.0.socket).write(&[1]) {
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    _ => break,
                }
            }
        }
    }
}

impl Receiver {
    pub(super) fn drain(&self) -> io::Result<()> {
        let mut bytes = [0u8; 64];
        loop {
            match (&self.socket).read(&mut bytes) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "video wake closed",
                    ));
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        // Owner resets BEFORE inspecting latest state/render updates. A callback
        // racing before this reset is covered by that inspection; later ones wake
        // the next pass. No frame or root snapshot is queued per notification.
        self.wake.0.pending.store(false, Ordering::Release);
        Ok(())
    }
}

impl AsFd for Receiver {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.socket.as_fd()
    }
}
