"""Shared constants for the Evident Web IDE backend.

The paths and the global serialization lock that every server-side helper
needs. Kept in one tiny module so the helper modules (`render`, `analysis`,
`runtime_io`, `solve`, `smtlib_tools`) and `server` all import the same
values without reaching into each other.
"""
import os
import threading

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
VIZ = os.path.join(ROOT, "viz")
STATIC = os.path.join(os.path.dirname(os.path.abspath(__file__)), "static")
EVIDENT = os.path.join(ROOT, "runtime", "target", "release", "evident")

REACH_LIMIT = 400                              # bounded exploration cap for the live stats
_LOCK = threading.Lock()                       # matplotlib + z3 are not thread-safe; serialize
