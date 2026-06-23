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

# Storage + model layer (ledger shape, load/save/find, role/close rules).
from task_store import (
    DB, CRITICS, ROLES,
    _now, _load, _save, _nid, _find, _die, _check_role, _maybe_close,
)


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
  .wrap { max-width: 1500px; margin: 0 auto; padding: 28px 28px 64px; }
  h2 { font-size: 13px; text-transform: uppercase; letter-spacing: .08em;
       color: var(--dim); margin: 0; }
  .sec-head { display: flex; align-items: baseline; gap: 12px; margin: 30px 0 14px; }
  .count { color: var(--dim); font-size: 12px; }
  .toggle { background: var(--panel); border: 1px solid var(--line); color: var(--fg);
            border-radius: 8px; padding: 5px 12px; font-size: 12px; cursor: pointer;
            margin-left: auto; transition: border-color .12s ease, background .12s ease; }
  .toggle:hover { border-color: #38414f; background: #1d212a; }

  /* ── status dashboard ───────────────────────────────────────── */
  .dash { background: var(--panel); border: 1px solid var(--line); border-radius: 18px;
          padding: 22px 24px; box-shadow: 0 8px 24px rgba(0,0,0,.28); }
  .dash-head { display: flex; align-items: baseline; justify-content: space-between;
               gap: 12px; margin-bottom: 18px; }
  .dash-head h1 { font-size: 17px; margin: 0; font-weight: 650; letter-spacing: .01em; }
  .updated { color: var(--dim); font-size: 12px; }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: 14px; }
  .stat { background: var(--bg); border: 1px solid var(--line); border-radius: 13px;
          padding: 16px 18px; position: relative; overflow: hidden; }
  .stat::before { content: ""; position: absolute; left: 0; top: 0; bottom: 0; width: 4px; background: var(--line); }
  .stat.open::before { background: var(--open); }
  .stat.prog::before { background: var(--prog); }
  .stat.wait::before { background: var(--wait); }
  .stat.closed::before { background: var(--closed); }
  .stat.warn::before { background: var(--warn); }
  .stat .num { font-size: 40px; font-weight: 700; line-height: 1; font-variant-numeric: tabular-nums; }
  .stat .lbl { margin-top: 9px; font-size: 12px; text-transform: uppercase;
               letter-spacing: .06em; color: var(--dim); }
  .stat .sub { font-size: 11px; color: var(--dim); margin-top: 3px; }
  /* shared status color keys (bar segments + legend dots + tiles) */
  .k-open { background: var(--open); } .k-in_progress { background: var(--prog); }
  .k-worker_done { background: var(--wait); } .k-closed { background: var(--closed); }
  .bar { display: flex; height: 12px; border-radius: 6px; overflow: hidden;
         margin: 22px 0 12px; background: var(--bg); }
  .bar > span { display: block; min-width: 2px; }
  .bar-legend { display: flex; flex-wrap: wrap; gap: 16px; font-size: 12px; color: var(--dim); }
  .dot { display: inline-block; width: 9px; height: 9px; border-radius: 2px;
         margin-right: 6px; vertical-align: middle; }
  .labels { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 18px;
            padding-top: 16px; border-top: 1px solid var(--line); }
  .chip { background: var(--bg); border: 1px solid var(--line); border-radius: 999px;
          padding: 4px 11px; font-size: 12px; color: var(--dim); }
  .chip b { color: var(--accent); margin-left: 5px; }

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
  .badge { font-size: 12px; padding: 3px 10px; border-radius: 7px; border: 1px solid var(--line);
           text-transform: uppercase; letter-spacing: .04em; font-weight: 600; }
  .st-open { color: var(--open); border-color: rgba(154,164,178,.35); background: rgba(154,164,178,.10); }
  .st-in_progress { color: var(--prog); border-color: rgba(216,166,87,.4); background: rgba(216,166,87,.12); }
  .st-worker_done { color: var(--wait); border-color: rgba(125,174,163,.4); background: rgba(125,174,163,.12); }
  .st-closed { color: var(--ok); border-color: rgba(76,175,80,.45); background: rgba(76,175,80,.12); }
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
    .wrap { padding: 16px 14px 48px; }
    .dash { padding: 16px 16px; border-radius: 14px; }
    .stats { grid-template-columns: repeat(2, 1fr); gap: 10px; }
    .stat { padding: 12px 14px; }
    .stat .num { font-size: 30px; }
    .sec-head { margin: 22px 0 10px; }
    .grid { gap: 12px; }
    .card { flex-basis: 100%; min-width: 0; max-width: none; padding: 11px 14px;
            border-radius: 10px; gap: 6px; }
    .card:hover { transform: none; }
    /* compact one-line hero cards on mobile: clamp title, drop tags */
    .title { font-size: 14px; -webkit-line-clamp: 1; }
    .card .tags { display: none; }
    .overlay { padding: 0; align-items: stretch; }
    .modal { max-width: none; min-height: 100%; border-radius: 0; padding: 20px 18px; }
  }
