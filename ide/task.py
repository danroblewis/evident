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
  python3 ide/task.py serve [--host 127.0.0.1] [--port 8787]   # live web view, reloads every 5s
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


_PAGE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Evident web-IDE — task & concern ledger</title>
<style>
  :root {
    --bg: #0f1115; --panel: #171a21; --line: #262b36; --fg: #e6e9ef;
    --dim: #8b93a3; --accent: #6ea8fe;
    --open: #9aa4b2; --prog: #d8a657; --wait: #7daea3; --closed: #4caf50;
    --warn: #e06c75; --ok: #4caf50;
  }
  * { box-sizing: border-box; }
  body { margin: 0; background: var(--bg); color: var(--fg);
         font: 14px/1.5 -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; }
  header { padding: 18px 28px; border-bottom: 1px solid var(--line);
           display: flex; align-items: baseline; gap: 16px; }
  header h1 { font-size: 18px; margin: 0; font-weight: 600; }
  header .meta { color: var(--dim); font-size: 12px; }
  .wrap { max-width: 1500px; margin: 0 auto; padding: 24px 28px 64px; }
  h2 { font-size: 13px; text-transform: uppercase; letter-spacing: .08em;
       color: var(--dim); margin: 0; }
  .sec-head { display: flex; align-items: baseline; gap: 12px; margin: 28px 0 14px; }
  .toggle { background: var(--panel); border: 1px solid var(--line); color: var(--fg);
            border-radius: 8px; padding: 5px 12px; font-size: 12px; cursor: pointer;
            transition: border-color .12s ease, background .12s ease; }
  .toggle:hover { border-color: #38414f; background: #1d212a; }
  .summary { display: flex; flex-wrap: wrap; gap: 10px; margin-bottom: 8px; }
  .pill { background: var(--panel); border: 1px solid var(--line); border-radius: 999px;
          padding: 4px 12px; font-size: 12px; color: var(--dim); }
  .pill b { color: var(--fg); }

  /* hero-card flex flow: cards grow to fill the row, wrap to the next */
  .grid { display: flex; flex-flow: row wrap; gap: 16px; align-items: stretch; }
  .card { background: var(--panel); border: 1px solid var(--line); border-radius: 14px;
          padding: 18px 20px 16px; position: relative; overflow: hidden;
          display: flex; flex-flow: column; gap: 10px;
          flex: 1 1 360px; min-width: 320px; max-width: 560px; cursor: pointer;
          box-shadow: 0 1px 0 rgba(255,255,255,.02), 0 8px 24px rgba(0,0,0,.28);
          transition: transform .12s ease, box-shadow .12s ease, border-color .12s ease; }
  .card:hover { transform: translateY(-2px); border-color: #38414f;
                box-shadow: 0 10px 30px rgba(0,0,0,.4); }
  /* accent strip down the left edge, colored by status */
  .card::before { content: ""; position: absolute; left: 0; top: 0; bottom: 0; width: 4px;
                  background: var(--line); }
  .card.s-open::before { background: var(--open); }
  .card.s-in_progress::before { background: var(--prog); }
  .card.s-worker_done::before { background: var(--wait); }
  .card.s-closed::before { background: var(--closed); }
  .card.c-open::before { background: var(--warn); }

  .card .top { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .id { color: var(--dim); font-variant-numeric: tabular-nums; font-size: 12px; }
  .title { font-weight: 650; font-size: 16px; line-height: 1.3; flex-basis: 100%;
           display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical;
           overflow: hidden; }
  .badge { font-size: 11px; padding: 2px 8px; border-radius: 6px; border: 1px solid var(--line);
           text-transform: uppercase; letter-spacing: .04em; }
  .st-open { color: var(--open); } .st-in_progress { color: var(--prog); }
  .st-worker_done { color: var(--wait); } .st-closed { color: var(--ok); border-color: var(--ok); }
  .appr { font-family: ui-monospace, Menlo, monospace; font-size: 12px; color: var(--dim); margin-left: auto; }
  .appr .yes { color: var(--ok); } .appr .no { color: var(--dim); }
  .detail { color: var(--dim); white-space: pre-wrap; }
  .tags { display: flex; flex-wrap: wrap; gap: 5px; }
  .tag { font-size: 11px; background: #20242e; border-radius: 5px; padding: 1px 7px; color: var(--accent); }
  .concern.cleared { opacity: .55; }
  .concern .who { color: var(--dim); font-size: 12px; }
  .log { margin-top: auto; border-top: 1px solid var(--line); padding-top: 10px;
         display: flex; flex-flow: column; gap: 2px; }
  .log .row { color: var(--dim); font-size: 12px; }
  .log .row .at { color: #5b6270; }
  .empty { color: var(--dim); font-style: italic; }
  a.anchor { color: var(--accent); text-decoration: none; }

  /* modal */
  .overlay { position: fixed; inset: 0; background: rgba(0,0,0,.6); backdrop-filter: blur(2px);
             display: none; align-items: flex-start; justify-content: center;
             padding: 6vh 20px; overflow-y: auto; z-index: 50; }
  .overlay.show { display: flex; }
  .modal { background: var(--panel); border: 1px solid var(--line); border-radius: 16px;
           width: 100%; max-width: 640px; padding: 24px 26px; position: relative;
           box-shadow: 0 20px 60px rgba(0,0,0,.55); }
  .modal .x { position: absolute; top: 14px; right: 16px; cursor: pointer; color: var(--dim);
              font-size: 22px; line-height: 1; background: none; border: none; }
  .modal .x:hover { color: var(--fg); }
  .modal h3 { margin: 0 26px 12px 0; font-size: 19px; line-height: 1.3; }
  .modal .meta-row { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; margin-bottom: 16px; }
  .modal .field { margin: 14px 0; }
  .modal .field .k { font-size: 11px; text-transform: uppercase; letter-spacing: .07em;
                     color: var(--dim); margin-bottom: 4px; }
  .modal .field .v { white-space: pre-wrap; }
  .modal .log { margin-top: 16px; }

  /* mobile-adaptive */
  @media (max-width: 640px) {
    header { padding: 14px 16px; flex-direction: column; gap: 4px; }
    header h1 { font-size: 16px; }
    .wrap { padding: 16px 14px 48px; }
    h2 { margin: 20px 0 10px; }
    .grid { gap: 12px; }
    .card { flex-basis: 100%; min-width: 0; max-width: none; padding: 11px 14px;
            border-radius: 10px; gap: 6px; }
    .card:hover { transform: none; }
    /* compact one-line hero cards on mobile: clamp title, drop tags + log */
    .title { font-size: 14px; -webkit-line-clamp: 1; }
    .card .tags, .card .log { display: none; }
    .overlay { padding: 0; align-items: stretch; }
    .modal { max-width: none; min-height: 100%; border-radius: 0; padding: 20px 18px; }
  }
</style>
</head>
<body>
<header>
  <h1>Evident web-IDE ledger</h1>
  <span class="meta" id="meta">loading…</span>
</header>
<div class="wrap">
  <div class="summary" id="summary"></div>
  <div class="sec-head">
    <h2>Tasks</h2>
    <button class="toggle" id="toggleDone">Show completed</button>
  </div>
  <div class="grid" id="tasks"></div>
  <div class="sec-head"><h2>Open concerns</h2></div>
  <div class="grid" id="concerns"></div>
</div>
<div class="overlay" id="overlay">
  <div class="modal" id="modal" role="dialog" aria-modal="true">
    <button class="x" id="modalClose" aria-label="close">×</button>
    <div id="modalBody"></div>
  </div>
</div>
<script>
const CRITICS = ["ide-critic", "ide-critic-newcomer", "ide-critic-expert"];
const SHORT = {"ide-critic": "critic", "ide-critic-newcomer": "newcomer", "ide-critic-expert": "expert"};
function esc(s) {
  return String(s == null ? "" : s).replace(/[&<>"]/g, c => (
    {"&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;"}[c]));
}
function appr(t) {
  return CRITICS.map(c => {
    const ok = (t.approvals || []).includes(c);
    return `<span class="${ok ? "yes" : "no"}" title="${esc(c)}">${SHORT[c]}${ok ? "✓" : "·"}</span>`;
  }).join(" ");
}
function logHtml(item) {
  return (item.log || []).map(l =>
    `<div class="row"><span class="at">${esc((l.at||"").replace("T"," ").replace("Z",""))}</span> — `
    + `${esc(l.by)} <b>${esc(l.act)}</b>${l.note ? ": " + esc(l.note) : ""}</div>`).join("");
}
let STATE = {tasks: [], concerns: []};
function taskCard(t) {
  const tags = (t.tags || []).map(x => `<span class="tag">${esc(x)}</span>`).join("");
  const ro = t.reopened ? ` <span class="id">reopened×${t.reopened}</span>` : "";
  const log = logHtml(t);
  return `<div class="card s-${esc(t.status)}" data-kind="task" data-id="${t.id}">
    <div class="top">
      <span class="id">#${t.id}</span>
      <span class="badge st-${esc(t.status)}">${esc(t.status)}</span>${ro}
      <span class="appr">${appr(t)}</span>
    </div>
    <div class="title">${esc(t.title)}</div>
    ${tags ? `<div class="tags">${tags}</div>` : ""}
    ${log ? `<div class="log">${log}</div>` : ""}
  </div>`;
}
function concernCard(c) {
  const cleared = c.status !== "open";
  return `<div class="card concern ${cleared ? "cleared" : "c-open"}" data-kind="concern" data-id="${c.id}">
    <div class="top">
      <span class="id">#${c.id}</span>
      <span class="badge">${esc(c.status)}</span>
      <span class="who" style="margin-left:auto">by ${esc(c.by)}${c.task ? " · task #" + c.task : ""}</span>
    </div>
    <div class="title">${esc(c.title)}</div>
  </div>`;
}
function field(k, v) {
  return v ? `<div class="field"><div class="k">${esc(k)}</div><div class="v">${esc(v)}</div></div>` : "";
}
function taskModal(t) {
  const log = logHtml(t);
  return `<h3>${esc(t.title)}</h3>
    <div class="meta-row">
      <span class="id">#${t.id}</span>
      <span class="badge st-${esc(t.status)}">${esc(t.status)}</span>
      ${t.reopened ? `<span class="id">reopened×${t.reopened}</span>` : ""}
      <span class="appr">${appr(t)}</span>
    </div>
    ${field("Detail", t.detail)}
    ${field("Created by", `${t.created_by || "?"}${t.created ? " · " + t.created.replace("T"," ").replace("Z","") : ""}`)}
    ${t.addresses_concern ? field("Addresses", "concern #" + t.addresses_concern) : ""}
    ${(t.tags||[]).length ? `<div class="field"><div class="k">Tags</div><div class="tags">${(t.tags||[]).map(x=>`<span class="tag">${esc(x)}</span>`).join("")}</div></div>` : ""}
    ${field("Approvals", (t.approvals||[]).join(", ") || "none yet")}
    ${log ? `<div class="field"><div class="k">Activity</div><div class="log">${log}</div></div>` : ""}`;
}
function concernModal(c) {
  return `<h3>${esc(c.title)}</h3>
    <div class="meta-row">
      <span class="id">#${c.id}</span>
      <span class="badge">${esc(c.status)}</span>
      <span class="who">by ${esc(c.by)}</span>
    </div>
    ${field("Detail", c.detail)}
    ${field("Raised on", c.created ? c.created.replace("T"," ").replace("Z","") : "")}
    ${c.task ? field("Linked task", "#" + c.task) : ""}
    ${c.cleared ? field("Cleared", c.cleared.replace("T"," ").replace("Z","")) : ""}`;
}
function openModal(kind, id) {
  const item = (kind === "task" ? STATE.tasks : STATE.concerns).find(x => x.id === id);
  if (!item) return;
  document.getElementById("modalBody").innerHTML =
    kind === "task" ? taskModal(item) : concernModal(item);
  document.getElementById("overlay").classList.add("show");
}
function closeModal() { document.getElementById("overlay").classList.remove("show"); }
document.getElementById("modalClose").addEventListener("click", closeModal);
document.getElementById("overlay").addEventListener("click", e => {
  if (e.target.id === "overlay") closeModal();
});
document.addEventListener("keydown", e => { if (e.key === "Escape") closeModal(); });
["concerns", "tasks"].forEach(gid => document.getElementById(gid).addEventListener("click", e => {
  const card = e.target.closest(".card");
  if (card && card.dataset.id) openModal(card.dataset.kind, Number(card.dataset.id));
}));
let showCompleted = false;
const ORDER = {in_progress: 0, worker_done: 1, open: 2, closed: 3};
function renderTasks() {
  const all = STATE.tasks || [];
  const doneCount = all.filter(t => t.status === "closed").length;
  // active view hides completed; completed view shows only those
  const shown = all.filter(t => showCompleted ? t.status === "closed" : t.status !== "closed");
  const sorted = shown.slice().sort((a, b) =>
    (ORDER[a.status] ?? 9) - (ORDER[b.status] ?? 9) || a.id - b.id);
  document.getElementById("tasks").innerHTML =
    sorted.length ? sorted.map(taskCard).join("")
                  : `<div class="empty">${showCompleted ? "no completed tasks" : "no active tasks"}</div>`;
  const btn = document.getElementById("toggleDone");
  btn.textContent = showCompleted ? "Show active" : `Show completed (${doneCount})`;
}
document.getElementById("toggleDone").addEventListener("click", () => {
  showCompleted = !showCompleted;
  renderTasks();
});
function renderConcerns() {
  const openC = (STATE.concerns || []).filter(c => c.status === "open");
  document.getElementById("concerns").innerHTML =
    openC.length ? openC.map(concernCard).join("")
                 : `<div class="empty">no open concerns</div>`;
}
async function refresh() {
  try {
    const db = await (await fetch("/api/data", {cache: "no-store"})).json();
    const tasks = db.tasks || [], concerns = db.concerns || [];
    STATE = {tasks, concerns};
    const byStatus = {};
    tasks.forEach(t => byStatus[t.status] = (byStatus[t.status] || 0) + 1);
    const openC = concerns.filter(c => c.status === "open");
    document.getElementById("summary").innerHTML =
      `<span class="pill"><b>${tasks.length}</b> tasks</span>`
      + ["open", "in_progress", "worker_done", "closed"].map(s =>
          `<span class="pill">${s}: <b>${byStatus[s] || 0}</b></span>`).join("")
      + `<span class="pill"><b>${openC.length}</b> open concerns</span>`;
    renderTasks();
    renderConcerns();
    const now = new Date().toLocaleTimeString();
    document.getElementById("meta").textContent =
      `${tasks.length} tasks · ${openC.length} open concerns · updated ${now}`;
  } catch (e) {
    document.getElementById("meta").textContent = "reload failed: " + e;
  }
}
refresh();
setInterval(refresh, 5000);
</script>
</body>
</html>
"""


def cmd_serve(db, a):
    from http.server import BaseHTTPRequestHandler, HTTPServer

    class Handler(BaseHTTPRequestHandler):
        def _send(self, code, body, ctype):
            data = body.encode("utf-8") if isinstance(body, str) else body
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def do_GET(self):
            if self.path == "/api/data":
                self._send(200, json.dumps(_load()), "application/json")
            elif self.path in ("/", "/index.html"):
                self._send(200, _PAGE, "text/html; charset=utf-8")
            elif self.path == "/favicon.ico":
                self.send_response(204); self.end_headers()
            else:
                self._send(404, "not found", "text/plain")

        def log_message(self, *args):
            pass  # quiet

    srv = HTTPServer((a.host, a.port), Handler)
    url = f"http://{a.host}:{a.port}/"
    print(f"serving task ledger at {url}  (reloads every 5s; Ctrl-C to stop)")
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        print("\nstopped.")
        srv.server_close()


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

    s = sub.add_parser("serve"); s.add_argument("--host", default="127.0.0.1")
    s.add_argument("--port", type=int, default=8787); s.set_defaults(fn=cmd_serve)

    a = p.parse_args()
    if a.cmd == "serve":
        a.fn(_load(), a)              # long-lived reader; re-reads on its own — no lock
        return
    # Serialize concurrent writers (the worker + the three critics all mutate the ledger):
    # hold an exclusive lock across the whole load → modify → save so no write clobbers another.
    import fcntl
    with open(DB + ".lock", "w") as _lk:
        fcntl.flock(_lk, fcntl.LOCK_EX)
        db = _load()
        a.fn(db, a)


if __name__ == "__main__":
    main()
