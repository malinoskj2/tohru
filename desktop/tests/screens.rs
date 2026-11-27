mod common;

use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tempfile::TempDir;
use tohru_desktop::screens::{self, Capture, Event, Gate, INDEX, LATEST, Owner, Policy, Store};

const SCREENS: &str = env!("CARGO_BIN_EXE_tohru-screens");

#[derive(Clone, Default)]
struct Fake {
    outputs: Arc<Mutex<Vec<String>>>,
    grabs: Arc<Mutex<Vec<Option<String>>>>,
    delay: Duration,
    fast_grabs: usize,
    stall_outputs: bool,
    lock_after_grab: Option<Arc<Mutex<bool>>>,
}

impl Fake {
    fn with(outputs: &[&str]) -> Self {
        let fake = Self::default();
        *fake.outputs.lock().unwrap() = outputs.iter().map(|o| (*o).to_owned()).collect();
        fake
    }
}

impl Capture for Fake {
    fn outputs(&mut self, cancelled: &dyn Fn() -> bool) -> Option<Vec<String>> {
        if self.stall_outputs {
            loop {
                if cancelled() {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        Some(self.outputs.lock().unwrap().clone())
    }

    fn grab(&mut self, output: Option<&str>, path: &Path, cancelled: &dyn Fn() -> bool) -> bool {
        let count = {
            let mut grabs = self.grabs.lock().unwrap();
            grabs.push(output.map(str::to_owned));
            grabs.len()
        };
        let delay = if count > self.fast_grabs {
            self.delay
        } else {
            Duration::ZERO
        };
        let deadline = Instant::now() + delay;
        while Instant::now() < deadline {
            if cancelled() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(locked) = &self.lock_after_grab {
            *locked.lock().unwrap() = true;
        }
        fs::write(path, b"jpeg").is_ok()
    }
}

struct Lock(Arc<Mutex<bool>>);

impl Gate for Lock {
    fn locked(&mut self) -> bool {
        *self.0.lock().unwrap()
    }
}

/// A scratch folder on tmpfs, as the runner refuses persistent disks.
fn memory() -> TempDir {
    TempDir::new_in("/dev/shm").unwrap()
}

fn never() -> bool {
    false
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().mode() & 0o777
}

fn owned(root: &TempDir) -> (PathBuf, Owner) {
    let dir = root.path().join("screens");
    let owner = Owner::take(&dir, Duration::from_millis(200)).unwrap();
    (dir, owner)
}

#[test]
fn start_removes_leftovers_and_makes_a_private_folder() {
    let root = memory();
    let dir = root.path().join("screens");
    fs::create_dir_all(dir.join("DP-1")).unwrap();
    fs::write(dir.join("DP-1/1-000001.jpg"), b"old").unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    let _owner = Owner::take(&dir, Duration::from_millis(200)).unwrap();
    assert!(entries(&dir).is_empty());
    assert_eq!(mode(&dir), 0o700);
}

#[test]
fn a_symlinked_folder_is_refused() {
    let root = memory();
    let elsewhere = root.path().join("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    fs::write(elsewhere.join("keep"), b"x").unwrap();
    let dir = root.path().join("screens");
    std::os::unix::fs::symlink(&elsewhere, &dir).unwrap();
    assert!(Owner::take(&dir, Duration::from_millis(200)).is_err());
    assert!(!screens::clear(&dir).unwrap_or(false));
    assert!(elsewhere.join("keep").exists());
}

#[test]
fn captures_every_monitor_into_a_bounded_ring_with_latest_links() {
    let root = memory();
    let (dir, _owner) = owned(&root);
    let mut fake = Fake::with(&["DP-1", "HDMI-A-1"]);
    let mut store = Store::new(dir.clone(), 3);
    for _ in 0..5 {
        assert_eq!(
            store.capture(&mut fake, false, &mut never, &never).unwrap(),
            Some(2)
        );
    }
    for output in ["DP-1", "HDMI-A-1"] {
        let folder = dir.join(output);
        let frames: Vec<String> = entries(&folder)
            .into_iter()
            .filter(|n| n != LATEST)
            .collect();
        assert_eq!(frames.len(), 3, "{output}: {frames:?}");
        assert!(frames.last().unwrap().ends_with("-000005.jpg"));
        assert_eq!(
            fs::read_link(folder.join(LATEST)).unwrap(),
            PathBuf::from(frames.last().unwrap())
        );
        for frame in &frames {
            assert_eq!(mode(&folder.join(frame)), 0o600);
        }
        assert_eq!(mode(&folder), 0o700);
    }
    let index: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join(INDEX)).unwrap()).unwrap();
    assert_eq!(mode(&dir.join(INDEX)), 0o600);
    let monitors = index["monitors"].as_array().unwrap();
    assert_eq!(monitors.len(), 2);
    assert_eq!(monitors[0]["output"], "DP-1");
    assert_eq!(monitors[0]["frames"].as_array().unwrap().len(), 3);
    let note = screens::note_for(&dir, monitors[0]["captured_ms"].as_u64().unwrap() + 4000);
    assert!(note.contains("DP-1/latest.jpg (4 s old)"), "{note}");
}

#[test]
fn unnamed_outputs_fall_back_to_one_combined_image_and_unplugged_ones_are_dropped() {
    let root = memory();
    let (dir, _owner) = owned(&root);
    let mut fake = Fake::with(&["DP-1", "eDP-1"]);
    let mut store = Store::new(dir.clone(), 10);
    store.capture(&mut fake, true, &mut never, &never).unwrap();
    *fake.outputs.lock().unwrap() = vec!["eDP-1".into()];
    store.capture(&mut fake, true, &mut never, &never).unwrap();
    assert_eq!(entries(&dir), ["eDP-1", INDEX]);
    fake.outputs.lock().unwrap().clear();
    store.capture(&mut fake, true, &mut never, &never).unwrap();
    assert_eq!(entries(&dir), ["all", INDEX]);
    assert_eq!(fake.grabs.lock().unwrap().last().unwrap(), &None);
}

struct Serving {
    send: mpsc::Sender<Event>,
    stop: Arc<AtomicBool>,
    thread: std::thread::JoinHandle<std::io::Result<()>>,
}

impl Serving {
    fn stop(self) -> std::io::Result<()> {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.send.send(Event::Stop);
        self.thread.join().unwrap()
    }
}

fn serving(dir: &Path, fake: &Fake, locked: &Arc<Mutex<bool>>) -> Serving {
    let (send, events) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let (dir, mut fake, mut gate) = (dir.to_owned(), fake.clone(), Lock(locked.clone()));
    let policy = Policy {
        interval: Duration::from_secs(3600),
        keep: 10,
    };
    let thread = std::thread::spawn(move || {
        screens::serve(&dir, policy, &mut fake, &mut gate, &events, &flag)
    });
    Serving { send, stop, thread }
}

#[test]
fn serving_captures_at_once_and_stop_deletes_everything() {
    let root = memory();
    let (dir, _owner) = owned(&root);
    let fake = Fake::with(&["DP-1"]);
    let locked = Arc::new(Mutex::new(false));
    let serve = serving(&dir, &fake, &locked);
    common::wait_for(|| dir.join("DP-1").join(LATEST).exists().then_some(()));
    serve.send.send(Event::Capture).unwrap();
    common::wait_for(|| (fake.grabs.lock().unwrap().len() >= 2).then_some(()));
    serve.stop().unwrap();
    assert!(entries(&dir).is_empty());
}

#[test]
fn a_closed_channel_also_deletes_everything() {
    let root = memory();
    let (dir, _owner) = owned(&root);
    let fake = Fake::with(&["DP-1"]);
    let serve = serving(&dir, &fake, &Arc::new(Mutex::new(false)));
    common::wait_for(|| dir.join(INDEX).exists().then_some(()));
    drop(serve.send);
    serve.thread.join().unwrap().unwrap();
    assert!(entries(&dir).is_empty());
}

#[test]
fn a_stop_during_a_slow_grab_cancels_the_rest_of_the_batch() {
    let root = memory();
    let (dir, _owner) = owned(&root);
    let mut fake = Fake::with(&["DP-1", "HDMI-A-1"]);
    fake.delay = Duration::from_secs(2);
    let serve = serving(&dir, &fake, &Arc::new(Mutex::new(false)));
    common::wait_for(|| (!fake.grabs.lock().unwrap().is_empty()).then_some(()));
    let asked = Instant::now();
    serve.stop().unwrap();
    assert!(
        asked.elapsed() < Duration::from_millis(500),
        "{:?}",
        asked.elapsed()
    );
    assert_eq!(fake.grabs.lock().unwrap().len(), 1);
    assert!(entries(&dir).is_empty());
}

#[test]
fn a_lock_in_the_middle_of_a_batch_publishes_nothing_more_and_purges() {
    let root = memory();
    let (dir, _owner) = owned(&root);
    let locked = Arc::new(Mutex::new(false));
    let mut fake = Fake::with(&["DP-1", "HDMI-A-1"]);
    fake.lock_after_grab = Some(locked.clone());
    let serve = serving(&dir, &fake, &locked);
    common::wait_for(|| (!fake.grabs.lock().unwrap().is_empty()).then_some(()));
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fake.grabs.lock().unwrap().len(), 1);
    assert!(entries(&dir).is_empty(), "{:?}", entries(&dir));
    serve.stop().unwrap();
}

fn seed(dir: &Path) {
    fs::create_dir_all(dir.join("DP-1")).unwrap();
    fs::write(dir.join("DP-1/1-000001.jpg"), b"published").unwrap();
}

#[test]
fn a_stop_during_stalled_discovery_ends_at_once_and_purges() {
    let root = memory();
    let (dir, _owner) = owned(&root);
    seed(&dir);
    let mut fake = Fake::with(&["DP-1"]);
    fake.stall_outputs = true;
    let serve = serving(&dir, &fake, &Arc::new(Mutex::new(false)));
    std::thread::sleep(Duration::from_millis(100));
    let asked = Instant::now();
    serve.stop().unwrap();
    assert!(
        asked.elapsed() < Duration::from_millis(300),
        "{:?}",
        asked.elapsed()
    );
    assert!(fake.grabs.lock().unwrap().is_empty());
    assert!(entries(&dir).is_empty());
}

#[test]
fn a_lock_during_a_slow_grab_revokes_published_frames_promptly() {
    let root = memory();
    let (dir, _owner) = owned(&root);
    let locked = Arc::new(Mutex::new(false));
    let mut fake = Fake::with(&["DP-1", "HDMI-A-1"]);
    fake.delay = Duration::from_secs(3);
    fake.fast_grabs = 2;
    let serve = serving(&dir, &fake, &locked);
    common::wait_for(|| dir.join("HDMI-A-1").join(LATEST).exists().then_some(()));
    serve.send.send(Event::Capture).unwrap();
    common::wait_for(|| (fake.grabs.lock().unwrap().len() == 3).then_some(()));
    let locked_at = Instant::now();
    *locked.lock().unwrap() = true;
    common::wait_for(|| entries(&dir).is_empty().then_some(()));
    assert!(
        locked_at.elapsed() < Duration::from_millis(600),
        "{:?}",
        locked_at.elapsed()
    );
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(fake.grabs.lock().unwrap().len(), 3);
    assert!(entries(&dir).is_empty());
    serve.stop().unwrap();
}

#[test]
fn locking_purges_and_stops_capturing() {
    let root = memory();
    let (dir, _owner) = owned(&root);
    let fake = Fake::with(&["DP-1"]);
    let locked = Arc::new(Mutex::new(false));
    let serve = serving(&dir, &fake, &locked);
    common::wait_for(|| dir.join(INDEX).exists().then_some(()));
    *locked.lock().unwrap() = true;
    serve.send.send(Event::Capture).unwrap();
    common::wait_for(|| entries(&dir).is_empty().then_some(()));
    let grabs = fake.grabs.lock().unwrap().len();
    serve.send.send(Event::Capture).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fake.grabs.lock().unwrap().len(), grabs);
    assert!(entries(&dir).is_empty());
    *locked.lock().unwrap() = false;
    serve.send.send(Event::Capture).unwrap();
    common::wait_for(|| dir.join(INDEX).exists().then_some(()));
    serve.stop().unwrap();
    assert!(entries(&dir).is_empty());
}

#[test]
fn a_folder_on_a_persistent_disk_is_refused() {
    let root = TempDir::new_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    if screens::in_memory(root.path()) {
        return;
    }
    assert!(matches!(
        Owner::take(&root.path().join("screens"), Duration::from_millis(200)),
        Err(screens::ScreensError::Persistent(_))
    ));
}

#[test]
fn clear_leaves_a_live_runner_alone() {
    let root = memory();
    let (dir, owner) = owned(&root);
    fs::write(dir.join("x"), b"x").unwrap();
    assert!(!screens::clear(&dir).unwrap());
    assert!(dir.join("x").exists());
    drop(owner);
    assert!(screens::clear(&dir).unwrap());
    assert!(entries(&dir).is_empty());
}

#[test]
fn stdin_lines() {
    assert_eq!(screens::parse("capture\n"), Some(Event::Capture));
    assert_eq!(screens::parse(" stop "), Some(Event::Stop));
    assert_eq!(screens::parse("{\"cmd\":\"upload\"}"), None);
}

struct Bin {
    root: TempDir,
    _scripts: TempDir,
    fakes: PathBuf,
}

impl Bin {
    fn new() -> Self {
        let root = memory();
        // tmpfs may be mounted noexec, so the fake commands live on disk.
        let scripts = TempDir::new().unwrap();
        let fakes = scripts.path().to_owned();
        common::script(
            &fakes,
            "wayland-info",
            "printf \"interface: 'wl_output', version: 4, name: 8\\n\\tname: DP-1\\ninterface: 'wl_output', version: 4, name: 9\\n\\tname: HDMI-A-1\\n\"\n",
        );
        common::script(
            &fakes,
            "grim",
            "for last; do :; done\nprintf jpeg > \"$last\"\n",
        );
        Self {
            root,
            _scripts: scripts,
            fakes,
        }
    }

