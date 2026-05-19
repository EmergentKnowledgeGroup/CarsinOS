from types import SimpleNamespace
import os

from engine.mcp import MCPServer, ServerConfig, start_http_server, stop_http_server
from engine.runtime import server as runtime_server


class _FakeStore:
    def list_atoms(self):
        return []


class _FakeAdapters:
    def names(self):
        return []


class _FakeRuntime:
    def __init__(self):
        self.retriever = SimpleNamespace(store=_FakeStore())
        self._episode_index = SimpleNamespace(cards=[])


def test_runtime_health_uses_runtime_state_root_for_disk_usage(monkeypatch, tmp_path):
    calls: list[str] = []
    runtime_root = (tmp_path / 'runtime_state').resolve()
    diagnostics_root = (tmp_path / 'diagnostics').resolve()
    expected_probe_root = tmp_path.resolve()

    def fake_disk_usage(target: str):
        calls.append(target)
        return SimpleNamespace(total=10 * 1024**3, used=1 * 1024**3, free=9 * 1024**3)

    monkeypatch.setattr(runtime_server.shutil, 'disk_usage', fake_disk_usage)
    monkeypatch.setattr(runtime_server, 'DIAGNOSTICS_ROOT', diagnostics_root)

    server = SimpleNamespace(
        runtime=_FakeRuntime(),
        adapter_registry=_FakeAdapters(),
        server_address=('127.0.0.1', 7340),
        runtime_version='0.1.0',
        runtime_launch_mode='normal',
        active_runtime_binding={},
        writeback_policy={},
        runtime_root=str(runtime_root),
    )

    payload = runtime_server._runtime_health(server)

    assert payload['service'] == 'modelnumquamoblita-runtime'
    assert calls == [str(expected_probe_root)]


def test_wizard_pid_alive_returns_true_for_current_pid_without_os_kill(monkeypatch):
    def fail_kill(_pid: int, _sig: int):
        raise AssertionError('os.kill should not run for the current pid')

    monkeypatch.setattr(runtime_server.os, 'kill', fail_kill)

    assert runtime_server._wizard_pid_alive(os.getpid()) is True


def test_wizard_runtime_lock_summary_skips_pid_probe_for_owned_lock(monkeypatch):
    fake_server = SimpleNamespace(
        active_runtime_binding={'episodes_path': 'Z:\\modelNumquamOblita\\runtime\\episodes\\episode_cards.reviewed.json'},
        active_runtime_lock={'token': 'owned-token'},
        server_address=('127.0.0.1', 7340),
    )

    monkeypatch.setattr(
        runtime_server,
        '_wizard_read_runtime_lock',
        lambda: {
            'pid': os.getpid(),
            'host': '127.0.0.1',
            'port': 7340,
            'token': 'owned-token',
            'checked_at': '2026-03-16T00:00:00+00:00',
        },
    )

    def fail_pid_probe(_pid: int):
        raise AssertionError('owned lock path should not probe pid liveness')

    monkeypatch.setattr(runtime_server, '_wizard_pid_alive', fail_pid_probe)

    payload = runtime_server._wizard_runtime_lock_summary(fake_server)

    assert payload['status'] == 'owned'
    assert payload['cleanup_allowed'] is False


def test_windows_hidden_subprocess_kwargs_request_no_console(monkeypatch):
    class _FakeStartupInfo:
        def __init__(self):
            self.dwFlags = 0
            self.wShowWindow = None

    monkeypatch.setattr(runtime_server.os, 'name', 'nt', raising=False)
    monkeypatch.setattr(runtime_server.subprocess, 'CREATE_NO_WINDOW', 0x08000000, raising=False)
    monkeypatch.setattr(runtime_server.subprocess, 'STARTF_USESHOWWINDOW', 0x00000001, raising=False)
    monkeypatch.setattr(runtime_server.subprocess, 'SW_HIDE', 0, raising=False)
    monkeypatch.setattr(runtime_server.subprocess, 'STARTUPINFO', _FakeStartupInfo, raising=False)

    kwargs = runtime_server._windows_hidden_subprocess_kwargs()

    assert kwargs['creationflags'] == 0x08000000
    assert isinstance(kwargs['startupinfo'], _FakeStartupInfo)
    assert kwargs['startupinfo'].dwFlags == 0x00000001
    assert kwargs['startupinfo'].wShowWindow == 0


