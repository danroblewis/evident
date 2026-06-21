#!/usr/bin/env python3
"""Task & concern tracker for the Evident web-IDE goal loop.

Two object types live in ide/tasks.json:

  TASK     — a unit of work. It is CLOSED only by TWO acts: the worker marks it `done`
             AND all three critics `approve` it. A critic who finds a "done" task not
             actually finished may `reopen` it (and should log a concern saying why).

  CONCERN  — a worry raised by a critic or Iris while using the tool. ONLY its author may
             `clear` it. The worker reads concerns and creates tasks to resolve them, but
             the worker NEVER clears a concern — that is the author's call once satisfied.

Roles:
  worker                — the main agent (adds tasks, starts/finishes them, files concerns
                          as tasks; may NOT approve or clear critics' concerns).
  ide-critic            \\
  ide-critic-newcomer    >  the three CRITICS — the only roles that approve/reopen tasks;
  ide-critic-expert     /   each may also add tasks and concerns, and clear its OWN concerns.
  ide-feature-designer  — Iris; may add tasks and concerns and clear her own concerns.

A task needs ALL THREE critic approvals (plus the worker's `done`) to close.

Usage (run from the repo root):
  python3 ide/task.py add "<title>" [--detail "..."] [--by ROLE] [--tag T] [--from-concern ID]
  python3 ide/task.py concern "<title>" --by ROLE [--detail "..."] [--task ID]
  python3 ide/task.py list [--concerns] [--open|--needs-approval|--closed] [--by ROLE] [--json]
  python3 ide/task.py show ID
  python3 ide/task.py start ID [--by ROLE]                 # -> in_progress
  python3 ide/task.py done ID [--note "..."]               # worker: -> awaiting approvals
  python3 ide/task.py approve ID --by CRITIC [--note "..."]
  python3 ide/task.py reopen ID --by CRITIC [--concern "..."]
  python3 ide/task.py clear-concern ID --by AUTHOR
  python3 ide/task.py summary
"""
import argparse
import json
import os
import sys
from datetime import datetime, timezone

DB = os.path.join(os.path.dirname(os.path.abspath(__file__)), "tasks.json")
CRITICS = ("ide-critic", "ide-critic-newcomer", "ide-critic-expert")
ROLES = set(CRITICS) | {"worker", "ide-feature-designer"}


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
    if t["worker_done"] and all(c in t["approvals"] for c in CRITICS):
        t["status"] = "closed"


# ── commands ──────────────────────────────────────────────────────────────────────
def cmd_add(db, a):
    t = {"id": _nid(db), "title": a.title, "detail": a.detail or "", "status": "open",
         "created_by": a.by, "created": _now(), "worker_done": False, "approvals": [],
         "reopened": 0, "tags": ([a.tag] if a.tag else []), "log": []}
    if a.from_concern is not None:
        c = _find(db["concerns"], a.from_concern)
        if not c:
            _die(f"no concern #{a.from_concern}")
        t["addresses_concern"] = a.from_concern
        t["log"].append({"at": _now(), "by": a.by, "act": f"created to address concern #{a.from_concern}"})
    db["tasks"].append(t)
    _save(db)
    print(f"task #{t['id']} added: {t['title']}")


def cmd_concern(db, a):
    _check_role(a.by, ROLES, "concern")
    c = {"id": _nid(db), "title": a.title, "detail": a.detail or "", "by": a.by,
         "status": "open", "task": a.task, "created": _now()}
    db["concerns"].append(c)
    _save(db)
    print(f"concern #{c['id']} logged by {a.by}: {c['title']}")


def cmd_start(db, a):
    t = _find(db["tasks"], a.id) or _die(f"no task #{a.id}")
    t["status"] = "in_progress"
    t["log"].append({"at": _now(), "by": a.by, "act": "started"})
    _save(db)
    print(f"task #{a.id} -> in_progress")


def cmd_done(db, a):
    t = _find(db["tasks"], a.id) or _die(f"no task #{a.id}")
    t["worker_done"] = True
    t["status"] = "worker_done"
    if a.note:
        t["log"].append({"at": _now(), "by": "worker", "act": "done", "note": a.note})
    _maybe_close(t)
    _save(db)
    need = [c for c in CRITICS if c not in t["approvals"]]
    print(f"task #{a.id} marked done by worker." +
          (f" awaiting approval from: {', '.join(need)}" if t["status"] != "closed"
           else " ALL approvals in — CLOSED."))


def cmd_approve(db, a):
    _check_role(a.by, set(CRITICS), "approve")
    t = _find(db["tasks"], a.id) or _die(f"no task #{a.id}")
    if not t["worker_done"]:
        _die(f"task #{a.id} is not worker-done yet — nothing to approve")
    if a.by not in t["approvals"]:
        t["approvals"].append(a.by)
    t["log"].append({"at": _now(), "by": a.by, "act": "approved", "note": a.note or ""})
    _maybe_close(t)
    _save(db)
    need = [c for c in CRITICS if c not in t["approvals"]]
    print(f"task #{a.id} approved by {a.by}." +
          (" CLOSED — all three critics approved + worker done." if t["status"] == "closed"
           else f" still awaiting: {', '.join(need)}"))


