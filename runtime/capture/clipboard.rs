//! Foreground, parent-owned image/png clipboard server. No daemon or capture backend.
use anyhow::{Context, bail};
use std::io::{Read, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wl_clipboard_rs::copy::{ClipboardType, MimeSource, MimeType, Options, Source};
use wl_clipboard_rs::paste::{self, Seat};

const OWNER_MIME: &str = "application/x-tohru-capture-owner";

fn owns_marker(token: &[u8]) -> bool {
    let ready = paste::get_mime_types_ordered(paste::ClipboardType::Regular, Seat::Unspecified)
        .is_ok_and(|types| types.iter().any(|mime| mime == "image/png"));
    if !ready {
        return false;
    }
    let Ok((pipe, _)) = paste::get_contents(
        paste::ClipboardType::Regular,
        Seat::Unspecified,
        paste::MimeType::Specific(OWNER_MIME),
    ) else {
        return false;
    };
    let mut observed = Vec::new();
    pipe.take(129).read_to_end(&mut observed).is_ok() && observed == token
}

fn run() -> anyhow::Result<()> {
    if std::env::args_os().len() != 1 {
        bail!("tohru-capture-clipboard accepts PNG on stdin only");
    }
    let limit = tohru_shell::capture::MAX_PIXELS * 4 + 1024 * 1024;
    let mut png = Vec::new();
    std::io::stdin()
        .lock()
        .take(limit + 1)
        .read_to_end(&mut png)
        .context("reading capture PNG")?;
    if png.len() as u64 > limit || !png.starts_with(b"\x89PNG\r\n\x1a\n") {
        bail!("capture PNG input is invalid or over budget");
    }
    let token = format!(
        "{}:{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    )
    .into_bytes();
    let mut options = Options::new();
    options
        .foreground(true)
        .clipboard(ClipboardType::Regular)
        .omit_additional_text_mime_types(true);
    let prepared = options
        .prepare_copy_multi(vec![
            MimeSource {
                source: Source::Bytes(png.into_boxed_slice()),
                mime_type: MimeType::Specific("image/png".into()),
            },
            MimeSource {
                source: Source::Bytes(token.clone().into_boxed_slice()),
                mime_type: MimeType::Specific(OWNER_MIME.into()),
            },
        ])
        .context("preparing image clipboard ownership")?;
    // prepare_copy queues set_selection; it does not flush/ack ownership. Read
    // back this offer's bounded nonce while the foreground library serves it.
    let verifier = std::thread::Builder::new()
        .name("capture-clipboard-ready".into())
        .spawn(move || {
            let until = Instant::now() + Duration::from_secs(2);
            while Instant::now() < until {
                if owns_marker(&token) {
                    let mut out = std::io::stdout().lock();
                    out.write_all(b"READY\n")
                        .and_then(|()| out.flush())
                        .context("acknowledging clipboard ownership")?;
                    return Ok::<(), anyhow::Error>(());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            bail!("clipboard ownership was not observed before readiness deadline")
        })
        .context("starting clipboard ownership verification")?;
    let served = prepared.serve().context("serving image clipboard");
    let ownership_result = verifier
        .join()
        .map_err(|_| anyhow::anyhow!("clipboard verifier failed"))?;
    served?;
    ownership_result?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("tohru-capture-clipboard: {error:#}");
        std::process::exit(1);
    }
}
