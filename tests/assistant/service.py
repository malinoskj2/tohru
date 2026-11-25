#!/usr/bin/env python3
"""Drive bin/tohru-assistant against fake provider CLIs; nothing reaches a real provider."""
import json
import os
import runpy
import sys
import sqlite3
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import threading
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "bin/tohru-assistant"

FAKE_CLAUDE = r'''#!/usr/bin/env python3
import json, os, sys, time
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps({"cli": os.path.basename(sys.argv[0]), "argv": args, "cwd": os.getcwd(), "auto_memory_off": os.environ.get("CLAUDE_CODE_DISABLE_AUTO_MEMORY") == "1", "orca_env": {k: v for k, v in os.environ.items() if k.startswith("ORCA_")}, "stdin": sys.stdin.read() if "-p" in args else ""}) + "\n")
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
    log.write(json.dumps({"cli": os.path.basename(sys.argv[0]), "argv": args, "cwd": os.getcwd(), "orca_env": {k: v for k, v in os.environ.items() if k.startswith("ORCA_")}, "stdin": ""}) + "\n")
conversation = args[args.index("--conversation") + 1] if "--conversation" in args else "agy-1"
print(json.dumps({"event": "init", "conversation_id": conversation}))
print(json.dumps({"event": "step_update", "step_update": {"step_type": "agent_response", "text_delta": "Hello."}}))
print(json.dumps({"event": "result", "result": {"status": "SUCCESS", "response": "Hello."}}))
'''

FAKE_CODEX = r'''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps({"cli": os.path.basename(sys.argv[0]), "argv": args, "cwd": os.getcwd(), "orca_env": {k: v for k, v in os.environ.items() if k.startswith("ORCA_")}, "stdin": sys.stdin.read() if args[:1] == ["exec"] else ""}) + "\n")
if args[:2] == ["login", "status"]:
    sys.exit(1)
print(json.dumps({"type": "thread.started", "thread_id": "thread-1"}))
print(json.dumps({"type": "item.completed", "item": {"type": "agent_message", "text": "Sixteen cups."}}))
print(json.dumps({"type": "turn.completed"}))
'''

FAKE_GEMINI = r'''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps({"cli": os.path.basename(sys.argv[0]), "argv": args, "cwd": os.getcwd(), "orca_env": {k: v for k, v in os.environ.items() if k.startswith("ORCA_")}, "stdin": ""}) + "\n")
session = args[args.index("--resume") + 1] if "--resume" in args else "gem-1"
print(json.dumps({"type": "init", "session_id": session}))
print(json.dumps({"type": "message", "role": "assistant", "content": "Hi from Gemini."}))
'''

FAKE_OPENCODE = r'''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as log:
    log.write(json.dumps({"cli": os.path.basename(sys.argv[0]), "argv": args, "cwd": os.getcwd(), "orca_env": {k: v for k, v in os.environ.items() if k.startswith("ORCA_")}, "stdin": ""}) + "\n")
session = args[args.index("--session") + 1] if "--session" in args else "oc-1"
print(json.dumps({"type": "text", "sessionID": session, "part": {"text": "Hi from OpenCode."}}))
'''

