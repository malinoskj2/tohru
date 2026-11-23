#!/usr/bin/env python3
"""Exercise patched Orca RPC/CLI -> real Tohru CLI -> real voice policy, without devices."""
import argparse
import json
import os
from pathlib import Path
import queue
import shutil
import socket
import subprocess
import sys
import threading

ROOT = Path(__file__).resolve().parents[2]
CHAT = "12345678-1234-4234-8234-123456789abc"
REQUEST = "orca:voice-request"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--orca-source", required=True, type=Path)
    parser.add_argument("--lua", default="lua5.4")
    args = parser.parse_args()
    source = args.orca_source.resolve()
    output = ROOT / ".tmp/orca-progress/integration"
    output.mkdir(parents=True, exist_ok=True)
    node = shutil.which("node")
    lua = shutil.which(args.lua)
    if not node or not lua:
        parser.error("node and Lua 5.4 are required")

    # Bundle production modules; replace only desktop-host and journal/database adapters.
    build = output / "bundle.cjs"
    # Async build permits explicit resolver adapters; no running Orca is imported or contacted.
    build.write_text("""
const fs=require('node:fs'), esbuild=require(process.argv[2]+'/node_modules/esbuild');
const source=process.argv[2], fixture=process.argv[3], out=process.argv[4];
esbuild.build({stdin:{contents:fs.readFileSync(fixture,'utf8'),resolveDir:source,loader:'js'},
  bundle:true,platform:'node',format:'cjs',outfile:out,external:['electron'],plugins:[{
    name:'isolated-host-adapters',setup(build){
      build.onResolve({filter:/bot-chat-session$/},()=>({path:'host',namespace:'fixture'}));
      build.onResolve({filter:/bot-session-tabs$/},()=>({path:'membership',namespace:'fixture'}));
      build.onLoad({filter:/.*/,namespace:'fixture'},({path})=>({contents:path==='host'
        ? 'export async function requireStructuredHost(runtime){return runtime.host}'
        : 'export function botOfSession(bots,id){return bots.list().find(bot=>bot.chat===id)??null}',loader:'js'}));
    }
  }]}).catch(error=>{console.error(error);process.exitCode=1});
""")
    bundle = output / "fixture.cjs"
    subprocess.run([node, str(build), str(source), str(Path(__file__).with_name("transport-fixture.mjs")), str(bundle)], check=True)
    ipc_log = output / "ipc.jsonl"
    ipc_log.write_text("")
    runtime = output / "pleamar-fixture"
    runtime.write_text(f"#!{sys.executable}\nimport json,os,sys\nwith open(os.environ['VOICE_TEST_IPC'],'a') as out: out.write(json.dumps(sys.argv[1:])+'\\n')\n")
    runtime.chmod(0o755)
    endpoint = output / "rpc.sock"
    if endpoint.exists():
        parser.error(f"Existing fixture socket requires inspecting its owner before retry: {endpoint}")
    shim = output / "orca-fixture"
    shim.write_text(f"#!{sys.executable}\n" + """
import json,os,socket,sys
with socket.socket(socket.AF_UNIX) as client:
    client.settimeout(10); client.connect(os.environ['VOICE_TEST_SOCKET'])
    client.sendall((json.dumps({'op':'cli','args':sys.argv[1:]})+'\\n').encode())
    result=json.loads(client.makefile().readline())
    print(json.dumps(result)); sys.exit(0 if result.get('ok') else 1)
""")
    shim.chmod(0o755)
    env = {**os.environ, "VOICE_TEST_SOCKET": str(endpoint), "VOICE_TEST_IPC": str(ipc_log),
           "TOHRU_RUNTIME": str(runtime), "TOHRU_ORCA_BIN": str(shim),
           "ORCA_DESKTOP_ASSISTANT_COMMAND": str(ROOT / "bin/tohru"), "ORCA_AGENT_SESSION_ID": CHAT}
    node_log = (output / "producer.log").open("w")
    server = subprocess.Popen([node, str(bundle)], env=env, stdout=subprocess.PIPE, stderr=node_log, text=True)
    voice = subprocess.Popen([lua, "integrations/orca/consumer-fixture.lua"], cwd=ROOT,
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=node_log, text=True, bufsize=1)
    received = queue.Queue()
    threading.Thread(target=lambda: [received.put(line.rstrip("\n")) for line in voice.stdout], daemon=True).start()
    trace = []
    producer_output = None

    def capture_producer():
        for line in server.stdout:
            node_log.write(line)
            node_log.flush()

    def rpc(message, ok=True):
        with socket.socket(socket.AF_UNIX) as client:
            client.settimeout(10); client.connect(str(endpoint))
            client.sendall((json.dumps(message) + "\n").encode())
            result = json.loads(client.makefile().readline())
        if ok:
            assert result.get("ok") is not False, result
        else:
            assert result.get("ok") is False, result
        return result

    def command(line):
        voice.stdin.write(line + "\n"); voice.stdin.flush()

    def drain():
        command("sync")
        batch = []
        while True:
            line = received.get(timeout=10)
            if line == "synced":
                return batch
            batch.append(line); trace.append(line)

    offset = 0
    def deliver():
        nonlocal offset
        data = ipc_log.read_text().splitlines()
        for raw in data[offset:]:
            messages = json.loads(raw)
            assert messages[:2] == ["--say", "tohru"]
            for message in messages[2:]: command(message)
        offset = len(data)
        return drain()

    def update(kind, sequence, request=REQUEST, **extra):
        params = {"callerSessionId": CHAT, "task": "workspace", "kind": kind, "sequence": sequence,
                  "text": {"ack": "I'll create that workspace now.", "progress": "The workspace exists; the agent is starting.",
                           "done": "The agent session is attached and verified.", "failed": "The agent could not start."}.get(kind, "Cancel task speech.")}
        if request is not None: params["request"] = request
        params.update(extra)
        return {"op": "rpc", "method": "bots.voice.task", "params": params}

    try:
        assert server.stdout.readline().strip() == "SERVER_READY"
        producer_output = threading.Thread(target=capture_producer, daemon=True)
        producer_output.start()
        early_call = subprocess.run([str(shim), "bot", "voice-task", "--task", "workspace", "--kind", "ack",
                                     "--sequence", "1", "--text", "I'll create that workspace now.", "--json"],
                                    env=env, capture_output=True, text=True, timeout=15)
        assert early_call.returncode == 0, early_call.stderr + early_call.stdout
        early = json.loads(early_call.stdout)
        assert early["result"] == {"request": REQUEST, "delivery": "pending"}
        assert ipc_log.read_text() == ""  # No arbitrary sleep or premature delivery.
        command(f"start {CHAT} {REQUEST}")
        ready = drain()
        assert f"ready\t{CHAT}\t{REQUEST}" in ready
        registered = subprocess.run([str(ROOT / "bin/tohru-assistant"), "voice-ready", "--conversation", CHAT,
                                     "--request", REQUEST], env=env, capture_output=True, text=True, timeout=15)
        assert registered.returncode == 0, registered.stderr + registered.stdout
        assert json.loads(registered.stdout)["registered"] is True
        assert deliver()[-1] == "speech\tI'll create that workspace now."
        assert rpc({"op":"task-mode"}) == {"active": True}
        rpc({"op":"worker-start"})
        command("finished"); drain()
        rpc(update("progress", 2)); assert deliver()[-1] == "speech\tThe workspace exists; the agent is starting."
        rpc({"op":"worker-done"})
        assert deliver() == []  # Raw worker mail is evidence, never speech.
        rpc(update("done", 3)); assert deliver()[-1] == "speech\tThe agent session is attached and verified."
        rpc({"op":"legacy-final"}); assert deliver() == []
        assert rpc(update("done", 3))["result"]["delivery"] == "replay"
        assert deliver() == []
        rpc({**update("progress", 4), "caller": "22345678-1234-4234-8234-123456789abc"}, ok=False)
        rpc({**update("progress", 4), "paired": True}, ok=False)
        rpc(update("worker_done", 4), ok=False)
        rpc({"op":"new-turn", "request":"orca:new-request"})
        command(f"start {CHAT} orca:new-request"); drain()
        rpc(update("done", 4), ok=False)
        rpc({"op":"rpc", "method":"bots.voice.ready", "params":{"chat":CHAT,"request":"orca:new-request"}})
        rpc(update("ack", 1, "orca:new-request")); deliver()
        command("hide"); drain()
        rpc(update("failed", 2, "orca:new-request")); assert not any(row.startswith("speech\t") for row in deliver())
        rpc({"op":"new-turn", "request":"text-request", "text":"[tohru-desktop]\nText request"})
        rpc(update("ack", 1, None), ok=False)
        (output / "trace.json").write_text(json.dumps(trace, indent=2) + "\n")
        print("PASS: early ack held until real desktop registration; RPC/CLI -> Tohru IPC -> production voice policy")
        print("PASS: progress, verified completion, replay, caller/remote/raw-mail rejection, supersession, hide and text-only origin")
        print(f"Synthetic speech trace: {output / 'trace.json'}")
    finally:
        voice.stdin.close()
        try: voice.wait(timeout=5)
        except subprocess.TimeoutExpired: voice.terminate(); voice.wait(timeout=5)
        server.terminate(); server.wait(timeout=5)
        if producer_output is not None: producer_output.join(timeout=5)
        node_log.close()
        endpoint.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
