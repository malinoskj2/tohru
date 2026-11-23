#!/usr/bin/env python3
"""The helper composes existing native authority; it never replaces admission."""
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

path = Path(__file__).resolve().parents[1] / "bin/tohru-voice-status"
loader = importlib.machinery.SourceFileLoader("voice_status", str(path))
spec = importlib.util.spec_from_loader(loader.name, loader)
module = importlib.util.module_from_spec(spec)
loader.exec_module(module)
REQUEST = "orca:1795437663530-current"


class Client:
    def __init__(self):
        self.calls = []
        self.entries = [{"role": "user", "text": "[tohru-desktop:voice]\nHow is it going?",
                         "interaction": {"kind": "owner", "requestId": REQUEST}}]
        self.status = {"dispatchId": "ctx_owned", "runId": "run_owned", "taskId": "task_owned",
                       "state": "ready", "stage": "input_accepted", "liveness": "live", "activity": "working"}
        self.receipt = {"request": REQUEST, "delivery": "dispatched"}
        self.rejected = False

    def call(self, arguments):
        self.calls.append(arguments)
        if arguments[0] == "agent":
            return {"entries": list(self.entries)}
        if arguments[0] == "orchestration":
            return self.status
        if self.rejected:
            raise module.StatusError("native admission rejected current request or ownership")
        return self.receipt


class StatusTests(unittest.TestCase):
    def test_success_binds_snapshot_and_preserves_native_receipt(self):
        client = Client()
        result = module.acknowledge(client, "home", "ctx_owned", "run_owned")
        self.assertEqual(result["delivery"], "dispatched")
        self.assertEqual(client.calls[-1][6:8], ["--sequence", "1"])
        self.assertEqual(client.calls[-1][4:6], ["--request", REQUEST])
        self.assertEqual(len(client.calls), 3)

    def test_internal_mail_does_not_replace_actual_owner(self):
        client = Client()
        client.entries.append({"role": "user", "text": "worker mail", "interaction": {"kind": "internal"}})
        module.acknowledge(client, "home", "ctx_owned", "run_owned")
        self.assertEqual(client.calls[-1][5], REQUEST)

    def test_terminal_unknown_foreign_and_unready_fail_before_publish(self):
        for key, value in [("state", "succeeded"), ("state", "failed"), ("state", "stopped"),
                           ("state", "starting"), ("state", "accepted"), ("stage", "unknown"),
                           ("activity", "idle"), ("liveness", "unknown"),
                           ("dispatchId", "ctx_foreign"), ("runId", "run_foreign")]:
            with self.subTest(key=key, value=value):
                client = Client()
                client.status[key] = value
                with self.assertRaises(module.StatusError):
                    module.acknowledge(client, "home", "ctx_owned", "run_owned")
                self.assertEqual(len(client.calls), 2)

    def test_malformed_or_typed_owner_does_not_query_worker(self):
        for entries in [[], [{"role": "user", "interaction": {"kind": "owner"}, "text": "typed"}],
                        [{"role": "user", "interaction": {"kind": "owner", "requestId": "invented"},
                          "text": "[tohru-desktop:voice]\nhello"}]]:
            client = Client()
            client.entries = entries
            with self.assertRaises(module.StatusError):
                module.acknowledge(client, "home", "ctx_owned", "run_owned")
            self.assertEqual(len(client.calls), 1)

    def test_native_rejection_never_retries_or_falls_back_to_latest(self):
        client = Client()
        client.rejected = True
        with self.assertRaises(module.StatusError):
            module.acknowledge(client, "home", "ctx_owned", "run_owned")
        self.assertEqual(len(client.calls), 3)
        self.assertIn(REQUEST, client.calls[-1])

    def test_replay_and_hidden_pending_remain_native_receipts(self):
        for delivery in ("replay", "pending"):
            client = Client()
            client.receipt["delivery"] = delivery
            result = module.acknowledge(client, "home", "ctx_owned", "run_owned")
            self.assertEqual(result["delivery"], delivery)
            self.assertEqual(len(client.calls), 3)


class ProcessTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.fixture = self.root / "fixture.json"
        self.log = self.root / "calls.jsonl"
        self.cli = self.root / "orca-test"
        self.cli.write_text(f"#!{sys.executable}\n" + '''
import json,os,sys,time
from pathlib import Path
args=sys.argv[1:]
with open(os.environ['STATUS_TEST_LOG'],'a') as log:log.write(json.dumps(args)+'\\n')
fixture=json.loads(Path(os.environ['STATUS_TEST_FIXTURE']).read_text())
Path(os.environ['STATUS_TEST_FIXTURE']+'.pid').write_text(str(os.getpid()))
if fixture.get('behavior')=='oversize':
    sys.stdout.buffer.write(b'x'*(3*1024*1024));sys.stdout.flush();time.sleep(30)
if fixture.get('behavior')=='timeout':time.sleep(30)
if fixture.get('behavior')=='malformed':print('not json');sys.exit(0)
if args[:2]==['agent','read']:result={'entries':fixture['entries']}
elif args[:2]==['orchestration','worker-show']:result=fixture['status']
else:
    request=args[args.index('--request')+1]
    if fixture.get('reject') or request!=fixture.get('current',fixture['receipt']['request']):
        print(json.dumps({'ok':False,'error':'native rejection'}));sys.exit(1)
    result=fixture['receipt']
print(json.dumps({'ok':True,'result':result}))
''')
        self.cli.chmod(0o755)
        client = Client()
        self.data = {"entries": client.entries, "status": client.status, "receipt": client.receipt}
        self.env = {**os.environ, "ORCA_AGENT_SESSION_ID": "12345678-1234-4234-8234-123456789abc",
                    "ORCA_CLI_COMMAND": str(self.cli), "ORCA_ENVIRONMENT": "local",
                    "STATUS_TEST_LOG": str(self.log), "STATUS_TEST_FIXTURE": str(self.fixture)}
        self.env.pop("ORCA_PAIRING_CODE", None)

    def invoke(self, *extra):
        self.fixture.write_text(json.dumps(self.data))
        return subprocess.run([sys.executable, str(path), "--dispatch", "ctx_owned", "--run", "run_owned", *extra],
                              env=self.env, capture_output=True, text=True, timeout=5)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def test_actual_cli_process_passes_snapshot_request_once(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertEqual(json.loads(result.stdout)["result"]["request"], REQUEST)
        self.assertEqual(len(self.calls()), 3)
        self.assertIn(REQUEST, self.calls()[-1])
        self.assertNotIn("--from", self.calls()[-1])

    def test_new_owner_terminal_race_or_foreign_admission_never_retries(self):
        for rejection in ({"current":"orca:new-owner"}, {"reject":True}):
            with self.subTest(rejection=rejection):
                self.data.update(rejection)
                result = self.invoke()
                self.assertEqual(result.returncode, 1)
                self.assertFalse(json.loads(result.stdout)["ok"])
                self.assertIn(REQUEST, self.calls()[-1])
                self.assertEqual(sum(c[:2]==["bot","voice-task"] for c in self.calls()), 1)
                self.log.unlink()

    def test_bad_identity_missing_caller_and_remote_fail_before_cli(self):
        for key, value in [("ORCA_AGENT_SESSION_ID", ""), ("ORCA_PAIRING_CODE", "remote"),
                           ("ORCA_ENVIRONMENT", "ssh:remote")]:
            with self.subTest(key=key):
                original = self.env.get(key)
                self.env[key] = value
                self.assertEqual(self.invoke().returncode, 1)
                self.assertEqual(self.calls(), [])
                if original is None:
                    self.env.pop(key)
                else:
                    self.env[key] = original
        self.assertEqual(self.invoke("--dispatch", "../../foreign").returncode, 1)
        self.assertEqual(self.calls(), [])

    def test_oversized_output_is_stopped_while_running_and_reaped(self):
        self.data['behavior'] = 'oversize'
        self.fixture.write_text(json.dumps(self.data))
        with patch.dict(os.environ, self.env, clear=True):
            client = module.Orca()
            started = time.monotonic()
            with self.assertRaisesRegex(module.StatusError, 'byte limit'):
                client.call(['agent','read'])
        self.assertLess(time.monotonic()-started, 3)
        pid = int(Path(str(self.fixture)+'.pid').read_text())
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)
        self.assertEqual(len(self.calls()), 1)

    def test_timeout_reaps_owned_child_and_never_retries(self):
        self.data['behavior'] = 'timeout'
        self.fixture.write_text(json.dumps(self.data))
        with patch.dict(os.environ, self.env, clear=True):
            client = module.Orca()
            client.deadline = time.monotonic()+0.2
            with self.assertRaisesRegex(module.StatusError, 'timed out'):
                client.call(['agent','read'])
        pid = int(Path(str(self.fixture)+'.pid').read_text())
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)
        self.assertEqual(len(self.calls()), 1)

    def test_malformed_output_is_not_parsed_as_success(self):
        self.data['behavior'] = 'malformed'
        result = self.invoke()
        self.assertEqual(result.returncode, 1)
        self.assertIn('Invalid Orca response', json.loads(result.stdout)['error'])
        self.assertEqual(len(self.calls()), 1)


if __name__ == "__main__":
    unittest.main()