FAKE_ORCA_TEMPLATE = r'''#!/usr/bin/env python3
import json, os, sys
log_path = os.environ.get("FAKE_ORCA_LOG")
if log_path:
    with open(log_path, "a") as f:
        f.write(json.dumps({"argv": sys.argv[1:], "user_data": os.environ.get("ORCA_USER_DATA_PATH", "")}) + "\n")
running = {running!r}
print(json.dumps({"ok": True, "result": {"app": {"running": running}, "runtime": {"reachable": running}}}))
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
        system = root / "system-bin"
        system.mkdir()
        for directory in ("/usr/bin", "/bin"):
            for entry in Path(directory).iterdir() if Path(directory).is_dir() else ():
                if entry.name != "orca-ide" and not (system / entry.name).exists():
                    (system / entry.name).symlink_to(entry)
        self.env = {
            "PATH": f"{self.bin}:{system}:" + os.path.dirname(os.path.realpath(subprocess.check_output(["sh", "-c", "command -v python3"], text=True).strip())),
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
            self.assertEqual(call["argv"][call["argv"].index("--permission-mode") + 1], "bypassPermissions")
            self.assertNotIn("--tools", call["argv"])
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
        self.assertIn("--dangerously-bypass-approvals-and-sandbox", codex[0]["argv"])
        self.assertNotIn("read-only", codex[0]["argv"])

    def test_completed_codex_message_is_speakable_before_turn_end(self):
        voice = runpy.run_path(str(ROOT / "bin/tohru-voice"))
        stream = voice["SpeechStream"]()
        result = self.invoke("chat", "--provider", "codex", stdin="How many cups?")
        self.assertEqual(result.returncode, 0, result.stderr)
        for event in self.events(result):
            if event["event"] == "delta":
                stream.feed(event["text"])
        spoken, ready, stop = [], threading.Event(), threading.Event()
        def consume():
            for part in stream.parts(stop):
                spoken.append(part)
                ready.set()
        worker = threading.Thread(target=consume)
        worker.start()
        try:
            self.assertTrue(ready.wait(1), "completed message waited for the turn to end")
            self.assertEqual(spoken, ["Sixteen cups."])
        finally:
            stop.set()
            stream.feed("", final=True)
            worker.join(1)

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

    def test_voice_failures_are_spoken_in_tohrus_words_with_the_real_error(self):
        source = FAKE_CLAUDE.replace('words = os.environ.get("FAKE_ANSWER", "Eight fluid ounces.").split(" ")',
                                     'failure = "Something just failed" in " ".join(args)\n'
                                     'words = [\'{"text":"I tripped over that model name."}\'] if failure else os.environ.get("FAKE_ANSWER", "").split(" ")')
        self.fake("claude", source.replace('"subtype": "success", "result"', '"subtype": "success", "is_error": not failure, "result": " ".join(words) if failure else "There\'s an issue with the selected model (bogus).", "_"'))
        events = self.chat("--provider", "claude", "--model", "bogus", "--voice", FAKE_ANSWER="x")
        self.assertEqual(events[-1]["event"], "error")
        self.assertIn("selected model (bogus)", events[-1]["message"])
        self.assertEqual(events[-1]["spoken"], "I tripped over that model name.")
        failure = self.calls("claude")[-1]
        self.assertIn("selected model (bogus)", failure["stdin"])
        self.assertEqual(failure["argv"][failure["argv"].index("--tools") + 1], "")
        typed = self.chat("--provider", "claude", "--model", "bogus", FAKE_ANSWER="x")
        self.assertNotIn("spoken", typed[-1])

    def test_voice_fast_default_preserves_typed_and_explicit_choices(self):
        for flags, model, effort in (
            ([], 'gpt-6.2-sol', 'medium'),
            (['--voice'], 'gpt-6.2-sol', 'medium'),
            (['--voice', '--draft'], 'gpt-6-luna', 'low'),
            (['--voice', '--draft', '--semantic'], 'gpt-6.2-sol', 'low'),
            (['--voice', '--draft', '--model', 'gpt-6.2-sol'], 'gpt-6.2-sol', 'medium'),
            (['--voice', '--draft', '--effort', 'high'], 'gpt-6.2-sol', 'high'),
            (['--voice', '--draft', '--semantic', '--model', 'gpt-6.2-sol', '--effort', 'high'], 'gpt-6.2-sol', 'high'),
            (['--voice', '--draft', '--model', 'gpt-6.2-sol', '--effort', 'high', '--scenario-model', 'gpt-6.1-sol', '--scenario-effort', 'low'], 'gpt-6.1-sol', 'low'),
            (['--voice', '--draft', '--scenario-effort', 'medium'], 'gpt-6-luna', 'medium'),
            (['--voice', '--draft', '--semantic', '--scenario-model', 'gpt-6.2-astra'], 'gpt-6.2-astra', 'low'),
            (['--voice', '--scenario-model', 'gpt-6-luna'], 'gpt-6.2-sol', 'medium'),
        ):
            events = self.events(self.invoke('chat', '--provider', 'codex', *flags, stdin='Hello'))
            argv = self.calls('codex')[-1]['argv']
            self.assertEqual(argv[argv.index('-m')+1], model)
            self.assertIn('model_reasoning_effort="'+effort+'"', argv)
            self.assertEqual((events[0]['model'], events[0]['effort']), (model, effort))
        providers = json.loads(self.invoke('providers').stdout)
        codex = next(p for p in providers if p['id']=='codex')
        self.assertEqual((codex['voice_draft_model_id'],codex['voice_draft_effort']), ('gpt-6-luna','low'))
        self.assertEqual((codex['semantic_model_id'],codex['semantic_effort']), ('gpt-6.2-sol','low'))
        configured = json.loads(self.invoke('providers', '--models', json.dumps({'codex': {
            'model':'gpt-6.2-sol', 'effort':'high',
            'voice_models':{'model':'gpt-6-luna','effort':'low'},
            'semantic_models':{'model':'gpt-6.2-astra','effort':'medium'},
        }})).stdout)
        codex = next(p for p in configured if p['id']=='codex')
        self.assertEqual((codex['model_id'],codex['effort']), ('gpt-6.2-sol','high'))
        self.assertEqual((codex['voice_draft_model_id'],codex['voice_draft_effort']), ('gpt-6-luna','low'))
        self.assertEqual((codex['semantic_model_id'],codex['semantic_effort']), ('gpt-6.2-astra','medium'))

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
        extraction = next(c["argv"] for c in self.calls("claude") if "--no-session-persistence" in c["argv"])
        self.assertEqual(extraction[extraction.index("--tools") + 1], "")
        self.assertNotIn("--permission-mode", extraction)
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

    def make_orca_shim(self, user_data, running=True):
        shim = Path(user_data) / "linux-orca-cli-shim" / "orca"
        shim.parent.mkdir(parents=True, exist_ok=True)
        shim.write_text(FAKE_ORCA_TEMPLATE.replace("{running!r}", repr(running)))
        shim.chmod(0o755)
        return shim

    def test_orca_absent_and_never_resolves_bare_orca(self):
        bare_marker = Path(self.directory.name) / "bare-orca-ran"
        self.fake("orca", f"#!/bin/sh\ntouch '{bare_marker}'\nexit 0\n")
        self.chat("--provider", "claude", "--persona", "off")
        call = self.calls("claude")[-1]
        self.assertFalse(bare_marker.exists())
        self.assertEqual(call["orca_env"], {})
        self.assertNotIn("--append-system-prompt", call["argv"])

        self.chat("--provider", "claude", "--persona", "off", "--orca", "true")
        forced = self.calls("claude")[-1]
        self.assertFalse(bare_marker.exists())
        self.assertEqual(forced["orca_env"], {})
        system = forced["argv"][forced["argv"].index("--append-system-prompt") + 1]
        self.assertIn("no Orca CLI shim or `orca-ide`", system)
        self.assertIn("never run bare `orca`", system.lower())

    def test_orca_shim_detection_caches_status_and_strips_terminal_vars(self):
        user_data = Path(self.directory.name) / "orca-user-data"
        shim = self.make_orca_shim(user_data, running=True)
        orca_log = Path(self.directory.name) / "orca-probes.jsonl"
        leaked = {
            "ORCA_USER_DATA_PATH": str(user_data),
            "ORCA_WORKTREE_ID": "wt-leak",
            "ORCA_TERMINAL_HANDLE": "term-leak",
            "ORCA_TAB_ID": "tab-leak",
            "ORCA_PANE_KEY": "pane-leak",
            "ORCA_AGENT_SESSION_ID": "sess-leak",
            "ORCA_AGENT_HOOK_TOKEN": "tok-leak",
            "FAKE_ORCA_LOG": str(orca_log),
        }
        self.chat("--provider", "claude", "--persona", "off", **leaked)
        self.chat("--provider", "claude", "--persona", "off", **leaked)
        calls = self.calls("claude")
        self.assertEqual(calls[0]["orca_env"], {"ORCA_CLI_COMMAND": str(shim), "ORCA_USER_DATA_PATH": str(user_data)})
        self.assertEqual(calls[1]["orca_env"], {"ORCA_CLI_COMMAND": str(shim), "ORCA_USER_DATA_PATH": str(user_data)})
        system = calls[0]["argv"][calls[0]["argv"].index("--append-system-prompt") + 1]
        self.assertIn("currently running", system)
        self.assertIn("NOT running inside an Orca terminal or worktree", system)
        self.assertIn("may or may not be talking about Orca", system)
        self.assertIn(f"{shim} skills get orca-cli", system)
        self.assertIn(f"{shim} skills get orchestration", system)
        self.assertEqual(len(orca_log.read_text().splitlines()), 1)
        self.assertEqual(list(self.scratch.iterdir()), [])

    def test_orca_not_running_still_gives_cli_context(self):
        xdg_orca = Path(self.env["XDG_CONFIG_HOME"]) / "orca"
        shim = self.make_orca_shim(xdg_orca, running=False)
        self.chat("--provider", "claude", "--persona", "off")
        call = self.calls("claude")[-1]
        self.assertEqual(call["orca_env"], {"ORCA_CLI_COMMAND": str(shim), "ORCA_USER_DATA_PATH": str(xdg_orca)})
        system = call["argv"][call["argv"].index("--append-system-prompt") + 1]
        self.assertIn("not currently running", system)
        self.assertIn(f"{shim} skills get orca-cli", system)

    def test_orca_context_reaches_every_provider_via_its_own_mechanism(self):
        orca_ide = self.bin / "orca-ide"
        orca_ide.write_text(FAKE_ORCA_TEMPLATE.replace("{running!r}", "True"))
        orca_ide.chmod(0o755)
        expected_env = {"ORCA_CLI_COMMAND": str(orca_ide), "ORCA_USER_DATA_PATH": str(Path(self.env["XDG_CONFIG_HOME"]) / "orca")}
        self.fake("agy", FAKE_AGY)
        self.fake("gemini", FAKE_GEMINI)
        self.fake("opencode", FAKE_OPENCODE)

        self.chat("--provider", "claude", "--persona", "off")
        claude = self.calls("claude")[-1]
        self.assertEqual(claude["orca_env"], expected_env)
        self.assertIn(f"{orca_ide} skills get orca-cli", claude["argv"][claude["argv"].index("--append-system-prompt") + 1])

        self.chat("--provider", "codex", "--persona", "off")
        codex = self.calls("codex")[-1]
        self.assertEqual(codex["orca_env"], expected_env)
        dev = next(a for a in codex["argv"] if a.startswith("developer_instructions="))
        self.assertIn(f"{orca_ide} skills get orca-cli", json.loads(dev.split("=", 1)[1]))

        for name in ("agy", "gemini", "opencode"):
            cid = self.chat("--provider", name, "--persona", "off", stdin="First")[0]["conversation"]
            self.chat("--provider", name, "--persona", "off", "--conversation", cid, stdin="Second")
            turns = self.calls(name)
            self.assertEqual(turns[0]["orca_env"], expected_env, name)
            self.assertEqual(turns[1]["orca_env"], expected_env, name)
            self.assertIn(f"{orca_ide} skills get orca-cli", turns[0]["argv"][-1], name)
            self.assertEqual(turns[1]["argv"][-1], "Second", name)

    def test_orca_false_disables_detection_env_and_prompt(self):
        user_data = Path(self.directory.name) / "orca-disabled"
        shim = self.make_orca_shim(user_data, running=True)
        orca_log = Path(self.directory.name) / "orca-disabled.jsonl"
        self.chat("--provider", "claude", "--persona", "off", "--orca", "false",
                  ORCA_USER_DATA_PATH=str(user_data), ORCA_CLI_COMMAND=str(shim),
                  ORCA_WORKTREE_ID="wt-leak", FAKE_ORCA_LOG=str(orca_log))
        call = self.calls("claude")[-1]
        self.assertFalse(orca_log.exists())
        self.assertEqual(call["orca_env"], {})
        self.assertNotIn("--append-system-prompt", call["argv"])


    def test_semantic_protocol_is_explicit_and_tool_free(self):
        for answer, decision in (("WAIT", "wait"), ("RESPOND\nA short answer.", "respond"),
                                 ("Ordinary prose says WAIT", "error"), ("RESPOND: answer", "error")):
            events = self.events(self.invoke("chat", "--provider", "claude", "--voice", "--draft", "--semantic",
                                             stdin="Maybe a map", FAKE_ANSWER=answer))
            self.assertEqual(next(e['decision'] for e in events if e['event'] == 'semantic'), decision)
            if decision != 'respond':
                self.assertFalse(any(e['event'] in ('delta', 'done') for e in events))
            argv = self.calls('claude')[-1]['argv']
            self.assertEqual(argv[argv.index('--tools') + 1], '')
        ordinary = self.events(self.invoke("chat", "--provider", "claude", "--voice", "--draft",
                                           stdin="Quote a label", FAKE_ANSWER="WAIT"))
        self.assertFalse(any(e['event'] == 'semantic' for e in ordinary))

    def test_decision_precedes_answer_without_streaming_unvalidated_text(self):
        env = {**self.env, 'FAKE_ANSWER': 'RESPOND\nFirst second third fourth.', 'FAKE_DELAY': '0.08'}
        process = subprocess.Popen([str(HELPER), 'chat', '--provider', 'claude', '--voice', '--draft', '--semantic'],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env)
        process.stdin.write('Explain this please')
        process.stdin.close()
        events = [(json.loads(line), time.monotonic()) for line in process.stdout]
        process.stdout.close()
        self.assertEqual(process.wait(), 0)
        intent = next(t for e,t in events if e['event'] == 'semantic')
        delta = next(t for e,t in events if e['event'] == 'delta')
        self.assertGreater(delta-intent, .15)
        self.assertEqual(events[-1][0]['event'], 'done')

    def test_optional_delivery_column_keeps_schema_one_compatible(self):
        result = self.chat('--provider', 'claude', stdin='Hello')
        cid = result[0]['conversation']
        db = sqlite3.connect(self.data / 'assistant.sqlite')
        self.assertEqual(db.execute('PRAGMA user_version').fetchone()[0], 1)
        # The old helper inserts explicit columns and ignores delivery on read.
        db.execute("INSERT INTO turns(id,conversation_id,seq,role,text,time) VALUES(?,?,?,?,?,?)",
                   ('a'*32,cid,2,'user','Old writer',1))
        db.commit()
        self.assertEqual(db.execute('SELECT delivery FROM turns WHERE id=?', ('a'*32,)).fetchone()[0], '')
        db.close()

    def test_draft_forks_session_without_writing_history_until_committed(self):
        # First turn establishes a Claude session.
        first = self.chat("--provider", "claude", "--voice", stdin="How many ounces in a cup?")
        cid = first[0]["conversation"]
        orig_session = self.calls("claude")[0]["argv"][self.calls("claude")[0]["argv"].index("--session-id") + 1]
        # A discarded draft on that conversation uses --resume + --fork-session, tools disabled, and writes nothing to SQLite.
        draft = self.events(self.invoke("chat", "--provider", "claude", "--conversation", cid, "--voice", "--draft",
                                        stdin="What is the weather in Tokyo?", FAKE_ANSWER="Sunny."))
        self.assertEqual(draft[-1]["event"], "done")
        self.assertTrue(draft[0].get("draft"))
        draft_argv = self.calls("claude")[-1]["argv"]
        self.assertEqual(draft_argv[draft_argv.index("--resume") + 1], orig_session)
        self.assertIn("--fork-session", draft_argv)
        self.assertEqual(draft_argv[draft_argv.index("--tools") + 1], "")
        self.assertIn("--strict-mcp-config", draft_argv)
        self.assertNotIn("--permission-mode", draft_argv)
        self.assertNotIn("bypassPermissions", draft_argv)
        self.assertIn("<<needs-tools>>", draft_argv[draft_argv.index("--append-system-prompt") + 1])
        # A draft that replies with <<needs-tools>> emits needs_tools and leaves history and session untouched.
        marker = self.events(self.invoke("chat", "--provider", "claude", "--conversation", cid, "--voice", "--draft",
                                         stdin="Open the sound panel", FAKE_ANSWER="<<needs-tools>>"))
        self.assertEqual(marker[-1], {"event": "needs_tools", "conversation": cid})
        self.assertNotEqual(self.invoke("commit", "--provider", "claude", "--conversation", cid, "--voice",
                                        stdin=json.dumps({"prompt": "Open the sound panel", "answer": "<<needs-tools>>"})).returncode, 0)
        # History and saved provider session are untouched by discarded and needs_tools drafts.
        self.assertEqual(len(self.show(cid)["turns"]), 2)
        self.chat("--provider", "claude", "--conversation", cid, "--voice", stdin="And a pint?")
        self.assertEqual(self.calls("claude")[-1]["argv"][self.calls("claude")[-1]["argv"].index("--resume") + 1], orig_session)
        # Committing a draft records the turn and advances the session without another provider call.
        before_calls = len(self.calls("claude"))
        committed = self.events(self.invoke("commit", "--provider", "claude", "--conversation", cid, "--voice", "--session", "fork-99",
                                            stdin=json.dumps({"prompt": "What is the weather in Tokyo?", "answer": "Sunny."})))
        self.assertEqual(committed[-1], {"event": "committed", "conversation": cid})
        self.assertEqual(len(self.calls("claude")), before_calls)
        turns = self.show(cid)["turns"]
        self.assertEqual((turns[-2]["text"], turns[-1]["text"]), ("What is the weather in Tokyo?", "Sunny."))
        self.chat("--provider", "claude", "--conversation", cid, "--voice", stdin="Thanks")
        self.assertEqual(self.calls("claude")[-1]["argv"][self.calls("claude")[-1]["argv"].index("--resume") + 1], "fork-99")
        # Fresh and forked Codex drafts run read-only with MCP disabled and no bypass flag.
        self.invoke("chat", "--provider", "codex", "--voice", "--draft", stdin="Fresh codex draft")
        codex_fresh = self.calls("codex")[-1]["argv"]
        self.assertEqual(codex_fresh[0], "exec")
        self.assertIn("--sandbox", codex_fresh)
        self.assertIn("read-only", codex_fresh)
        self.assertIn("mcp_servers={}", codex_fresh)
        self.assertIn('sandbox_mode="read-only"', codex_fresh)
        self.assertNotIn("--dangerously-bypass-approvals-and-sandbox", codex_fresh)
        cid2 = self.chat("--provider", "codex", "--voice", stdin="First codex turn")[0]["conversation"]
        self.invoke("chat", "--provider", "codex", "--conversation", cid2, "--voice", "--draft", stdin="Draft codex turn")
        codex_draft = self.calls("codex")[-1]["argv"]
        self.assertEqual(codex_draft[:2], ["exec", "fork"])
        self.assertIn("mcp_servers={}", codex_draft)
        self.assertIn('sandbox_mode="read-only"', codex_draft)
        self.assertNotIn("--dangerously-bypass-approvals-and-sandbox", codex_draft)
        self.assertEqual(len(self.show(cid2)["turns"]), 2)
        # Unsupported provider (agy) declines drafts without spawning the CLI.
        self.fake("agy", FAKE_AGY)
        res = self.events(self.invoke("chat", "--provider", "agy", "--voice", "--draft", stdin="Hello there friend"))
        self.assertEqual(res, [{"event": "draft_unsupported"}])
        self.assertEqual(self.calls("agy"), [])

    def test_rollback_drops_cancelled_fragment_and_interrupted_context_reaches_model(self):
        # First fragment cancelled before speaking: rollback removes the empty conversation so only the merged turn remains.
        frag = self.chat("--provider", "claude", "--voice", stdin="Tell me about the history")
        cid = frag[0]["conversation"]
        rb = self.events(self.invoke("rollback", cid))
        self.assertEqual(rb[-1], {"event": "rolled_back", "conversation": cid, "deleted": True})
        self.assertEqual(self.invoke("show", cid).returncode, 1)
        merged = self.chat("--provider", "claude", "--voice", stdin="Tell me about the history of Rome")
        cid_m = merged[0]["conversation"]
        self.assertEqual([t["text"] for t in self.show(cid_m)["turns"] if t["role"] == "user"], ["Tell me about the history of Rome"])
        # Interruption context reaches the model while history keeps only the user's clean words.
        self.chat("--provider", "claude", "--conversation", cid_m, "--voice",
                  "--interrupted", "Rome was founded in…", stdin="Wait, what about Athens?")
        sent = self.calls("claude")[-1]["stdin"]
        self.assertIn('Application playback evidence: Rome was founded in…', sent)
        self.assertIn('not proof it was heard', sent)
        previous = self.show(cid_m)["turns"][-3]
        self.assertIn('not proof it was heard', previous['delivery'])
        self.assertEqual(previous['text'], 'Eight fluid ounces.')
        self.assertIn("Wait, what about Athens?", sent)
        self.assertEqual(self.show(cid_m)["turns"][-2]["text"], "Wait, what about Athens?")


FAKE_ORCA = r'''#!/usr/bin/env python3
import json, os, sys, uuid
from pathlib import Path