    fn dir(&self) -> PathBuf {
        self.root.path().join("screens")
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(SCREENS);
        command
            .args(args)
            .env("PATH", common::path_with(&self.fakes))
            .env("XDG_RUNTIME_DIR", self.root.path())
            .env("TOHRU_SCREENS_DIR", self.dir())
            .env_remove("XDG_SESSION_ID");
        command
    }

    fn run(&self) -> Child {
        let child = self
            .command(&["run", "--interval-ms", "500"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let dir = self.dir();
        common::wait_for(|| {
            (dir.join("DP-1").join(LATEST).exists() && dir.join("HDMI-A-1").join(LATEST).exists())
                .then_some(())
        });
        child
    }
}

fn exited(child: &mut Child) -> std::process::ExitStatus {
    common::wait_for(|| child.try_wait().unwrap())
}

#[test]
fn the_runner_cleans_up_when_its_parent_closes_stdin() {
    let bin = Bin::new();
    let mut child = bin.run();
    assert_eq!(mode(&bin.dir()), 0o700);
    drop(child.stdin.take());
    assert!(exited(&mut child).success());
    assert!(entries(&bin.dir()).is_empty());
}

#[test]
fn the_runner_cleans_up_on_stop_and_on_sigterm() {
    let bin = Bin::new();
    let mut child = bin.run();
    child.stdin.as_mut().unwrap().write_all(b"stop\n").unwrap();
    assert!(exited(&mut child).success());
    assert!(entries(&bin.dir()).is_empty());

    let mut child = bin.run();
    let pid = rustix::process::Pid::from_raw(child.id().cast_signed()).unwrap();
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
    assert!(exited(&mut child).success());
    assert!(entries(&bin.dir()).is_empty());
}

#[test]
fn a_killed_runner_is_reaped_and_leftovers_go_on_clear_and_on_the_next_start() {
    let bin = Bin::new();
    let mut child = bin.run();
    child.kill().unwrap();
    child.wait().unwrap();
    let dir = bin.dir();
    common::wait_for(|| entries(&dir).is_empty().then_some(()));
    fs::create_dir_all(dir.join("DP-1")).unwrap();
    fs::write(dir.join("DP-1/0-000000.jpg"), b"stale").unwrap();
    assert!(bin.command(&["clear"]).status().unwrap().success());
    assert!(entries(&bin.dir()).is_empty());

    let stale = dir.join("DP-1").join("0-000000.jpg");
    fs::create_dir_all(dir.join("DP-1")).unwrap();
    fs::write(&stale, b"stale").unwrap();
    let mut child = bin
        .command(&["run", "--interval-ms", "500"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    common::wait_for(|| (!stale.exists() && dir.join("DP-1").join(LATEST).exists()).then_some(()));
    drop(child.stdin.take());
    assert!(exited(&mut child).success());
    assert!(entries(&bin.dir()).is_empty());
}

#[test]
fn a_new_runner_takes_over_from_an_old_one() {
    let bin = Bin::new();
    let mut old = bin.run();
    let mut new = bin
        .command(&["run", "--interval-ms", "500"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    assert!(exited(&mut old).success());
    let dir = bin.dir();
    common::wait_for(|| dir.join("DP-1").join(LATEST).exists().then_some(()));
    drop(new.stdin.take());
    assert!(exited(&mut new).success());
    assert!(entries(&dir).is_empty());
}

#[test]
fn sigterm_during_a_slow_grim_exits_at_once_without_the_other_monitors() {
    let bin = Bin::new();
    let log = bin.root.path().join("grabs");
    common::script(
        &bin.fakes,
        "grim",
        &format!(
            "echo \"$*\" >> {}\nsleep 2\nfor last; do :; done\nprintf jpeg > \"$last\"\n",
            log.display()
        ),
    );
    let mut child = bin
        .command(&["run", "--interval-ms", "500"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    common::wait_for(|| log.exists().then_some(()));
    let pid = rustix::process::Pid::from_raw(child.id().cast_signed()).unwrap();
    let asked = Instant::now();
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
    assert!(exited(&mut child).success());
    assert!(
        asked.elapsed() < Duration::from_millis(600),
        "{:?}",
        asked.elapsed()
    );
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
    assert!(entries(&bin.dir()).is_empty());
}

#[test]
fn sigterm_during_a_stalled_wayland_info_exits_at_once() {
    let bin = Bin::new();
    let started = bin.root.path().join("asked");
    common::script(
        &bin.fakes,
        "wayland-info",
        &format!("touch {}\nsleep 5\n", started.display()),
    );
    let mut child = bin
        .command(&["run", "--interval-ms", "500"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    common::wait_for(|| started.exists().then_some(()));
    let pid = rustix::process::Pid::from_raw(child.id().cast_signed()).unwrap();
    let asked = Instant::now();
    rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
    assert!(exited(&mut child).success());
    assert!(
        asked.elapsed() < Duration::from_millis(600),
        "{:?}",
        asked.elapsed()
    );
    assert!(entries(&bin.dir()).is_empty());
}
