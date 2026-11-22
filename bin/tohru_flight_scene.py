#!/usr/bin/env python3
"""Read focused Hyprland geometry during one entrance. No titles or captures."""

import hashlib
import json
import math
import os
from pathlib import Path
import socket
import sys
import subprocess
import time


def query(path, command):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(0.18)
        client.connect(str(path))
        client.sendall(b"j/" + command)
        with client.makefile("rb") as stream:
            raw = stream.read(262145)
    if len(raw) > 262144:
        raise ValueError("oversized compositor response")
    return json.loads(raw)


def clear_landing(clients, target, output):
    # Hyprland does not supply a general stacking order here. Be conservative
    # about switching the landed actor back to its normal overlay level.
    if len(clients) > 256:
        return False
    right = output["ox"] + output["w"]
    bottom = output["oy"] + output["h"]
    for client in clients:
        if not client.get("mapped") or client.get("hidden"):
            continue
        if client.get("workspace", {}).get("id") != output[
            "workspace"
        ] and not client.get("pinned"):
            continue
        if client.get("address") == target:
            continue
        x, y = client["at"]
        w, h = client["size"]
        if x < right and x + w > right - 228 and y < bottom and y + h > bottom - 248:
            return False
    return True


def flight_output(layers, owner_pid):
    """Hyprland reports mapped layer surfaces grouped by output and level."""
    matches = [
        name
        for name, output in layers.items()
        for surfaces in output["levels"].values()
        for surface in surfaces
        if surface.get("namespace") == "tohru-dragon-flight"
        and surface.get("pid") == owner_pid
    ]
    return matches[0] if len(matches) == 1 else None


def snapshot(monitors, win, clients, output):
    selected = [m for m in monitors if m["name"] == output]
    if len(selected) != 1:
        return {}
    m = selected[0]
    scale, rotation = m["scale"], m["transform"]
    if not 0.25 <= scale <= 8 or rotation not in range(8):
        return {}
    w, h = m["width"], m["height"]
    if rotation % 2:
        w, h = h, w
    workspace = m["activeWorkspace"]["id"]
    out = dict(
        output=m["name"],
        output_id=m["id"],
        ox=m["x"],
        oy=m["y"],
        w=round(w / scale),
        h=round(h / scale),
        scale=scale,
        rotation=rotation,
        workspace=workspace,
        fullscreen=any(
            c.get("mapped")
            and not c.get("hidden")
            and c.get("monitor") == m["id"]
            and (c.get("workspace", {}).get("id") == workspace or c.get("pinned"))
            and c.get("fullscreen", 0)
            for c in clients
        ),
        blocked=bool(
            m.get("specialWorkspace", {}).get("name")
            or m.get("specialWorkspace", {}).get("id")
            or m.get("solitary") not in (None, 0, "0", "0x0")
            or (win.get("monitor") == m["id"] and win.get("fullscreen", 0))
        ),
    )
    out["blocked"] |= out["fullscreen"]
    if (
        win.get("mapped")
        and not win.get("hidden", True)
        and win.get("address")
        and win.get("monitor") == m["id"]
        and win.get("workspace", {}).get("id") == workspace
        and clear_landing(clients, win["address"], out)
    ):
        x, y = win["at"]
        width, height = win["size"]
        if all(math.isfinite(v) and abs(v) <= 65536 for v in (x, y, width, height)):
            out["target"] = dict(
                id=win["address"], x=x - m["x"], y=y - m["y"], w=width, h=height
            )
    return out


def session_state(session):
    if not session or not session.isalnum():
        return {}
    try:
        reply = subprocess.run(
            ["loginctl", "show-session", session, "-p", "Active", "-p", "LockedHint"],
            capture_output=True,
            text=True,
            check=True,
            timeout=0.3,
        )
        fields = dict(
            line.split("=", 1) for line in reply.stdout.splitlines() if "=" in line
        )
        result = {}
        if fields.get("Active") in ("yes", "no"):
            result["session_active"] = fields["Active"] == "yes"
        if fields.get("LockedHint") in ("yes", "no"):
            result["unlocked"] = fields["LockedHint"] == "no"
        return result
    except (OSError, subprocess.SubprocessError, UnicodeError):
        return {}


def main():
    signature = os.environ.get("HYPRLAND_INSTANCE_SIGNATURE", "")
    runtime = Path(os.environ["XDG_RUNTIME_DIR"])
    identity = (
        os.environ.get("WAYLAND_DISPLAY", "")
        + "\0"
        + os.environ.get("XDG_SESSION_ID", "")
    )
    # The native lock authority creates this endpoint only during its lock cycle.
    # For decorative motion, even a stale endpoint is reason to stay still.
    lock = runtime / (
        "tohru-lock-" + hashlib.sha256(identity.encode()).hexdigest()[:16] + ".status"
    )
    hypr = bool(
        signature and "/" not in signature and os.environ.get("PLEAMAR_GENERIC") != "1"
    )
    path = runtime / "hypr" / signature / ".socket.sock"
    if hypr:
        try:
            with (path.parent / "hyprland.lock").open("rb") as lockfile:
                display = lockfile.read(1024).decode().splitlines()[1]
            hypr = (runtime / display).samefile(runtime / os.environ["WAYLAND_DISPLAY"])
        except (KeyError, IndexError, OSError, UnicodeError):
            hypr = False
    # Pleamar spawns this executable directly. Its PID distinguishes this
    # character from another running scene advertising the same namespace.
    owner_pid = os.getppid()
    actor = sys.argv[1:] == ["--actor"]
    root = Path(__file__).resolve().parents[1]
    art = next(
        (
            str(p)
            for p in (
                root / "shell/assistant/dragon",
                root / "share/tohru/shell/assistant/dragon",
            )
            if (p / "body.webp").is_file()
        ),
        "",
    )
    previous, status = None, {}
    for index in range(336 if actor else 64):
        scene = {}
        if hypr:
            try:
                output = flight_output(query(path, b"layers"), owner_pid)
                if not output and previous is None:
                    # The first visible configure can race the geometry helper.
                    for _attempt in range(3):
                        time.sleep(0.035)
                        output = flight_output(query(path, b"layers"), owner_pid)
                        if output:
                            break
                scene = snapshot(
                    query(path, b"monitors"),
                    query(path, b"activewindow"),
                    query(path, b"clients"),
                    output,
                )
            except (OSError, ValueError, KeyError, TypeError, IndexError):
                hypr = False
        if actor:
            if index % 6 == 0:
                status = session_state(os.environ.get("XDG_SESSION_ID"))
            scene.update(status, art=art)
            if scene.get("unlocked") is False or scene.get("session_active") is False:
                scene["blocked"] = True
        if lock.exists():
            if actor:
                scene["unlocked"] = False
            scene["blocked"] = True
        if scene != previous:
            print(json.dumps(scene, separators=(",", ":")), flush=True)
            previous = scene
        time.sleep(0.18)


if __name__ == "__main__":
    main()
