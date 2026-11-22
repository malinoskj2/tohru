#!/usr/bin/env python3
"""Maintained desktop idle notifications, never a lock or suspend owner.

Systemd owns `serve`. External ownership uses a passive swayidle notification
client containing only timeout/resume callbacks; Tohru ownership adds callbacks
to its existing swayidle process instead. No timestamps infer input activity.
"""
import argparse
import fcntl
import json
import os
from pathlib import Path
import secrets
import shlex
import signal
import socket
import subprocess
import sys
import time

from tohru_portability import BIN, config, probe, protocols, session_directory


def endpoint():
    directory = session_directory()
    directory.mkdir(mode=0o700, parents=True, exist_ok=True)
    info = directory.stat()
    if info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError("AFK runtime directory must be private and user-owned")
    return directory / "afk.socket", directory / "afk.json"


def read_registration():
    _, path = endpoint()
    if path.stat().st_size > 4096:
        raise ValueError("invalid AFK registration")
    value = json.loads(path.read_text())
    if (value.get("schema") != 1 or type(value.get("generation")) is not int
            or not 1 <= value["generation"] <= 2147483646
            or type(value.get("seconds")) is not int or not 1 <= value["seconds"] <= 86400):
        raise ValueError("invalid AFK registration")
    return value


def callback(generation, event):
    return shlex.join([sys.executable, str(BIN / "tohru_afk_idle.py"), "notify", event,
                       "--generation", str(generation)])


def sway_arguments(seconds, generation):
    # Deliberately no -w, before-sleep, lock, unlock, after-resume, or suspend.
    # swayidle delegates keyboard/pointer observation and inhibitor semantics to
    # the compositor's maintained idle protocol rather than reading input devices.
    return ["swayidle", "-C", "/dev/null", "timeout", str(seconds), callback(generation, "idle"),
            "resume", callback(generation, "resume")]


def owned_callbacks(values):
    """Capture a source generation when the owned idle process is constructed.

    Missing/restarted adapters safely leave AFK inactive until that owner is
    explicitly restarted. Reading a fresh token inside a delayed callback would
    let an old callback impersonate a new observer generation.
    """
    if not values.get("assistant.afk_enabled"):
        return []
    try:
        registration = read_registration()
    except (OSError, ValueError, KeyError):
        return []
    if registration["seconds"] != values["assistant.afk_idle_seconds"]:
        return []
    return sway_arguments(registration["seconds"], registration["generation"])[3:]


def notify(event, generation):
    address, _ = endpoint()
    with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as client:
        client.settimeout(.3)
        client.connect(str(address))
        client.send(json.dumps({"event": event, "generation": generation}).encode())


def emit(name, generation):
    runtime = os.environ.get("TOHRU_RUNTIME", "pleamar")
    out, state = probe([runtime, "--say", "tohru", f"emit afk_{name} {generation}"], timeout=1)
    return out is not None


def session_state():
    session = os.environ.get("XDG_SESSION_ID")
    if not session:
        return False
    out, _ = probe(["loginctl", "show-session", session, "-p", "Active", "-p", "LockedHint", "-p", "State"], timeout=.5)
    if out is None:
        return False
    properties = dict(line.split("=", 1) for line in out.splitlines() if "=" in line)
    return properties.get("Active") == "yes" and properties.get("LockedHint") == "no" and properties.get("State") == "active"


def session_monitor():
    try:
        child = subprocess.Popen(["busctl", "--system", "--match=type='signal',sender='org.freedesktop.login1'", "monitor"],
                                 stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, start_new_session=True)
        os.set_blocking(child.stdout.fileno(), False)
        return child
    except OSError:
        return None


def terminate(child):
    if child is None:
        return
    try:
        os.killpg(child.pid, signal.SIGTERM)
        child.wait(timeout=2)
    except ProcessLookupError:
        child.wait(timeout=2)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait(timeout=2)
    if child.stdout is not None:
        child.stdout.close()


def serve():
    values, _ = config()
    if not values["assistant.afk_enabled"] or values["idle.owner"] == "disabled":
        return 0
    caps, state = protocols()
    if caps is None or "ext_idle_notifier_v1" not in caps:
        raise ValueError("AFK inactive: ext_idle_notifier_v1 unavailable or unknown (" + state + ")")
    address, registration = endpoint()
    generation = secrets.randbelow(2147483646) + 1
    seconds = values["assistant.afk_idle_seconds"]
    stopped = False

    def shutdown(signum, frame):
        nonlocal stopped
        stopped = True

    signal.signal(signal.SIGTERM, shutdown)
    signal.signal(signal.SIGINT, shutdown)
    child, monitor = None, None
    with (address.parent / "afk.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as server:
            address.unlink(missing_ok=True)
            server.bind(str(address))
            server.settimeout(.2)
            registration.write_text(json.dumps({"schema": 1, "generation": generation, "seconds": seconds}))
            os.chmod(registration, 0o600)
            try:
                # A heartbeat establishes source liveness only. It never reports
                # idle, creates an episode, or resets an already consumed one.
                next_heartbeat = 0
                monitor = session_monitor()
                if values["idle.owner"] == "external" and values["assistant.afk_idle_source"] == "passive":
                    child = subprocess.Popen(sway_arguments(seconds, generation), start_new_session=True)
                while not stopped:
                    if child is not None and child.poll() is not None:
                        raise ValueError("AFK idle observer exited; current idle state is unknown")
                    if monitor is not None and monitor.poll() is None:
                        try:
                            changed = os.read(monitor.stdout.fileno(), 16384)
                        except BlockingIOError:
                            changed = b""
                        if changed:
                            # Any logind signal invalidates the previous proof
                            # before a fresh bounded observation. This includes
                            # lock, inactive session and PrepareForSleep signals.
                            emit("unsafe", generation)
                            next_heartbeat = 0
                    if time.monotonic() >= next_heartbeat:
                        emit("source", generation)
                        safe = monitor is not None and monitor.poll() is None and session_state()
                        emit("safe" if safe else "unsafe", generation)
                        next_heartbeat = time.monotonic() + 2
                    try:
                        raw = server.recv(1024)
                    except socket.timeout:
                        continue
                    try:
                        message = json.loads(raw)
                    except (ValueError, UnicodeDecodeError):
                        continue
                    if not isinstance(message, dict) or type(message.get("generation")) is not int or message.get("generation") != generation:
                        continue
                    event = message.get("event")
                    if event in ("idle", "resume"):
                        emit("source", generation)
                        emit(event, generation)
            finally:
                emit("lost", generation)
                # Keep both observer groups inside the systemd helper lifetime.
                terminate(child)
                terminate(monitor)
                address.unlink(missing_ok=True)
                registration.unlink(missing_ok=True)
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("serve")
    sub.add_parser("callbacks")
    notice = sub.add_parser("notify")
    notice.add_argument("event", choices=("idle", "resume"))
    notice.add_argument("--generation", type=int, required=True)
    args = parser.parse_args()
    if args.command == "serve":
        return serve()
    if args.command == "callbacks":
        value = read_registration()
        print(json.dumps({"seconds": value["seconds"], "idle": callback(value["generation"], "idle"),
                          "resume": callback(value["generation"], "resume")}))
        return 0
    notify(args.event, args.generation)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError) as error:
        print(f"tohru-afk-idle: {error}", file=sys.stderr)
        sys.exit(1)
