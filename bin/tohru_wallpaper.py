"""mpvpaper ownership: one foreground supervisor per Wayland session.

Only children created here are terminated. Commands use a private Unix socket;
mpv IPC paths are never borrowed from another desktop's wallpaper processes.
"""
import fcntl
import json
import math
import os
from pathlib import Path
import shlex
import shutil
import signal
import socket
import subprocess
import sys
import time

from tohru_portability import BIN, config, outputs, session_directory, xdg

VIDEO = {".mp4", ".webm", ".mkv", ".mov", ".gif"}
STILL = {".jpg", ".jpeg", ".png", ".webp"}


def paths(values):
    return (Path(values["wallpaper.directory"] or Path.home()/"Pictures/Wallpapers"),
            Path(values["wallpaper.assignments"] or xdg("STATE")/"tohru/wallpaper/assignments.json"),
            session_directory()/"wallpaper")


def assignment_data(file):
    if not file.exists():
        return {}
    if file.stat().st_size > 1024*1024:
        raise ValueError("wallpaper assignments exceed 1 MiB")
    data = json.loads(file.read_text()).get("assignments", {})
    if not isinstance(data, dict) or not all(isinstance(k,str) and isinstance(v,str) for k,v in data.items()):
        raise ValueError("invalid wallpaper assignments")
    return data


def save(file, assignments):
    import tempfile
    file.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(mode="w", dir=file.parent, delete=False) as stream:
        temp = Path(stream.name)
        try:
            json.dump({"assignments": assignments}, stream)
            stream.flush()
            os.fsync(stream.fileno())
            os.replace(temp, file)
        finally:
            temp.unlink(missing_ok=True)


