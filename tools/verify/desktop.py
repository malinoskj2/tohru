#!/usr/bin/env python3
"""Drive Tohru's public CLI against disposable state and owned transports."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import traceback

ROOT = Path(__file__).resolve().parents[2]
SERVICE_LOG_LIMIT = 4 * 1024 * 1024
RECORDED_OUTPUT_LIMIT = 64 * 1024


def bounded(text):
    if len(text) <= RECORDED_OUTPUT_LIMIT:
        return text
    return text[:RECORDED_OUTPUT_LIMIT] + f"\n[truncated {len(text) - RECORDED_OUTPUT_LIMIT} characters]"


def group_members(pgid):
    members = []
    for stat in Path("/proc").glob("[0-9]*/stat"):
        try:
            fields = stat.read_text().rsplit(")", 1)[1].split()
        except OSError:
            continue
        if int(fields[2]) == pgid and fields[0] != "Z":
            members.append(int(stat.parent.name))
    return members


def signal_group(pgid, signum):
    try:
        os.killpg(pgid, signum)
    except ProcessLookupError:
        pass


def pump(source, path, limit):
    # Drain past the limit so a chatty child never blocks on a full pipe.
    stored = dropped = 0
    with source, path.open("wb") as log:
        while chunk := source.read1(65536):
            keep = chunk[:max(0, limit - stored)]
            log.write(keep)
            stored += len(keep)
            dropped += len(chunk) - len(keep)
        if dropped:
            log.write(f"\n[dropped {dropped} bytes past {limit}]\n".encode())


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def private_env(scratch, runtime=None):
    # Start from an allowlist: inherited runtime/scene/bus/audio/activation selectors
    # must never turn an isolated run into a request to the live session.
    env = {key: os.environ[key] for key in
           ("PATH", "LD_LIBRARY_PATH", "FONTCONFIG_FILE", "FONTCONFIG_PATH", "LANG", "LC_ALL",
            "CARGO_TARGET_DIR", "TOHRU_DESKTOP_TARGET")
           if key in os.environ}
    env.update(HOME=str(scratch / "home"), XDG_RUNTIME_DIR=str(scratch / "run"),
               PLEAMAR_SOCKETS=str(scratch / "run/ipc"),
               DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(scratch / "run/bus"),
               DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(scratch / "run/no-system-bus"),
               PULSE_SERVER="unix:" + str(scratch / "run/no-audio"),
               PULSE_RUNTIME_PATH=str(scratch / "run/pulse"),
               PULSE_STATE_PATH=str(scratch / "pulse-state"),
               PULSE_CLIENTCONFIG=str(scratch / "pulse-client.conf"),
               PIPEWIRE_REMOTE="no-pipewire", PIPEWIRE_RUNTIME_DIR=str(scratch / "run"),
               WLR_BACKENDS="headless", WLR_HEADLESS_OUTPUTS="1", WLR_LIBINPUT_NO_DEVICES="1",
               WLR_RENDERER="pixman", WGPU_BACKEND="vulkan", PLEAMAR_BACKEND="vulkan",
               PLEAMAR_GENERIC="1")
    for kind in ("CONFIG", "CACHE", "DATA", "STATE"):
        env[f"XDG_{kind}_HOME"] = str(scratch / kind.lower())
    if runtime:
        env["TOHRU_RUNTIME"] = str(runtime)
    return env


class Run:
    def __init__(self, feature, runtime):
        (ROOT / ".tmp").mkdir(exist_ok=True)
        self.evidence = Path(tempfile.mkdtemp(prefix="verify-", dir=ROOT / ".tmp"))
        # Short paths are necessary for Unix socket sockaddr_un's 108-byte limit.
        self.scratch = Path(tempfile.mkdtemp(prefix="v-", dir=ROOT / ".tmp"))
        self.scratch.chmod(0o700)
        self.runtime = runtime
        self.feature = feature
        self.env = private_env(self.scratch, runtime)
        for path in ("home", "run", "run/ipc", "config", "cache", "data", "state"):
            (self.scratch / path).mkdir(parents=True, exist_ok=True, mode=0o700)
        (self.scratch / "pulse-client.conf").write_text("autospawn = no\n")
        (self.scratch / "config/tohru").mkdir()
        (self.scratch / "config/tohru/config.toml").write_text(
            "[assistant]\nvoice = false\n[assistant.discord]\nenabled = false\n"
            "[polkit]\nagent = false\n[appearance]\ncorners = false\n")
        # Captures refuse a persistent disk; the scratch runtime is on one.
        self.screens_dir = Path(tempfile.mkdtemp(prefix="tohru-screens-verify-", dir="/dev/shm")) / "screens"
        self.env["TOHRU_SCREENS_DIR"] = str(self.screens_dir)
        self.owners = []
        self.handles = []
        self.events = (self.evidence / "commands.jsonl").open("w")
        self.started = time.time()
        self.phase("launch")

    def phase(self, phase, **fields):
        self.events.write(json.dumps(dict(time=time.time(), phase=phase, **fields)) + "\n")
        self.events.flush()

    def command(self, argv, check=True):
        start = time.time()
        try:
            result = subprocess.run(list(map(str, argv)), env=self.env, text=True,
                                    capture_output=True, timeout=15, close_fds=True)
        except subprocess.TimeoutExpired as error:
            self.phase("command", argv=list(map(str, argv)), started=start, timeout=15)
            raise RuntimeError(f"Command timed out: {argv}") from error
        self.phase("command", argv=list(map(str, argv)), started=start,
                   exit=result.returncode, stdout=bounded(result.stdout), stderr=bounded(result.stderr))
        if check and result.returncode:
            raise RuntimeError(f"Command failed ({result.returncode}): {argv}: {result.stderr[:500]}")
        return result

    def start(self, argv, name, extra=None):
        process = subprocess.Popen(list(map(str, argv)), env={**self.env, **(extra or {})},
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, close_fds=True,
                                   start_new_session=True)
        reader = threading.Thread(target=pump, daemon=True,
                                  args=(process.stdout, self.evidence / f"{name}.log", SERVICE_LOG_LIMIT))
        reader.start()
        self.handles.append(reader)
        self.owners.append((name, process))
        self.phase("process-start", name=name, pid=process.pid, argv=list(map(str, argv)))
        return process

    def wait(self, operation, label):
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            for name, process in self.owners:
                if process.poll() is not None:
                    raise RuntimeError(f"{name} exited early: {process.returncode}; see {name}.log")
            value = operation()
            if value:
                return value
            time.sleep(0.1)
        raise RuntimeError(f"Timed out waiting for {label}")

    def cli(self, *args, check=True):
        return self.command([ROOT / "bin/tohru", *args], check=check)

    def get(self, fact):
        return self.command([self.runtime, "--say", "tohru", "get " + fact], check=False).stdout.strip()

    def launch(self):
        bus = self.scratch / "bus.conf"
        bus.write_text('<busconfig><type>session</type><listen>' + self.env["DBUS_SESSION_BUS_ADDRESS"] +
                       '</listen><auth>EXTERNAL</auth><policy context="default">'
                       '<allow send_destination="*"/><allow receive_sender="*"/>'
                       '<allow own="*"/></policy></busconfig>')
        self.start(["dbus-daemon", "--nofork", "--config-file=" + str(bus)], "bus")
        self.wait(lambda: (self.scratch / "run/bus").is_socket(), "private bus")
        config = self.scratch / "sway.conf"
        config.write_text("output HEADLESS-1 mode 1280x720@60Hz\noutput HEADLESS-1 bg #30232b solid_color\n")
        self.start(["sway", "--unsupported-gpu", "-c", config], "sway")
        socket = self.wait(lambda: next((self.scratch / "run").glob("sway-ipc.*.sock"), None), "Sway IPC")
        display = self.wait(lambda: next((p for p in (self.scratch / "run").glob("wayland-*") if p.is_socket()), None), "Wayland")
        self.env.update(WAYLAND_DISPLAY=display.name, SWAYSOCK=str(socket))
        self.app = self.start([ROOT / "bin/tohru-run"], "bar")
        self.wait(self.status, "usable JSON bar status (acceptance alone is insufficient)")

    def status(self):
        """The bar's settings status, or None while the render is busy or settings are not loaded."""
        result = self.cli("status", "--json", check=False)
        try:
            status = json.loads(result.stdout)
        except json.JSONDecodeError:
            return None
        return status if result.returncode == 0 and isinstance(status, dict) and "settings" in status else None

    def doctor(self):
        self.phase("doctor")
        for name, process in self.owners:
            if process.poll() is not None:
                raise RuntimeError(f"Owned {name} is not alive")
        expected = self.scratch / "run"
        for key in ("XDG_RUNTIME_DIR", "PLEAMAR_SOCKETS"):
            if not Path(self.env[key]).resolve().is_relative_to(expected):
                raise RuntimeError(f"Transport escaped: {key}")
        status = self.wait(self.status, "usable JSON bar status")
        outputs = json.loads(self.command(["swaymsg", "-t", "get_outputs", "-r"]).stdout)
        if not any(row["name"] == "HEADLESS-1" and row["active"] for row in outputs):
            raise RuntimeError("Owned display has no active HEADLESS-1")
        self.wait(lambda: bool(self.get("bar_time")), "usable bar clock")
        (self.evidence / "doctor.json").write_text(json.dumps({
            "result": "passed", "runtime": str(self.runtime), "runtime_sha256": digest(self.runtime),
            "owners": {name: process.pid for name, process in self.owners},
            "outputs": outputs, "status": status,
            "environment": self.env}, indent=2) + "\n")

    def capture(self, name):
        result = self.command([self.runtime, "--say", "tohru", "describe json"])
        json.loads(result.stdout)
        (self.evidence / f"{name}.json").write_text(result.stdout)
        self.command(["grim", "-o", "HEADLESS-1", self.evidence / f"{name}.png"])

    def calendar(self):
        self.phase("drive", feature="calendar-cli")
        self.cli("panel", "close")
        self.wait(lambda: self.get("cal_open") == "false", "closed calendar")
        self.capture("calendar-before")
        self.cli("panel", "open", "calendar")
        self.wait(lambda: self.get("cal_open") == "true" and bool(self.get("cal_title")), "open populated calendar")
        self.capture("calendar-open")
        self.cli("panel", "toggle", "calendar")
        self.wait(lambda: self.get("cal_open") == "false", "toggle closed calendar")
        self.cli("panel", "toggle", "calendar")
        self.wait(lambda: self.get("cal_open") == "true", "toggle opened calendar")
        self.cli("panel", "close")
        self.wait(lambda: self.get("cal_open") == "false", "panel close")
        self.capture("calendar-after")

    def settings(self):
        self.phase("drive", feature="settings-cli")
        config = self.scratch / "config/tohru/config.toml"
        self.cli("config", "set", "appearance.corner_size", "29")
        def persisted():
            import tomllib
            return config.exists() and tomllib.loads(config.read_text()).get("appearance", {}).get("corner_size") == 29
        self.wait(persisted, "persisted corner_size 29")
        self.cli("config", "reload")
        def observed():
            status = self.status()
            return status if status and status["settings"]["appearance.corner_size"]["value"] == 29 \
                and self.get("settings_error") == "" else None
        status = self.wait(observed, "observed settings")
        (self.evidence / "config.toml").write_bytes(config.read_bytes())
        (self.evidence / "settings-status.json").write_text(json.dumps(status))
        self.capture("settings")

    def screens(self):
        self.phase("drive", feature="screens")
        folder = self.screens_dir

        def frames():
            return sorted(p.name for p in folder.glob("*/[0-9]*.jpg")) if folder.is_dir() else []

        def captured():
            latest = folder / "HEADLESS-1/latest.jpg"
            return frames() if latest.is_file() and (folder / "index.json").is_file() else None

        def empty():
            return not folder.exists() or not any(folder.iterdir())

        def runners():
            found = self.command(["pgrep", "-f", "tohru-screens run"], check=False).stdout.split()
            return [int(pid) for pid in found]

        def listing(name):
            rows = [{"path": str(p.relative_to(folder)), "mode": oct(p.lstat().st_mode & 0o777),
                     "link": os.readlink(p) if p.is_symlink() else None}
                    for p in sorted(folder.rglob("*"))] if folder.is_dir() else []
            (self.evidence / f"{name}.json").write_text(json.dumps(rows, indent=2) + "\n")
            return rows

        self.cli("assistant", "close")
        self.wait(lambda: self.get("ai_open") == "false", "closed popup")
        self.wait(empty, "no captures while hidden")
        self.cli("assistant", "open")
        self.wait(lambda: self.get("ai_open") == "true", "open popup")
        self.wait(captured, "immediate capture on appear")
        first = frames()
        self.wait(lambda: len(frames()) > len(first) and frames(), "interval capture")
        rows = listing("screens-open")
        if (folder.stat().st_mode & 0o777) != 0o700 or any(
                r["mode"] not in ("0o600", "0o700") for r in rows if not r["link"]):
            raise RuntimeError("Capture permissions are not private")
        self.cli("assistant", "close")
        self.wait(empty, "captures deleted on hide")
        listing("screens-closed")

        self.cli("assistant", "open")
        self.wait(captured, "capture after reopen")
        stale = runners()
        if len(stale) != 1:
            raise RuntimeError(f"Expected one runner, found {stale}")
        os.kill(stale[0], signal.SIGKILL)
        self.wait(lambda: (runners() and runners() != stale and captured()) or None, "runner restarted after crash")
        if any(name in frames() for name in first):
            raise RuntimeError("Crash leftovers survived the restart")
        self.cli("assistant", "close")
        self.wait(empty, "captures deleted after recovery")

        self.cli("assistant", "open")
        self.wait(captured, "capture before bar exit")
        self.app.kill()
        self.app.wait(timeout=10)
        deadline = time.monotonic() + 10
        while not empty() and time.monotonic() < deadline:
            time.sleep(0.1)
        listing("screens-after-bar-exit")
        if not empty():
            raise RuntimeError("Captures survived the bar's exit")
        self.owners = [(name, p) for name, p in self.owners if p is not self.app]

    def cleanup(self):
        self.phase("cleanup")
        failures = []
        leftovers = {}
        for name, process in reversed(self.owners):
            try:
                # Signal the group even when its leader has exited: children such as
                # Xwayland keep the group alive after the leader is gone.
                signal_group(process.pid, signal.SIGTERM)
                deadline = time.monotonic() + 8
                while (process.poll() is None or group_members(process.pid)) and time.monotonic() < deadline:
                    time.sleep(0.05)
                if process.poll() is None or group_members(process.pid):
                    signal_group(process.pid, signal.SIGKILL)
                    process.wait(timeout=5)
                    deadline = time.monotonic() + 5
                    while group_members(process.pid) and time.monotonic() < deadline:
                        time.sleep(0.05)
                remaining = group_members(process.pid)
                if remaining:
                    leftovers[name] = remaining
                    failures.append(f"{name}: group {process.pid} still has {remaining}")
                self.phase("process-stop", name=name, pid=process.pid, exit=process.returncode,
                           group_remaining=remaining)
            except Exception as error:
                failures.append(f"{name}: {error}")
        for reader in self.handles:
            reader.join(timeout=5)
            if reader.is_alive():
                failures.append("log reader did not finish")
        shutil.rmtree(self.screens_dir.parent, ignore_errors=True)
        if not failures:
            shutil.rmtree(self.scratch)
        (self.evidence / "cleanup.json").write_text(json.dumps({"failures": failures,
            "scratch": str(self.scratch), "scratch_removed": not self.scratch.exists(),
            "processes": {name: process.poll() for name, process in self.owners},
            "group_leftovers": leftovers}, indent=2) + "\n")
        self.events.close()
        return failures