args = sys.argv[1:]
state_path = Path(os.environ["FAKE_ORCA_STATE"])
log_path = Path(os.environ["FAKE_ORCA_LOG"])
with open(log_path, "a") as f:
    f.write(json.dumps({
        "argv": args,
        "stripped_env": all(k not in os.environ for k in ("ORCA_AGENT_SESSION_ID", "ORCA_TERMINAL_HANDLE", "ORCA_ENVIRONMENT")),
    }) + "\n")

if os.environ.get("FAKE_ORCA_OFFLINE") == "1":
    print(json.dumps({"ok": False, "error": {"code": "runtime_unavailable", "message": "Orca is not running"}}))
    sys.exit(1)

state = json.loads(state_path.read_text())

def save():
    state_path.write_text(json.dumps(state))

if args[:2] == ["bot", "list"]:
    print(json.dumps({"ok": True, "result": {"bots": state["bots"], "unreachable": []}}))
    sys.exit(0)

if args[:2] == ["bot", "voice-ready"]:
    print(json.dumps({"ok": True, "result": {"registered": True}}))
    sys.exit(0)

if args[:2] == ["bot", "voice-side"]:
    phase = args[args.index("--phase") + 1]
    request = args[args.index("--request") + 1]
    text = Path(args[args.index("--text-file") + 1]).read_text()
    home = next(c["id"] for c in state["chats"] if c.get("home"))
    if phase == "begin":
        if home not in state.get("pending", {}):
            result = {"route": "home"}
        else:
            history = list(state["entries"][home])
            state["entries"][home].append({"kind":"message", "role":"user", "text":text, "interaction":{"kind":"internal"}})
            result = {"route":"side", "chat":home, "request":request, "history":history,
                      "approvals":[{"title":"Approve deploy", "state":"pending"}]}
    else:
        result = {"recorded": os.environ.get("FAKE_SIDE_REJECT") != "1"}
        if result["recorded"]:
            state["entries"][home].append({"kind":"message", "role":"assistant", "text":text, "interaction":{"kind":"internal"}})
    save()
    print(json.dumps({"ok":True, "result":result}))
    sys.exit(0)

