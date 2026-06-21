"use strict";

// LaTeX-style Unicode input: type \word + a non-letter, get the operator.
const UNI = {
  in: "∈", notin: "∉", forall: "∀", exists: "∃", implies: "⇒", impliedby: "⟸",
  mapsto: "↦", to: "→", langle: "⟨", rangle: "⟩", leq: "≤", le: "≤", geq: "≥",
  ge: "≥", neq: "≠", ne: "≠", Delta: "Δ", neg: "¬", land: "∧", lor: "∨",
  cup: "∪", cap: "∩", times: "×", cdot: "·", subseteq: "⊆", emptyset: "∅",
};

const DEFAULT_PROGRAM =
`fsm accumulate
    i   ∈ Int = (is_first_tick ? 0 : (_i ≥ 5 ? _i : _i + 1))
    sum ∈ Int = (is_first_tick ? 0 : (_i ≥ 5 ? _sum : _sum + _i))`;

const $ = (s) => document.querySelector(s);

const cm = CodeMirror.fromTextArea($("#code"), {
  theme: "dracula", lineNumbers: true, lineWrapping: false,
  viewportMargin: Infinity, value: DEFAULT_PROGRAM,
});
cm.setValue(DEFAULT_PROGRAM);

// --- Unicode input method ---------------------------------------------------------
cm.on("inputRead", (cm_, change) => {
  const typed = change.text.join("");
  if (!typed || /[a-zA-Z\\]/.test(typed[typed.length - 1])) return; // commit on a non-letter
  const cur = cm_.getCursor();
  const before = cm_.getLine(cur.line).slice(0, cur.ch);
  const mt = before.match(/\\([a-zA-Z]+)(.)$/);
  if (mt && UNI[mt[1]]) {
    const start = { line: cur.line, ch: cur.ch - mt[0].length };
    cm_.replaceRange(UNI[mt[1]] + mt[2], start, cur);
  }
});

// --- the live loop ----------------------------------------------------------------
let timer = null, activeView = null, lastSource = "";

function setStatus(text, cls) { const s = $("#status"); s.textContent = text; s.className = cls || "dim"; }

function paint(data, ms) {
  $("#latency").textContent = ms != null ? `${ms} ms` : "";
  if (!data.ok) {
    setStatus("error", "err");
    $("#errors").hidden = false;
    $("#errors").textContent = data.error || "analysis failed";
    if (data.dropped) $("#honesty").innerHTML = `<span class="dropped">⚠ ${data.dropped} dropped constraint(s)</span>`;
    return;
  }
  $("#errors").hidden = true;
  setStatus("ok", "ok");
  $("#banner").className = "live";
  $("#banner").textContent = "◆ " + data.banner;
  activeView = data.view;

  // tabs
  const tabs = $("#tabs");
  tabs.innerHTML = "";
  (data.views || []).forEach((v) => {
    const el = document.createElement("div");
    el.className = "tab" + (v === activeView ? " on" : "");
    el.textContent = v.replace(/_/g, " ");
    el.onclick = () => run(v);
    tabs.appendChild(el);
  });

  // the rendered view
  $("#view").innerHTML = data.png
    ? `<img alt="${data.view}" src="data:image/png;base64,${data.png}">`
    : `<div class="ph">no view for this program</div>`;

  // the honesty line
  const dropCls = data.dropped ? "dropped" : "clean";
  const dropTxt = data.dropped ? `⚠ ${data.dropped} dropped constraint(s)` : "✓ 0 dropped constraints";
  $("#honesty").innerHTML =
    `<span class="${dropCls}">${dropTxt}</span>` +
    `<span class="dim">${data.states} reachable states · ${data.edges} transitions</span>` +
    `<span class="dim">vars: ${(data.vars || []).join(", ")}</span>`;
}

async function run(view) {
  const source = cm.getValue();
  lastSource = source;
  setStatus("computing…", "busy");
  const t0 = performance.now();
  try {
    const res = await fetch("/api/analyze", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source, view: view || activeView }),
    });
    const data = await res.json();
    paint(data, Math.round(performance.now() - t0));
  } catch (e) {
    setStatus("server error", "err");
    $("#errors").hidden = false;
    $("#errors").textContent = "could not reach the backend: " + e;
  }
}

cm.on("change", () => { clearTimeout(timer); timer = setTimeout(() => run(), 350); });

// kick off
run();