</style>
</head>
<body>
<div class="wrap">
  <div class="dash">
    <div class="dash-head">
      <h1>Task ledger</h1>
      <span class="updated" id="updated">loading…</span>
    </div>
    <div class="stats" id="stats"></div>
    <div class="bar" id="bar"></div>
    <div class="bar-legend" id="barLegend"></div>
    <div class="labels" id="labels"></div>
  </div>

  <div class="sec-head"><h2>Active</h2><span class="count" id="activeCount"></span></div>
  <div class="grid" id="tasks"></div>

  <div class="sec-head"><h2>Open concerns</h2><span class="count" id="concernCount"></span></div>
  <div class="grid" id="concerns"></div>

  <div class="sec-head">
    <h2>Done &amp; completed</h2>
    <button class="toggle" id="toggleDone">Show</button>
  </div>
  <div class="grid" id="done" hidden></div>
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
  return `<div class="card s-${esc(t.status)}" data-kind="task" data-id="${t.id}">
    <div class="top">
      <span class="id">#${t.id}</span>
      <span class="badge st-${esc(t.status)}">${esc(t.status)}</span>${ro}
      <span class="appr">${appr(t)}</span>
    </div>
    <div class="title">${esc(t.title)}</div>
    ${tags ? `<div class="tags">${tags}</div>` : ""}
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
["tasks", "concerns", "done"].forEach(gid => document.getElementById(gid).addEventListener("click", e => {
  const card = e.target.closest(".card");
  if (card && card.dataset.id) openModal(card.dataset.kind, Number(card.dataset.id));
}));

const ORDER = {in_progress: 0, open: 1, worker_done: 2, closed: 3};
const STATUSES = ["open", "in_progress", "worker_done", "closed"];
const STAT_DEFS = [
  {key: "open",        lbl: "Open",              cls: "open"},
  {key: "in_progress", lbl: "In progress",       cls: "prog"},
  {key: "worker_done", lbl: "Awaiting approval", cls: "wait"},
  {key: "closed",      lbl: "Completed",         cls: "closed"},
];
const LEG = {open: "Open", in_progress: "In progress", worker_done: "Awaiting", closed: "Completed"};

function renderDashboard() {
  const tasks = STATE.tasks || [], concerns = STATE.concerns || [];
  const c = {open: 0, in_progress: 0, worker_done: 0, closed: 0};
  tasks.forEach(t => { if (c[t.status] != null) c[t.status]++; });
  const openC = concerns.filter(x => x.status === "open").length;
  const total = tasks.length || 1;
  const pct = Math.round((c.closed / total) * 100);

  const tiles = STAT_DEFS.map(s =>
    `<div class="stat ${s.cls}"><div class="num">${c[s.key]}</div><div class="lbl">${s.lbl}</div>`
    + (s.key === "closed" ? `<div class="sub">${pct}% of all</div>` : "") + `</div>`).join("")
    + `<div class="stat warn"><div class="num">${openC}</div><div class="lbl">Open concerns</div></div>`;
  document.getElementById("stats").innerHTML = tiles;

  document.getElementById("bar").innerHTML = STATUSES.map(s => {
    const w = (c[s] / total) * 100;
    return w > 0 ? `<span class="k-${s}" style="width:${w}%" title="${LEG[s]}: ${c[s]}"></span>` : "";
  }).join("");
  document.getElementById("barLegend").innerHTML = STATUSES.map(s =>
    `<span><span class="dot k-${s}"></span>${LEG[s]} ${c[s]}</span>`).join("");

  const tagCount = {};
  tasks.forEach(t => (t.tags || []).forEach(tg => tagCount[tg] = (tagCount[tg] || 0) + 1));
  const tags = Object.entries(tagCount).sort((a, b) => b[1] - a[1]);
  document.getElementById("labels").innerHTML = tags.length
    ? tags.map(([tg, n]) => `<span class="chip">${esc(tg)}<b>${n}</b></span>`).join("")
    : `<span class="count">no labels yet</span>`;

  document.getElementById("updated").textContent = "updated " + new Date().toLocaleTimeString();
}
function byStatusThenId(a, b) { return (ORDER[a.status] ?? 9) - (ORDER[b.status] ?? 9) || a.id - b.id; }
function renderActive() {
  const active = (STATE.tasks || [])
    .filter(t => t.status === "open" || t.status === "in_progress").sort(byStatusThenId);
  document.getElementById("tasks").innerHTML =
    active.length ? active.map(taskCard).join("") : `<div class="empty">no active tasks</div>`;
  document.getElementById("activeCount").textContent = `${active.length} task${active.length === 1 ? "" : "s"}`;
}
function renderConcerns() {
  const openC = (STATE.concerns || []).filter(c => c.status === "open");
  document.getElementById("concerns").innerHTML =
    openC.length ? openC.map(concernCard).join("") : `<div class="empty">no open concerns</div>`;
  document.getElementById("concernCount").textContent = `${openC.length}`;
}
let showDone = false;
function renderDone() {
  const done = (STATE.tasks || [])
    .filter(t => t.status === "worker_done" || t.status === "closed")
    .sort((a, b) => (ORDER[a.status] - ORDER[b.status]) || b.id - a.id);  // newest first within group
  const el = document.getElementById("done");
  el.hidden = !showDone;
  el.innerHTML = done.length ? done.map(taskCard).join("") : `<div class="empty">none yet</div>`;
  document.getElementById("toggleDone").textContent = `${showDone ? "Hide" : "Show"} (${done.length})`;
}
document.getElementById("toggleDone").addEventListener("click", () => {
  showDone = !showDone;
  renderDone();
});
async function refresh() {
  try {
    const db = await (await fetch("/api/data", {cache: "no-store"})).json();
    STATE = {tasks: db.tasks || [], concerns: db.concerns || []};
    renderDashboard();
    renderActive();
    renderConcerns();
    renderDone();
  } catch (e) {
    document.getElementById("updated").textContent = "reload failed: " + e;
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
