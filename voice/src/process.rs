use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// A process and its pipes must not outlive their owning voice operation.
pub struct OwnedChild(pub Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn output(
    command: &mut Command,
    input: &[u8],
    timeout: Duration,
    limit: usize,
) -> Result<Vec<u8>> {
    let mut child = OwnedChild(
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("start audio utility")?,
    );
    let stdin = child.0.stdin.take().context("audio utility stdin")?;
    let stdout = child.0.stdout.take().context("audio utility stdout")?;
    let bytes = input.to_vec();
    let writer = thread::spawn(move || {
        let mut stdin = stdin;
        stdin.write_all(&bytes)
    });
    let reader = thread::spawn(move || {
        let mut data = Vec::new();
        stdout
            .take(limit as u64 + 1)
            .read_to_end(&mut data)
            .map(|_| data)
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.0.kill();
            let _ = child.0.wait();
            let _ = writer.join();
            let _ = reader.join();
            bail!("audio utility timed out");
        }
        thread::sleep(Duration::from_millis(5));
    };
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("audio utility writer panicked"))??;
    let data = reader
        .join()
        .map_err(|_| anyhow::anyhow!("audio utility reader panicked"))??;
    anyhow::ensure!(status.success(), "audio utility failed: {status}");
    anyhow::ensure!(data.len() <= limit, "audio utility output exceeds limit");
    Ok(data)
}