def update_assignment(file, name, path):
    file.parent.mkdir(parents=True, exist_ok=True)
    with file.with_suffix(file.suffix+".lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        data = assignment_data(file)
        if path is None:
            data.pop(name, None)
        else:
            data[name] = path
        save(file, data)
        return data


def valid_file(directory, name):
    path = Path(name).resolve(strict=True)
    if not path.is_file() or path.parent != directory.resolve() or path.suffix.lower() not in VIDEO|STILL:
        raise ValueError("wallpaper must be a supported regular file directly in wallpaper.directory")
    return str(path)


def ipc(path, command):
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(1)
        connection.connect(str(path))
        connection.sendall(json.dumps({"command": command, "request_id": 1}).encode()+b"\n")
        data = b""
        received = 0
        deadline = time.monotonic()+1
        while received <= 65536 and time.monotonic() < deadline:
            connection.settimeout(max(.001,deadline-time.monotonic()))
            chunk = connection.recv(4096)
            if not chunk:
                break
            data += chunk
            received += len(chunk)
            while b"\n" in data:
                line, data = data.split(b"\n",1)
                response = json.loads(line)
                if response.get("request_id") == 1:
                    if response.get("error") != "success":
                        raise ValueError("wallpaper backend refused command")
                    return response.get("data")
        raise ValueError("wallpaper backend did not answer")


class Owner:
    def __init__(self, values, directory, state, runtime):
        self.values, self.directory, self.state, self.runtime = values, directory, state, runtime
        self.assignments = assignment_data(state)
        self.children = {}
        self.monitors = []
        self.discovery = "unknown"
        self.errors = {}
        self.retry_at = {}

    def socket(self, name):
        import hashlib
        return self.runtime / (hashlib.sha256(name.encode()).hexdigest()[:16]+".sock")

    def stop(self, name):
        child = self.children.pop(name, None)
        if child:
            child.terminate()
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.kill(); child.wait()
        self.socket(name).unlink(missing_ok=True)

    def start(self, name, file):
        self.stop(name)
        options = self.values["wallpaper.options"]
        # Prevent configured options from replacing our IPC endpoint or launching scripts.
        if any(word in options for word in ("input-ipc", "script", "config", "include")):
            raise ValueError("wallpaper.options cannot override IPC, config or scripts")
        argv = [self.values["wallpaper.executable"], "-o",
                f"--no-config --idle=yes --image-display-duration=inf --input-ipc-server={shlex.quote(str(self.socket(name)))} {options}", name, file]
        child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL)
        self.children[name] = child
        deadline = time.monotonic()+4
        while not self.socket(name).exists():
            if child.poll() is not None or time.monotonic() > deadline:
                self.stop(name)
                raise ValueError("mpvpaper failed to create its IPC socket; see journal")
            time.sleep(.05)

    def reconcile(self):
        monitors, state = outputs()
        self.discovery = state
        if state != "ok":
            return  # A temporary discovery failure does not tear down live wallpapers.
        names = {item["name"] for item in monitors}
        self.monitors = monitors
        self.retry_at = {name:at for name,at in self.retry_at.items() if name in names}
        self.errors = {name:message for name,message in self.errors.items() if name in names}
        for name in list(self.children):
            if name not in names:
                self.stop(name)
        for name in names:
            child = self.children.get(name)
            if child and child.poll() is None:
                continue
            if time.monotonic() < self.retry_at.get(name,0):
                continue
            if name in self.assignments:
                try:
                    self.start(name, valid_file(self.directory, self.assignments[name]))
                    self.errors.pop(name,None)
                except (OSError,ValueError) as error:
                    self.errors[name] = str(error)
                    self.retry_at[name] = time.monotonic()+5

    def playing(self, name):
        if name not in self.children or self.children[name].poll() is not None:
            return None
        try:
            result = {"path": ipc(self.socket(name), ["get_property", "path"])}
        except (OSError,ValueError):
            return None
        for key,prop,default in (("pos","time-pos",0),("duration","duration",0),("paused","pause",False)):
            try:
                value = ipc(self.socket(name), ["get_property",prop])
                result[key] = value if value is not None else default
            except (OSError,ValueError):
                result[key] = default
        return result

    def command(self, args):
        action = args[0]
        if action in ("scan", "status"):
            result = {"playing": {item["name"]: self.playing(item["name"]) for item in self.monitors},
                      "backend": "mpvpaper", "enabled": True, "owner": "tohru", "discovery": self.discovery,
                      "errors": self.errors}
            if action == "scan":
                result.update(scan(self.directory,self.state,self.monitors))
            return result
        if len(args) < 2 or args[1] not in {m["name"] for m in self.monitors}:
            raise ValueError("output is not currently advertised")
        name = args[1]
        if action == "playing":
            return self.playing(name)
        if action in ("apply","load"):
            if len(args) not in (3,4):
                raise ValueError("apply/load requires output, file and optional start seconds")
            file = valid_file(self.directory,args[2])
            if name not in self.children or self.children[name].poll() is not None:
                self.start(name,file)
            ipc(self.socket(name),["loadfile",file,"replace"])
            if len(args) == 4:
                ipc(self.socket(name),["seek",seconds(args[3]),"absolute"])
            if action == "apply":
                self.assignments = update_assignment(self.state,name,file)
        elif action == "pause" and len(args)==3 and args[2] in ("yes","no"):
            ipc(self.socket(name),["set_property","pause",args[2]=="yes"])
        elif action == "seek" and len(args)==3:
            ipc(self.socket(name),["seek",seconds(args[2]),"absolute"])
        elif action == "stop" and len(args)==2:
            self.stop(name)
            self.assignments = update_assignment(self.state,name,None)
        else:
            raise ValueError("unsupported wallpaper command")
        return {"ok":True}


def seconds(value):
    result = float(value)
    if not math.isfinite(result) or not 0 <= result <= 864000:
        raise ValueError("seconds must be finite and in 0..864000")
    return result


