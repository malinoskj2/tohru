#!/usr/bin/env python3
"""Drive bin/tohru-assistant against fake provider CLIs; nothing reaches a real provider."""
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "bin/tohru-assistant"

FAKE_CLAUDE = r'''#!/usr/bin/env python3
import json, os, sys, time
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps({"argv": args, "cwd": os.getcwd(), "stdin": sys.stdin.read() if "-p" in args else ""}) + "\n")
if args[:2] == ["auth", "status"]:
    print(json.dumps({"loggedIn": os.environ.get("FAKE_SIGNED_IN") == "1"})); sys.exit(0)
session = args[args.index("--resume") + 1] if "--resume" in args else args[args.index("--session-id") + 1]
print(json.dumps({"type": "system", "subtype": "init", "session_id": session}), flush=True)
words = os.environ.get("FAKE_ANSWER", "Eight fluid ounces.").split(" ")
for i, w in enumerate(words):
    text = w if i == 0 else " " + w
    print(json.dumps({"type": "stream_event", "session_id": session, "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": text}}}), flush=True)
    time.sleep(float(os.environ.get("FAKE_DELAY", "0")))
print(json.dumps({"type": "result", "subtype": "success", "result": " ".join(words), "session_id": session}), flush=True)
'''

FAKE_CODEX = r'''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps({"argv": args, "cwd": os.getcwd(), "stdin": sys.stdin.read() if args[:1] == ["exec"] else ""}) + "\n")
if args[:2] == ["login", "status"]:
    sys.exit(1)
print(json.dumps({"type": "thread.started", "thread_id": "thread-1"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": "Sixteen cups."}}))
print(json.dumps({"type": "turn.completed"}))
'''


