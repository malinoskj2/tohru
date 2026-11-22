#!/usr/bin/env python3
"""DM API and idle receiver contracts; loopback HTTP and fake providers only."""
import contextlib
import importlib.machinery
import importlib.util
import io
import json
import os
from pathlib import Path
import signal
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "bin"))
spec = importlib.util.spec_from_file_location("discord_tests", Path(__file__).with_name("discord.py"))
fixtures = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixtures)
d = fixtures.discord


class DM(unittest.TestCase):
    setUp = fixtures.Discord.setUp

    def setup_dm(self):
        self.values.pop("guild_id")
        self.values.pop("channels")
        self.values.update(destination="dm", receive=True)
        self.receiver = d.Receiver(self.client)

    def metadata(self, **changes):
        return {"id": "7", "type": 1, "recipients": [{"id": "9"}], **changes}

    def message(self, ident="101", content="Hello", **changes):
        return {"id": ident, "channel_id": "7", "type": 0, "author": {"id": "9"},
                "content": content, "timestamp": "2026-11-22T00:00:00Z", **changes}

    def page(self, *messages, single=False):
        self.replies.extend([(200, self.metadata(), {}), (200, self.metadata(), {}), (200, messages[0] if single else list(messages), {})])

    def bootstrap(self):
        self.setup_dm()
        self.page(self.message("100", "old history"))
        self.assertEqual(self.receiver.poll(), {"event": "idle"})

    def pending(self):
        self.bootstrap()
        self.page(self.message())
        self.assertEqual(self.receiver.poll(), {"event": "incoming", "id": "101"})

    def test_dm_without_guild_or_channel_fixed_recipient(self):
        self.setup_dm()
        self.page(self.message())
        result = self.client.execute({"action": "read", "channel": "dm"})
        self.assertEqual(result["channel"], "7")
        self.assertEqual(self.requests[0][0:2], ("POST", "/api/v10/users/@me/channels"))
        self.assertEqual(self.requests[0][3], {"recipient_id": "9"})
        with self.assertRaises(d.DiscordError):
            self.client.execute({"action": "read", "channel": "8", "recipient_id": "8"})

    def test_missing_user_and_bad_destination_fail_before_network(self):
        self.setup_dm()
        for changes in ({"allowed_user_id": ""}, {"allowed_user_id": "@name"}, {"destination": "group"}):
            with self.subTest(changes=changes):
                with patch.dict(self.values, changes):
                    with self.assertRaises(d.DiscordError):
                        self.client.execute({"action": "read", "channel": "dm"})
        self.assertEqual(self.requests, [])

    def test_reject_group_other_recipient_and_malformed_metadata(self):
        self.setup_dm()
        for changes in ({"type": 3}, {"type": True}, {"guild_id": "1"}, {"recipients": [{"id": "8"}]},
                        {"recipients": [{"id": "9"}, {"id": "8"}]}, {"recipients": [{"id": "9", "bot": True}]}, {"id": "invalid"}):
            self.replies.append((200, self.metadata(**changes), {}))
            with self.assertRaises(d.DiscordError):
                self.client.execute({"action": "read", "channel": "dm"})

    def test_spoofed_actual_channel_and_message_channel(self):
        self.setup_dm()
        self.replies.extend([(200, self.metadata(), {}), (200, self.metadata(id="8"), {})])
        with self.assertRaises(d.DiscordError):
            self.client.execute({"action": "read", "channel": "dm"})
        self.page(self.message(channel_id="8"))
        with self.assertRaises(d.DiscordError):
            self.client.execute({"action": "read", "channel": "dm"})

    def test_author_bot_webhook_system_and_nested_reference_filters(self):
        self.setup_dm()
        self.page(self.message("101", "foreign secret", author={"id": "8"}),
                  self.message("102", "bot secret", author={"id": "9", "bot": True}),
                  self.message("103", "webhook secret", webhook_id="99"),
                  self.message("104", "system secret", type=7),
                  self.message("105", "missing author", author=None),
                  self.message("106", "accepted", referenced_message={"content": "nested secret"}))
        result = self.client.execute({"action": "read", "channel": "dm"})
        self.assertEqual([m["content"] for m in result["messages"]], ["accepted"])
        self.assertNotIn("secret", json.dumps(result))

    def test_bootstrap_pending_restart_claim_and_deduplication(self):
        self.pending()
        previous = len(self.requests)
        restarted = d.Receiver(self.client)
        self.assertEqual(restarted.poll(), {"event": "incoming", "id": "101"})
        self.assertEqual(len(self.requests), previous)
        self.page(self.message(), single=True)
        message, conversation, _ = restarted.claim("101")
        self.assertEqual(message["content"], "Hello")
        self.assertEqual(len(conversation), 32)
        with self.assertRaises(d.DiscordError):
            restarted.claim("101")
        self.page(self.message())
        self.assertEqual(restarted.poll(), {"event": "idle"})
        self.assertEqual(self.receiver.path.stat().st_mode & 0o777, 0o600)

    def test_bounded_pending_defers_second_message_without_losing_it(self):
        self.bootstrap()
        self.page(self.message("102", "second"), self.message("101", "first"))
        self.assertEqual(self.receiver.poll()["id"], "101")
        self.page(self.message("101", "first"), single=True)
        self.receiver.claim("101")
        self.page(self.message("102", "second"))
        self.assertEqual(self.receiver.poll()["id"], "102")
        self.assertLess(self.receiver.path.stat().st_size, 16000)

    def test_shared_turn_ownership_and_malformed_state(self):
        self.pending()
        with self.receiver.turn():
            with self.assertRaisesRegex(d.DiscordError, "busy"):
                with d.Receiver(self.client).turn():
                    self.fail("second receiver acquired a live turn")
        self.assertEqual(self.receiver.poll()["id"], "101")
        state = json.loads(self.receiver.path.read_text())
        state["pending"] = {"id": "spoof"}
        self.receiver.path.write_text(json.dumps(state))
        with self.assertRaisesRegex(d.DiscordError, "state is invalid"):
            self.receiver.poll()

    def test_forward_cursor_drains_more_than_one_rest_page(self):
        self.bootstrap()
        accepted = []
        for ident in range(101, 152):
            self.page(*(self.message(str(i)) for i in reversed(range(ident, min(152, ident + 50)))))
            candidate = self.receiver.poll()
            self.assertEqual(candidate["id"], str(ident))
            self.page(self.message(str(ident)), single=True)
            accepted.append(self.receiver.claim(str(ident))[0]["id"])
        self.assertEqual(accepted, [str(i) for i in range(101, 152)])
        self.assertEqual(json.loads(self.receiver.path.read_text())["cursor"], "151")

    def test_specific_claim_reference_mismatch_is_rejected(self):
        self.pending()
        self.page(self.message("102"), single=True)
        with self.assertRaisesRegex(d.DiscordError, "different message reference"):
            self.receiver.claim("101")
        self.assertIn("pending", json.loads(self.receiver.path.read_text()))

    def test_changed_user_revoked_receive_read_and_reset(self):
        self.pending()
        for changes in ({"receive": False}, {"read": False}, {"enabled": False}, {"allowed_user_id": "8"}):
            with patch.dict(self.values, changes):
                with self.assertRaises(d.DiscordError):
                    self.receiver.claim("101")
        self.receiver.reset()
        self.page(self.message("120", "historical after reset"))
        self.assertEqual(self.receiver.poll(), {"event": "idle"})

    def test_cancellation_before_claim_preserves_pending(self):
        self.pending()
        self.client.cancel()
        with self.assertRaises(d.DiscordError):
            self.receiver.claim("101")
        self.assertIn("pending", json.loads(self.receiver.path.read_text()))

    def test_inaccessible_dm_and_rate_limit_shared(self):
        self.setup_dm()
        for code in (401, 403, 404):
            self.replies.append((code, {"message": self.secret}, {}))
            with self.assertRaises(d.DiscordError) as caught:
                self.receiver.poll()
            self.assertNotIn(self.secret, str(caught.exception))
        self.replies.append((429, {"retry_after": 60}, {}))
        with self.assertRaises(d.DiscordError):
            self.receiver.poll()
        count = len(self.requests)
        with self.assertRaises(d.DiscordError):
            self.receiver.poll()
        self.assertEqual(count, len(self.requests))

    def test_dm_desktop_ack_suppresses_resolution_and_fallback_posts(self):
        self.setup_dm()
        self.values["desktop_first"] = True
        self.client.notify = lambda *_: "acknowledged"
        self.assertFalse(self.client.execute({"action": "post", "channel": "dm", "content": "reply"})["sent"])
        self.assertEqual(self.requests, [])
        for outcome in ("unavailable", "unacknowledged"):
            self.client.notify = lambda *_, outcome=outcome: outcome
            self.replies.extend([(200, self.metadata(), {}), (200, self.metadata(), {}), (200, {"id": "150"}, {})])
            result = self.client.execute({"action": "post", "channel": "dm", "content": "reply"})
            self.assertEqual(result["desktop_attempt"], outcome)
            self.assertEqual(self.requests[-1][3]["allowed_mentions"]["parse"], [])

    def test_receive_to_fake_provider_reply_read_only_and_revocation(self):
        # Load the real assistant command in-process to inject only the HTTP transport.
        from types import SimpleNamespace
        loader = importlib.machinery.SourceFileLoader("dm_assistant", str(ROOT / "bin/tohru-assistant"))
        spec = importlib.util.spec_from_loader(loader.name, loader)
        assistant = importlib.util.module_from_spec(spec)
        with patch.dict(os.environ, {"XDG_STATE_HOME": str(self.root / "assistant-state"),
                                     "XDG_DATA_HOME": str(self.root / "assistant-data"),
                                     "TOHRU_ASSISTANT_DATA": str(self.root / "assistant-data/tohru"),
                                     "TOHRU_ASSISTANT_SCRATCH": str(self.root / "scratch"),
                                     "XDG_CONFIG_HOME": str(self.root / "config"),
                                     "XDG_CACHE_HOME": str(self.root / "cache")}):
            loader.exec_module(assistant)
            with patch.object(assistant, "DiscordClient", side_effect=AssertionError("remote reception opened local adapter")), patch.object(sys, "stdin", io.StringIO("")), contextlib.redirect_stdout(io.StringIO()) as output:
                self.assertEqual(assistant.main(["chat", "--provider", "claude", "--backend", "orca", "--discord-incoming", "101"]), 1)
                self.assertIn("unsupported", output.getvalue())
            connections = []
            connect = assistant.connect
            def tracked_connect(*args, **kwargs):
                connection = connect(*args, **kwargs)
                connections.append(connection)
                return connection
            assistant.connect = tracked_connect
            self.addCleanup(lambda: [connection.close() for connection in connections])
            self.pending()
            client = self.client
            real_load = client.load
            def fake_provider(*args, **kwargs):
                self.assertNotIn(d.TOKEN_ENV, kwargs["extra_env"])
                self.assertNotIn("TOHRU_DISCORD_SOCKET", kwargs["extra_env"])
                self.assertIn("--tools", args[1])
                if revoke:
                    self.values["receive"] = False
                return SimpleNamespace(session=None, text="Fake reply", error=None), 0, [], stopped
            for post, revoke, stopped, desktop in ((True, False, False, None), (False, False, False, None), (True, True, False, None), (True, False, True, None),
                                                   (True, False, False, "acknowledged"), (True, False, False, "unavailable"), (True, False, False, "unacknowledged")):
                with self.subTest(post=post, revoke=revoke, stopped=stopped, desktop=desktop):
                    client.load = real_load
                    self.values.update(post=post, receive=True, desktop_first=desktop is not None)
                    def notify(content, seconds, verify, cancelled):
                        verify()
                        return desktop
                    client.notify = notify
                    self.receiver.reset()
                    self.page(self.message("100"))
                    self.receiver.poll()
                    self.page(self.message())
                    self.receiver.poll()
                    self.page(self.message(), single=True)  # claim revalidation
                    before = len(self.requests)
                    sends = post and not revoke and not stopped and desktop != "acknowledged"
                    if sends:
                        self.replies.extend([(200, self.metadata(), {}), (200, self.metadata(), {}), (200, {"id": "200"}, {})])
                    output = io.StringIO()
                    with patch.object(assistant, "DiscordClient", return_value=client), patch.object(assistant.BY_ID["claude"], "installed", return_value=True), patch.object(assistant, "run_provider", side_effect=fake_provider), patch.object(sys, "stdin", io.StringIO("ignored spoofed prompt")), contextlib.redirect_stdout(output):
                        result = assistant.main(["chat", "--provider", "claude", "--discord-incoming", "101", "--orca", "false", "--memory", "off"])
                    self.assertEqual(result, 1 if revoke else 0, output.getvalue())
                    posts = [r for r in self.requests[before:] if r[1] == "/api/v10/channels/7/messages" and r[0] == "POST"]
                    self.assertEqual(len(posts), 1 if sends else 0)
                    if desktop == "acknowledged":
                        self.assertIn('"delivery": "desktop"', output.getvalue())
                    self.assertNotIn(self.secret, output.getvalue())
                    self.assertIn("Discord DM", output.getvalue())
        signal.signal(signal.SIGINT, signal.default_int_handler)
        signal.signal(signal.SIGTERM, signal.SIG_DFL)


if __name__ == "__main__":
    unittest.main()
