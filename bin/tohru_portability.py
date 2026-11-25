"""Shared standalone configuration, bounded observations and generated integration.

No probing function changes desktop state or imports a speech/model package.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unicodedata

BIN = Path(os.environ.get("TOHRU_BIN_DIR") or Path(__file__).resolve().parent).resolve()
ROOT = BIN.parent
DATA = ROOT / "share/tohru"
DEFAULTS = {
    "shortcuts.mode": "compositor", "shortcuts.launcher_trigger": "",
    "shortcuts.session_trigger": "", "shortcuts.assistant_trigger": "",
    "wallpaper.enabled": False, "wallpaper.backend": "mpvpaper",
    "wallpaper.directory": "", "wallpaper.assignments": "",
    "wallpaper.executable": "mpvpaper", "wallpaper.options": "no-audio loop-file=inf",
    "idle.owner": "external", "idle.lock_seconds": 300, "idle.suspend_seconds": 0,
    "appearance.font_text": "Noto Sans", "appearance.font_display": "Noto Sans",
    "appearance.font_mono": "DejaVu Sans Mono",
    "assistant.voice_python": "python3", "assistant.voice_environment": "",
    "assistant.voice": True, "assistant.voice_source": "", "assistant.tts_engine": "kokoro",
    "assistant.vad_model": "", "assistant.asr_model": "", "assistant.turn_model": "",
    "assistant.kws_model": "", "assistant.tts_model": "", "assistant.voice_wake": "asr",
}


def xdg(kind):
    default = {"CONFIG": ".config", "STATE": ".local/state", "CACHE": ".cache"}[kind]
    return Path(os.environ.get(f"XDG_{kind}_HOME") or Path.home() / default)


def flatten(data, prefix=""):
    result = {}
    for key, value in data.items():
        name = f"{prefix}.{key}" if prefix else key
        if isinstance(value, dict):
            result.update(flatten(value, name))
        else:
            result[name] = value
    return result


def read_toml(path):
    if path.stat().st_size > 1024 * 1024:
        raise ValueError("configuration exceeds 1 MiB")
    with path.open("rb") as stream:
        data = stream.read(1024 * 1024 + 1)
    if len(data) > 1024 * 1024:
        raise ValueError("configuration exceeds 1 MiB")
    return tomllib.loads(data.decode("utf-8"))


def config():
    values, managed = dict(DEFAULTS), set()
    paths = [(xdg("CONFIG") / "tohru/config.toml", False)]
    if os.environ.get("TOHRU_MANAGED_CONFIG"):
        managed_path=Path(os.environ["TOHRU_MANAGED_CONFIG"])
        if managed_path.resolve() == paths[0][0].resolve():
            raise ValueError("managed config must be separate from writable config")
        paths.append((managed_path, True))
    for path, owned in paths:
        if not path.exists():
            if owned:
                raise ValueError("managed config is missing")
            continue
        layer = flatten(read_toml(path))
        values.update(layer)
        if owned:
            managed.update(layer)
    for name in ("wallpaper.enabled", "assistant.voice"):
        if type(values[name]) is not bool:
            raise ValueError(f"{name} must be boolean")
    if values["shortcuts.mode"] not in ("compositor","portal","disabled"):
        raise ValueError("shortcuts.mode must be compositor, portal or disabled")
    for action in ("launcher","session","assistant"):
        if not isinstance(values["shortcuts."+action+"_trigger"],str) or len(values["shortcuts."+action+"_trigger"].encode())>256:
            raise ValueError("invalid shortcut preferred trigger")
    if values["wallpaper.backend"] != "mpvpaper":
        raise ValueError("supported wallpaper backend: mpvpaper")
    if values["idle.owner"] not in ("external", "tohru", "disabled"):
        raise ValueError("idle.owner must be external, tohru or disabled")
    for name in ("idle.lock_seconds", "idle.suspend_seconds"):
        if type(values[name]) is not int or not 0 <= values[name] <= 86400:
            raise ValueError(f"{name} must be an integer in 0..86400")
    for name, default in DEFAULTS.items():
        if isinstance(default, str) and (not isinstance(values[name], str) or any(unicodedata.category(c) == "Cc" for c in values[name])):
            raise ValueError(f"invalid {name}")
    for name in ("appearance.font_text", "appearance.font_display", "appearance.font_mono"):
        if not values[name].strip() or len(values[name].encode())>256 or ":" in values[name]:
            raise ValueError(f"invalid {name}")
    for name in ("wallpaper.executable", "wallpaper.options", "assistant.voice_python"):
        if not values[name] or len(values[name].encode())>4096:
            raise ValueError(f"invalid {name}")
    for name in ("wallpaper.directory", "wallpaper.assignments", "assistant.voice_environment"):
        if len(values[name].encode())>4096 or values[name] and not Path(values[name]).is_absolute():
            raise ValueError(f"{name} must be an absolute path")
    if values["idle.owner"] == "tohru" and values["idle.suspend_seconds"] and values["idle.lock_seconds"] and values["idle.suspend_seconds"] <= values["idle.lock_seconds"]:
        raise ValueError("idle.suspend_seconds must exceed idle.lock_seconds")
    return values, managed


def probe(args, timeout=3):
    """Bound time and stored output, including a broken/hostile helper's output."""
    import selectors
    import signal
    import time
    process = None
    completed = False
    try:
        process = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                   start_new_session=True)
        data = bytearray()
        deadline = time.monotonic() + timeout
        with selectors.DefaultSelector() as selector:
            selector.register(process.stdout, selectors.EVENT_READ)
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return None, "timeout"
                if not selector.select(remaining):
                    return None, "timeout"
                chunk = os.read(process.stdout.fileno(), 65536)
                if not chunk:
                    try:
                        code = process.wait(timeout=max(.001, deadline-time.monotonic()))
                    except subprocess.TimeoutExpired:
                        return None, "timeout"
                    completed = True
                    return data.decode("utf-8", "replace") if code == 0 else None, "ok" if code == 0 else "unavailable"
                data.extend(chunk)
                if len(data) > 256 * 1024:
                    return None, "oversized response"
    except FileNotFoundError:
        return None, "missing"
    except OSError:
        return None, "permission denied or unavailable"
    finally:
        if process is not None and not completed:
            # Kill the entire probe group even if the leader already exited: it
            # may have left descendants holding the pipe open.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait()
        if process is not None and process.stdout:
            process.stdout.close()


