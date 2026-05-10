"""End-to-end test for the multi-page HTTP demo (site.ev).

Verifies path routing — GET /, /about, /contact each return 200 with the
right page; anything else returns 404. Each page contains hyperlinks back
to the others.
"""

from __future__ import annotations

import socket
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

import pytest

PROJECT_ROOT = Path(__file__).parent.parent.parent
DEMO         = PROJECT_ROOT / 'programs' / 'http_demo' / 'site.ev'


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
def site_server():
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
            _, err = proc.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            _, err = proc.communicate()
        pytest.fail(f"server didn't start on port {port}\nstderr: {err.decode()[:500]}")
    yield port
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()


def _get(port: int, path: str):
    """GET path, return (status, body). Treats 4xx/5xx as data, not exceptions."""
    try:
        with urllib.request.urlopen(f'http://127.0.0.1:{port}{path}', timeout=10) as r:
            return r.status, r.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()


def test_root_page(site_server):
    status, body = _get(site_server, '/')
    assert status == 200
    assert '<h1>Evident HTTP Demo</h1>' in body
    assert "/about" in body
    assert "/contact" in body


def test_about_page(site_server):
    status, body = _get(site_server, '/about')
    assert status == 200
    assert '<h1>About</h1>' in body
    assert "href='/'" in body  # link back home


def test_contact_page(site_server):
    status, body = _get(site_server, '/contact')
    assert status == 200
    assert '<h1>Contact</h1>' in body
    assert 'github.com' in body
    assert "href='/'" in body


def test_unknown_path_returns_404(site_server):
    status, body = _get(site_server, '/nope')
    assert status == 404
    assert '404' in body
    assert "href='/'" in body


def test_paths_route_distinctly(site_server):
    """The home page must not respond to /about, and vice versa."""
    _, root_body  = _get(site_server, '/')
    _, about_body = _get(site_server, '/about')
    assert root_body != about_body
    assert '<h1>Evident HTTP Demo</h1>' in root_body
    assert '<h1>About</h1>'             in about_body
