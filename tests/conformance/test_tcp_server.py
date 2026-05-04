"""End-to-end test for the TCP plugin via the HTTP demo.

Spawns the demo (which uses the raw TCPSocket primitive and parses HTTP
in Evident) on a random port, hits it with urllib, asserts responses.
"""

from __future__ import annotations

import socket
import subprocess
import sys
import time
import urllib.request
from pathlib import Path

import pytest

PROJECT_ROOT = Path(__file__).parent.parent.parent
DEMO         = PROJECT_ROOT / 'programs' / 'http_demo' / 'server.ev'


def _free_port() -> int:
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.bind(('127.0.0.1', 0))
    port = s.getsockname()[1]
    s.close()
    return port


def _wait_for_port(host: str, port: int, timeout: float = 10.0) -> bool:
    deadline = time.time() + timeout
    while time.time() < deadline:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.settimeout(0.2)
        try:
            s.connect((host, port))
            s.close()
            return True
        except OSError:
            time.sleep(0.1)
        finally:
            try: s.close()
            except OSError: pass
    return False


@pytest.fixture
def http_server():
    port = _free_port()
    proc = subprocess.Popen(
        [sys.executable, 'evident.py', 'execute', str(DEMO),
         '--port', str(port)],
        cwd=str(PROJECT_ROOT),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if not _wait_for_port('127.0.0.1', port, timeout=10.0):
        proc.terminate()
        try:
            out, err = proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            out, err = proc.communicate()
        pytest.fail(f"server didn't start on port {port}\nstderr: {err.decode()[:500]}")
    yield port
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()


def test_get_returns_200(http_server):
    port = http_server
    with urllib.request.urlopen(f'http://127.0.0.1:{port}/', timeout=10) as resp:
        assert resp.status == 200
        body = resp.read().decode()
    assert body == 'Hello from Evident\n'


def test_get_with_path(http_server):
    port = http_server
    with urllib.request.urlopen(f'http://127.0.0.1:{port}/anything/here', timeout=10) as resp:
        assert resp.status == 200
        assert resp.read().decode() == 'Hello from Evident\n'


def test_multiple_sequential_requests(http_server):
    port = http_server
    for i in range(3):
        with urllib.request.urlopen(f'http://127.0.0.1:{port}/r{i}', timeout=10) as resp:
            assert resp.status == 200
            assert resp.read().decode() == 'Hello from Evident\n'
