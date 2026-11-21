#!/usr/bin/env python3
"""End-to-end greeter check against a mock GREETD_SOCK daemon on a private headless Sway."""
import argparse
import json
import os
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

sys.dont_write_bytecode = True

ROOT = Path(__file__).resolve().parents[2]


def read_frame(conn):
    header = b""
    while len(header) < 4:
        chunk = conn.recv(4 - len(header))
        if not chunk:
            return None
        header += chunk
    length = struct.unpack("=I", header)[0]
    body = b""
    while len(body) < length:
        chunk = conn.recv(length - len(body))
        if not chunk:
            return None
        body += chunk
    return json.loads(body.decode("utf-8"))


def write_frame(conn, obj):
    payload = json.dumps(obj).encode("utf-8")
    conn.sendall(struct.pack("=I", len(payload)) + payload)


class MockGreetd:
    def __init__(self, sock_path: Path, expected_password: str):
        self.sock_path = sock_path
        self.expected_password = expected_password
        self.requests = []
        self.started = None
        self.stop_event = threading.Event()
        self.server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.server.bind(str(sock_path))
        self.server.listen(4)
        self.server.settimeout(0.2)
        self.thread = threading.Thread(target=self._serve, daemon=True)
        self.thread.start()

    def _serve(self):
        while not self.stop_event.is_set():
            try:
                conn, _ = self.server.accept()
            except socket.timeout:
                continue
            except OSError:
                break
            with conn:
                conn.settimeout(5.0)
                while not self.stop_event.is_set():
                    req = read_frame(conn)
                    if req is None:
                        break
                    self.requests.append(req)
                    kind = req.get("type")
                    if kind == "create_session":
                        write_frame(
                            conn,
                            {
                                "type": "auth_message",
                                "auth_message_type": "secret",
                                "auth_message": "Password: ",
                            },
                        )
                    elif kind == "post_auth_message_response":
                        if req.get("response") == self.expected_password:
                            write_frame(conn, {"type": "success"})
                        else:
                            write_frame(
                                conn,
                                {
                                    "type": "error",
                                    "error_type": "auth_error",
                                    "description": "Incorrect password",
                                },
                            )
                    elif kind == "start_session":
                        self.started = req
                        write_frame(conn, {"type": "success"})
                    elif kind == "cancel_session":
                        write_frame(conn, {"type": "success"})
                    else:
                        write_frame(
                            conn,
                            {
                                "type": "error",
                                "error_type": "error",
                                "description": f"Unexpected request {kind}",
                            },
                        )

    def close(self):
        self.stop_event.set()
        try:
            self.server.close()
        except OSError:
            pass
        self.thread.join(timeout=2)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--runtime", required=True, help="Path to patched pleamar binary")
    parser.add_argument("--wtype", default=None, help="Optional path to wtype for live key injection")
    parser.add_argument("--grim", default=None, help="Optional path to grim to capture a preview screenshot")
    parser.add_argument("--screenshot", default=None, help="Destination path for optional preview screenshot")
    args = parser.parse_args()

    runtime = Path(args.runtime).resolve()
    assert runtime.is_file(), f"missing runtime: {runtime}"

    # 1. Preflight both styled and plain greeter scenes plus lock scenes.
    for scene in [
        ROOT / "shell/greeter/greeter.plm",
        ROOT / "shell/greeter/plain.plm",
        ROOT / "shell/lock/lock.plm",
        ROOT / "shell/lock/plain.plm",
    ]:
        res = subprocess.run([str(runtime), "--check", str(scene)], capture_output=True, text=True)
        assert res.returncode == 0, f"--check failed for {scene}: {res.stderr}\n{res.stdout}"
    print("greeter-native: --check passed for greeter, greeter-plain, lock, lock-plain")

    # 2. If sway is available, boot a private headless Sway + mock GREETD_SOCK.
    sway = subprocess.run(["which", "sway"], capture_output=True, text=True)
    if sway.returncode != 0:
        print("greeter-native: sway not on PATH; skipping headless compositor leg")
        return

    with tempfile.TemporaryDirectory(prefix="tohru-greeter-native-") as tmp:
        tmp_path = Path(tmp)
        run_dir = tmp_path / "run"
        ipc_dir = run_dir / "ipc"
        share_dir = tmp_path / "share"
        sessions_dir = share_dir / "wayland-sessions"
        state_dir = tmp_path / "state"
        for d in (run_dir, ipc_dir, sessions_dir, state_dir):
            d.mkdir(parents=True, exist_ok=True)
        os.chmod(run_dir, 0o700)

        (sessions_dir / "hyprland.desktop").write_text(
            "[Desktop Entry]\nName=Hyprland\nExec=/bin/true\nDesktopNames=Hyprland\n"
        )
        cfg_file = tmp_path / "greeter.toml"
        cfg_file.write_text(
            '[greeter]\nstyle = "flooded-monolith"\ndefault_user = "jesse"\ndefault_session = "hyprland"\n'
        )
        sway_cfg = tmp_path / "sway.conf"
        sway_cfg.write_text("output HEADLESS-1 mode 1920x1080\n")

        sock_path = run_dir / "greetd.sock"
        mock = MockGreetd(sock_path, "correct-horse")

        env = {
            **os.environ,
            "HOME": "/var/empty",
            "XDG_RUNTIME_DIR": str(run_dir),
            "XDG_CACHE_HOME": str(tmp_path / "cache"),
            "PLEAMAR_SOCKETS": str(ipc_dir),
            "PLEAMAR_NO_PORTAL": "1",
            "PLEAMAR_GENERIC": "1",
            "TOHRU_RUNTIME": str(runtime),
            "TOHRU_MANAGED_CONFIG": str(cfg_file),
            "TOHRU_GREETER_SESSIONS_DIRS": str(share_dir),
            "TOHRU_GREETER_STATE_DIR": str(state_dir),
            "GREETD_SOCK": str(sock_path),
            "WLR_BACKENDS": "headless",
            "WLR_HEADLESS_OUTPUTS": "1",
            "WLR_LIBINPUT_NO_DEVICES": "1",
        }
        for k in ("WAYLAND_DISPLAY", "SWAYSOCK", "DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE"):
            env.pop(k, None)

        sway_log = tmp_path / "sway.log"
        greeter_log = tmp_path / "greeter.log"
        sway_log_f = sway_log.open("w+")
        greeter_log_f = greeter_log.open("w+")

        sway_proc = subprocess.Popen(
            ["sway", "--config", str(sway_cfg)],
            env=env,
            stdout=sway_log_f,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        greeter_proc = None
        try:
            deadline = time.monotonic() + 10
            wayland_display = None
            while time.monotonic() < deadline:
                for p in run_dir.glob("wayland-*"):
                    if not p.name.endswith(".lock"):
                        wayland_display = p.name
                        break
                if wayland_display:
                    break
                time.sleep(0.1)
            assert wayland_display, f"headless sway never created wayland socket:\n{sway_log.read_text()}"
            env["WAYLAND_DISPLAY"] = wayland_display

            greeter_proc = subprocess.Popen(
                [str(ROOT / "bin/tohru-greeter")],
                env=env,
                stdout=greeter_log_f,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )

            # Wait for command socket to appear and verify --say refuses orders!
            deadline = time.monotonic() + 10
            refused = False
            while time.monotonic() < deadline:
                if greeter_proc.poll() is not None:
                    raise AssertionError(
                        f"greeter exited early ({greeter_proc.returncode}):\n{greeter_log.read_text()}"
                    )
                res = subprocess.run(
                    [str(runtime), "--say", "tohru-greeter", "quit"],
                    env=env,
                    capture_output=True,
                    text=True,
                )
                if "greeter takes no orders" in (res.stdout + res.stderr):
                    refused = True
                    break
                time.sleep(0.15)
            assert refused, f"greeter did not refuse --say orders:\n{greeter_log.read_text()}"
            print("greeter-native: greeter running on headless sway and refuses --say orders")

            # Wait for the primary overlay surface to map and render before sending input.
            deadline = time.monotonic() + 10
            primary_ready = False
            while time.monotonic() < deadline:
                if greeter_proc.poll() is not None:
                    raise AssertionError(
                        f"greeter exited early ({greeter_proc.returncode}):\n{greeter_log.read_text()}"
                    )
                if "· primary" in greeter_log.read_text():
                    primary_ready = True
                    break
                time.sleep(0.1)
            assert primary_ready, f"primary surface never mapped:\n{greeter_log.read_text()}"
            time.sleep(0.35)

            if args.grim and args.screenshot:
                Path(args.screenshot).parent.mkdir(parents=True, exist_ok=True)
                subprocess.run([args.grim, args.screenshot], env=env, check=True)
                print(f"greeter-native: captured preview screenshot at {args.screenshot}")

            if args.wtype:
                # Headless Sway keeps only the first virtual keyboard's focus, so one wtype types
                # both attempts. Its pauses let Pleamar take the keymap, let text land before each
                # Return, and let the rejection settle before the second attempt.
                subprocess.run(
                    [args.wtype, "-s", "500", "wrong", "-s", "300", "-k", "Return",
                     "-s", "1500", "correct-horse", "-s", "300", "-k", "Return"],
                    env=env,
                    check=True,
                )
                try:
                    rc = greeter_proc.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    raise AssertionError(
                        f"greeter timed out after valid password.\n"
                        f"mock.requests={mock.requests!r}\n"
                        f"greeter.log:\n{greeter_log.read_text()}\n"
                        f"sway.log:\n{sway_log.read_text()}"
                    )
                assert rc == 0, f"greeter exited {rc} after valid password:\n{greeter_log.read_text()}"
                assert mock.started is not None, f"valid password did not call start_session: {mock.requests!r}"
                answers = [r.get("response") for r in mock.requests if r.get("type") == "post_auth_message_response"]
                assert answers == ["wrong", "correct-horse"], f"unexpected answers: {mock.requests!r}"
                kinds = [r.get("type") for r in mock.requests]
                assert kinds.index("cancel_session") < kinds.index("start_session"), f"wrong password was not cancelled first: {kinds!r}"
                log_text = greeter_log.read_text()
                assert "correct-horse" not in log_text, "password leaked to logs!"
                print("greeter-native: full wrong/right auth + start_session + clean exit verified!")
        finally:
            if greeter_proc and greeter_proc.poll() is None:
                try:
                    os.killpg(greeter_proc.pid, 15)
                except ProcessLookupError:
                    pass
                try:
                    greeter_proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(greeter_proc.pid, 9)
                    except ProcessLookupError:
                        pass
            if sway_proc.poll() is None:
                try:
                    os.killpg(sway_proc.pid, 15)
                except ProcessLookupError:
                    pass
                try:
                    sway_proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    try:
                        os.killpg(sway_proc.pid, 9)
                    except ProcessLookupError:
                        pass
            greeter_log_f.close()
            sway_log_f.close()
            mock.close()


if __name__ == "__main__":
    main()
