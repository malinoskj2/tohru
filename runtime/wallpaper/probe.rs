//! One bounded catalog response, isolated from the shell's UI and shutdown threads.
use anyhow::{Context, bail};
use std::io::Write;
use std::path::Path;

fn run() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let Some(folder) = args.next() else {
        bail!("usage: tohru-wallpaper-probe ABSOLUTE_FOLDER")
    };
    if folder != "--identities" && args.next().is_some() {
        bail!("usage: tohru-wallpaper-probe ABSOLUTE_FOLDER");
    }
    let catalog = if folder == "--identities" {
        let paths: Vec<std::path::PathBuf> = std::env::args_os()
            .skip(2)
            .take(33)
            .map(Into::into)
            .collect();
        tohru_shell::wallpaper::identities(&paths)
            .context("checking explicitly selected wallpaper identities")?
    } else {
        tohru_shell::wallpaper::catalog(Path::new(&folder))
            .context("listing the selected wallpaper folder")?
    };
    let bytes = serde_json::to_vec(&catalog).context("encoding wallpaper catalog")?;
    if bytes.len() > tohru_shell::wallpaper::MAX_RESPONSE_BYTES {
        bail!("wallpaper catalog serialization exceeds 1 MiB");
    }
    std::io::stdout()
        .lock()
        .write_all(&bytes)
        .context("writing wallpaper catalog")?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("tohru-wallpaper-probe: {error:#}");
        std::process::exit(1);
    }
}
