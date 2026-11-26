#!/usr/bin/env python3
"""Assistant Discord routing through the native commands; fake provider CLIs only."""
import importlib.util
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True


spec = importlib.util.spec_from_file_location("assistant_service", Path(__file__).with_name("service.py"))
service = importlib.util.module_from_spec(spec)
spec.loader.exec_module(service)


class ProviderRouting(unittest.TestCase):
    setUp = service.Assistant.setUp
    fake = service.Assistant.fake
    invoke = service.Assistant.invoke

    def test_missing_blank_and_invalid_author_leave_chat_working_without_broker(self):
        directory = Path(self.env["XDG_CONFIG_HOME"]) / "tohru"
        directory.mkdir(parents=True)
        base = '[assistant.discord]\nenabled=true\nread=true\npost=true\nguild_id="1"\nchannels="2"\n'
        guard = "import os\nassert 'TOHRU_DISCORD_SOCKET' not in os.environ\nassert 'TOHRU_DISCORD_BOT_TOKEN' not in os.environ\n"
        self.fake("claude", service.FAKE_CLAUDE.replace("#!/usr/bin/env python3\n", "#!/usr/bin/env python3\n" + guard, 1))
        for setup in ("", 'allowed_user_id=""\n', 'allowed_user_id="@spoof"\n', 'allowed_user_id=9\n'):
            with self.subTest(setup=setup):
                (directory / "config.toml").write_text(base + setup)
                self.log.write_text("")
                result = self.invoke("chat", "--provider", "claude", "--orca", "false", "--memory", "off", stdin="Hello", TOHRU_DISCORD_BOT_TOKEN="unused-secret")
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("allowed_user_id", self.log.read_text())
                self.assertNotIn("unused-secret", self.log.read_text() + result.stdout + result.stderr)

    def test_all_five_provider_shell_tools_reach_broker_without_token(self):
        config_dir = Path(self.env["XDG_CONFIG_HOME"]) / "tohru"
        config_dir.mkdir(parents=True)
        secret = "routing-secret-never-in-provider"
        (config_dir / "config.toml").write_text('[assistant.discord]\nenabled=true\nread=true\npost=false\nallowed_user_id="9"\nguild_id="1"\nchannels="2"\n')
        marker = Path(self.directory.name) / "tool-results"
        tool = str(ROOT / "bin/tohru-discord")
        probe = f'''import os, subprocess, json
assert "TOHRU_DISCORD_BOT_TOKEN" not in os.environ
result = subprocess.run([{tool!r}, "read", "99"], capture_output=True, text=True, timeout=5)
with open({str(marker)!r}, "a") as f:
    f.write(json.dumps({{"cli": os.path.basename(__file__), "result": result.stdout, "socket": os.environ.get("TOHRU_DISCORD_SOCKET")}}) + "\\n")
'''
        for name in ("claude", "codex", "gemini", "agy", "opencode"):
            source = getattr(service, "FAKE_" + name.upper())
            self.fake(name, source.replace("#!/usr/bin/env python3\n", "#!/usr/bin/env python3\n" + probe, 1))
            result = self.invoke("chat", "--provider", name, "--orca", "false", "--memory", "off", stdin="Read Discord channel 2", TOHRU_DISCORD_BOT_TOKEN=secret)
            self.assertEqual(result.returncode, 0, (name, result.stderr))
        records = [json.loads(line) for line in marker.read_text().splitlines()]
        self.assertEqual(len(records), 5)
        for record in records:
            self.assertIn("allowlist", record["result"])
            self.assertFalse(Path(record["socket"]).exists())
        self.assertNotIn(secret, self.log.read_text() + marker.read_text())
        for record in map(json.loads, self.log.read_text().splitlines()):
            text = json.dumps(record)
            self.assertIn("Discord tools", text)
            self.assertNotIn(" post CHANNEL", text)

    def test_disabled_incomplete_and_draft_turns_and_exports(self):
        directory = Path(self.env["XDG_CONFIG_HOME"]) / "tohru"
        directory.mkdir(parents=True)
        config_path = directory / "config.toml"
        config_path.write_text('[assistant.discord]\nenabled=true\nread=true\npost=true\nguild_id="1"\nchannels="2"\ntoken="inline-accident-do-not-export"\n[assistant.discord.credentials]\ntoken="nested-accident-do-not-export"\n')
        # Incomplete enabled setup still allows an ordinary chat.
        result = self.invoke("chat", "--provider", "claude", "--orca", "false", stdin="Hello")
        self.assertEqual(result.returncode, 0)
        self.assertIn("Discord unavailable", self.log.read_text())
        # Effective native/runtime settings can disable a managed-on integration.
        self.log.write_text("")
        result = self.invoke("chat", "--provider", "claude", "--orca", "false", "--discord-settings", '{"enabled":false}', stdin="Hello")
        self.assertEqual(result.returncode, 0)
        self.assertNotIn("Discord", self.log.read_text())
        result = self.invoke("export")
        self.assertNotIn("inline-accident-do-not-export", result.stdout)
        self.assertNotIn("nested-accident-do-not-export", result.stdout)
        config_path.write_text('[assistant.discord]\nenabled=true\nread=true\npost=true\nallowed_user_id="9"\nguild_id="1"\nchannels="2"\n')
        guard = "import os\nassert 'TOHRU_DISCORD_SOCKET' not in os.environ\nassert 'TOHRU_DISCORD_BOT_TOKEN' not in os.environ\n"
        self.fake("claude", service.FAKE_CLAUDE.replace("#!/usr/bin/env python3\n", "#!/usr/bin/env python3\n" + guard, 1))
        self.log.write_text("")
        result = self.invoke("chat", "--provider", "claude", "--orca", "false", "--draft", stdin="Hello", TOHRU_DISCORD_BOT_TOKEN="not-in-draft")
        self.assertEqual(result.returncode, 0)
        self.assertNotIn("Discord", self.log.read_text())


    def test_memory_extraction_has_no_discord_credentials_socket_or_guidance(self):
        directory = Path(self.env["XDG_CONFIG_HOME"]) / "tohru"
        directory.mkdir(parents=True)
        (directory / "config.toml").write_text('[assistant.discord]\nenabled=true\nread=true\npost=true\nallowed_user_id="9"\nguild_id="1"\nchannels="2"\n')
        secret = "memory-secret-must-stay-parent-local"
        result = self.invoke("chat", "--provider", "claude", "--orca", "false", stdin="Hello", TOHRU_DISCORD_BOT_TOKEN=secret)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Discord tools", self.log.read_text())
        cid = json.loads(result.stdout.splitlines()[0])["conversation"]
        guard = "import os\nassert 'TOHRU_DISCORD_BOT_TOKEN' not in os.environ\nassert 'TOHRU_DISCORD_SOCKET' not in os.environ\n"
        self.fake("claude", service.FAKE_CLAUDE.replace("#!/usr/bin/env python3\n", "#!/usr/bin/env python3\n" + guard, 1))
        self.log.write_text("")
        result = self.invoke("remember", cid, FAKE_ANSWER=service.Assistant.EXTRACTED,
                             TOHRU_DISCORD_BOT_TOKEN=secret, TOHRU_DISCORD_SOCKET="inherited-must-be-stripped")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("Discord tools", self.log.read_text())
        self.assertNotIn(secret, self.log.read_text())


