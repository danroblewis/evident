"""HTTP server plugin — listens on a TCP port, multiplexes client connections,
presents one ready request per tick to Evident as `server.request.*`.

Approach B from docs/design/sockets-and-http-server.md: the plugin handles
all multi-connection concurrency via select(); the Evident program sees a
single request → response shape per tick (WSGI-style).
"""

from __future__ import annotations

import select
import socket
from typing import Any

from ..plugin import Plugin


_BACKLOG       = 64
_RECV_CHUNK    = 4096
_SELECT_TIMEOUT = 0.05  # seconds — short so the loop ticks even when idle


class _ClientState:
    __slots__ = ('sock', 'buffer')
    def __init__(self, sock: socket.socket):
        self.sock   = sock
        self.buffer = bytearray()


class HTTPServerPlugin(Plugin):
    """One HTTPServer per Evident program. Hosts a listening socket on
    (host, port) and routes one ready request per tick into the constraint
    solve. Closes connections after sending the response (HTTP/1.0)."""

    handles_types = {'HTTPServer'}

    def __init__(self, host: str = '127.0.0.1', port: int = 8080):
        super().__init__()
        self.host = host
        self.port = port
        self.listen_sock: socket.socket | None = None
        self.clients: dict[int, _ClientState] = {}   # fd → state
        self.current_fd: int | None = None            # fd of this tick's request

    # ── Lifecycle ────────────────────────────────────────────────────────────

    def start(self) -> None:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.bind((self.host, self.port))
        s.listen(_BACKLOG)
        s.setblocking(False)
        self.listen_sock = s
        # User-visible: which port we actually got
        import sys
        print(f"HTTP server listening on {self.host}:{self.port}",
              file=sys.stderr, flush=True)

    def stop(self) -> None:
        for st in self.clients.values():
            try: st.sock.close()
            except Exception: pass
        self.clients.clear()
        if self.listen_sock is not None:
            try: self.listen_sock.close()
            except Exception: pass
            self.listen_sock = None

    # ── Per-tick ─────────────────────────────────────────────────────────────

    def before_step(self, _state) -> dict[str, Any]:
        self.current_fd = None
        if self.listen_sock is None:
            return self._no_request()

        # Build read-set: listening socket + every active client
        rset = [self.listen_sock] + [st.sock for st in self.clients.values()]
        try:
            ready, _, _ = select.select(rset, [], [], _SELECT_TIMEOUT)
        except (OSError, ValueError):
            return self._no_request()

        for s in ready:
            if s is self.listen_sock:
                self._accept()
            else:
                self._read(s)

        # Find the first client whose buffer holds a complete request line + headers
        for fd, st in self.clients.items():
            i = st.buffer.find(b'\r\n\r\n')
            if i < 0:
                continue
            request_line = bytes(st.buffer[:st.buffer.find(b'\r\n')])
            method, path = self._parse_request_line(request_line)
            self.current_fd = fd
            return self._request(method, path)

        return self._no_request()

    def after_step(self, bindings) -> bool:
        if self.current_fd is None:
            return True

        st = self.clients.pop(self.current_fd, None)
        if st is None:
            return True

        # Find the variable name (e.g. 'server') we're driving
        var = next(iter(self.matched_vars))
        status = int(bindings.get(f'{var}.response.status', 0) or 0)
        body   = str(bindings.get(f'{var}.response.body',   '') or '')

        try:
            if status > 0:
                reason = _STATUS_REASON.get(status, 'OK')
                response = (
                    f"HTTP/1.0 {status} {reason}\r\n"
                    f"Content-Length: {len(body.encode())}\r\n"
                    f"Connection: close\r\n"
                    f"\r\n"
                    f"{body}"
                ).encode()
                try: st.sock.sendall(response)
                except OSError: pass
            # status = 0 means "no response" — just close
        finally:
            try: st.sock.close()
            except OSError: pass

        return True

    # ── Internals ────────────────────────────────────────────────────────────

    def _accept(self) -> None:
        try:
            conn, _addr = self.listen_sock.accept()
        except (OSError, BlockingIOError):
            return
        conn.setblocking(False)
        self.clients[conn.fileno()] = _ClientState(conn)

    def _read(self, sock: socket.socket) -> None:
        fd = sock.fileno()
        st = self.clients.get(fd)
        if st is None:
            return
        try:
            data = sock.recv(_RECV_CHUNK)
        except (OSError, BlockingIOError):
            self._drop(fd)
            return
        if not data:                  # peer closed
            self._drop(fd)
            return
        st.buffer.extend(data)

    def _drop(self, fd: int) -> None:
        st = self.clients.pop(fd, None)
        if st is not None:
            try: st.sock.close()
            except OSError: pass

    def _parse_request_line(self, line: bytes) -> tuple[str, str]:
        try:
            parts = line.decode('iso-8859-1').split(' ')
            method = parts[0] if len(parts) > 0 else ''
            path   = parts[1] if len(parts) > 1 else ''
        except Exception:
            method, path = '', ''
        return method, path

    def _request(self, method: str, path: str) -> dict[str, Any]:
        var = next(iter(self.matched_vars))
        return {
            f'{var}.request.has_request': True,
            f'{var}.request.method':      method,
            f'{var}.request.path':        path,
        }

    def _no_request(self) -> dict[str, Any]:
        var = next(iter(self.matched_vars))
        return {
            f'{var}.request.has_request': False,
            f'{var}.request.method':      '',
            f'{var}.request.path':        '',
        }


_STATUS_REASON = {
    200: 'OK',
    400: 'Bad Request',
    404: 'Not Found',
    500: 'Internal Server Error',
}