def json_probe(args):
    out, state = probe(args)
    try:
        return json.loads(out) if out is not None else None, state
    except ValueError:
        return None, "invalid response"


def session_directory():
    runtime = os.environ.get("XDG_RUNTIME_DIR", "")
    if not runtime or not Path(runtime).is_dir() or Path(runtime).stat().st_uid != os.getuid():
        raise ValueError("XDG_RUNTIME_DIR must be an existing directory owned by this user")
    identity = os.environ.get("WAYLAND_DISPLAY", "") + "\0" + os.environ.get("XDG_SESSION_ID", "")
    if not os.environ.get("WAYLAND_DISPLAY"):
        raise ValueError("WAYLAND_DISPLAY is missing")
    return Path(runtime) / ("tohru-" + hashlib.sha256(identity.encode()).hexdigest()[:16])


def protocols():
    out, state = probe(["wayland-info"])
    if out is None:
        return None, state
    return sorted(set(re.findall(r"interface: '([^']+)'", out))), state


def parse_outputs(report):
    # wayland-utils 1.1 output format (upstream wayland-info.c), not workspace IPC.
    records = {}
    blocks = re.split(r"(?=interface: ')", report)
    for block in blocks:
        header = re.match(r"interface: 'wl_output',[^\n]*name:\s*(\d+)", block)
        if not header:
            continue
        item = {}
        name = re.search(r"^\s*name:\s*(.*?)\s*$", block, re.M)
        if name:
            item["name"] = name[1]
        scale = re.search(r"\bscale:\s*(\d+)", block)
        item["scale"] = max(1, int(scale[1])) if scale else 1
        transform = re.search(r"output_transform:\s*([^,\n]+)", block)
        if transform:
            item["transform"] = transform[1]
        mode = re.search(r"width:\s*(\d+) px, height:\s*(\d+) px,[^\n]*\n\s*flags:[^\n]*current", block)
        if mode:
            width,height = int(mode[1]),int(mode[2])
            if item.get("transform", "") in ("90°", "270°", "flipped 90°", "flipped 270°"):
                width,height = height,width
            item.update(width=width/item["scale"],height=height/item["scale"])
        records[int(header[1])] = item
    for block in blocks:
        if not block.startswith("interface: 'zxdg_output_manager_v1'"):
            continue
        for output in re.split(r"\bxdg_output_v1\s*\n", block)[1:]:
            identity = re.search(r"output:\s*(\d+)", output)
            if not identity or int(identity[1]) not in records:
                continue
            item = records[int(identity[1])]
            name = re.search(r"name: '([^']+)'", output)
            if name:
                item.setdefault("name", name[1])
            size = re.search(r"logical_width:\s*(\d+), logical_height:\s*(\d+)", output)
            if size:
                item.update(width=int(size[1]),height=int(size[2]))
    return sorted((item for item in records.values() if item.get("name")),key=lambda item:item["name"])