if args[:3] == ["bot", "item", "list"]:
    print(json.dumps({"ok":True, "result":{"items":[{"title":"Native build", "links":[{"ref":"dispatch:ctx_build", "state":"working"}]}]}}))
    sys.exit(0)

if args[:2] == ["orchestration", "worker-list"]:
    print(json.dumps({"ok":True, "result":{"workers":[{"dispatchId":"ctx_build", "terminalState":"retained"}]}}))
    sys.exit(0)

if args[:2] == ["orchestration", "worker-show"]:
    print(json.dumps({"ok":True, "result":{"dispatch":{"id":"ctx_build", "state":"working"}, "task":{"status":"in_progress"}}}))
    sys.exit(0)

if args[:2] == ["worktree", "list"]:
    print(json.dumps({"ok":True, "result":{"worktrees":[{"branch":"current-build"}], "scope":{"covered":["local"]}}}))
    sys.exit(0)

if args[:2] == ["bot", "chats"]:
    print(json.dumps({"ok": True, "result": {"bot": state["bots"][0]["name"] if state["bots"] else "Tohru", "chats": state["chats"]}}))
    sys.exit(0)

if args[:2] == ["agent", "read"]:
    sid = args[2].split(":", 1)[1]
    chat = next((c for c in state["chats"] if c["id"] == sid), None)
    if chat is None:
        print(json.dumps({"ok": False, "error": {"code": "not_found", "message": "chat not found"}}))
        sys.exit(1)
    pending = state.get("pending", {}).get(sid)
    if pending:
        pending["polls"] -= 1
        if pending["polls"] <= 0:
            state["entries"][sid].append({"kind": "message", "role": "assistant", "text": pending["reply"]})
            chat["lastSaid"] = pending["reply"]
            chat["lastActivityAt"] = chat.get("lastActivityAt", 1700000000000) + 1000
            del state["pending"][sid]
            save()
            print(json.dumps({"ok": True, "result": {"orcaSessionId": sid, "state": "done", "entries": state["entries"][sid]}}))
            sys.exit(0)
        save()
        entries = list(state["entries"][sid])
        if pending.get("partial") and pending["polls"] == 1:
            entries.append({"kind": "message", "role": "assistant", "text": pending["partial"]})
        print(json.dumps({"ok": True, "result": {"orcaSessionId": sid, "state": "working", "entries": entries}}))
        sys.exit(0)
    print(json.dumps({"ok": True, "result": {"orcaSessionId": sid, "state": "done", "entries": state["entries"].get(sid, [])}}))
    sys.exit(0)

if args[:2] == ["bot", "send"]:
    body_file = args[args.index("--text-file") + 1]
    body = Path(body_file).read_text(encoding="utf-8")
    subject = args[args.index("--subject") + 1] if "--subject" in args else "Desktop"
    chat_arg = args[args.index("--chat") + 1] if "--chat" in args else None
    if chat_arg == "new":
        cid = str(uuid.uuid4())
        chat = {"id": cid, "title": subject, "lastActivityAt": 1700000010000, "lastAsked": body, "lastSaid": "", "home": False, "closed": False}
        state["chats"].append(chat)
        state["entries"][cid] = []
    else:
        chat = next((c for c in state["chats"] if c["id"] == chat_arg), None) or next(c for c in state["chats"] if c.get("home"))
        if chat.get("closed"):
            print(json.dumps({"ok": False, "error": {"code": "invalid_argument", "message": "Chat is closed or does not belong to this bot"}}))
            sys.exit(1)
        cid = chat["id"]
    message_id = str(uuid.uuid4())
    state["entries"].setdefault(cid, []).append({"kind": "message", "role": "user", "text": body, "itemId": message_id})
    chat["lastAsked"] = body
    reply = os.environ.get("FAKE_ORCA_REPLY", "Eight fluid ounces from Orca.")
    polls = int(os.environ.get("FAKE_ORCA_POLLS", "2"))
    partial = os.environ.get("FAKE_ORCA_PARTIAL", "")
    state.setdefault("pending", {})[cid] = {"polls": polls, "reply": reply, "partial": partial}
    save()
    print(json.dumps({"ok": True, "result": {"promptItemId": message_id, "sessionId": cid}}))
    sys.exit(0)