def scan(directory,state,monitors):
    files=[]
    if directory.is_dir():
        for path in sorted(directory.iterdir()):
            if path.is_file() and path.suffix.lower() in VIDEO|STILL:
                st=path.stat()
                files.append({"path":str(path),"size":st.st_size,"mtime":int(st.st_mtime),
                              "kind":"video" if path.suffix.lower() in VIDEO else "still"})
    return {"dir":str(directory),"files":files,"assignments":assignment_data(state),"monitors":monitors}


def serve(values,directory,state,runtime):
    if not values["wallpaper.enabled"]:
        print("tohru-wallpaper: wallpaper.enabled is false; no owner started",file=sys.stderr)
        return
    if not shutil.which(values["wallpaper.executable"]):
        raise ValueError("configured mpvpaper executable is missing")
    runtime.mkdir(parents=True,exist_ok=True,mode=0o700)
    with (runtime/"owner.lock").open("w") as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        control=runtime/"control.sock"
        control.unlink(missing_ok=True)
        owner=Owner(values,directory,state,runtime)
        running=True
        def stop(*_):
            nonlocal running
            running=False
        signal.signal(signal.SIGTERM,stop); signal.signal(signal.SIGINT,stop)
        with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as server:
            server.bind(str(control)); os.chmod(control,0o600);server.listen(8);server.settimeout(.5)
            refresh=0
            try:
                while running:
                    if time.monotonic() >= refresh:
                        owner.reconcile();refresh=time.monotonic()+3
                    try:
                        connection,_=server.accept()
                    except socket.timeout:
                        continue
                    with connection:
                        connection.settimeout(2)
                        try:
                            data=b""
                            while b"\n" not in data and len(data)<65536:
                                chunk=connection.recv(4096)
                                if not chunk:break
                                data+=chunk
                            args=json.loads(data)
                            if not isinstance(args,list) or not args or not all(isinstance(a,str) for a in args):
                                raise ValueError("invalid command")
                            response={"result":owner.command(args)}
                        except (ValueError,OSError,IndexError) as error:
                            response={"error":str(error)}
                        try:connection.sendall(json.dumps(response).encode()+b"\n")
                        except OSError:pass
            finally:
                for name in list(owner.children):owner.stop(name)
                control.unlink(missing_ok=True)


def request(runtime,args):
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as connection:
        connection.settimeout(15);connection.connect(str(runtime/"control.sock"))
        connection.sendall(json.dumps(args).encode()+b"\n")
        data=b""
        while b"\n" not in data and len(data)<1024*1024:
            chunk=connection.recv(65536)
            if not chunk:break
            data+=chunk
        response=json.loads(data)
        if "error" in response:raise ValueError(response["error"])
        return response["result"]


def main(args):
    values,_=config();directory,state,runtime=paths(values)
    if not args:raise ValueError("usage: tohru-wallpaper serve|scan|status|apply|load|pause|seek|stop|thumbs|snapshot")
    if args[0]=="serve":return serve(values,directory,state,runtime)
    if args[0] in ("thumbs","snapshot"):
        environment=os.environ.copy()
        environment["TOHRU_WALLPAPER_DIR"]=str(directory)
        return subprocess.call([str(BIN/"tohru-wallpaper-media"),*args],env=environment)
    try:
        result=request(runtime,args)
    except (FileNotFoundError,ConnectionRefusedError):
        if args[0] not in ("scan","status"):
            raise ValueError("Tohru wallpaper owner unavailable; explicitly enable wallpaper.enabled and start tohru-wallpaper.service")
        monitors,discovery=outputs()
        result={"playing":{},"enabled":values["wallpaper.enabled"],"owner":"unavailable" if values["wallpaper.enabled"] else "disabled","discovery":discovery}
        if args[0]=="scan":result.update(scan(directory,state,monitors))
    print(json.dumps(result))
    return 0


if __name__=="__main__":
    try:sys.exit(main(sys.argv[1:]))
    except (OSError,ValueError) as error:
        print(f"tohru-wallpaper: {error}",file=sys.stderr);sys.exit(1)
