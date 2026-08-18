import importlib.util
import json
import sys
import threading
import time
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path
from tempfile import TemporaryDirectory
from urllib import parse as url_parse


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "channel_soak_runner.py"
SPEC = importlib.util.spec_from_file_location("channel_soak_runner", SCRIPT_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("failed to load channel_soak_runner module")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class StubState:
    def __init__(self):
        self.event_counter = 0
        self.created_at_seed = int(time.time() * 1000) + 1000
        self.events = []
        self.session_counter = 0
        self.run_counter = 0
        self.approval_counter = 0
        self.approvals = {}
        self.runs = {}
        self.reconnect_attempts = {"telegram": 3, "discord": 4}

    def add_ingest_event(self, provider):
        self.event_counter += 1
        created_at = self.created_at_seed + self.event_counter
        event_id = f"ev-{provider}-{self.event_counter}"
        metadata = {
            "decision": "accepted",
            "run_immediately": True,
            "outbound_reply_status": "sent",
            "outbound_delivery_mode": "transport",
            "outbound_chunk_count": 1,
            "outbound_reply_error": None,
        }
        self.events.append(
            {
                "event_id": event_id,
                "request_id": f"req-{event_id}",
                "correlation_id": f"corr-{event_id}",
                "principal": "static_bearer",
                "action": f"channel.{provider}.ingest",
                "resource": f"{provider}:resource",
                "decision": "allow",
                "reason": None,
                "transport": "internal",
                "status": "200",
                "error_code": None,
                "session_id": f"session-{provider}-{self.event_counter}",
                "run_id": f"run-{provider}-{self.event_counter}",
                "metadata_json": json.dumps(metadata),
                "created_at": created_at,
            }
        )
        return f"run-{provider}-{self.event_counter}", f"session-{provider}-{self.event_counter}"

    def create_session(self):
        self.session_counter += 1
        return f"session-{self.session_counter}"

    def create_run_with_approval(self, session_id):
        self.run_counter += 1
        run_id = f"run-approval-{self.run_counter}"
        self.approval_counter += 1
        approval_id = f"approval-{self.approval_counter}"
        self.runs[run_id] = {"session_id": session_id}
        self.approvals[approval_id] = {"approval_id": approval_id, "run_id": run_id, "status": "requested"}
        return run_id

    def list_requested_approvals(self):
        return [item for item in self.approvals.values() if item["status"] == "requested"]

    def resolve_approval(self, provider, action_payload):
        if provider == "telegram":
            parts = action_payload.split(":")
            approval_id = parts[-1] if len(parts) == 3 else ""
        else:
            parts = action_payload.split("|")
            approval_id = parts[-1] if len(parts) == 3 else ""
        if approval_id not in self.approvals:
            return None
        self.approvals[approval_id]["status"] = "approved"
        return self.approvals[approval_id]


def make_handler(state):
    class Handler(BaseHTTPRequestHandler):
        def _json(self, status, payload):
            body = json.dumps(payload).encode("utf-8")
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def do_GET(self):
            parsed = url_parse.urlparse(self.path)
            if parsed.path == "/api/v1/channels/runtime/status":
                self._json(
                    200,
                    {
                        "updated_at": 1_700_000_000_000,
                        "items": [
                            {
                                "provider": "telegram",
                                "lifecycle_state": "running",
                                "healthy": True,
                                "detail": "ok",
                                "last_error": None,
                                "reconnect_attempts": state.reconnect_attempts["telegram"],
                                "updated_at": 1_700_000_000_000,
                            },
                            {
                                "provider": "discord",
                                "lifecycle_state": "running",
                                "healthy": True,
                                "detail": "ok",
                                "last_error": None,
                                "reconnect_attempts": state.reconnect_attempts["discord"],
                                "updated_at": 1_700_000_000_000,
                            },
                        ],
                    },
                )
                return

            if parsed.path == "/api/v1/security/audit":
                query = url_parse.parse_qs(parsed.query)
                action = query.get("action", [""])[0]
                created_after = int(query.get("created_after", ["0"])[0] or 0)
                items = [
                    event
                    for event in state.events
                    if event["action"] == action and int(event["created_at"]) > created_after
                ]
                self._json(200, {"items": items})
                return

            if parsed.path == "/api/v1/approvals":
                self._json(200, {"items": state.list_requested_approvals()})
                return

            self._json(404, {"error": "not_found"})

        def do_POST(self):
            parsed = url_parse.urlparse(self.path)
            length = int(self.headers.get("content-length", "0") or 0)
            body = self.rfile.read(length).decode("utf-8") if length else "{}"
            payload = json.loads(body) if body else {}

            if parsed.path == "/api/v1/channels/telegram/inbound":
                run_id, session_id = state.add_ingest_event("telegram")
                self._json(
                    200,
                    {
                        "decision": "accepted",
                        "reason": None,
                        "session_id": session_id,
                        "message_id": "msg-telegram",
                        "run_id": run_id,
                    },
                )
                return

            if parsed.path == "/api/v1/channels/discord/inbound":
                run_id, session_id = state.add_ingest_event("discord")
                self._json(
                    200,
                    {
                        "decision": "accepted",
                        "reason": None,
                        "session_id": session_id,
                        "message_id": "msg-discord",
                        "run_id": run_id,
                    },
                )
                return

            if parsed.path == "/api/v1/sessions":
                session_id = state.create_session()
                self._json(201, {"session": {"session_id": session_id}})
                return

            if parsed.path.startswith("/api/v1/sessions/") and parsed.path.endswith("/messages"):
                self._json(201, {"message": {"message_id": "msg-1"}})
                return

            if parsed.path.startswith("/api/v1/sessions/") and parsed.path.endswith("/runs"):
                session_id = parsed.path.split("/")[4]
                run_id = state.create_run_with_approval(session_id)
                self._json(201, {"run": {"run_id": run_id, "status": "failed"}})
                return

            if parsed.path == "/api/v1/channels/approvals/resolve":
                provider = str(payload.get("provider", "")).strip()
                action_payload = str(payload.get("action_payload", "")).strip()
                approval = state.resolve_approval(provider, action_payload)
                if approval is None:
                    self._json(400, {"error": "invalid_payload"})
                    return
                self._json(200, {"approval": approval})
                return

            self._json(404, {"error": "not_found"})

        def log_message(self, format, *args):  # noqa: A003
            return

    return Handler


class ChannelSoakRunnerTests(unittest.TestCase):
    def _start_stub(self):
        state = StubState()
        handler = make_handler(state)
        server = HTTPServer(("127.0.0.1", 0), handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        return state, server, thread

    def test_happy_path_generates_green_report(self):
        state, server, thread = self._start_stub()
        with TemporaryDirectory() as temp_dir:
            code = MODULE.main(
                [
                    "--base-url",
                    f"http://127.0.0.1:{server.server_port}",
                    "--token",
                    "test-token",
                    "--telegram-chat-id",
                    "1001",
                    "--telegram-user-id",
                    "2001",
                    "--discord-channel-id",
                    "discord-c1",
                    "--discord-author-id",
                    "discord-u1",
                    "--iterations",
                    "2",
                    "--interval-seconds",
                    "1",
                    "--no-sleep",
                    "--output-dir",
                    temp_dir,
                    "--label",
                    "unit",
                ]
            )
            self.assertEqual(code, 0)
            latest = Path(temp_dir) / "channel-soak-latest.json"
            self.assertTrue(latest.exists())
            payload = json.loads(latest.read_text(encoding="utf-8"))
            self.assertEqual(payload["status"], "green")
            self.assertEqual(payload["providers"]["telegram"]["attempted"], 2)
            self.assertEqual(payload["providers"]["discord"]["attempted"], 2)
            self.assertEqual(payload["providers"]["telegram"]["outbound_reply_status_counts"]["sent"], 2)
            self.assertEqual(payload["providers"]["discord"]["outbound_reply_status_counts"]["sent"], 2)
            self.assertEqual(payload["approval_roundtrip"]["status"], "passed")
            self.assertEqual(payload["approval_roundtrip"]["telegram"]["status"], "passed")
            self.assertEqual(payload["approval_roundtrip"]["discord"]["status"], "passed")
            self.assertGreaterEqual(len(state.events), 4)
        server.shutdown()
        thread.join(timeout=2)
        server.server_close()

    def test_dry_run_returns_zero_and_writes_report(self):
        with TemporaryDirectory() as temp_dir:
            code = MODULE.main(
                [
                    "--dry-run",
                    "--telegram-chat-id",
                    "1001",
                    "--telegram-user-id",
                    "2001",
                    "--iterations",
                    "1",
                    "--output-dir",
                    temp_dir,
                    "--label",
                    "dryrun",
                ]
            )
            self.assertEqual(code, 0)
            latest = Path(temp_dir) / "channel-soak-latest.json"
            payload = json.loads(latest.read_text(encoding="utf-8"))
            self.assertEqual(payload["status"], "dry_run")


if __name__ == "__main__":
    unittest.main()