class OrcaBoundary(unittest.TestCase):
    HOME_UUID = service.OrcaBotBackend.HOME_UUID
    setUp = service.OrcaBotBackend.setUp
    write_orca_state = service.OrcaBotBackend.write_orca_state
    invoke = service.OrcaBotBackend.invoke
    orca_calls = service.OrcaBotBackend.orca_calls

    def configure_discord(self):
        directory = Path(self.env["XDG_CONFIG_HOME"]) / "tohru"
        directory.mkdir(parents=True)
        (directory / "config.toml").write_text('[assistant]\nbackend="orca"\n[assistant.discord]\nenabled=true\nread=true\npost=true\nallowed_user_id="9"\nguild_id="1"\nchannels="2"\n')
        self.env.update(TOHRU_DISCORD_BOT_TOKEN="fake-boundary-secret", TOHRU_DISCORD_SOCKET="never-forward-this-socket")
        executable = self.bin / "orca"
        guard = "import os\nassert 'TOHRU_DISCORD_BOT_TOKEN' not in os.environ\nassert 'TOHRU_DISCORD_SOCKET' not in os.environ\n"
        executable.write_text(service.FAKE_ORCA.replace("#!/usr/bin/env python3\n", "#!/usr/bin/env python3\n" + guard, 1))

    def test_remote_turn_explains_local_only_boundary_without_credentials(self):
        self.configure_discord()
        draft = self.invoke("chat", "--provider", "orca:Tohru", "--draft", stdin="Hello")
        self.assertEqual(json.loads(draft.stdout)["event"], "draft_unsupported")
        self.assertEqual(self.orca_calls(), [])
        result = self.invoke("chat", "--provider", "orca:Tohru", stdin="Read Discord")
        self.assertEqual(result.returncode, 0, result.stderr)
        state = json.loads(self.orca_state.read_text())
        body = state["chats"][0]["lastAsked"]
        self.assertIn("unavailable in this Orca bot session", body)
        self.assertIn("assistant.backend = local", body)
        self.assertNotIn("Discord tools are available", body)
        self.assertNotIn("TOHRU_DISCORD_SOCKET", body)
        self.assertNotIn("fake-boundary-secret", json.dumps(state) + self.orca_log.read_text())

    def test_explicit_offline_fallback_keeps_uuid_and_uses_guarded_local_broker(self):
        self.configure_discord()
        marker = Path(self.directory.name) / "fallback-tool.json"
        tool = str(ROOT / "bin/tohru-discord")
        guard = ("import os, sys, subprocess, json\n"
                 "assert 'TOHRU_DISCORD_BOT_TOKEN' not in os.environ\n"
                 "if '-p' in sys.argv:\n"
                 f"    result = subprocess.run([{tool!r}, 'read', '99'], capture_output=True, text=True, timeout=5)\n"
                 f"    open({str(marker)!r}, 'w').write(json.dumps({{'result': result.stdout, 'socket': os.environ.get('TOHRU_DISCORD_SOCKET')}}))\n")
        (self.bin / "claude").write_text(service.FAKE_CLAUDE.replace("#!/usr/bin/env python3\n", "#!/usr/bin/env python3\n" + guard, 1))
        result = self.invoke("chat", "--provider", "orca:Tohru", "--orca-offline", "local", "--orca", "false",
                             "--conversation", self.HOME_UUID, stdin="Read Discord", FAKE_ORCA_OFFLINE="1")
        self.assertEqual(result.returncode, 0, result.stderr)
        events = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual(events[0]["conversation"], self.HOME_UUID)
        probe = json.loads(marker.read_text())
        self.assertIn("allowlist", probe["result"])
        self.assertFalse(Path(probe["socket"]).exists())
        self.assertNotIn("fake-boundary-secret", self.fake_claude_log.read_text())


if __name__ == "__main__":
    unittest.main()
