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
import tempfile
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


def process_start(pid):
    path=Path('/proc')/str(pid)
    if path.stat().st_uid!=os.getuid():raise ValueError("idle manager must belong to this user")
    return int((path/'stat').read_text().rsplit(')',1)[1].split()[19])


def callback(generation, event, manager=None):
    args=[sys.executable, str(BIN / "tohru_afk_idle.py"), "notify", event,"--generation", str(generation)]
    if manager:args += ["--manager-pid",str(manager[0]),"--manager-start",str(manager[1])]
    return shlex.join(args)


def sway_arguments(seconds, generation, manager=None):
    # Deliberately no -w, before-sleep, lock, unlock, after-resume, or suspend.
    # swayidle delegates keyboard/pointer observation and inhibitor semantics to
    # the compositor's maintained idle protocol rather than reading input devices.
    return ["swayidle", "-C", "/dev/null", "timeout", str(seconds), callback(generation, "idle",manager),
            "resume", callback(generation, "resume",manager)]


def owned_callbacks(values):
    """Capture a source generation when the owned idle process is constructed.

    Missing/restarted adapters safely leave AFK inactive until that owner is
    explicitly restarted. Reading a fresh token inside a delayed callback would
    let an old callback impersonate a new observer generation.
    """
    values = effective_config(values)
    if not values.get("assistant.afk_enabled"):
        return []
    try:
        registration = read_registration()
    except (OSError, ValueError, KeyError):
        return []
    if registration["seconds"] != values["assistant.afk_idle_seconds"]:
        return []
    manager=(os.getpid(),process_start(os.getpid()))
    return sway_arguments(registration["seconds"], registration["generation"],manager)[3:]


def effective_config(values):
    path = endpoint()[0].parent / "afk-effective.json"
    if not path.exists():
        return values
    if path.stat().st_size > 2048:
        raise ValueError("invalid effective AFK configuration")
    data = json.loads(path.read_text())
    if (not isinstance(data, dict) or data.get("enabled") is not True
            or type(data.get("seconds")) is not int or not 1 <= data["seconds"] <= 86400
            or data.get("source") not in ("passive", "callbacks")
            or data.get("owner") not in ("external", "tohru", "disabled")):
        raise ValueError("invalid effective AFK configuration")
    return {**values, "assistant.afk_enabled": True, "assistant.afk_idle_seconds": data["seconds"],
            "assistant.afk_idle_source": data["source"], "idle.owner": data["owner"]}


def episode_state(directory):
    path=directory/"afk-episode.json"
    if not path.exists():
        return {"seed":secrets.randbelow(2147483646)+1,"episode":0,"idle":False}
    if path.stat().st_size>1024:raise ValueError("invalid AFK episode state")
    state=json.loads(path.read_text())
    if (not isinstance(state,dict) or type(state.get("seed")) is not int or not 0<=state["seed"]<=2147483646
            or type(state.get("episode")) is not int or not 0<=state["episode"]<=2147483646
            or type(state.get("idle")) is not bool):
        raise ValueError("invalid AFK episode state")
    return state


def episode_event(directory,state,event):
    if event=="idle":
        if state["idle"]:return False
        if state["episode"]>=2147483646:raise ValueError("AFK episode counter exhausted")
        state["episode"]+=1
        state["idle"]=True
    else:state["idle"]=False
    # Persist before notification: a crash or VM/helper restart may skip a gag,
    # but cannot roll again during the same observed idle episode. A missed
    # resume while the helper is down conservatively waits for another actual
    # compositor resume; no elapsed-time guess reconstructs missing input.
    with tempfile.NamedTemporaryFile(mode="w",dir=directory,prefix="afk-episode-",delete=False) as stream:
        temporary=Path(stream.name)
        json.dump(state,stream)
    os.replace(temporary,directory/"afk-episode.json")
    return True


