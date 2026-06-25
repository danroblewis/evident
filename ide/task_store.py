#!/usr/bin/env python3
"""Storage + model layer for the Evident web-IDE task & concern ledger.

The on-disk shape (ide/tasks.json) and the low-level read/modify/save helpers
live here; the CLI dispatch, command bodies, and the live web view live in
task.py. Keeping the ledger I/O in one small module means a tool that only wants
to read the data (the server, a future dashboard) can import it without dragging
in argparse and the HTML page.

Schema of ide/tasks.json:
  {"seq": <int>, "tasks": [<task>...], "concerns": [<concern>...]}

  TASK    — a unit of work; CLOSED only when the worker marks it `done` AND all
            three critics `approve` it (see _maybe_close).
  CONCERN — a worry raised by a critic or Iris; only its author may clear it.
"""
import json
import os
import sys
from datetime import datetime, timezone

DB = os.path.join(os.path.dirname(os.path.abspath(__file__)), "tasks.json")
CRITICS = ("ide-critic", "ide-critic-newcomer", "ide-critic-expert")
# Only the EXPERT's approval is REQUIRED to close a task (user decision 2026-06-23: Expert-only
# reviews; Marek + Sam are paused, brought back occasionally). All three may still approve/reopen,
# but the close gate is the expert alone — the worker is no longer blocked on a 3-critic quorum.
REQUIRED_CRITICS = ("ide-critic-expert",)
ROLES = set(CRITICS) | {"worker", "ide-feature-designer", "ide-architect"}


def _now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _load():
    if not os.path.exists(DB):
        return {"seq": 0, "tasks": [], "concerns": []}
    with open(DB) as f:
        return json.load(f)


def _save(db):
    with open(DB, "w") as f:
        json.dump(db, f, indent=2)
        f.write("\n")


def _nid(db):
    db["seq"] = db.get("seq", 0) + 1
    return db["seq"]


def _find(items, i):
    return next((x for x in items if x["id"] == i), None)


def _die(msg):
    print(f"task: {msg}", file=sys.stderr)
    sys.exit(2)


def _check_role(by, allowed, what):
    if by not in allowed:
        _die(f"{what} requires --by one of {sorted(allowed)} (got {by!r})")


def _maybe_close(t):
    if t["worker_done"] and all(c in t["approvals"] for c in REQUIRED_CRITICS):
        t["status"] = "closed"


# ── one-line CLI renderings of a task / concern (used by list + summary) ─────────────
def _task_line(t):
    badge = {"open": "○", "in_progress": "◐", "worker_done": "◑", "closed": "●"}.get(t["status"], "?")
    appr = "".join("✓" if c in t["approvals"] else "·" for c in CRITICS)
    tags = (" [" + ",".join(t["tags"]) + "]") if t.get("tags") else ""
    ro = f" reopened×{t['reopened']}" if t["reopened"] else ""
    return f"  {badge} #{t['id']:<3} [{t['status']:<11}] appr:{appr}{ro}{tags}  {t['title']}"


def _concern_line(c):
    badge = "!" if c["status"] == "open" else "✓"
    tt = f" (task #{c['task']})" if c.get("task") else ""
    return f"  {badge} #{c['id']:<3} [{c['status']:<7}] by {c['by']}{tt}  {c['title']}"