def outputs():
    out, state = probe(["wayland-info"])
    if out is None:
        return [], state
    result = parse_outputs(out)
    return result, "ok" if result or "interface: 'wl_output'" not in out else "unsupported-output-names"


def shortcut_control(operation="status"):
    import socket
    if operation not in ("status","setup","configure"):
        raise ValueError("unknown shortcut operation")
    endpoint=session_directory()/"shortcuts.sock"
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as client:
        client.settimeout(5)
        client.connect(str(endpoint))
        client.sendall(operation.encode()+b"\n")
        data=bytearray()
        while b"\n" not in data and len(data)<128*1024:
            chunk=client.recv(16384)
            if not chunk:break
            data.extend(chunk)
        result=json.loads(data)
        if not isinstance(result,dict) or 'error' in result:
            raise ValueError("shortcut owner operation unavailable")
        return result


def shortcuts(operation="status"):
    values,_=config()
    mode=values["shortcuts.mode"]
    if mode=="portal":
        try:return shortcut_control(operation)
        except (OSError,ValueError):
            if operation!="status":raise
            return {"owner":"Tohru actions via desktop portal", "state":"owner-unavailable", "editable":False,
                    "setup_available":False,"configure_available":False,"bindings":[],"registration":"portal-session"}
    if operation!="status":raise ValueError("compositor-managed shortcuts are read-only; portal mode is not enabled")
    if mode=="disabled":return {"owner":"disabled","state":"disabled","editable":False,"bindings":[]}
    # Runtime IPC only: a text file parse would not establish active bindings.
    if os.environ.get("HYPRLAND_INSTANCE_SIGNATURE") and not os.environ.get("PLEAMAR_GENERIC"):
        bindings, state = json_probe(["hyprctl", "-j", "binds"])
        if isinstance(bindings, list):
            rows = []
            for item in bindings[:512]:
                if not isinstance(item,dict):
                    continue
                mask = item.get("modmask", 0)
                mods = [name for bit, name in ((1,"Shift"),(4,"Ctrl"),(8,"Alt"),(64,"Super")) if isinstance(mask,int) and mask & bit]
                key = str(item.get("key") or item.get("keycode", ""))
                rows.append({"shortcut": "+".join(mods+[key]), "action": str(item.get("dispatcher", "")),
                             "description": str(item.get("description", "")), "submap": str(item.get("submap", "")),
                             "arguments": str(item.get("arg", "")),
                             "trigger": "release" if item.get("release") else "press",
                             "flags": ", ".join(flag for flag in ("release","repeat","locked","longPress","non_consuming") if item.get(flag))})
            owner = "Hyprland-managed"
        else:
            return {"owner": "Hyprland-managed", "state": state, "editable": False, "bindings": []}
    elif os.environ.get("SWAYSOCK"):
        # Sway exposes loaded config, not a complete active binding table. Label that precisely.
        response, state = json_probe(["swaymsg", "-t", "get_config", "-r"])
        return {"owner": "Sway-managed", "state": "runtime binding enumeration unsupported",
                "editable": False, "bindings": [], "config_available": isinstance(response, dict)}
    else:
        return {"owner": "compositor-managed or unavailable", "state": "unsupported",
                "editable": False, "bindings": []}
    seen = set()
    for row in rows:
        identity = (row["submap"], row["shortcut"], row["trigger"])
        row["conflict"] = identity in seen
        seen.add(identity)
    return {"owner": owner, "state": "runtime IPC snapshot" + (" (first 512 entries)" if len(bindings)>512 else ""), "editable": False, "bindings": rows}