def notify(event, generation, manager=None):
    address, _ = endpoint()
    if event=="resume":
        # Input cancellation must not wait behind a logind/heartbeat probe in
        # the server. This only stops presentation; durable episode rearm still
        # belongs to the server's source-liveness check.
        try:
            registration=read_registration()
            identity=manager or [registration.get("observer_pid"),registration.get("observer_start")]
            if (registration["generation"]==generation and type(identity[0]) is int
                    and process_start(identity[0])==identity[1]):
                emit("resume",generation)
        except (OSError,ValueError,TypeError,IndexError):pass
    with socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM) as client:
        client.settimeout(.3)
        client.connect(str(address))
        client.send(json.dumps({"event": event, "generation": generation,"manager":manager}).encode())


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
    values = effective_config(values)
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
    last_manager=None
    pending_resume=None
    def manager_alive(identity):
        if child is not None:return child.poll() is None
        if not isinstance(identity,(list,tuple)) or len(identity)!=2 or any(type(value) is not int for value in identity):return False
        try:return identity is not None and process_start(identity[0])==identity[1]
        except (OSError,ValueError,TypeError,IndexError):return False
    with (address.parent / "afk.lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        episode=episode_state(address.parent)
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
                unreachable = 0
                monitor = session_monitor()
                if values["idle.owner"] == "external" and values["assistant.afk_idle_source"] == "passive":
                    child = subprocess.Popen(sway_arguments(seconds, generation), start_new_session=True)
                    registration.write_text(json.dumps({"schema":1,"generation":generation,"seconds":seconds,
                        "observer_pid":child.pid,"observer_start":process_start(child.pid)}))
                while not stopped:
                    if child is not None and child.poll() is not None:
                        raise ValueError("AFK idle observer exited; current idle state is unknown")
                    if pending_resume is not None and time.monotonic()>=pending_resume[0]:
                        if manager_alive(pending_resume[1]):episode_event(address.parent,episode,"resume")
                        pending_resume=None
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
                        reachable = emit("source", generation)
                        emit("seed", episode["seed"])
                        unreachable = 0 if reachable else unreachable + 1
                        if unreachable >= 3:
                            print("AFK native bar unavailable; stopping passive observers", file=sys.stderr)
                            break
                        emit("threshold", seconds)
                        safe = manager_alive(last_manager) and monitor is not None and monitor.poll() is None and session_state()
                        emit("safe" if safe else "unsafe", generation)
                        next_heartbeat = time.monotonic() + 2
                    try:
                        raw = server.recv(1024)
                    except socket.timeout:
                        continue
                    if stopped:
                        break
                    if child is not None and child.poll() is not None:
                        raise ValueError("AFK idle observer exited; ignoring its shutdown callbacks")
                    try:
                        message = json.loads(raw)
                    except (ValueError, UnicodeDecodeError):
                        continue
                    if not isinstance(message, dict) or type(message.get("generation")) is not int or message.get("generation") != generation:
                        continue
                    event = message.get("event")
                    manager=message.get("manager")
                    if not manager_alive(manager):continue
                    last_manager=manager
                    if event=="resume":
                        # Stop the gag immediately, but only rearm persistence
                        # once the source is still alive. swayidle also runs
                        # resume commands during shutdown, which is not input.
                        emit("resume",generation)
                        pending_resume=(time.monotonic()+.2,manager)
                    elif event=="idle" and episode_event(address.parent,episode,event):
                        emit("source", generation)
                        emit("seed", episode["seed"])
                        emit("threshold", seconds)
                        emit("episode",episode["episode"])
                        safe=monitor is not None and monitor.poll() is None and session_state()
                        emit("safe" if safe else "unsafe",generation)
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
    callbacks=sub.add_parser("callbacks")
    callbacks.add_argument("--manager-pid",type=int,required=True)
    sub.add_parser("assets")
    control = sub.add_parser("control")
    control.add_argument("action", choices=("start", "stop"))
    control.add_argument("--idle-seconds", type=int)
    control.add_argument("--source", choices=("passive", "callbacks"))
    control.add_argument("--owner", choices=("external", "tohru", "disabled"))
    notice = sub.add_parser("notify")
    notice.add_argument("event", choices=("idle", "resume"))
    notice.add_argument("--generation", type=int, required=True)
    notice.add_argument("--manager-pid",type=int)
    notice.add_argument("--manager-start",type=int)
    args = parser.parse_args()
    if args.command == "assets":
        for path in (BIN.parent/"shell/assistant/dragon",BIN.parent/"share/tohru/shell/assistant/dragon"):
            if all((path/name).is_file() for name in ("afk.webp","afk-closeup.png","afk-bust.png")):
                print(json.dumps({"art":str(path.resolve())}));return 0
        raise ValueError("AFK artwork unavailable")
    if args.command == "serve":
        return serve()
    if args.command == "control":
        directory=endpoint()[0].parent
        effective=directory/"afk-effective.json"
        values=config()[0] if args.action=="start" else {"idle.owner":"external"}
        try:before=effective_config(values)
        except (OSError,ValueError):
            if args.action=="start":raise
            before=values
        if args.action=="start" and args.idle_seconds is not None:
            if not 1 <= args.idle_seconds <= 86400 or args.source is None or args.owner is None:
                raise ValueError("invalid effective AFK source")
            data={"enabled":True,"seconds":args.idle_seconds,"source":args.source,"owner":args.owner}
            with tempfile.NamedTemporaryFile(mode="w",dir=directory,prefix="afk-effective-",delete=False) as stream:
                temporary=Path(stream.name)
                json.dump(data,stream)
            os.replace(temporary,effective)
        action="restart" if args.action=="start" else "stop"
        out,state=probe(["systemctl","--user",action,"tohru-afk.service"],timeout=4)
        if args.action=="stop":effective.unlink(missing_ok=True)
        if out is None:raise ValueError("AFK service "+state+"; install the optional generated unit")
        owner=args.owner if args.action=="start" else before["idle.owner"]
        if owner=="tohru":
            if args.action=="start":
                deadline=time.monotonic()+1
                while time.monotonic()<deadline:
                    try:
                        if read_registration()["seconds"]==args.idle_seconds:break
                    except (OSError,ValueError):pass
                    time.sleep(.02)
            # Only the explicitly owned, already active Tohru manager is refreshed.
            # try-restart never starts another lock/suspend owner or touches an
            # external hypridle/swayidle process.
            probe(["systemctl","--user","try-restart","tohru-idle.service"],timeout=4)
        return 0
    if args.command == "callbacks":
        value = read_registration()
        manager=(args.manager_pid,process_start(args.manager_pid))
        print(json.dumps({"seconds": value["seconds"], "idle": callback(value["generation"], "idle",manager),
                          "resume": callback(value["generation"], "resume",manager)}))
        return 0
    manager=[args.manager_pid,args.manager_start] if args.manager_pid is not None and args.manager_start is not None else None
    notify(args.event, args.generation,manager)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError) as error:
        print(f"tohru-afk-idle: {error}", file=sys.stderr)
        sys.exit(1)
