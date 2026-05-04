"""Raw TCP socket plugin.

Owns one listening socket. Accepts one client connection at a time;
later clients wait in the OS backlog. Each tick:

  before_step:
    - if no client is connected, try to accept one (non-blocking)
    - if a client is connected, read whatever bytes are available
    - present has_conn + received (accumulated across ticks)

  after_step:
    - if Evident's `send` is non-empty, write those bytes to the client
    - if Evident's `close` is true, close the client (next tick will
      try to accept the next one)

The plugin knows nothing about HTTP — it's pure transport. Whatever
protocol parsing happens is done by the Evident program reading
`received` and writing `send`.
"""

from __future__ import annotations

import errno
import select
import socket
from typing import Any

from ..plugin import Plugin


_BACKLOG       = 64
_RECV_CHUNK    = 4096
_SELECT_TIMEOUT = 0.05  # seconds — short so the loop ticks even when idle


class TCPSocketPlugin(Plugin):
    """One TCPSocket per Evident program. Single connection at a time."""

    handles_types = {'TCPSocket'}

    def __init__(self, host: str = '127.0.0.1', port: int = 8080):
        super().__init__()
        self.host = host
        self.port = port
        self.listen_sock: socket.socket | None = None
        self.client_sock: socket.socket | None = None
        self.recv_buffer = bytearray()

    # ── Lifecycle ────────────────────────────────────────────────────────────

    def start(self) -> None:
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        s.bind((self.host, self.port))
        s.listen(_BACKLOG)
        s.setblocking(False)
        self.listen_sock = s
        import sys
        print(f"TCP listening on {self.host}:{self.port}",
              file=sys.stderr, flush=True)

    def stop(self) -> None:
        self._close_client()
        if self.listen_sock is not None:
            try: self.listen_sock.close()
            except OSError: pass
            self.listen_sock = None

    # ── Per-tick ─────────────────────────────────────────────────────────────

    def before_step(self, _state) -> dict[str, Any]:
        if self.listen_sock is None:
            return self._snapshot(connected=False, buffer=b'')

        # If we don't have a client, try to accept one (non-blocking).
        if self.client_sock is None:
            self._try_accept()

        # If we have a client, drain whatever's available without blocking.
        if self.client_sock is not None:
            self._read_available()

        return self._snapshot(
            connected=self.client_sock is not None,
            buffer=bytes(self.recv_buffer),
        )

    def after_step(self, bindings) -> bool:
        var  = next(iter(self.matched_vars))
        send = bindings.get(f'{var}.out', '') or ''
        do_close = bool(bindings.get(f'{var}.close', False))

        if self.client_sock is not None and send:
            try:
                self.client_sock.sendall(str(send).encode('iso-8859-1'))
            except OSError:
                self._close_client()
                return True

        if self.client_sock is not None and do_close:
            self._close_client()

        return True

    # ── Internals ────────────────────────────────────────────────────────────

    def _try_accept(self) -> None:
        # Block briefly via select so a single-connection ping/curl actually lands.
        try:
            r, _, _ = select.select([self.listen_sock], [], [], _SELECT_TIMEOUT)
        except (OSError, ValueError):
            return
        if not r:
            return
        try:
            conn, _addr = self.listen_sock.accept()
        except (OSError, BlockingIOError):
            return
        conn.setblocking(False)
        self.client_sock = conn
        self.recv_buffer = bytearray()

    def _read_available(self) -> None:
        # Drain the socket (non-blocking) until empty or peer closed.
        while True:
            try:
                data = self.client_sock.recv(_RECV_CHUNK)
            except BlockingIOError:
                return
            except OSError as e:
                if e.errno in (errno.EAGAIN, errno.EWOULDBLOCK):
                    return
                self._close_client()
                return
            if not data:                  # peer closed
                # Keep the buffer so Evident sees the final bytes; mark conn None.
                # But for the simple demo we treat peer-close as conn-gone.
                self._close_client(keep_buffer=True)
                return
            self.recv_buffer.extend(data)

    def _close_client(self, *, keep_buffer: bool = False) -> None:
        if self.client_sock is not None:
            try: self.client_sock.close()
            except OSError: pass
            self.client_sock = None
        if not keep_buffer:
            self.recv_buffer = bytearray()

    def _snapshot(self, *, connected: bool, buffer: bytes) -> dict[str, Any]:
        var = next(iter(self.matched_vars))
        # Decode as latin-1 so any byte round-trips losslessly into a string.
        return {
            f'{var}.has_conn': connected,
            f'{var}.received': buffer.decode('iso-8859-1'),
        }