print(json.dumps({"ok": False, "error": {"code": "unknown", "message": "unexpected args"}}))
sys.exit(1)
'''


class OrcaBotBackend(unittest.TestCase):
    def quick(self, prompt, **env):
        return self.invoke("chat", "--backend", "orca", "--orca-bot", "Tohru", "--provider", "orca:Tohru",
                           "--orca-voice-chat", "quick", "--voice", stdin=prompt, **env)

    def busy_home(self):
        state = json.loads(self.orca_state.read_text())
        state["pending"][self.HOME_UUID] = {"polls":1000, "reply":"Home long task result."}
        state["entries"][self.HOME_UUID].append({"kind":"message", "role":"assistant", "text":"FULL HOME CONTEXT " + "detail " * 1000})
        self.orca_state.write_text(json.dumps(state))

    def test_quick_idle_uses_home_and_does_not_run_a_side_model(self):
        result = self.quick("Hello")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Eight fluid ounces from Orca.", result.stdout)
        self.assertFalse(self.fake_claude_log.read_text())
        self.assertEqual(len(json.loads(self.orca_state.read_text())["chats"]), 1)
        metrics = [json.loads(line) for line in (self.scratch / "home-metrics.jsonl").read_text().splitlines()]
        self.assertEqual(metrics[0]["event"], "owner_wait")
        self.assertGreaterEqual(metrics[0]["duration_ms"], 0)
        self.assertNotIn("Hello", json.dumps(metrics))

    def test_quick_busy_records_only_one_answer_in_home_and_reads_actual_dispatch(self):
        self.busy_home()
        answer = "The build is still running; deploy approval is pending."
        result = self.quick("How is the build doing?", FAKE_ANSWER=json.dumps({"route":"answer", "text":answer}))
        self.assertEqual(result.returncode, 0, result.stderr)
        events = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual([event["text"] for event in events if event["event"] == "delta"], [answer])
        state = json.loads(self.orca_state.read_text())
        self.assertEqual(len(state["chats"]), 1)
        self.assertEqual(state["pending"][self.HOME_UUID]["polls"], 1000)
        self.assertEqual(state["entries"][self.HOME_UUID][-1]["text"], answer)
        self.assertFalse(any(call["argv"][:2] == ["bot", "send"] for call in self.orca_calls()))
        self.assertTrue(any(call["argv"][:2] == ["orchestration", "worker-show"] and "--full" in call["argv"] for call in self.orca_calls()))
        model = json.loads(self.fake_claude_log.read_text().splitlines()[-1])
        self.assertIn("--no-session-persistence", model["argv"])
        self.assertIn("--strict-mcp-config", model["argv"])
        self.assertEqual(model["argv"][model["argv"].index("--tools") + 1], "")
        self.assertEqual(model["argv"][model["argv"].index("--setting-sources") + 1], "")
        self.assertIn("detail " * 1000, model["stdin"])
        self.assertIn("Approve deploy", model["stdin"])
        self.assertIn("in_progress", model["stdin"])
        self.assertTrue(model["orca_env"] == {})

    def test_quick_work_and_invalid_model_output_queue_the_original_owner_turn_once(self):
        for output in ('{"route":"home"}', 'not JSON', '{"route":"answer","text":"unsafe", "command":"rm"}'):
            self.write_orca_state()
            self.busy_home()
            before = len(self.orca_calls())
            result = self.quick("Build the feature", FAKE_ANSWER=output)
            self.assertEqual(result.returncode, 0, result.stderr)
            sent = [call for call in self.orca_calls()[before:] if call["argv"][:2] == ["bot", "send"]]
            self.assertEqual(len(sent), 1)
            self.assertNotIn("new", sent[0]["argv"])
            self.assertEqual(sent[0]["argv"][sent[0]["argv"].index("--delivery") + 1], "now")
            entries = json.loads(self.orca_state.read_text())["entries"][self.HOME_UUID]
            self.assertEqual(entries[-2]["text"], "[tohru-desktop:voice]\nBuild the feature")
            events = [json.loads(line) for line in result.stdout.splitlines()]
            self.assertEqual([event["text"] for event in events if event["event"] == "voice_notice"],
                             ["One moment, I'm in the middle of something.", "I'll take care of that."])
            self.assertEqual(len([event for event in events if event["event"] == "delta"]), 1)

    def test_quick_responder_speaks_as_tohru_in_the_first_person(self):
        self.busy_home()
        result = self.quick("Fix the bar", FAKE_ANSWER='{"route":"home","text":"I\'m on it."}')
        self.assertEqual(result.returncode, 0, result.stderr)
        events = [json.loads(line) for line in result.stdout.splitlines()]
        notices = [event["text"] for event in events if event["event"] == "voice_notice"]
        self.assertEqual(notices, ["One moment, I'm in the middle of something.", "I'm on it."])
        state = json.loads(self.orca_state.read_text())
        self.assertIn("I'm on it.", [entry.get("text") for entry in state["entries"][self.HOME_UUID]])
        model = json.loads(self.fake_claude_log.read_text().splitlines()[0])
        system = " ".join(model["argv"])
        self.assertIn("Speak only in the first person", system)
        self.assertIn("never mention Home", system)
        self.assertNotIn('"home":', model["stdin"])
        for line in notices:
            self.assertNotRegex(line, r"\bHome\b")

    def test_quick_superseded_request_never_speaks_or_starts_work(self):
        self.busy_home()
        result = self.quick("Are you there?", FAKE_ANSWER='{"route":"answer","text":"Yes."}', FAKE_SIDE_REJECT="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        events = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(events[-1]["event"], "stopped")
        self.assertFalse(any(event["event"] == "delta" for event in events))
        self.assertFalse(any(call["argv"][:2] == ["bot", "send"] for call in self.orca_calls()))

    def test_quick_never_executes_a_desktop_action(self):
        self.busy_home()
        command_log = self.bin / "panel-command.json"
        fake = self.bin / "tohru"
        fake.write_text("#!/usr/bin/env python3\nimport json,sys\nfrom pathlib import Path\nPath(" + repr(str(command_log)) + ").write_text(json.dumps(sys.argv[1:]))\n")
        fake.chmod(0o755)
        result = self.quick("Show the calendar panel", FAKE_ANSWER='{"route":"home"}')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(command_log.exists())
        self.assertTrue(self.fake_claude_log.read_text())
        self.assertTrue(any(call["argv"][:2] == ["bot", "send"] for call in self.orca_calls()))

    def test_quick_interruption_records_the_question_without_speaking_or_queuing_it(self):
        self.busy_home()
        process = subprocess.Popen([sys.executable, str(HELPER), "chat", "--backend", "orca", "--orca-bot", "Tohru",
                                    "--provider", "orca:Tohru", "--orca-voice-chat", "quick", "--voice"],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                   env={**os.environ, **self.env, "FAKE_ANSWER":json.dumps({"route":"answer", "text":"a " * 100}), "FAKE_DELAY":"0.1"})
        process.stdin.write("Are you available?")
        process.stdin.close()
        deadline = time.monotonic() + 5
        while not self.fake_claude_log.read_text() and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(self.fake_claude_log.read_text())
        process.send_signal(signal.SIGINT)
        output = process.stdout.read()
        errors = process.stderr.read()
        self.assertEqual(process.wait(timeout=5), 0, errors)
        events = [json.loads(line) for line in output.splitlines()]
        self.assertEqual(events[-1]["event"], "stopped")
        self.assertFalse(any(event["event"] == "delta" for event in events))
        state = json.loads(self.orca_state.read_text())
        self.assertEqual(state["entries"][self.HOME_UUID][-1]["text"], "You stopped me before I answered.")
        self.assertFalse(any(call["argv"][:2] == ["bot", "send"] for call in self.orca_calls()))
        process.stdout.close()
        process.stderr.close()

    def test_queued_request_uses_native_alias_and_internal_mail_does_not_take_owner_authority(self):
        with patch.object(sys, "path", [str(ROOT / "bin"), *sys.path]):
            service = runpy.run_path(str(HELPER), run_name="tohru_test")
        wait = service["orca_wait_for_reply"]
        owner = {"role": "user", "id": "orca:1133797", "request_id": "orca:0887907",
                 "input_kind": "owner", "text": "Are you there"}
        mail = {"role": "user", "id": "mail", "input_kind": "internal", "text": "unrelated status"}
        answer = {"role": "assistant", "input_kind": "answer", "request_id": "orca:0887907", "completion_id": "answer-final", "text": "Yes, I am here."}
        internal_answer = {"role": "assistant", "input_kind": "internal", "text": "PRIVATE CONTROL RESPONSE"}
        pages = [([], "working", ""), ([owner, mail], "working", ""),
                 ([owner, mail, answer, internal_answer], "working", "")]
        origins, spoken = [], []
        with patch.dict(wait.__globals__, orca_read_messages=lambda *_a, **_k: pages.pop(0)), patch("time.sleep"):
            result = wait("chat", 0, "Are you there", spoken.append, message_id="orca:0887907",
                          on_origin=lambda stable, canonical: origins.append((stable, canonical)))
        self.assertEqual(result, ("Yes, I am here.", False, ""))
        self.assertEqual(origins, [("orca:0887907", "orca:1133797")])
        self.assertEqual(spoken, ["Yes, I am here."])

    def test_foreground_reuses_its_actor_while_home_has_unfinished_work(self):
        state = json.loads(self.orca_state.read_text())
        state["pending"][self.HOME_UUID] = {"polls": 1000, "reply": "Home long tool finished."}
        self.orca_state.write_text(json.dumps(state))
        def foreground(prompt, **env):
            return self.invoke("chat", "--backend", "orca", "--orca-bot", "Tohru", "--provider", "orca:Tohru",
                               "--orca-voice-chat", "foreground", "--voice", stdin=prompt, **env)
        first = foreground("Are you available?", FAKE_ORCA_REPLY="I am here while that work continues.")
        self.assertEqual(first.returncode, 0, first.stderr)
        events = [json.loads(line) for line in first.stdout.splitlines()]
        actor = events[0]["conversation"]
        self.assertNotEqual(actor, self.HOME_UUID)
        state = json.loads(self.orca_state.read_text())
        self.assertGreater(state["pending"][self.HOME_UUID]["polls"], 900)
        body = state["entries"][actor][0]["text"]
        self.assertIn("Home Chat and its existing tasks/history/approvals remain intact", body)
        self.assertIn("Hello from Orca UI", body)
        self.assertIn('"state": "working"', body)
        self.assertIn("asynchronous Orca workers", body)
        # Worker mail can become lastAsked. Registry ownership survives that and
        # a fresh helper process, so another wake does not silently create a chat.
        next(c for c in state["chats"] if c["id"] == actor)["lastAsked"] = "worker result notification"
        self.orca_state.write_text(json.dumps(state))
        second = foreground("And now?", FAKE_ORCA_REPLY="Still available.")
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(json.loads(second.stdout.splitlines()[0])["conversation"], actor)
        state = json.loads(self.orca_state.read_text())
        self.assertEqual(len(state["chats"]), 2)
        self.assertIn(self.HOME_UUID, state["pending"])
        shown = json.loads(self.invoke("show", "--backend", "orca", "--orca-bot", "Tohru", actor).stdout)
        self.assertEqual(shown["turns"][0]["text"], "Are you available?")
        # The popup can type into the same actor explicitly. A new typed Home
        # request still goes to Home; selecting foreground never moves it.
        typed = self.invoke("chat", "--backend", "orca", "--orca-bot", "Tohru", "--provider", "orca:Tohru",
                            "--conversation", actor, stdin="Typed follow-up", FAKE_ORCA_REPLY="Same conversation.")
        self.assertEqual(typed.returncode, 0, typed.stderr)
        self.assertEqual(json.loads(typed.stdout.splitlines()[0])["conversation"], actor)

    def test_home_is_the_voice_default_and_explicit_foreground_is_preserved(self):
        args = ("chat", "--backend", "orca", "--orca-bot", "Tohru", "--provider", "orca:Tohru", "--voice")
        default = self.invoke(*args, stdin="Hello foreground")
        self.assertEqual(default.returncode, 0, default.stderr)
        self.assertEqual(json.loads(default.stdout.splitlines()[0])["conversation"], self.HOME_UUID)
        explicit = self.invoke(*args, "--orca-voice-chat", "home", stdin="Hello Home")
        self.assertEqual(explicit.returncode, 0, explicit.stderr)
        self.assertEqual(json.loads(explicit.stdout.splitlines()[0])["conversation"], self.HOME_UUID)

    def test_foreground_fails_closed_for_missing_home_or_retired_actor(self):
        args = ("chat", "--backend", "orca", "--orca-bot", "Tohru", "--provider", "orca:Tohru",
                "--orca-voice-chat", "foreground", "--voice")
        state = json.loads(self.orca_state.read_text())
        state["chats"] = []
        self.orca_state.write_text(json.dumps(state))
        missing = self.invoke(*args, stdin="Hello")
        self.assertNotEqual(missing.returncode, 0)
        self.assertFalse(any(c["argv"][:2] == ["bot", "send"] for c in self.orca_calls()))
        self.write_orca_state()
        first = self.invoke(*args, stdin="Hello")
        self.assertEqual(first.returncode, 0, first.stderr)
        state = json.loads(self.orca_state.read_text())
        next(c for c in state["chats"] if not c.get("home"))["closed"] = True
        self.orca_state.write_text(json.dumps(state))
        sends = sum(c["argv"][:2] == ["bot", "send"] for c in self.orca_calls())
        retired = self.invoke(*args, stdin="Hello again")
        self.assertNotEqual(retired.returncode, 0)
        self.assertEqual(sum(c["argv"][:2] == ["bot", "send"] for c in self.orca_calls()), sends)

    def test_replayed_commentary_and_full_history_window_emit_only_completed_final(self):
        with patch.object(sys, "path", [str(ROOT / "bin"), *sys.path]):
            service = runpy.run_path(str(HELPER), run_name="tohru_test")
        wait = service["orca_wait_for_reply"]
        prompt = {"role": "user", "text": "same prompt", "id": "our-turn"}
        commentary = {"role": "assistant", "text": "I will check that."}
        final = {"role": "assistant", "text": "The answer is eight."}
        # The bounded page remains the same length as before sending; repeated
        # commentary and an unknown state must never become speech.
        page = [prompt, commentary, commentary]
        replies = [(page, "working", ""), (page, "", ""),
                   ([prompt, commentary, final], "working", ""),
                   ([prompt, commentary, final], "done", "")]
        spoken = []
        with patch.dict(wait.__globals__, orca_read_messages=lambda *_a, **_k: replies.pop(0)), patch("time.sleep"):
            answer, stopped, error = wait("chat", 3, "same prompt", spoken.append, message_id="our-turn")
        self.assertEqual((answer, stopped, error), ("The answer is eight.", False, ""))
        self.assertEqual(spoken, ["The answer is eight."])

    HOME_UUID = "4f71ebb6-db33-43e9-af8f-e5bc79a11077"

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="tohru-orca-backend-")
        self.addCleanup(self.directory.cleanup)
        root = Path(self.directory.name)
        self.bin = root / "bin"
        self.bin.mkdir()
        self.home = root / "home"
        self.bots = self.home / ".orca/bots"
        self.tohru_bot = self.bots / "tohru"
        self.tohru_bot.mkdir(parents=True)
        (self.tohru_bot / "bot.json").write_text(json.dumps({"id": "bot-1", "name": "Tohru", "agent": "claude"}))
        (self.tohru_bot / "SOUL.md").write_text("# Tohru\n\nDefault stub.\n")
        (self.tohru_bot / "MEMORY.md").write_text(
            "# Memory\n\nOne line per fact, newest first. Longer notes go in memory/<topic>.md.\n"
            "- 2026-11-20: Prefers metric units\n"
            "- 2026-11-21: Works on Tohru shell\n"
        )
        (self.tohru_bot / "memory").mkdir()
        (self.tohru_bot / "memory/projects.md").write_text("# Projects\nTohru and Orca.\n")
        self.data = root / "data"
        self.scratch = root / "scratch"
        self.fake_claude_log = root / "claude.jsonl"
        self.fake_claude_log.write_text("")
        self.orca_log = root / "orca.jsonl"
        self.orca_log.write_text("")
        self.orca_state = root / "orca-state.json"
        self.write_orca_state()
        for name, src in (("claude", FAKE_CLAUDE), ("orca", FAKE_ORCA)):
            p = self.bin / name
            p.write_text(src)
            p.chmod(0o755)
        self.env = {
            "PATH": f"{self.bin}:/usr/bin:/bin:" + os.path.dirname(os.path.realpath(subprocess.check_output(["sh", "-c", "command -v python3"], text=True).strip())),
            "HOME": str(self.home),
            "XDG_DATA_HOME": str(root / "xdg-data"),
            "XDG_CONFIG_HOME": str(root / "xdg-config"),
            "TOHRU_ASSISTANT_DATA": str(self.data),
            "TOHRU_ASSISTANT_SCRATCH": str(self.scratch),
            "TOHRU_ORCA_BIN": str(self.bin / "orca"),
            "TOHRU_ORCA_BOTS_DIR": str(self.bots),
            "TOHRU_ORCA_POLL_MS": "20",
            "FAKE_LOG": str(self.fake_claude_log),
            "FAKE_SIGNED_IN": "1",
            "FAKE_ORCA_STATE": str(self.orca_state),
            "FAKE_ORCA_LOG": str(self.orca_log),
            "ORCA_AGENT_SESSION_ID": "caller-should-be-stripped",
            "ORCA_ENVIRONMENT": "remote-should-be-stripped",
        }

    def write_orca_state(self, bots=None):
        state = {
            "bots": bots if bots is not None else [{"id": "bot-1", "name": "Tohru", "agent": "claude", "paused": False}],
            "chats": [{
                "id": self.HOME_UUID,
                "title": "Home",
                "lastActivityAt": 1700000000000,
                "lastAsked": "Hello from Orca UI",
                "lastSaid": "Hi! Ready in Orca.",
                "home": True,
                "closed": False,
            }],
            "entries": {
                self.HOME_UUID: [
                    {"kind": "message", "role": "user", "text": "Hello from Orca UI"},
                    {"kind": "message", "role": "assistant", "text": "Hi! Ready in Orca."},
                ],
            },
            "pending": {},
        }
        self.orca_state.write_text(json.dumps(state))

    def invoke(self, *args, stdin="", **env):
        return subprocess.run([str(HELPER), *args], input=stdin, env={**self.env, **env}, text=True,
                              capture_output=True, timeout=30)

    def orca_calls(self):
        text = self.orca_log.read_text().strip()
        return [json.loads(line) for line in text.splitlines()] if text else []

    def test_default_local_backend_never_touches_orca(self):
        providers = json.loads(self.invoke("providers").stdout)
        self.assertEqual([p["id"] for p in providers], ["claude"])
        self.assertEqual(self.orca_calls(), [])

    def test_voice_ready_registration_strips_agent_scope_and_rejects_invalid_identity(self):
        registered = self.invoke("voice-ready", "--conversation", self.HOME_UUID, "--request", "orca:prompt")
        self.assertEqual(registered.returncode, 0, registered.stderr)
        self.assertEqual(json.loads(registered.stdout), {"event": "voice_ready", "registered": True, "message": ""})
        self.assertEqual(self.orca_calls()[-1]["argv"], ["bot", "voice-ready", "--chat", self.HOME_UUID,
                                                       "--request", "orca:prompt", "--json"])
        self.assertTrue(self.orca_calls()[-1]["stripped_env"])
        calls = len(self.orca_calls())
        invalid = self.invoke("voice-ready", "--conversation", self.HOME_UUID, "--request", "bad\nidentity")
        self.assertEqual(invalid.returncode, 2)
        self.assertEqual(len(self.orca_calls()), calls)

    def test_orca_providers_online_paused_missing_and_offline(self):
        online = json.loads(self.invoke("providers", "--backend", "orca", "--orca-bot", "Tohru").stdout)
        self.assertEqual(len(online), 1)
        self.assertEqual((online[0]["id"], online[0]["signed_in"], online[0]["can_signin"], online[0]["model"]),
                         ("orca:Tohru", True, False, "Orca · claude"))
        self.assertTrue(self.orca_calls()[0]["stripped_env"])

        self.write_orca_state(bots=[{"id": "bot-1", "name": "Tohru", "agent": "claude", "paused": True}])
        paused = json.loads(self.invoke("providers", "--backend", "orca", "--orca-bot", "Tohru").stdout)
        self.assertFalse(paused[0]["signed_in"])
        self.assertIn("paused", paused[0]["offline_title"].lower())

        self.write_orca_state(bots=[])
        missing = json.loads(self.invoke("providers", "--backend", "orca", "--orca-bot", "Tohru").stdout)
        self.assertFalse(missing[0]["signed_in"])
        self.assertIn("orca init", missing[0]["offline_body"])

        offline_err = json.loads(self.invoke("providers", "--backend", "orca", "--orca-offline", "error", FAKE_ORCA_OFFLINE="1").stdout)
        self.assertEqual((offline_err[0]["signed_in"], offline_err[0]["can_signin"]), (False, False))
        self.assertIn("isn’t running", offline_err[0]["offline_title"])

        offline_local = json.loads(self.invoke("providers", "--backend", "orca", "--orca-offline", "local", FAKE_ORCA_OFFLINE="1").stdout)
        self.assertEqual(offline_local[0]["id"], "claude")
        self.assertTrue(offline_local[0]["offline_fallback"])

    def test_orca_chat_routes_to_home_voice_new_chat_and_unwraps_show(self):
        res = self.invoke("chat", "--backend", "orca", "--orca-bot", "Tohru", "--provider", "orca:Tohru",
                          stdin="How many ounces in a cup?", FAKE_ORCA_PARTIAL="Eight fluid", FAKE_ORCA_REPLY="Eight fluid ounces.")
        self.assertEqual(res.returncode, 0, res.stderr)
        events = [json.loads(l) for l in res.stdout.splitlines()]
        self.assertEqual(events[0]["event"], "start")
        self.assertEqual(events[0]["conversation"], self.HOME_UUID)
        self.assertEqual(events[0]["voice_request"], "")
        self.assertEqual("".join(e["text"] for e in events if e["event"] == "delta"), "Eight fluid ounces.")
        self.assertEqual(events[-1], {"event": "done", "conversation": self.HOME_UUID})

        send_call = next(c for c in self.orca_calls() if c["argv"][:2] == ["bot", "send"])
        self.assertIn("Tohru", send_call["argv"])
        self.assertIn("--request-id", send_call["argv"])
        self.assertNotIn("--from", send_call["argv"])
        self.assertIn(self.HOME_UUID, send_call["argv"])

        # Voice turn in a new chat when --orca-voice-chat new.
        vres = self.invoke("chat", "--backend", "orca", "--orca-bot", "Tohru", "--orca-voice-chat", "new",
                           "--provider", "orca:Tohru", "--voice", stdin="What is the weather?", FAKE_ORCA_REPLY="Clear skies.")
        self.assertEqual(vres.returncode, 0, vres.stderr)
        vevents = [json.loads(l) for l in vres.stdout.splitlines()]
        new_cid = vevents[0]["conversation"]
        self.assertEqual(vevents[0]["voice_request"], "")
        self.assertTrue(next(e for e in vevents if e["event"] == "voice_origin")["request"])
        self.assertNotEqual(new_cid, self.HOME_UUID)
        self.assertEqual("".join(e["text"] for e in vevents if e["event"] == "delta"), "Clear skies.")

        # History lists Home Chat first and searches across title/lastAsked/lastSaid.
        hist = json.loads(self.invoke("history", "--backend", "orca", "--orca-bot", "Tohru").stdout)
        self.assertEqual(hist[0]["id"], self.HOME_UUID)
        self.assertEqual(len(hist), 2)
        found = json.loads(self.invoke("history", "--backend", "orca", "--orca-bot", "Tohru", "--query", "Clear skies").stdout)
        self.assertEqual([c["id"] for c in found], [new_cid])

        # Show unwraps desktop envelope on desktop turns while keeping Bots-panel turns verbatim.
        shown_home = json.loads(self.invoke("show", "--backend", "orca", "--orca-bot", "Tohru", self.HOME_UUID).stdout)
        self.assertEqual([t["text"] for t in shown_home["turns"]], [
            "Hello from Orca UI", "Hi! Ready in Orca.", "How many ounces in a cup?", "Eight fluid ounces.",
        ])
        self.assertNotIn("voice", shown_home["turns"][0])
        shown_voice = json.loads(self.invoke("show", "--backend", "orca", "--orca-bot", "Tohru", new_cid).stdout)
        self.assertTrue(shown_voice["turns"][0]["voice"])
        self.assertEqual(shown_voice["turns"][0]["text"], "What is the weather?")

    def test_start_marks_the_current_home_after_a_new_home_chat(self):
        new_home = "9a3c2f10-7b4e-4c1d-8e2f-5a6b7c8d9e0f"
        state = json.loads(self.orca_state.read_text())
        state["chats"][0].update(home=False, closed=True, reason="ended")
        state["chats"].insert(0, {"id": new_home, "title": "Home", "lastActivityAt": 1700000005000,
                                  "lastAsked": None, "lastSaid": None, "home": True, "closed": False})
        state["entries"][new_home] = []
        self.orca_state.write_text(json.dumps(state))
        chat = ("chat", "--backend", "orca", "--orca-bot", "Tohru", "--provider", "orca:Tohru", "--voice")

        pinned = self.invoke(*chat, "--orca-voice-chat", "home", "--conversation", self.HOME_UUID, stdin="Still there?")
        self.assertNotEqual(pinned.returncode, 0)
        self.assertIn("Chat is closed", json.loads(pinned.stdout.splitlines()[-1])["message"])

        for mode in ("home", "quick"):
            result = self.invoke(*chat, "--orca-voice-chat", mode, stdin="Still there?")
            self.assertEqual(result.returncode, 0, result.stderr)
            start = json.loads(result.stdout.splitlines()[0])
            self.assertEqual((start["conversation"], start["home"]), (new_home, True), mode)

        fresh = self.invoke(*chat, "--orca-voice-chat", "new", stdin="Side thread")
        start = json.loads(fresh.stdout.splitlines()[0])
        self.assertNotEqual(start["conversation"], new_home)
        self.assertFalse(start["home"])

    def test_orca_stop_and_offline_modes(self):
        proc = subprocess.Popen(
            [str(HELPER), "chat", "--backend", "orca", "--orca-bot", "Tohru", "--provider", "orca:Tohru"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True,
            env={**self.env, "FAKE_ORCA_POLLS": "200", "TOHRU_ORCA_POLL_MS": "50"},
        )
        proc.stdin.write("Long overnight check")
        proc.stdin.close()
        first = json.loads(proc.stdout.readline())
        self.assertEqual(first["event"], "start")
        proc.send_signal(signal.SIGINT)
        rest = [json.loads(l) for l in proc.stdout.read().splitlines() if l.strip()]
        proc.stdout.close()
        self.assertEqual(proc.wait(timeout=10), 0)
        self.assertEqual(rest[-1]["event"], "stopped")

        err_res = self.invoke("chat", "--backend", "orca", "--orca-offline", "error", "--provider", "orca:Tohru",
                              stdin="Hi", FAKE_ORCA_OFFLINE="1")
        self.assertEqual(err_res.returncode, 3)
        self.assertFalse((self.data / "assistant.sqlite").exists())

        fb_res = self.invoke("chat", "--backend", "orca", "--orca-offline", "local", "--provider", "orca:Tohru",
                             stdin="Hi offline", FAKE_ORCA_OFFLINE="1")
        self.assertEqual(fb_res.returncode, 0, fb_res.stderr)
        fb_events = [json.loads(l) for l in fb_res.stdout.splitlines()]
        self.assertEqual(fb_events[-1]["event"], "done")

    def test_orca_memory_sync_remember_and_init_migration(self):
        mem = json.loads(self.invoke("memory", "list", "--json", "--backend", "orca", "--orca-bot", "Tohru").stdout)
        self.assertEqual(len(mem["memories"]), 2)
        self.assertEqual(mem["relationship"]["source"], "orca")
        self.assertEqual(mem["summaries"][0]["conversation_id"], "projects")
        self.assertEqual(self.invoke("memory", "forget", mem["memories"][0]["id"][:8], "--backend", "orca", "--orca-bot", "Tohru").returncode, 0)
        sends = [c for c in self.orca_calls() if c["argv"][:2] == ["bot", "send"]]
        self.assertEqual(len(sends), 1)
        self.assertIn("Tohru", sends[0]["argv"])

        # remember is a no-op in orca mode.
        self.assertEqual(self.invoke("remember", self.HOME_UUID, "--backend", "orca", "--orca-bot", "Tohru").returncode, 0)
        self.assertEqual(self.fake_claude_log.read_text(), "")

        # sync reports online + home_chat + chat watermarks.
        synced = json.loads(self.invoke("sync", "--backend", "orca", "--orca-bot", "Tohru").stdout)
        self.assertTrue(synced["online"])
        self.assertEqual(synced["home_chat"], self.HOME_UUID)
        self.assertEqual(len(synced["chats"]), 1)

        # Seed local SQLite, then run `orca init` to seed SOUL.md and migrate memories without touching SQLite.
        self.invoke("chat", "--provider", "claude", stdin="I love NixOS")
        db = sqlite3.connect(self.data / "assistant.sqlite")
        with db:
            db.execute("INSERT INTO memories (id, kind, text, source_conversation, created, updated, confidence) VALUES (?, 'fact', ?, '', 1700000000, 1700000000, 0.9)",
                       ("a" * 32, "The user runs NixOS with Hyprland"))
            db.execute("INSERT INTO summaries (conversation_id, text, created, updated) VALUES (?, ?, 1700000000, 1700000000)",
                       ("b" * 32, "Discussed NixOS setup."))
            db.execute("INSERT INTO relationship (key, value, updated) VALUES ('conversations', '5', 1700000000)")
        db.close()

        init = self.invoke("orca", "init", "--bot", "Tohru")
        self.assertEqual(init.returncode, 0, init.stderr)
        payload = json.loads(init.stdout)
        self.assertTrue(payload["ok"])
        self.assertTrue(payload["soul_written"])
        self.assertEqual(payload["migrated_memories"], 1)
        self.assertTrue(payload["migrated_archive"])
        soul = (self.tohru_bot / "SOUL.md").read_text()
        self.assertIn("Desktop & Voice Interface", soul)
        self.assertIn("[tohru-desktop:voice]", soul)
        self.assertIn("The user runs NixOS with Hyprland", (self.tohru_bot / "MEMORY.md").read_text())
        self.assertTrue((self.tohru_bot / "memory/imported-tohru-sqlite.md").is_file())
        # Re-running does not duplicate facts.
        second = json.loads(self.invoke("orca", "init", "--bot", "Tohru").stdout)
        self.assertEqual(second["migrated_memories"], 0)
        self.assertFalse(second["soul_written"])


if __name__ == "__main__":
    unittest.main()
