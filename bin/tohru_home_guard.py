"""Bound foreground Home commands while leaving replies and background jobs alone."""
import argparse
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import time

REFUSAL = "Home has a five-second foreground work budget. Use Orca worker-start --background or Bash run_in_background=true, then end the turn; never use internal subagents."
SLOW = {"cargo": {"build", "test", "bench", "run", "install", "clippy"},
        "nix": {"build", "develop", "shell", "run", "flake"},
        "git": {"clone", "fetch", "pull"}, "pip": {"install", "download"},
        "pip3": {"install", "download"}, "uv": {"sync", "pip", "run"},
        "npm": {"install", "ci", "test", "run"}, "pnpm": {"install", "test", "run", "exec"}}
SLOW_PROGRAMS = {"make", "cmake", "ninja", "pytest", "curl", "wget", "sleep", "gpu-run", "gpu-game"}
QUICK_TOOLS = {"Read", "Glob", "Grep", "ToolSearch"}
SEPARATORS = {";", "&&", "||", "|", "&", "(", ")"}


def plan(tool, value, remaining_ms):
    if tool == "AskUserQuestion":
        return {"decision": "pass"}
    if tool in QUICK_TOOLS:
        return {"decision": "pass"} if 0 < remaining_ms <= 5000 else {"decision": "deny", "reason": REFUSAL}
    if tool != "Bash" or not isinstance(value, dict) or not isinstance(value.get("command"), str):
        return {"decision": "deny", "reason": REFUSAL}
    if value.get("run_in_background") is True:
        return {"decision": "pass", "background": True}
    if not 0 < remaining_ms <= 5000:
        return {"decision": "deny", "reason": REFUSAL}
    command = value["command"]
    try:
        lexer = shlex.shlex(command, posix=True, punctuation_chars=True)
        lexer.whitespace_split = True
        words = list(lexer)
    except ValueError:
        return {"decision": "deny", "reason": "Invalid shell command. " + REFUSAL}
    segments, segment = [], []
    for word in words:
        if word in SEPARATORS:
            if segment:
                segments.append(segment)
            segment = []
        else:
            segment.append(word)
    if segment:
        segments.append(segment)
    for arguments in segments:
        while arguments and ("=" in arguments[0] or arguments[0] in ("env", "command", "time", "then", "do", "if", "while", "until", "!")):
            arguments = arguments[1:]
        if not arguments:
            continue
        program = Path(arguments[0]).name
        if program in SLOW_PROGRAMS or any(word in SLOW.get(program, set()) for word in arguments[1:]):
            return {"decision": "deny", "reason": REFUSAL}
    if "worker-start" in words and "--background" not in words:
        return {"decision": "deny", "reason": REFUSAL}
    if "wait" in words or ("check" in words and "--wait" in words):
        return {"decision": "deny", "reason": REFUSAL}
    if "worker-show" in words and any(word in command for word in ("while ", "until ", "for ")):
        return {"decision": "deny", "reason": REFUSAL}
    executable = str(Path(__file__).resolve().with_name("tohru-home-guard"))
    wrapped = shlex.join([executable, "run", "--budget-ms", str(remaining_ms), "--command", command])
    return {"decision": "wrap", "input": {**value, "command": wrapped, "timeout": remaining_ms + 1500}}


def foreground(command, budget_ms):
    started = time.monotonic()
    child = subprocess.Popen(["bash", "-c", command], start_new_session=True)
    def stop(*_):
        try:
            os.killpg(child.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    prior = {sig: signal.signal(sig, stop) for sig in (signal.SIGINT, signal.SIGTERM)}
    timed_out = False
    try:
        try:
            code = child.wait(timeout=budget_ms / 1000)
        except subprocess.TimeoutExpired:
            timed_out = True
            stop()
            try:
                child.wait(timeout=1)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            code = 124
    finally:
        # Ordinary foreground commands may not leave untracked descendants behind.
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        for sig, handler in prior.items():
            signal.signal(sig, handler)
    if timed_out:
        print("The foreground deadline expired. The command may have taken effect; inspect its existing receipt or shared records before retrying.", file=sys.stderr)
        print(REFUSAL, file=sys.stderr)
    print(json.dumps({"event": "home_tool", "duration_ms": round((time.monotonic() - started) * 1000),
                      "timed_out": timed_out, "exit_code": code}), file=sys.stderr)
    return code if code >= 0 else 128 - code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="mode", required=True)
    policy = sub.add_parser("plan")
    policy.add_argument("--remaining-ms", type=int, required=True)
    run = sub.add_parser("run")
    run.add_argument("--budget-ms", type=int, required=True)
    run.add_argument("--command", required=True)
    args = parser.parse_args()
    if args.mode == "plan":
        value = json.loads(sys.stdin.buffer.read(65537))
        print(json.dumps(plan(value.get("tool"), value.get("input"), args.remaining_ms)))
        return 0
    if not 0 < args.budget_ms <= 5000:
        parser.error("--budget-ms must be between 1 and 5000")
    return foreground(args.command, args.budget_ms)
