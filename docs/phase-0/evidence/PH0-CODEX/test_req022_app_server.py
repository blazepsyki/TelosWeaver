"""Offline pipe-level checks for the REQ-022 documentation client (Spec §7.6)."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import unittest
from unittest import mock


def fake_server(case):
    sys.stdout.reconfigure(encoding="utf-8")

    def send(value):
        print(json.dumps(value, ensure_ascii=False), flush=True)

    for line in sys.stdin.buffer:
        request = json.loads(line)
        method = request.get("method")
        if "id" not in request or method is None:
            continue
        params = request.get("params", {})
        result = {}
        if method == "config/read":
            result = {"config": {"mcp_servers": {"offline-test": {}}}}
        elif method in ("thread/start", "thread/resume", "thread/fork"):
            sandbox = {"type": "readOnly", "networkAccess": False}
            if params["sandbox"] == "workspace-write":
                sandbox.update(type="workspaceWrite", writableRoots=[],
                               excludeTmpdirEnvVar=True, excludeSlashTmp=True)
                if case == "extra-root":
                    sandbox["writableRoots"] = ["/outside-workspace"]
                if case == "tmp-access":
                    sandbox["excludeSlashTmp"] = False
            if case == "policy-mismatch":
                sandbox["networkAccess"] = True
            result = {
                "thread": {"id": "owned-thread", "source": "exec" if case == "exec" else "vscode",
                           "cwd": params["cwd"]},
                "approvalPolicy": "never", "sandbox": sandbox,
            }
        elif method == "turn/start":
            # A policy failure must prevent generation, not merely change the exit code.
            if case in ("extra-root", "tmp-access", "policy-mismatch", "exec"):
                raise RuntimeError("turn/start reached after a policy/source refusal")
            send({"id": "tool-1", "method": "item/tool/call", "params": {}})
            send({"method": "turn/completed", "params": {
                "threadId": "other-thread", "turn": {"id": "owned-turn", "status": "failed"}}})
            if case != "timeout":
                send({"method": "item/agentMessage/delta", "params": {
                    "threadId": "owned-thread", "delta": params["input"][0]["text"]}})
                send({"method": "turn/completed", "params": {
                    "threadId": "owned-thread", "turn": {
                        "id": "owned-turn", "status": "failed" if case == "failed" else "completed"}}})
            # Completion precedes the response to exercise deferred notifications.
            result = {"turn": {"id": "owned-turn"}}
        elif method == "thread/archive" and case == "timeout":
            send({"id": request["id"], "error": {"code": -1}})
            continue
        send({"id": request["id"], "result": result})
    return 7 if case in ("timeout", "bad-exit") else 0


class ClientTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location(
            "req022_client", Path(__file__).with_name("req022_app_server.py"))
        cls.client = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.client)

    def run_client(self, case, *options):
        real_popen = subprocess.Popen
        output = io.StringIO()

        def spawn(*args, **kwargs):
            return real_popen([sys.executable, "-B", "-u", str(Path(__file__).resolve()),
                               "--fake-server", case], **kwargs)

        argv = ["req022_app_server.py", "--codex", sys.executable,
                "--cwd", str(Path(__file__).resolve().parent), "--model", "offline",
                "--timeout", "0.1", *options]
        with mock.patch.object(sys, "argv", argv), \
                mock.patch.object(sys, "stdin", io.StringIO("한국어 확인")), \
                mock.patch.object(self.client.subprocess, "Popen", spawn), \
                contextlib.redirect_stdout(output):
            code = self.client.main()
        return code, [json.loads(line) for line in output.getvalue().splitlines()]

    def test_early_completion_unicode_and_unimplemented_request(self):
        code, events = self.run_client("completed", "--archive")
        self.assertEqual(code, 0)
        self.assertTrue(any(e["kind"] == "client.rejected_request" for e in events))
        self.assertTrue(any(e["kind"] == "client.archived" for e in events))
        self.assertIn("한국어 확인", json.dumps(events, ensure_ascii=False))
        self.assertEqual(events[-1]["turnStatus"], "completed")

    def test_failed_turn_is_not_success_even_when_server_exits_zero(self):
        code, events = self.run_client("failed")
        self.assertEqual((code, events[-1]["serverExitCode"]), (1, 0))

    def test_nonzero_server_exit_is_not_success(self):
        code, events = self.run_client("bad-exit")
        self.assertEqual((code, events[-1]["turnStatus"]), (1, "completed"))

    def test_timeout_survives_archive_failure_and_nonzero_server_exit(self):
        code, events = self.run_client("timeout", "--archive")
        self.assertEqual((code, events[-1]["serverExitCode"]), (124, 7))
        self.assertTrue(any(e["kind"] == "client.archive_failed" for e in events))

    def test_permission_and_source_mismatch_prevent_turn_start(self):
        for case in ("policy-mismatch", "exec", "extra-root", "tmp-access"):
            with self.subTest(case=case):
                code, events = self.run_client(case, "--sandbox", "workspace-write")
                self.assertEqual(code, 1)
                self.assertEqual(events[-1]["turnStatus"], "not_started")
                self.assertEqual(events[-1]["serverExitCode"], 0)

    def test_resume_and_fork_complete(self):
        for option in ("--resume", "--fork"):
            with self.subTest(option=option):
                code, events = self.run_client("completed", option, "owned-source")
                self.assertEqual((code, events[-1]["turnStatus"]), (0, "completed"))


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--fake-server":
        raise SystemExit(fake_server(sys.argv[2]))
    unittest.main()