class Assistant(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="tohru-assistant-")
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        self.bin = root / "bin"
        self.bin.mkdir()
        self.home = root / "home"
        (self.home / ".claude").mkdir(parents=True)
        (self.home / ".claude/settings.json").write_text(json.dumps({"model": "claude-opus-5-5[1m]"}))
        self.data = root / "data"
        self.scratch = root / "scratch"
        self.log = root / "log.jsonl"
        self.log.write_text("")
        self.fake("claude", FAKE_CLAUDE)
        self.fake("codex", FAKE_CODEX)
        self.env = {
            "PATH": f"{self.bin}:/usr/bin:/bin:" + os.path.dirname(os.path.realpath(subprocess.check_output(["sh", "-c", "command -v python3"], text=True).strip())),
            "HOME": str(self.home),
            "XDG_DATA_HOME": str(root / "xdg-data"),
            "XDG_CONFIG_HOME": str(root / "xdg-config"),
            "TOHRU_ASSISTANT_DATA": str(self.data),
            "TOHRU_ASSISTANT_SCRATCH": str(self.scratch),
            "FAKE_LOG": str(self.log),
            "FAKE_SIGNED_IN": "1",
        }

    def fake(self, name, source):
        path = self.bin / name
        path.write_text(source)
        path.chmod(0o755)

    def invoke(self, *args, stdin="", **env):
        return subprocess.run([str(HELPER), *args], input=stdin, env={**self.env, **env}, text=True,
                              capture_output=True, timeout=30)

    def events(self, result):
        return [json.loads(line) for line in result.stdout.splitlines()]

    def calls(self, name):
        return [c for c in map(json.loads, self.log.read_text().splitlines()) if c["argv"][:2] not in (["auth", "status"], ["login", "status"])]

    def test_discovery_lists_only_installed_known_clis_with_auth_and_model(self):
        self.fake("unknown-ai", "#!/bin/sh\n")
        providers = json.loads(self.invoke("providers").stdout)
        self.assertEqual([p["id"] for p in providers], ["claude", "codex"])
        claude, codex = providers
        self.assertEqual((claude["slot"], claude["signed_in"], claude["model"]), (0, True, "Opus 5.5"))
        self.assertEqual((codex["slot"], codex["signed_in"]), (1, False))
        providers = json.loads(self.invoke("providers", FAKE_SIGNED_IN="0").stdout)
        self.assertFalse(providers[0]["signed_in"])

    def test_auth_probes_never_wait_on_a_prompt(self):
        self.fake("claude", "#!/bin/sh\nread line\nsleep 60\n")
        started = time.monotonic()
        providers = json.loads(self.invoke("providers").stdout)
        self.assertLess(time.monotonic() - started, 10)
        self.assertEqual(providers[0]["id"], "claude")

    def test_chat_streams_saves_and_resumes_one_session(self):
        first = self.events(self.invoke("chat", "--provider", "claude", stdin="How many ounces in a cup?\n"))
        self.assertEqual(first[0]["event"], "start")
        cid = first[0]["conversation"]
        self.assertEqual("".join(e["text"] for e in first if e["event"] == "delta"), "Eight fluid ounces.")
        self.assertEqual(first[-1], {"event": "done", "conversation": cid})
        second = self.events(self.invoke("chat", "--provider", "claude", "--conversation", cid, stdin="And a pint?"))
        self.assertEqual(second[-1]["event"], "done")
        calls = self.calls("claude")
        self.assertEqual(len(calls), 2)
        session = calls[0]["argv"][calls[0]["argv"].index("--session-id") + 1]
        self.assertEqual(calls[1]["argv"][calls[1]["argv"].index("--resume") + 1], session)
        for call in calls:
            self.assertIn("--tools", call["argv"])
            self.assertEqual(call["argv"][call["argv"].index("--tools") + 1], "")
            self.assertIn("--strict-mcp-config", call["argv"])
            self.assertEqual(Path(call["cwd"]), self.scratch)
        self.assertEqual(calls[1]["stdin"], "And a pint?")
        self.assertEqual(list(self.scratch.iterdir()), [])
        saved = json.loads((self.data / "chats" / f"{cid}.json").read_text())
        self.assertEqual(saved["title"], "How many ounces in a cup?")
        self.assertEqual([t["role"] for t in saved["turns"]], ["user", "assistant", "user", "assistant"])
        self.assertEqual(oct((self.data / "chats").stat().st_mode & 0o777), "0o700")

    def test_switching_provider_replays_the_transcript_once(self):
        cid = self.events(self.invoke("chat", "--provider", "claude", stdin="Cup?"))[0]["conversation"]
        self.invoke("chat", "--provider", "codex", "--conversation", cid, stdin="Gallon?")
        self.invoke("chat", "--provider", "codex", "--conversation", cid, stdin="Quart?")
        codex = [c for c in self.calls("codex") if c["argv"][0] == "exec"]
        self.assertIn("User: Cup?", codex[0]["stdin"])
        self.assertIn("Assistant: Eight fluid ounces.", codex[0]["stdin"])
        self.assertEqual(codex[1]["argv"][:2], ["exec", "resume"])
        self.assertIn("thread-1", codex[1]["argv"])
        self.assertEqual(codex[1]["stdin"], "Quart?")
        self.assertIn("read-only", codex[0]["argv"])

    def test_stop_interrupts_and_keeps_the_partial_answer(self):
        env = {**self.env, "FAKE_DELAY": "0.4", "FAKE_ANSWER": "one two three four five six seven eight nine ten"}
        process = subprocess.Popen([str(HELPER), "chat", "--provider", "claude"], stdin=subprocess.PIPE,
                                   stdout=subprocess.PIPE, text=True, env=env)
        process.stdin.write("Count")
        process.stdin.close()
        lines = [process.stdout.readline(), process.stdout.readline()]
        process.send_signal(signal.SIGINT)
        rest = process.stdout.read()
        process.stdout.close()
        self.assertEqual(process.wait(timeout=10), 0)
        events = [json.loads(line) for line in lines + rest.splitlines()]
        self.assertEqual(events[-1]["event"], "stopped")
        saved = json.loads((self.data / "chats" / f"{events[0]['conversation']}.json").read_text())
        self.assertTrue(saved["turns"][-1]["stopped"])
        self.assertNotIn("ten", saved["turns"][-1]["text"])

    def test_history_search_show_and_delete(self):
        one = self.events(self.invoke("chat", "--provider", "claude", stdin="Friendlier build-delay note"))[0]["conversation"]
        time.sleep(1.1)
        two = self.events(self.invoke("chat", "--provider", "claude", "--voice", stdin="Cups to ounces"))[0]["conversation"]
        listed = json.loads(self.invoke("history").stdout)
        self.assertEqual([c["id"] for c in listed], [two, one])
        self.assertTrue(listed[0]["voice"])
        self.assertEqual([c["id"] for c in json.loads(self.invoke("history", "--query", "BUILD-delay").stdout)], [one])
        found = json.loads(self.invoke("history", "--query", "fluid").stdout)
        self.assertEqual(len(found), 2)
        self.assertIn("fluid", found[0]["match"])
        self.assertEqual(json.loads(self.invoke("history", "--query", "nothing like this").stdout), [])
        shown = json.loads(self.invoke("show", one).stdout)
        self.assertNotIn("sessions", shown)
        self.assertEqual(self.invoke("delete", one).returncode, 0)
        self.assertEqual(self.invoke("show", one).returncode, 1)
        self.assertNotEqual(self.invoke("show", "../../etc/passwd").returncode, 0)

    def test_missing_provider_fails_without_saving(self):
        events = self.events(self.invoke("chat", "--provider", "gemini", stdin="hi"))
        self.assertEqual(events[-1]["event"], "error")
        self.assertFalse((self.data / "chats").exists())


if __name__ == "__main__":
    unittest.main()