def unit_text(name, spec, prefix, target, managed=None):
    # systemd escaping differs from shell escaping. Reject newlines and escape specifiers.
    def quote(value, expand=True):
        if any(c in str(value) for c in "\n\r\0"):
            raise ValueError("invalid unit value")
        escaped = str(value).replace('\\', '\\\\').replace('"', '\\"').replace('%','%%')
        return '"' + (escaped.replace('$','$$') if expand else escaped) + '"'
    command = spec["program"].split()
    executable = str(Path(prefix).resolve() / command[0])
    lines = ["[Unit]", f"Description={spec['description']}", f"PartOf={target}", f"After={target}",
             "ConditionEnvironment=WAYLAND_DISPLAY", "", "[Service]", "Type="+spec.get("type","simple"),
             "ExecStart="+" ".join(quote(x) for x in [executable]+command[1:]),
             "Restart=on-failure", "RestartSec=2", "KillMode="+spec.get("kill_mode","control-group"),
             "StandardOutput=journal", "StandardError=journal"]
    if spec.get("type") == "notify":
        lines += ["NotifyAccess=all", "TimeoutStartSec="+str(spec["timeout"])]
    if spec.get("restart_prevent_exit_status"):
        lines += ["RestartPreventExitStatus="+str(spec["restart_prevent_exit_status"])]
    if managed:
        lines += ["Environment="+quote("TOHRU_MANAGED_CONFIG="+managed, expand=False)]
    if spec.get("install", True):
        lines += ["", "[Install]", f"WantedBy={target}"]
    return "\n".join(lines)+"\n"


def generate_units(args):
    specs = json.loads((DATA / "session/services.json").read_text())
    selected = args.components.split(",")
    if not set(selected) <= specs.keys():
        raise ValueError("unknown component")
    if not re.fullmatch(r"[a-zA-Z0-9_.@:-]+\.target", args.target):
        raise ValueError("invalid target")
    # Lock is available on demand even when idle ownership stays external.
    selected = list(dict.fromkeys(selected+["lock"]))
    destination = Path(args.output)
    destination.mkdir(parents=True, exist_ok=True)
    for name in selected:
        path = destination / f"tohru-{name}.service"
        with path.open("x") as stream:
            stream.write(unit_text(name, specs[name], args.prefix, args.target, args.managed))
    print(f"Generated {len(selected)} units in {destination}; no services enabled or started")


def echo_cancel(args):
    source = DATA / "pipewire/60-tohru-echo-cancel.conf"
    content = source.read_text()
    if args.output:
        path = Path(args.output)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("x") as stream:
            stream.write(content)
        print(f"Created {path}; restart PipeWire yourself when convenient; routes unchanged")
    else:
        print(content, end="")


