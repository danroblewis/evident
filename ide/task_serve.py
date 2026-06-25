#!/usr/bin/env python3
"""Live web view for the Evident web-IDE task & concern ledger.

`task.py serve` starts a tiny stdlib HTTP server that renders the ledger
(ide/tasks.json) as a dashboard — status tiles, hero cards, and a modal detail
view — and polls `/api/data` every 5s so an open tab stays live as the worker and
critics mutate the ledger. This is the read-only presentation half; the CLI
command dispatch and the data layer live in `task.py` / `task_store.py`.

`cmd_serve` is wired into the CLI from task.py's argparse (`serve` subcommand).
"""
import json

from task_store import _load


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

