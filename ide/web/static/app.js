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
    i   ∈ Int
    sum ∈ Int
    is_first_tick ⇒
        i = 0
        sum = 0
    ¬is_first_tick ⇒
        Δi   = (_i < 5 ? 1 : 0)
        Δsum = (_i < 5 ? _i : 0)`;

const $ = (s) => document.querySelector(s);

const cm = CodeMirror.fromTextArea($("#code"), {
  theme: "dracula", lineNumbers: true, lineWrapping: false,
  viewportMargin: Infinity, value: DEFAULT_PROGRAM,
  smartIndent: false, electricChars: false, indentWithTabs: false, indentUnit: 4,
  extraKeys: {
    // Evident is indentation-sensitive (like Python). CodeMirror's default Enter
    // (newlineAndIndent) COPIES the previous line's indent, which then stacks on top
    // of the leading whitespace already present in typed/pasted text — so every line
    // drifts further right until the parser rejects the program. Insert a bare newline;
    // the text carries its own indentation. Tab inserts 4 spaces so nesting stays easy.
    "Enter": (e) => e.replaceSelection("\n"),
    "Tab": (e) => e.replaceSelection("    "),
    "Shift-Tab": (e) => e.execCommand("indentLess"),
  },
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
  const view = $("#view"), warn = $("#warnings");
  if (!data.ok) {
    setStatus("error", "err");
    $("#errors").hidden = false;
    $("#errors").textContent = data.error || "analysis failed";
    // the diagram on screen is from a PREVIOUS good run — mark it stale; never show
    // green reachable-state stats next to a red parse error.
    view.classList.add("stale");
    $("#banner").className = "stale";
    $("#honesty").innerHTML = data.dropped
      ? `<span class="dropped">⚠ ${data.dropped} dropped constraint(s)</span><span class="dim">diagram stale — fix the error</span>`
      : `<span class="dim">diagram stale — fix the error above</span>`;
    warn.hidden = !(data.dropped && data.warnings);
    if (!warn.hidden) warn.textContent = data.warnings;
    return;
  }
  view.classList.remove("stale");
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
  view.innerHTML = data.png
    ? `<img alt="${data.view}" src="data:image/png;base64,${data.png}">`
    : `<div class="ph">no view for this program</div>`;

  // the honesty line (branching ×N surfaces nondeterminism right next to the stats)
  const dropCls = data.dropped ? "dropped" : "clean";
  const dropTxt = data.dropped ? `⚠ ${data.dropped} dropped constraint(s)` : "✓ 0 dropped constraints";
  const branch = data.branching >= 2 ? ` · branching ×${data.branching}` : "";
  $("#honesty").innerHTML =
    `<span class="${dropCls}">${dropTxt}</span>` +
    `<span class="dim">${data.states} reachable states · ${data.edges} transitions${branch}</span>` +
    `<span class="dim">vars: ${(data.vars || []).join(", ")}</span>`;

  // which constraint(s) vanished — the actual dropped text, not just a count
  warn.hidden = !(data.dropped && data.warnings);
  if (!warn.hidden) warn.textContent = data.warnings;
}

async function run(view) {
  const source = cm.getValue();
  lastSource = source;
  const nm = source.match(/^\s*(?:fsm|claim|type|schema)\s+([A-Za-z_]\w*)/m);
  $("#fname").textContent = (nm ? nm[1] : "untitled") + ".ev";
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