def cmd_reopen(db, a):
    _check_role(a.by, set(CRITICS), "reopen")
    t = _find(db["tasks"], a.id) or _die(f"no task #{a.id}")
    t["worker_done"] = False
    t["approvals"] = []
    t["status"] = "in_progress" if t["reopened"] else "open"
    t["reopened"] += 1
    t["log"].append({"at": _now(), "by": a.by, "act": "reopened", "note": a.concern or ""})
    _save(db)
    msg = f"task #{a.id} REOPENED by {a.by} (approvals reset)."
    if a.concern:
        c = {"id": _nid(db), "title": a.concern, "detail": "", "by": a.by,
             "status": "open", "task": a.id, "created": _now()}
        db["concerns"].append(c)
        _save(db)
        msg += f" concern #{c['id']} logged."
    print(msg)


def cmd_clear_concern(db, a):
    c = _find(db["concerns"], a.id) or _die(f"no concern #{a.id}")
    if a.by != c["by"]:
        _die(f"only the author ({c['by']}) may clear concern #{a.id} (got {a.by!r})")
    c["status"] = "cleared"
    c["cleared"] = _now()
    _save(db)
    print(f"concern #{a.id} cleared by {a.by}")


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


def cmd_list(db, a):
    if a.json:
        print(json.dumps(db, indent=2))
        return
    if a.concerns:
        items = db["concerns"]
        if a.open:
            items = [c for c in items if c["status"] == "open"]
        if a.by:
            items = [c for c in items if c["by"] == a.by]
        print(f"CONCERNS ({len(items)}):")
        for c in items:
            print(_concern_line(c))
        return
    items = db["tasks"]
    if a.open:
        items = [t for t in items if t["status"] != "closed"]
    if a.needs_approval:
        items = [t for t in items if t["status"] == "worker_done"]
    if a.closed:
        items = [t for t in items if t["status"] == "closed"]
    if a.by:
        items = [t for t in items if t["created_by"] == a.by]
    print(f"TASKS ({len(items)}):  ○ open  ◐ in-progress  ◑ awaiting-approval  ● closed   appr=[critic,newcomer,expert]")
    for t in items:
        print(_task_line(t))


def cmd_show(db, a):
    t = _find(db["tasks"], a.id)
    if t:
        print(json.dumps(t, indent=2))
        return
    c = _find(db["concerns"], a.id)
    print(json.dumps(c, indent=2) if c else f"no task/concern #{a.id}")


def cmd_summary(db, a):
    ts = db["tasks"]
    by_status = {}
    for t in ts:
        by_status[t["status"]] = by_status.get(t["status"], 0) + 1
    oc = [c for c in db["concerns"] if c["status"] == "open"]
    print("SUMMARY")
    print(f"  tasks: {len(ts)}  ({', '.join(f'{k}={v}' for k, v in sorted(by_status.items())) or 'none'})")
    print(f"  awaiting approval (worker-done, not yet 3 approvals): "
          f"{sum(1 for t in ts if t['status'] == 'worker_done')}")
    print(f"  open concerns: {len(oc)}")
    for c in oc:
        print(_concern_line(c))


def main():
    p = argparse.ArgumentParser(prog="task", description="Evident web-IDE task & concern tracker")
    sub = p.add_subparsers(dest="cmd", required=True)

    s = sub.add_parser("add"); s.add_argument("title"); s.add_argument("--detail")
    s.add_argument("--by", default="worker"); s.add_argument("--tag")
    s.add_argument("--from-concern", type=int); s.set_defaults(fn=cmd_add)

    s = sub.add_parser("concern"); s.add_argument("title"); s.add_argument("--by", required=True)
    s.add_argument("--detail"); s.add_argument("--task", type=int); s.set_defaults(fn=cmd_concern)

    s = sub.add_parser("start"); s.add_argument("id", type=int)
    s.add_argument("--by", default="worker"); s.set_defaults(fn=cmd_start)

    s = sub.add_parser("done"); s.add_argument("id", type=int); s.add_argument("--note")
    s.set_defaults(fn=cmd_done)

    s = sub.add_parser("approve"); s.add_argument("id", type=int); s.add_argument("--by", required=True)
    s.add_argument("--note"); s.set_defaults(fn=cmd_approve)

    s = sub.add_parser("reopen"); s.add_argument("id", type=int); s.add_argument("--by", required=True)
    s.add_argument("--concern"); s.set_defaults(fn=cmd_reopen)

    s = sub.add_parser("clear-concern"); s.add_argument("id", type=int)
    s.add_argument("--by", required=True); s.set_defaults(fn=cmd_clear_concern)

    s = sub.add_parser("list"); s.add_argument("--concerns", action="store_true")
    s.add_argument("--open", action="store_true"); s.add_argument("--needs-approval", action="store_true")
    s.add_argument("--closed", action="store_true"); s.add_argument("--by"); s.add_argument("--json", action="store_true")
    s.set_defaults(fn=cmd_list)

    s = sub.add_parser("show"); s.add_argument("id", type=int); s.set_defaults(fn=cmd_show)
    s = sub.add_parser("summary"); s.set_defaults(fn=cmd_summary)

    a = p.parse_args()
    db = _load()
    a.fn(db, a)


if __name__ == "__main__":
    main()
