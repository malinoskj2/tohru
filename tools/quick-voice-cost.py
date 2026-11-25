#!/usr/bin/env python3
"""Measure one quick-voice exchange end to end with real Orca reads and a real Claude call.

Runs the given tohru-assistant's own orca_quick_voice. Only the two Home-writing
`voice-side` phases and the Home hand-off are replaced: `begin` returns the real
Home history read through `agent read`, and `finish` records nothing. Each run is
billed. Rows go to stdout as JSON lines with no prompt or answer text unless --show.
"""
import argparse
import importlib.machinery
import importlib.util
import json
import os
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def load(path, scratch):
    os.environ["TOHRU_ASSISTANT_SCRATCH"] = scratch
    sys.path.insert(0, str(ROOT / "bin"))
    loader = importlib.machinery.SourceFileLoader("tohru_assistant_measured", str(path))
    spec = importlib.util.spec_from_loader("tohru_assistant_measured", loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


class TimedGate:
    """Opens settle_ms after the exchange starts, as the voice service's settled event would."""

    def __init__(self, settle_ms):
        self.at = time.monotonic() + settle_ms / 1000
        self.waited_ms = None

    def wait(self):
        started = time.monotonic()
        time.sleep(max(0.0, self.at - started))
        self.waited_ms = round((time.monotonic() - started) * 1000)
        return True


def measure(module, home, bot, question, show, settle_ms=None):
    ok, page, error = module.orca_call(["agent", "read", f"orca_session_id:{home}", "--limit", "100", "--ids", "--json"], timeout=15)
    if not ok:
        raise SystemExit(f"cannot read Home history: {error}")
    row = {"history_bytes": len(json.dumps(page, ensure_ascii=False).encode())}
    events = []
    started = time.monotonic()

    def ms():
        return round((time.monotonic() - started) * 1000)

    def side(bot_name, phase, request, text):
        if phase == "begin":
            return True, {"route": "side", "chat": home, "request": request, "history": page, "approvals": []}, ""
        row["finish_ms"] = ms()
        return True, {"recorded": True}, ""

    def emit(value):
        events.append((ms(), value))
        if value.get("event") == "voice_notice" and "ack_ms" not in row:
            row["ack_ms"] = ms()
        if value.get("event") == "delta" and "first_delta_ms" not in row:
            row["first_delta_ms"] = ms()

    base_parser = module.StreamParser

    class Parser(base_parser):
        def feed(self, line):
            try:
                value = json.loads(line)
            except ValueError:
                value = None
            if isinstance(value, dict):
                if value.get("type") == "result":
                    usage = value.get("usage") or {}
                    row.update(cost_usd=value.get("total_cost_usd"), input=usage.get("input_tokens"),
                               cache_read=usage.get("cache_read_input_tokens"),
                               cache_write=usage.get("cache_creation_input_tokens"),
                               output=usage.get("output_tokens"), model_result_ms=ms())
                elif value.get("type") == "stream_event" and "model_first_token_ms" not in row:
                    delta = (value.get("event") or {}).get("delta") or {}
                    if delta.get("type") == "text_delta":
                        row["model_first_token_ms"] = ms()
            return super().feed(line)

    base_run = module.run_provider

    def run_provider(provider, argv, stdin_text, **kwargs):
        row["stdin_bytes"] = len(stdin_text.encode())
        row["model_start_ms"] = ms()
        row["argv_model"] = argv[argv.index("--model") + 1] if "--model" in argv else ""
        row["argv_effort"] = argv[argv.index("--effort") + 1] if "--effort" in argv else ""
        row["env"] = sorted((kwargs.get("extra_env") or {}).keys())
        return base_run(provider, argv, stdin_text, **kwargs)

    module.orca_side_record = side
    module.emit = emit
    module.home_metric = lambda *a, **k: None
    module.StreamParser = Parser
    module.run_provider = run_provider
    module.orca_chat_turn = lambda *a, **k: emit({"event": "handoff"}) or 0
    args = argparse.Namespace(persona="on", persona_file="", voice=True, provider="claude")
    gate = None
    if settle_ms is not None:
        gate = args.settle_gate = TimedGate(settle_ms)
    module.orca_quick_voice(args, question, bot, "error")
    if gate:
        row["settle_wait_ms"] = gate.waited_ms
    row["total_ms"] = ms()
    final = [value for _, value in events if value.get("event") in ("delta", "handoff")]
    row["route"] = "home" if any(v.get("event") == "handoff" for v in final) else "answer"
    if show:
        row["answer"] = "".join(v.get("text", "") for v in final if v.get("event") == "delta")
    return row


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--assistant", default=str(ROOT / "bin/tohru-assistant"))
    parser.add_argument("--home", required=True, help="Home chat session id to read history from")
    parser.add_argument("--bot", default="Tohru")
    parser.add_argument("--question", action="append", required=True)
    parser.add_argument("--label", default="")
    parser.add_argument("--show", action="store_true", help="Include the spoken answer in each row")
    parser.add_argument("--settle-ms", type=int, help="Open the settle gate this long after the turn starts")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="quick-voice-cost-") as scratch:
        module = load(args.assistant, scratch)
        for question in args.question:
            row = measure(module, args.home, args.bot, question, args.show, args.settle_ms)
            print(json.dumps({"label": args.label, "question_chars": len(question), **row}), flush=True)


if __name__ == "__main__":
    main()
