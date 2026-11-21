#!/usr/bin/env python3
"""Drive bin/tohru-assistant against fake provider CLIs; nothing reaches a real provider."""
import json
import os
import sqlite3
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
    log.write(json.dumps({"cli": os.path.basename(sys.argv[0]), "argv": args, "cwd": os.getcwd(), "auto_memory_off": os.environ.get("CLAUDE_CODE_DISABLE_AUTO_MEMORY") == "1", "stdin": sys.stdin.read() if "-p" in args else ""}) + "\n")
if args[:2] == ["auth", "status"]:
    print(json.dumps({"loggedIn": os.environ.get("FAKE_SIGNED_IN") == "1"})); sys.exit(0)
session = args[args.index("--resume") + 1] if "--resume" in args else args[args.index("--session-id") + 1] if "--session-id" in args else "ephemeral"
print(json.dumps({"type": "system", "subtype": "init", "session_id": session}), flush=True)
words = os.environ.get("FAKE_ANSWER", "Eight fluid ounces.").split(" ")
for i, w in enumerate(words):
    text = w if i == 0 else " " + w
    print(json.dumps({"type": "stream_event", "session_id": session, "event": {"type": "content_block_delta", "delta": {"type": "text_delta", "text": text}}}), flush=True)
    time.sleep(float(os.environ.get("FAKE_DELAY", "0")))