def greeter_check(args):
    import pwd
    import shlex
    checks = []
    def check(name, valid, detail):
        checks.append({"name": name, "state": "ok" if valid else "missing-or-unreadable", "detail": detail})
    data = read_toml(Path(args.config))
    session = data.get("default_session", {})
    command = session.get("command", "")
    user = session.get("user", "")
    if not isinstance(command, str) or not command or not isinstance(user, str) or not user:
        raise ValueError("greetd requires default_session command and user")
    argv = shlex.split(command)
    check("host-command", bool(argv and shutil.which(argv[0])), "Configured hosting command executable; arguments were not executed")
    try:
        account = pwd.getpwnam(user)
        check("greeter-user", account.pw_uid != 0, "Configured greeter account exists and is not root")
    except KeyError:
        check("greeter-user", False, "Create/use the distro greeter account; do not use a desktop account")
    settings = read_toml(Path(args.settings))
    greeter = settings.get("greeter", {})
    assets = [greeter.get("wallpaper")] + list(greeter.get("avatars", {}).values())
    for index, path in enumerate(assets):
        if path:
            check("asset."+str(index), isinstance(path,str) and Path(path).is_file() and os.access(path,os.R_OK), "Configured asset readability checked as current UID")
    dirs = args.sessions_dir or ["/usr/local/share", "/usr/share"]
    found = 0
    for directory in dirs:
        for kind in ("wayland-sessions", "xsessions"):
            path = Path(directory)/kind
            if path.is_dir():
                found += sum(1 for entry in list(path.glob("*.desktop"))[:64] if os.access(entry,os.R_OK))
    check("sessions", found > 0, f"{found} readable desktop session files; Exec/login not tested")
    check("greetd-pam", Path("/etc/pam.d/greetd").is_file(), "Have a distro administrator review the PAM stack")
    for role in ("font_text", "font_display"):
        family = settings.get("appearance", {}).get(role, "Noto Sans")
        if not isinstance(family,str):
            raise ValueError("greeter font families must be strings")
        match,state = probe(["fc-match", "--format=%{family}",family])
        check(role, bool(match), "Resolved: "+match[:160] if match else state)
    result = {"schema":1,"read_only":True,"checked_as_uid":os.getuid(),"checks":checks,
              "limits":"Run under the intended greeter identity/environment. No compositor, authentication, login session or privileged changes attempted."}
    print(json.dumps(result) if args.json else "\n".join(f"{c['state']}: {c['name']}: {c['detail']}" for c in checks)+"\n"+result["limits"])
    return 0 if all(c["state"]=="ok" for c in checks) else 1


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    units = commands.add_parser("session-units")
    units.add_argument("--output", required=True)
    units.add_argument("--prefix", default=str(BIN))
    units.add_argument("--target", default="graphical-session.target")
    units.add_argument("--components", default="bar,session,launcher")
    units.add_argument("--managed")
    echo = commands.add_parser("echo-cancel")
    echo.add_argument("--output")
    shortcuts_parser=commands.add_parser("shortcuts")
    shortcuts_parser.add_argument("operation",nargs="?",choices=("status","setup","configure"),default="status")
    doctor = commands.add_parser("doctor")
    doctor.add_argument("--json", action="store_true")
    greeter = commands.add_parser("greeter-check")
    greeter.add_argument("--config", default="/etc/greetd/config.toml")
    greeter.add_argument("--settings", default="/etc/greetd/tohru.toml")
    greeter.add_argument("--sessions-dir", action="append")
    greeter.add_argument("--json", action="store_true")
    args = parser.parse_args()
    if args.command == "session-units":
        generate_units(args)
    elif args.command == "echo-cancel":
        echo_cancel(args)
    elif args.command == "shortcuts":
        print(json.dumps(shortcuts(args.operation)))
    elif args.command == "doctor":
        from tohru_doctor import doctor
        return doctor(args.json)
    elif args.command == "greeter-check":
        return greeter_check(args)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError, tomllib.TOMLDecodeError) as error:
        print(f"tohru: {error}", file=sys.stderr)
        sys.exit(2)