def preflight(runtime):
    missing = [tool for tool in ("dbus-daemon", "sway", "swaymsg", "grim") if not shutil.which(tool)]
    if missing:
        raise RuntimeError("Missing tools: " + ", ".join(missing))
    if not runtime or not runtime.is_file() or not os.access(runtime, os.X_OK):
        raise RuntimeError("Pass an executable patched runtime with --runtime")
    # This metadata flag does not open a display or load a model.
    result = subprocess.run([str(runtime), "--tohru-capabilities"], capture_output=True, text=True, timeout=10)
    if result.returncode:
        raise RuntimeError("Runtime lacks --tohru-capabilities")
    caps = json.loads(result.stdout)
    if caps.get("schema") != 1 or caps.get("version") != "0.7.27" or caps.get("tohru_portability") != 1:
        raise RuntimeError("Unsupported runtime capability marker")
    payload = runtime.parent / ".pleamar-wrapped"
    return {"runtime": str(runtime), "sha256": digest(runtime),
            "wrapped_payload_sha256": digest(payload) if payload.is_file() else None, "capabilities": caps,
            "limit": "Contract marker and hash, not proof this runtime was built from this checkout"}


def main():
    def interrupted(signum, frame):
        raise KeyboardInterrupt(f"Received signal {signum}")
    signal.signal(signal.SIGTERM, interrupted)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("doctor", "run"))
    parser.add_argument("--feature", choices=("calendar", "settings", "screens"), default="calendar")
    parser.add_argument("--runtime", type=lambda p: Path(p).resolve())
    args = parser.parse_args()
    if args.operation == "doctor":
        print(json.dumps(preflight(args.runtime), indent=2))
        return
    run = Run(args.feature, args.runtime)
    print(run.evidence, flush=True)
    failure = None
    try:
        head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        source = subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True)
        metadata = dict(head=head, dirty=source, feature=args.feature, started=run.started,
                        runtime=preflight(args.runtime))
        (run.evidence / "launch.json").write_text(json.dumps(metadata, indent=2) + "\n")
        if not os.environ.get("GPU_LEASE_ID"):
            raise RuntimeError("Native drive requires gpu-run reservation through cleanup")
        run.launch()
        run.doctor()
        getattr(run, args.feature)()
    except BaseException as error:
        failure = repr(error)
        (run.evidence / "failure.txt").write_text(traceback.format_exc())
    finally:
        errors = run.cleanup()
    artifacts = {str(p.relative_to(run.evidence)): digest(p) for p in run.evidence.rglob("*") if p.is_file()}
    summary = dict(result="failed" if failure or errors else "passed", feature=args.feature,
                   started=run.started, ended=time.time(), failure=failure, cleanup_failures=errors,
                   evidence=str(run.evidence), artifacts_sha256=artifacts)
    (run.evidence / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({key: summary[key] for key in ("result", "feature", "failure", "evidence")}))
    if failure or errors:
        sys.exit(1)


if __name__ == "__main__":
    main()