def test_windows_stdio_handshake_subprocess_kwargs_hide_without_create_no_window(monkeypatch):
    class _FakeStartupInfo:
        def __init__(self):
            self.dwFlags = 0
            self.wShowWindow = None

    monkeypatch.setattr(runtime_server.os, 'name', 'nt', raising=False)
    monkeypatch.setattr(runtime_server.subprocess, 'CREATE_NO_WINDOW', 0x08000000, raising=False)
    monkeypatch.setattr(runtime_server.subprocess, 'STARTF_USESHOWWINDOW', 0x00000001, raising=False)
    monkeypatch.setattr(runtime_server.subprocess, 'SW_HIDE', 0, raising=False)
    monkeypatch.setattr(runtime_server.subprocess, 'STARTUPINFO', _FakeStartupInfo, raising=False)

    kwargs = runtime_server._windows_stdio_handshake_subprocess_kwargs()

    assert 'creationflags' not in kwargs
    assert isinstance(kwargs['startupinfo'], _FakeStartupInfo)
    assert kwargs['startupinfo'].dwFlags == 0x00000001
    assert kwargs['startupinfo'].wShowWindow == 0


def test_wizard_mcp_entries_equivalent_accepts_http_entries_with_and_without_metadata():
    left = {
        'type': 'http',
        'url': 'http://127.0.0.1:8765/mcp',
        'managed_by': 'modelnumquamoblita-desktop',
    }
    right = {
        'type': 'http',
        'url': 'http://127.0.0.1:8765/mcp',
    }
    assert runtime_server._wizard_mcp_entries_equivalent(left, right) is True


def test_wizard_mcp_payload_prefers_desktop_sidecar_url(monkeypatch):
    monkeypatch.setattr(
        runtime_server,
        '_wizard_desktop_mcp_sidecar_state',
        lambda: {'url': 'http://127.0.0.1:8765/mcp', 'status': 'ready'},
    )
    payload = runtime_server._wizard_mcp_payload(
        {
            'store_validation': {'path': '/tmp/store.sqlite3'},
            'published_set': {'episodes_path': '/tmp/episode_cards.reviewed.json'},
        },
        {},
    )
    assert payload['mcp_http_url'] == 'http://127.0.0.1:8765/mcp'
    assert payload['mcp_http_managed'] is True
    assert payload['mcp_http_status'] == 'ready'


def test_wizard_mcp_payload_prefers_desktop_sidecar_profile_defaults(monkeypatch):
    monkeypatch.setattr(runtime_server, '_wizard_desktop_mcp_sidecar_state', lambda: {})
    monkeypatch.setattr(
        runtime_server,
        '_wizard_desktop_mcp_sidecar_settings',
        lambda: {
            'profiles': {
                'draft': {'default_role': 'operator', 'compat_mode': 'strict', 'mutations_enabled': True},
                'reviewed': {'default_role': 'admin', 'compat_mode': 'strict', 'mutations_enabled': False},
            }
        },
    )
    payload = runtime_server._wizard_mcp_payload(
        {
            'store_validation': {'path': '/tmp/store.sqlite3'},
            'published_set': {'episodes_path': '/tmp/episode_cards.reviewed.json'},
        },
        {'artifact_mode': 'reviewed'},
    )
    assert payload['default_role'] == 'admin'
    assert payload['compat_mode'] == 'strict'
    assert payload['mutations_enabled'] is False


def test_wizard_mcp_handshake_supports_http_entries():
    server = MCPServer(
        config=ServerConfig(runtime_base_url='http://127.0.0.1:7340', transport='http'),
        api_client=SimpleNamespace(),
    )
    http_server, thread = start_http_server(server, host='127.0.0.1', port=0)
    host, port = http_server.server_address
    try:
        payload = runtime_server._wizard_mcp_handshake({'type': 'http', 'url': f'http://{host}:{port}/mcp'})
        assert payload['ok'] is True
        assert 'initialize' in payload
        assert 'tools' in payload
    finally:
        stop_http_server(http_server, thread)