print(json.dumps({"type": "result", "subtype": "success", "result": " ".join(words), "session_id": session}), flush=True)
'''

FAKE_AGY = r'''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps({"cli": os.path.basename(sys.argv[0]), "argv": args, "cwd": os.getcwd(), "stdin": ""}) + "\n")
conversation = args[args.index("--conversation") + 1] if "--conversation" in args else "agy-1"
print(json.dumps({"event": "init", "conversation_id": conversation}))
print(json.dumps({"event": "step_update", "step_update": {"step_type": "agent_response", "text_delta": "Hello."}}))
print(json.dumps({"event": "result", "result": {"status": "SUCCESS", "response": "Hello."}}))
'''

FAKE_CODEX = r'''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps({"cli": os.path.basename(sys.argv[0]), "argv": args, "cwd": os.getcwd(), "stdin": sys.stdin.read() if args[:1] == ["exec"] else ""}) + "\n")
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
        self.persona = root / "persona.md"
        self.persona.write_text("You are Tohru, an ancient dragon.")
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

    def show(self, cid):
        return json.loads(self.invoke("show", cid).stdout)

    def events(self, result):
        return [json.loads(line) for line in result.stdout.splitlines()]

    def calls(self, name):
        return [c for c in map(json.loads, self.log.read_text().splitlines())
                if c["cli"] == name and c["argv"][:2] not in (["auth", "status"], ["login", "status"])]

    def test_discovery_lists_only_installed_known_clis_with_auth_and_model(self):
        self.fake("unknown-ai", "#!/bin/sh\n")
        providers = json.loads(self.invoke("providers").stdout)
        self.assertEqual([p["id"] for p in providers], ["claude", "codex"])
        claude, codex = providers
        self.assertEqual((claude["slot"], claude["signed_in"], claude["model"]), (0, True, "Opus 5.5 · medium"))
        self.assertEqual((codex["slot"], codex["signed_in"], codex["model_id"], codex["effort"]), (1, False, "gpt-6.2-sol", "medium"))
        configured = json.loads(self.invoke("providers", "--models", json.dumps({"claude": {"model": "claude-sonnet-5-5", "effort": "high"}})).stdout)
        self.assertEqual((configured[0]["model"], configured[0]["model_id"]), ("Sonnet 5.5 · high", "claude-sonnet-5-5"))
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
        saved = self.show(cid)
        self.assertEqual(saved["title"], "How many ounces in a cup?")
        self.assertEqual([t["role"] for t in saved["turns"]], ["user", "assistant", "user", "assistant"])
        self.assertEqual(oct((self.data / "assistant.sqlite").stat().st_mode & 0o777), "0o600")
        self.assertEqual(oct(self.data.stat().st_mode & 0o777), "0o700")

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
        saved = self.show(events[0]["conversation"])
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
        self.assertFalse((self.data / "assistant.sqlite").exists())

    def chat(self, *args, stdin="hi", **env):
        return self.events(self.invoke("chat", *args, stdin=stdin, **env))

    def test_default_and_configured_models_reach_each_cli(self):
        self.chat("--provider", "claude")
        self.chat("--provider", "claude", "--model", "claude-sonnet-5-5", "--effort", "max")
        self.chat("--provider", "codex")
        self.chat("--provider", "codex", "--model", "gpt-6.1-sol", "--effort", "max")
        claude = self.calls("claude")
        self.assertEqual(claude[0]["argv"][claude[0]["argv"].index("--model") + 1], "claude-opus-5-5")
        self.assertEqual(claude[0]["argv"][claude[0]["argv"].index("--effort") + 1], "medium")
        self.assertEqual(claude[1]["argv"][claude[1]["argv"].index("--model") + 1], "claude-sonnet-5-5")
        self.assertEqual(claude[1]["argv"][claude[1]["argv"].index("--effort") + 1], "max")
        codex = [c for c in self.calls("codex") if c["argv"][0] == "exec"]
        self.assertEqual(codex[0]["argv"][codex[0]["argv"].index("-m") + 1], "gpt-6.2-sol")
        self.assertIn('model_reasoning_effort="medium"', codex[0]["argv"])
        self.assertIn('model_reasoning_effort="xhigh"', codex[1]["argv"])
        self.fake("agy", FAKE_AGY)
        self.chat("--provider", "agy")
        agy = [c for c in self.calls("agy") if c["argv"][0] == "-p"][-1]["argv"]
        self.assertEqual((agy[agy.index("--model") + 1], agy[agy.index("--effort") + 1]), ("gemini-4-neon", "high"))

    def test_a_rejected_model_shows_the_cli_error(self):
        self.fake("claude", FAKE_CLAUDE.replace('print(json.dumps({"type": "result", "subtype": "success", "result": " ".join(words), "session_id": session}), flush=True)',
                  'print(json.dumps({"type": "result", "subtype": "success", "is_error": True, "result": "There\'s an issue with the selected model (bogus).", "session_id": session}), flush=True)'))
        events = self.chat("--provider", "claude", "--model", "bogus", FAKE_ANSWER="x")
        self.assertEqual(events[-1]["event"], "error")
        self.assertIn("selected model (bogus)", events[-1]["message"])
        self.assertEqual(len(self.calls("claude")), 1)

    def test_persona_goes_through_each_clis_own_mechanism(self):
        self.chat("--provider", "claude", "--persona-file", str(self.persona), "--voice")
        argv = self.calls("claude")[0]["argv"]
        system = argv[argv.index("--append-system-prompt") + 1]
        self.assertTrue(self.calls("claude")[-1]["auto_memory_off"])
        self.assertIn("ancient dragon", system)
        self.assertIn("spoken aloud", system)
        self.assertEqual(self.calls("claude")[0]["stdin"], "hi")
        self.chat("--provider", "codex", "--persona-file", str(self.persona))
        codex = [c for c in self.calls("codex") if c["argv"][0] == "exec"][0]["argv"]
        instructions = next(a for a in codex if a.startswith("developer_instructions="))
        self.assertIn("ancient dragon", json.loads(instructions.split("=", 1)[1]))
        self.assertIn("typed", json.loads(instructions.split("=", 1)[1]))
        self.fake("agy", FAKE_AGY)
        cid = self.chat("--provider", "agy", "--persona-file", str(self.persona), stdin="First")[0]["conversation"]
        self.chat("--provider", "agy", "--persona-file", str(self.persona), "--conversation", cid, stdin="Second")
        agy = [c["argv"] for c in self.calls("agy")]
        self.assertTrue(agy[0][-1].startswith("You are Tohru, an ancient dragon.") and agy[0][-1].endswith("First"))
        self.assertEqual(agy[1][-1], "Second")
        self.assertEqual([t["text"] for t in self.show(cid)["turns"] if t["role"] == "user"], ["First", "Second"])
        self.chat("--provider", "claude", "--persona", "off")
        self.assertNotIn("--append-system-prompt", self.calls("claude")[-1]["argv"])
        shipped = self.chat("--provider", "claude")
        self.assertIn("thousands of years old", self.calls("claude")[-1]["argv"][self.calls("claude")[-1]["argv"].index("--append-system-prompt") + 1])
        self.assertEqual(shipped[-1]["event"], "done")

    EXTRACTED = json.dumps({"summary": "They asked about cups and said they bake bread.",
                            "facts": [{"text": "The user bakes sourdough bread on weekends", "kind": "fact", "confidence": 0.8},
                                      {"text": "The user prefers metric units", "kind": "preference", "confidence": 0.7}],
                            "nicknames": ["Tor"], "topics": ["baking"], "jokes": []})

    def remembered(self):
        cid = self.chat("--provider", "claude", stdin="How many grams of flour in a cup? I bake sourdough.")[0]["conversation"]
        before = len(self.calls("claude"))
        result = self.invoke("remember", cid, FAKE_ANSWER=self.EXTRACTED)
        self.assertEqual(result.returncode, 0, result.stderr)
        extraction = self.calls("claude")[before]
        self.assertIn("long-term memory", extraction["stdin"])
        self.assertIn("--no-session-persistence", extraction["argv"])
        return cid

    def memories(self):
        return json.loads(self.invoke("memory", "list", "--json").stdout)

    def test_memories_are_extracted_once_and_injected_within_budget(self):
        cid = self.remembered()
        state = self.memories()
        self.assertEqual(sorted(m["kind"] for m in state["memories"]), ["fact", "preference"])
        self.assertEqual(state["memories"][0]["source_conversation"], cid)
        self.assertEqual(state["relationship"]["conversations"], 1)
        self.assertEqual(state["relationship"]["nicknames"], ["Tor"])
        self.assertEqual(state["summaries"][0]["text"], "They asked about cups and said they bake bread.")
        calls = len(self.calls("claude"))
        self.assertEqual(self.invoke("remember", cid, FAKE_ANSWER=self.EXTRACTED).returncode, 0)
        self.assertEqual(len(self.calls("claude")), calls)
        self.chat("--provider", "claude", stdin="Any bread tips?")
        system = self.calls("claude")[-1]["argv"][self.calls("claude")[-1]["argv"].index("--append-system-prompt") + 1]
        self.assertIn("sourdough", system)
        self.assertIn("they call you Tor", system)
        db = sqlite3.connect(self.data / "assistant.sqlite")
        with db:
            for i in range(200):
                db.execute("INSERT INTO memories (id, kind, text, created, updated, confidence) VALUES (?, 'fact', ?, 1, ?, 0.5)",
                           (f"{i:032x}", f"The user owns houseplant number {i} which is a fern of some size", 2 + i))
        db.close()
        self.chat("--provider", "claude", stdin="Any bread tips?")
        system = self.calls("claude")[-1]["argv"][self.calls("claude")[-1]["argv"].index("--append-system-prompt") + 1]
        block = system[system.index("What you remember"):]
        self.assertLessEqual((len(block) + 3) // 4, 600)
        self.assertIn("sourdough", block)

    def test_malformed_extraction_is_rejected_without_storing(self):
        cid = self.chat("--provider", "claude", stdin="Hello there")[0]["conversation"]
        for bad in ("Sure! Here are the memories.", json.dumps({"summary": "x", "facts": [{"text": "y", "confidence": 3}]}),
                    json.dumps({"summary": "x", "facts": [], "password": "hunter2"}), json.dumps(["not", "an", "object"]),
                    json.dumps({"facts": []})):
            result = self.invoke("remember", cid, FAKE_ANSWER=bad)
            self.assertEqual(result.returncode, 1, bad)
            self.assertIn("rejected", result.stderr)
        state = self.memories()
        self.assertEqual((state["memories"], state["summaries"], state["relationship"]), ([], [], {}))

    def test_memory_off_spawns_no_extraction_and_injects_nothing(self):
        cid = self.chat("--provider", "claude", "--memory", "off", stdin="I bake sourdough")[0]["conversation"]
        calls = len(self.calls("claude"))
        self.assertEqual(self.invoke("remember", cid, "--memory", "off", FAKE_ANSWER=self.EXTRACTED).returncode, 0)
        self.assertEqual(len(self.calls("claude")), calls)
        self.remembered()
        self.chat("--provider", "claude", "--memory", "off", stdin="Bread?")
        self.assertNotIn("What you remember", " ".join(self.calls("claude")[-1]["argv"]))

    def test_forget_intent_cli_forget_and_clear(self):
        cid = self.remembered()
        reply = self.chat("--provider", "claude", "--conversation", cid, stdin="Forget that.")
        self.assertEqual(reply[-1]["event"], "done")
        self.assertIn("forgotten", "".join(e.get("text", "") for e in reply).lower())
        self.assertEqual(self.memories()["memories"], [])
        calls = len(self.calls("claude"))
        self.invoke("remember", cid, FAKE_ANSWER=self.EXTRACTED)
        self.assertEqual(len(self.calls("claude")), calls)
        self.remembered()
        reply = self.chat("--provider", "claude", stdin="Please forget about my sourdough")
        self.assertIn("let go of 1 thing", "".join(e.get("text", "") for e in reply))
        remaining = self.memories()["memories"]
        self.assertEqual([m["kind"] for m in remaining], ["preference"])
        self.assertEqual(self.invoke("memory", "forget", remaining[0]["id"][:8]).returncode, 0)
        self.assertEqual(self.memories()["memories"], [])
        self.assertEqual(self.invoke("memory", "forget", "deadbeef").returncode, 1)
        self.remembered()
        self.assertEqual(self.invoke("memory", "clear").returncode, 0)
        state = self.memories()
        self.assertEqual((state["memories"], state["summaries"], state["relationship"]), ([], [], {}))
        self.assertGreater(len(json.loads(self.invoke("history").stdout)), 0)

    def test_migrations_create_the_schema_and_refuse_a_newer_one(self):
        self.invoke("history")
        db = sqlite3.connect(self.data / "assistant.sqlite")
        self.assertEqual(db.execute("PRAGMA user_version").fetchone()[0], 1)
        self.assertEqual(db.execute("PRAGMA journal_mode").fetchone()[0], "wal")
        tables = {r[0] for r in db.execute("SELECT name FROM sqlite_master WHERE type = 'table'")}
        self.assertTrue({"conversations", "turns", "turns_fts", "memories", "memories_fts", "summaries", "relationship"} <= tables)
        db.execute("PRAGMA user_version = 9")
        db.close()
        result = self.invoke("history")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("newer Tohru", result.stderr)

    def test_export_import_round_trip_and_rejection(self):
        self.remembered()
        self.chat("--provider", "codex", "--voice", stdin="Gallon?")
        archive = Path(self.directory.name) / "export.json"
        self.assertEqual(self.invoke("export", str(archive)).returncode, 0)
        self.assertEqual(oct(archive.stat().st_mode & 0o777), "0o600")
        first = json.loads(archive.read_text())
        self.assertEqual((first["format"], first["schema"]), ("tohru-assistant-export", 1))
        self.assertEqual(len(first["conversations"]), 2)
        self.assertIn("thousands of years old", first["persona"]["text"])
        fresh = {"TOHRU_ASSISTANT_DATA": str(Path(self.directory.name) / "fresh")}
        self.assertNotEqual(self.invoke("import", str(archive), "--replace", **fresh).returncode, 0)
        self.assertEqual(self.invoke("import", str(archive), "--replace", "--yes", **fresh).returncode, 0)
        second = json.loads(self.invoke("export", **fresh).stdout)
        for archive_data in (first, second):
            archive_data.pop("exported")
        self.assertEqual(first, second)

        before = (self.data / "assistant.sqlite").read_bytes()
        broken = Path(self.directory.name) / "broken.json"
        for bad in ("{", json.dumps({"format": "tohru-assistant-export", "schema": 99}),
                    json.dumps({**second, "conversations": [{"id": "../x", "title": "", "provider": "", "turns": []}]}),
                    json.dumps({**second, "memories": [{"id": "0" * 32, "kind": "secret", "text": "x", "created": 1, "updated": 1, "confidence": 1}]})):
            broken.write_text(bad)
            result = self.invoke("import", str(broken))
            self.assertEqual(result.returncode, 1, bad)
        self.assertEqual(json.loads(self.invoke("export").stdout)["conversations"], json.loads(archive.read_text())["conversations"])

        newer = json.loads(archive.read_text())
        newer["memories"][0]["text"] = "The user bakes rye now"
        newer["memories"][0]["updated"] += 10
        older = json.loads(archive.read_text())
        older["memories"][1]["text"] = "stale"
        older["memories"][1]["updated"] -= 10
        newer["memories"][1] = older["memories"][1]
        broken.write_text(json.dumps(newer))
        result = self.invoke("import", str(broken), "--only", "memory")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["imported"]["memories"], 1)
        texts = sorted(m["text"] for m in self.memories()["memories"])
        self.assertIn("The user bakes rye now", texts)
        self.assertNotIn("stale", texts)


if __name__ == "__main__":
    unittest.main()
