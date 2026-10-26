"""Single-turn documentation example for REQ-022 (Spec §7.6); not a replacement for Core."""

import argparse
from collections import deque
import json
import math
import os
from pathlib import Path
import queue
import re
import subprocess
import sys
import threading
import time


def emit(kind, **fields):
    print(json.dumps({"kind": kind, **fields}, ensure_ascii=False), flush=True)


class Client:
    def __init__(self, executable, cwd):
        env = os.environ.copy()
        for key in ("CODEX_INTERNAL_ORIGINATOR_OVERRIDE", "CODEX_THREAD_ID", "CODEX_SESSION_ID"):
            env.pop(key, None)
        self.proc = subprocess.Popen(
            [executable, "app-server", "--stdio"], cwd=cwd, env=env,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=None,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
        )
        self.messages = queue.Queue(maxsize=128)
        self.deferred = deque()
        self.serial = 0
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        try:
            while True:
                line = self.proc.stdout.readline(1024 * 1024 + 1)
                if not line:
                    raise RuntimeError("App Server ended before the expected response")
                if len(line) > 1024 * 1024:
                    raise RuntimeError("App Server frame exceeded 1 MiB")
                self.messages.put(json.loads(line.decode("utf-8", errors="strict")))
        except Exception as error:
            self.messages.put(error)

    def send(self, value):
        self.proc.stdin.write((json.dumps(value, ensure_ascii=False) + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def receive(self, deadline):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("App Server deadline exceeded")
        try:
            value = self.messages.get(timeout=remaining)
        except queue.Empty:
            raise TimeoutError("App Server deadline exceeded") from None
        if isinstance(value, Exception):
            raise value
        if not isinstance(value, dict):
            raise RuntimeError("App Server sent a non-object frame")
        if "method" in value and "id" in value:
            # Never approve or execute a server-initiated tool request in this example.
            self.send({"id": value["id"], "error": {"code": -32601, "message": "unsupported client request"}})
            emit("client.rejected_request", method=value["method"])
            return None
        return value

    def request(self, method, params, timeout=10):
        self.serial += 1
        request_id = self.serial
        self.send({"id": request_id, "method": method, "params": params})
        deadline = time.monotonic() + timeout
        while True:
            value = self.receive(deadline)
            if value is None:
                continue
            if value.get("id") == request_id:
                if "error" in value:
                    raise RuntimeError(f"{method} rejected (code {value['error'].get('code')})")
                if "result" not in value:
                    raise RuntimeError("App Server response has no result")
                return value["result"]
            if "id" in value:
                raise RuntimeError("Unexpected response id")
            if len(self.deferred) >= 1024:
                raise RuntimeError("Too many notifications before a response")
            self.deferred.append(value)

    def close(self):
        try:
            self.proc.stdin.close()
        except (BrokenPipeError, OSError):
            pass
        try:
            return self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.terminate()
            try:
                return self.proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                return self.proc.wait()


def main():
    for stream in (sys.stdin, sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description="REQ-022 단일 Turn App Server 호출 예제")
    parser.add_argument("--codex", required=True, help="확장 동봉 CLI 절대 경로")
    parser.add_argument("--cwd", required=True, help="실제 작업 폴더 절대 경로")
    parser.add_argument("--model", required=True, help="해당 계정에서 확인한 모델 ID")
    parser.add_argument("--sandbox", choices=["read-only", "workspace-write"], default="read-only")
    parser.add_argument("--effort", default="low")
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--name", help="시험 대화를 찾기 위한 제목")
    parser.add_argument("--archive", action="store_true", help="완료 뒤 공식 Archive(시험 정리용)")
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--resume", help="동시 실행 중이 아닌, 호출 권한이 있는 Thread ID")
    source.add_argument("--fork", help="새 ID로 분기할, 호출 권한이 있는 원본 Thread ID")
    args = parser.parse_args()
    if not Path(args.codex).is_absolute() or not Path(args.codex).is_file():
        parser.error("--codex는 실제 실행 파일의 절대 경로여야 합니다")
    if not Path(args.cwd).is_absolute() or not Path(args.cwd).is_dir():
        parser.error("--cwd는 실제 작업 폴더의 절대 경로여야 합니다")
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("--timeout은 양수여야 합니다")
    prompt = sys.stdin.read(256 * 1024 + 1)
    if not prompt.strip() or len(prompt.encode("utf-8")) > 256 * 1024:
        parser.error("stdin 프롬프트는 비어 있지 않고 UTF-8 256 KiB 이하여야 합니다")

    client = Client(args.codex, args.cwd)
    thread_id = turn_id = None
    status = "not_started"
    exit_code = 1
    try:
        client.request("initialize", {
            "clientInfo": {"name": "telosweaver_req022", "version": "0.1.0"},
            "capabilities": {"experimentalApi": True},
        })
        client.send({"method": "initialized", "params": {}})
        # Read only server names; never print the effective configuration or its credentials.
        config = client.request("config/read", {"includeLayers": False})["config"]
        overrides = {}
        for name in config.get("mcp_servers", {}):
            if not re.fullmatch(r"[A-Za-z0-9_-]+", name):
                raise RuntimeError("MCP server name is outside this example's override support")
            overrides[f"mcp_servers.{name}.enabled"] = False
            overrides[f"mcp_servers.{name}.required"] = False
        overrides.update({
            "sandbox_workspace_write.writable_roots": [],
            "sandbox_workspace_write.network_access": False,
            "sandbox_workspace_write.exclude_tmpdir_env_var": True,
            "sandbox_workspace_write.exclude_slash_tmp": True,
        })
        params = {
            "cwd": args.cwd, "model": args.model, "sandbox": args.sandbox,
            "approvalPolicy": "never", "config": overrides,
        }
        if args.resume:
            method = "thread/resume"
            params["threadId"] = args.resume
        else:
            method = "thread/fork" if args.fork else "thread/start"
            params["ephemeral"] = False
            if args.fork:
                params["threadId"] = args.fork
        started = client.request(method, params)
        thread_id = started["thread"]["id"]
        native_source = started["thread"].get("source")
        emit("client.thread", threadId=thread_id, source=native_source, cwd=started["thread"].get("cwd"))
        expected_sandbox = "readOnly" if args.sandbox == "read-only" else "workspaceWrite"
        if (started.get("approvalPolicy") != "never"
                or started.get("sandbox", {}).get("type") != expected_sandbox
                or started.get("sandbox", {}).get("networkAccess") is not False):
            raise RuntimeError("Effective sandbox/approval policy differs from the requested policy")
        if args.sandbox == "workspace-write":
            sandbox = started["sandbox"]
            if (sandbox.get("writableRoots") != []
                    or sandbox.get("excludeTmpdirEnvVar") is not True
                    or sandbox.get("excludeSlashTmp") is not True):
                raise RuntimeError("Effective workspace write scope differs from the requested policy")
        if native_source not in ("cli", "vscode"):
            raise RuntimeError("Thread is outside the default VS Code list; create a fresh App Server thread or fork")
        emit("client.policy", approvalPolicy="never", sandbox=started["sandbox"])
        status = "start_requested"
        turn = client.request("turn/start", {
            "threadId": thread_id, "input": [{"type": "text", "text": prompt}],
            "model": args.model, "effort": args.effort, "approvalPolicy": "never",
        })
        turn_id = turn["turn"]["id"]
        status = "in_progress"
        deadline = time.monotonic() + args.timeout
        while True:
            if time.monotonic() >= deadline:
                raise TimeoutError("App Server deadline exceeded")
            event = client.deferred.popleft() if client.deferred else client.receive(deadline)
            if event is None:
                continue
            emit("server.notification", message=event)
            event_params = event.get("params", {})
            if event.get("method") == "turn/completed" and event_params.get("threadId") == thread_id:
                completed = event_params.get("turn", {})
                if completed.get("id") == turn_id:
                    status = completed["status"]
                    break
        if args.name:
            client.request("thread/name/set", {"threadId": thread_id, "name": args.name})
        exit_code = 0 if status == "completed" else 1
    except (Exception, KeyboardInterrupt) as error:
        emit("client.error", error=str(error) or type(error).__name__)
        if turn_id and status == "in_progress":
            try:
                client.request("turn/interrupt", {"threadId": thread_id, "turnId": turn_id}, timeout=5)
            except Exception:
                pass
        exit_code = 124 if isinstance(error, TimeoutError) else 1
    finally:
        if args.archive and thread_id:
            try:
                client.request("thread/archive", {"threadId": thread_id})
                emit("client.archived", threadId=thread_id)
            except Exception as error:
                emit("client.archive_failed", threadId=thread_id, error=str(error))
                if exit_code == 0:
                    exit_code = 1
        native_exit = client.close()
        if native_exit != 0 and exit_code == 0:
            exit_code = 1
        emit("client.result", threadId=thread_id, turnId=turn_id, turnStatus=status,
             serverExitCode=native_exit, exitCode=exit_code)
    return exit_code


if __name__ == "__main__":
    raise SystemExit(main())
