use std::io::Write;
use std::sync::{LazyLock, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use amane::{
    App, Full, Layer, LayerWindow, Margin, Parent, Rectangle, Service, Stack, Vertical, Widget,
};
use anyhow::Context;

use crate::{compositor, track::Track};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Live,
    Replay,
    Idle,
    Active,
}

struct Config {
    mode: Mode,
    seconds: Option<u64>,
}

static CONFIG: OnceLock<Config> = OnceLock::new();
static INPUT: LazyLock<(mpsc::Sender<usize>, Mutex<mpsc::Receiver<usize>>)> = LazyLock::new(|| {
    let (tx, rx) = mpsc::channel();
    (tx, Mutex::new(rx))
});
static FRAME_LOG: LazyLock<Option<Mutex<std::fs::File>>> = LazyLock::new(|| {
    std::env::var_os("AMANE_FRAME_LOG")
        .and_then(|path| std::fs::File::create(path).ok())
        .map(Mutex::new)
});
static EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);
const REPLAY: &[(u64, usize)] = &[
    (0, 1),
    (1000, 2),
    (2200, 3),
    (3400, 4),
    (4600, 5),
    (5800, 1),
    (7000, 5),
    (7120, 2),
    (7240, 4),
    (9000, 1),
];
const ACTIVE: [usize; 5] = [1, 5, 2, 4, 3];

struct Motion {
    track: Track,
}

impl Service for Motion {
    fn new() -> Self {
        let mut track = Track::default();
        track.sync(5, Some(1), Instant::now());
        Self { track }
    }

    fn listen() {
        let Some(config) = CONFIG.get() else { return };
        let start = Instant::now();
        let mut event = 0;
        let Ok(rx) = INPUT.1.lock() else { return };
        loop {
            let scheduled = match config.mode {
                Mode::Replay => REPLAY.get(event).copied(),
                Mode::Active => Some((event as u64 * 300, ACTIVE[event % ACTIVE.len()])),
                Mode::Live | Mode::Idle => None,
            };
            let moving = Self::read().track.animating();
            let scheduled_wait =
                scheduled.map(|(at, _)| Duration::from_millis(at).saturating_sub(start.elapsed()));
            let wait = if moving {
                Some(
                    scheduled_wait.map_or(Duration::from_micros(16_667), |wait| {
                        wait.min(Duration::from_micros(16_667))
                    }),
                )
            } else {
                scheduled_wait
            };
            let input = match wait {
                Some(wait) => rx.recv_timeout(wait).ok(),
                None => rx.recv().ok(),
            };
            let now = Instant::now();
            let mut state = Self::write();
            if let Some((at, target)) = scheduled {
                let at = start + Duration::from_millis(at);
                if now >= at {
                    state.track.advance(at);
                    state.track.sync(5, Some(target), at);
                    event += 1;
                }
            }
            state.track.advance(now);
            if let Some(target) = input {
                state.track.sync(5, Some(target), now);
            }
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut config = Config {
        mode: Mode::Live,
        seconds: None,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => {
                config.mode = match args.next().as_deref() {
                    Some("live") => Mode::Live,
                    Some("replay") => Mode::Replay,
                    Some("idle") => Mode::Idle,
                    Some("active") => Mode::Active,
                    _ => anyhow::bail!("--mode expects live, replay, idle, or active"),
                }
            }
            "--seconds" => {
                config.seconds = Some(args.next().context("--seconds needs an integer")?.parse()?)
            }
            "--help" => {
                println!("workspace-amane [--mode live|replay|idle|active] [--seconds N]");
                return Ok(());
            }
            _ => anyhow::bail!("unknown argument {arg}"),
        }
    }
    let live = config.mode == Mode::Live;
    if live {
        anyhow::ensure!(
            std::env::var_os("SWAYSOCK").is_some(),
            "live mode requires Sway SWAYSOCK; demos are explicit --mode choices"
        );
    }
    let seconds = config.seconds;
    CONFIG
        .set(config)
        .map_err(|_| anyhow::anyhow!("configuration was already initialized"))?;
    if live {
        let tx = INPUT.0.clone();
        std::thread::spawn(move || {
            if let Err(error) = compositor::watch(tx) {
                eprintln!("{error}");
            }
        });
    }
    if let Some(seconds) = seconds {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(seconds));
            std::process::exit(0);
        });
    }
    eprintln!(
        "amane revision=544cbfc23af37f84ac237cd9bec53173981772b3 seed=0x0123456789abcdef mode={:?}",
        CONFIG.get().map(|c| c.mode)
    );
    App::new().without_ipc().window(view).run();
    Ok(())
}

fn view() -> LayerWindow {
    if let Some(file) = &*FRAME_LOG
        && let Ok(mut file) = file.lock()
    {
        let _ = writeln!(file, "{}", EPOCH.elapsed().as_nanos());
    }
    let state = Motion::read();
    let mut children: Vec<Box<dyn Widget>> = vec![
        Box::new(
            Rectangle::new()
                .width(Parent)
                .height(31.0)
                .fill("#160e1461"),
        ),
        Box::new(
            Rectangle::new()
                .width(104.0)
                .height(42.0)
                .shader(concat!(env!("CARGO_MANIFEST_DIR"), "/src/track.frag"))
                .shader_values(state.track.uniforms())
                .translate(8.0, -5.5),
        ),
    ];
    for cell in 1..=5 {
        children.push(Box::new(
            Rectangle::new()
                .width(16.0)
                .height(22.0)
                .translate(20.0 + (cell - 1) as f32 * 16.0, 4.5)
                .on_click(move |_| {
                    if CONFIG.get().is_some_and(|c| c.mode == Mode::Live) {
                        if let Err(error) = compositor::focus(cell) {
                            eprintln!("{error}");
                        }
                    } else {
                        let _ = INPUT.0.send(cell);
                    }
                }),
        ));
    }
    LayerWindow::new()
        .width(Full)
        .height(42.0)
        .anchor_vertical(Vertical::Top)
        .layer(Layer::Top)
        .namespace("workspace-amane")
        .margin(Margin {
            left: 100,
            right: 100,
            top: 0,
            bottom: 0,
        })
        .child(Stack::new(children).width(Parent).height(42.0))
}
