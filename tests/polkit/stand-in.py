#!/usr/bin/env python3
"""A private stand-in for polkit, to drive Tohru's agent where no polkitd runs.

    python3 tests/polkit/stand-in.py RUNDIR

Starts an unprivileged dbus-daemon at RUNDIR/system-bus, owns
org.freedesktop.PolicyKit1 on it, and serves a fake polkit-agent-helper-1 on
RUNDIR/helper.socket whose password is "right" for every user. It writes
RUNDIR/env with DBUS_SYSTEM_BUS_ADDRESS, PLEAMAR_POLKIT_HELPER and
XDG_SESSION_ID for the agent, then takes lines on RUNDIR/control:

    begin COOKIE [UID ...]   ask the registered agent, as polkitd would
    cancel COOKIE            CancelAuthentication
    info COOKIE TEXT         the helper's next conversation for COOKIE starts with PAM_TEXT_INFO
    status                   every request and how it ended

Nothing here is privileged, and nothing it accepts lets anything through.
"""
import os
import signal
import socket
import subprocess
import sys
import threading
from pathlib import Path

from gi.repository import Gio, GLib

AUTHORITY = "org.freedesktop.PolicyKit1"
AUTHORITY_XML = """<node><interface name="org.freedesktop.PolicyKit1.Authority">
<method name="RegisterAuthenticationAgent"><arg type="(sa{sv})" direction="in"/><arg type="s" direction="in"/><arg type="o" direction="in"/></method>
<method name="UnregisterAuthenticationAgent"><arg type="(sa{sv})" direction="in"/><arg type="o" direction="in"/></method>
</interface></node>"""

run = Path(sys.argv[1]).resolve()
run.mkdir(parents=True, exist_ok=True)
bus_socket = run / "system-bus"
config = run / "bus.conf"
config.write_text(
    f"<busconfig><type>system</type><listen>unix:path={bus_socket}</listen><auth>EXTERNAL</auth>"
    '<policy context="default"><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/><allow own="*"/></policy></busconfig>'
)
daemon = subprocess.Popen(["dbus-daemon", "--nofork", "--nopidfile", "--print-address=1", f"--config-file={config}"], stdout=subprocess.PIPE, text=True)
address = daemon.stdout.readline().strip()

# What polkitd's subject would be: a terminal, so the dialog names it.
terminal_bin = run / "alacritty"
terminal_bin.write_bytes(Path(subprocess.check_output(["sh", "-c", "command -v sleep"], text=True).strip()).resolve().read_bytes())
terminal_bin.chmod(0o755)
terminal = subprocess.Popen([str(terminal_bin), "infinity"])

agent = {"name": None, "path": None}
requests = {}
infos = {}
lock = threading.Lock()


def authority_call(conn, sender, path, iface, method, params, invocation):
    if method == "RegisterAuthenticationAgent":
        _, _, agent_path = params.unpack()
        agent["name"], agent["path"] = sender, agent_path
        print(f"stand-in · agent registered: {sender} {agent_path}", flush=True)
    elif method == "UnregisterAuthenticationAgent":
        agent["name"] = None
    invocation.return_value(None)


conn = Gio.DBusConnection.new_for_address_sync(address, Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None)
node = Gio.DBusNodeInfo.new_for_xml(AUTHORITY_XML)
conn.register_object("/org/freedesktop/PolicyKit1/Authority", node.interfaces[0], authority_call, None, None)
conn.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "RequestName", GLib.Variant("(su)", (AUTHORITY, 4)), None, Gio.DBusCallFlags.NONE, -1, None)


def begin(cookie, uids):
    if not agent["name"]:
        return "no agent registered"
    identities = [("unix-user", {"uid": GLib.Variant("u", int(u))}) for u in uids]
    details = {"command_line": "/run/current-system/sw/bin/nixos-rebuild switch --flake .#home", "program": "/run/current-system/sw/bin/nixos-rebuild", "polkit.subject-pid": str(terminal.pid)}
    args = GLib.Variant("(sssa{ss}sa(sa{sv}))", ("org.freedesktop.policykit.exec", "Authentication is required to run nixos-rebuild as the super user", "", details, cookie, identities))
    requests[cookie] = "pending"

    def done(c, result):
        try:
            c.call_finish(result)
            requests[cookie] = "granted"
        except GLib.Error as e:
            requests[cookie] = "refused: " + e.message
        print(f"stand-in · {cookie} {requests[cookie]}", flush=True)

    conn.call(agent["name"], agent["path"], "org.freedesktop.PolicyKit1.AuthenticationAgent", "BeginAuthentication", args, None, Gio.DBusCallFlags.NONE, 0x7FFFFFFF, None, done)
    return "ok"


def cancel(cookie):
    conn.call(agent["name"], agent["path"], "org.freedesktop.PolicyKit1.AuthenticationAgent", "CancelAuthentication", GLib.Variant("(s)", (cookie,)), None, Gio.DBusCallFlags.NONE, -1, None, None)
    return "ok"


def command(line):
    words = line.split()
    if not words:
        return "?"
    if words[0] == "begin" and len(words) >= 2:
        return begin(words[1], words[2:] or [str(os.getuid()), "0"])
    if words[0] == "cancel" and len(words) == 2:
        return cancel(words[1])
    if words[0] == "info" and len(words) >= 3:
        infos[words[1]] = " ".join(words[2:])
        return "ok"
    if words[0] == "status":
        return " ".join(f"{k}={v}" for k, v in requests.items()) or "none"
    return "?"


def control():
    path = run / "control"
    path.unlink(missing_ok=True)
    server = socket.socket(socket.AF_UNIX)
    server.bind(str(path))
    server.listen()
    while True:
        client, _ = server.accept()
        with client:
            line = client.makefile().readline()
            reply = []
            ready = threading.Event()

            def on_main():
                reply.append(command(line))
                ready.set()
                return False

            GLib.idle_add(on_main)
            ready.wait(5)
            client.sendall(((reply or ["timeout"])[0] + "\n").encode())


def helper():
    path = run / "helper.socket"
    path.unlink(missing_ok=True)
    server = socket.socket(socket.AF_UNIX)
    server.bind(str(path))
    server.listen()
    while True:
        client, _ = server.accept()
        threading.Thread(target=converse, args=(client,), daemon=True).start()


def converse(client):
    with client:
        lines = client.makefile("r")
        user = lines.readline().strip()
        cookie = lines.readline().strip()
        try:
            if cookie in infos:
                client.sendall(f"PAM_TEXT_INFO {infos.pop(cookie)}\n".encode())
            client.sendall(b"PAM_PROMPT_ECHO_OFF Password: \n")
            answer = lines.readline()
            if not answer:
                return
            ok = answer.rstrip("\n") == "right"
            print(f"stand-in · helper {cookie} for {user}: {'SUCCESS' if ok else 'FAILURE'}", flush=True)
            client.sendall(b"SUCCESS\n" if ok else b"FAILURE\n")
        except OSError:
            pass


threading.Thread(target=control, daemon=True).start()
threading.Thread(target=helper, daemon=True).start()
(run / "env").write_text(f"DBUS_SYSTEM_BUS_ADDRESS={address}\nPLEAMAR_POLKIT_HELPER={run / 'helper.socket'}\nXDG_SESSION_ID=stand-in\n")
print(f"stand-in · ready at {address}", flush=True)
loop = GLib.MainLoop()
for s in (signal.SIGINT, signal.SIGTERM):
    GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, s, loop.quit)
try:
    loop.run()
finally:
    terminal.terminate()
    daemon.terminate()
    daemon.wait()
