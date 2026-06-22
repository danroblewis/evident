"use strict";

// --- typable-token input -----------------------------------------------------------
// Two ways to type the Unicode operators Evident's lexer expects:
//  (1) LaTeX-style backslash input: \word + a non-letter  →  the operator.
//  (2) bare mnemonic auto-replacement (Task #34): a standalone word/op-pair converts
//      as you type, WORD-BOUNDARY SAFE — `in`→∈ but `Int`/`min`/`Coining` stay put.
const UNI = {
  in: "∈", notin: "∉", forall: "∀", all: "∀", exists: "∃", any: "∃",
  implies: "⇒", imp: "⇒", then: "⇒", Rightarrow: "⇒", impliedby: "⟸", when: "⟸",
  mapsto: "↦", to: "→", langle: "⟨", rangle: "⟩", leq: "≤", le: "≤", geq: "≥",
  ge: "≥", neq: "≠", ne: "≠", Delta: "Δ", delta: "Δ", neg: "¬", not: "¬",
  land: "∧", and: "∧", lor: "∨", or: "∨",
  cup: "∪", cap: "∩", times: "×", cdot: "·", subseteq: "⊆", emptyset: "∅",
};

// Bare mnemonics that convert when the COMPLETE preceding word is one of these and a
// non-word char is then typed. The lexer accepts only `in`/`mapsto` as words and the
// four ASCII op-pairs natively; everything else here MUST be converted to the real glyph
// so the program lexes. (Task #34.)
const WORD_MNEMONICS = {
  in: "∈", notin: "∉", implies: "⇒", impliedby: "⟸", when: "⟸",
  forall: "∀", all: "∀", exists: "∃", any: "∃", delta: "Δ",
  and: "∧", or: "∨", not: "¬", mapsto: "↦", to: "→",
  langle: "⟨", rangle: "⟩", leq: "≤", geq: "≥", neq: "≠",
  times: "×", cdot: "·", cup: "∪", cap: "∩", subseteq: "⊆", emptyset: "∅",
};
// Two-char ASCII operator pairs: convert the instant the 2nd char is typed.
const OP_PAIRS = { "<=": "≤", ">=": "≥", "!=": "≠", "=>": "⇒" };

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

// Worked examples chosen to demonstrate DISTINCT model shapes and language features — not
// seven counters. The FSMs exercise different dynamics (a terminating ramp, a real cyclic
// machine, a 2-D phase spiral, a wild integer orbit, nondeterministic drift); the claims
// show algorithms expressed as constraints (solve them with ⊨ Solve).
const SAMPLES = {
  "counter · a terminating clock (FSM)":
`fsm counter
    count ∈ Int
    is_first_tick ⇒ count = 0
    ¬is_first_tick ⇒ Δcount = (_count < 5 ? 1 : 0)
    done ∈ Bool = (count ≥ 5)`,
  "accumulate · a driven pipeline (FSM)": DEFAULT_PROGRAM,
  "vending · stock, coins & a vault (FSM)":
`-- A real vending machine: coins accumulate (up to a capacity), products sell from stock
-- into the operator's vault, the customer can cancel for a refund, and the operator
-- services it. The free \`act\` each tick makes the machine nondeterministic.
enum Mode = Idle | Coining | Dispensing | Refunding | Servicing
enum Act  = InsertCoin | Purchase | Cancel | Service

fsm vending
    mode    ∈ Mode
    0 ≤ balance ∈ Int ≤ 5      -- coins in the receptacle (capacity 5)
    0 ≤ stock   ∈ Int ≤ 3      -- units of product remaining
    0 ≤ vault   ∈ Int ≤ 12     -- money the operator has collected
    act     ∈ Act              -- free customer/operator choice each tick

    is_first_tick ⇒
        mode = Idle
        balance = 0
        stock = 3
        vault = 0

    ¬is_first_tick ⇒
        act = InsertCoin ⇒
            _balance < 5 ⇒
                mode = Coining
                balance = _balance + 1
                stock = _stock
                vault = _vault
            _balance ≥ 5 ⇒
                mode = Coining
                balance = _balance
                stock = _stock
                vault = _vault
        (act = Purchase ∧ _balance ≥ 3 ∧ _stock > 0) ⇒
            mode = Dispensing
            balance = _balance - 3
            stock = _stock - 1
            vault = _vault + 3
        (act = Purchase ∧ (_balance < 3 ∨ _stock = 0)) ⇒
            mode = Idle
            balance = _balance
            stock = _stock
            vault = _vault
        act = Cancel ⇒
            mode = Refunding
            balance = 0
            stock = _stock
            vault = _vault
        act = Service ⇒
            mode = Servicing
            balance = _balance
            stock = 3
            vault = 0`,
  "traffic light · a cyclic state machine (FSM)":
`enum Light = Red | Green | Yellow

fsm traffic
    light ∈ Light
    timer ∈ Int
    is_first_tick ⇒ (light = Red ∧ timer = 0)
    ¬is_first_tick ⇒
        _timer ≥ 2 ⇒
            timer = 0
            _light = Red    ⇒ light = Green
            _light = Green  ⇒ light = Yellow
            _light = Yellow ⇒ light = Red
        _timer < 2 ⇒
            Δtimer = 1
            light = _light`,
  "oscillator · a damped spring (FSM, phase spiral)":
`-- Two interacting real variables — position and velocity. Open the phase_portrait view:
-- the trajectory spirals in (pos, vel) space. The solver finds the equilibrium at the
-- origin, and the structure line reports it as an UNSTABLE one (the orbit diverges from it).
fsm oscillator
    pos ∈ Real
    vel ∈ Real
    is_first_tick ⇒ (pos = 60.0 ∧ vel = 0.0)
    ¬is_first_tick ⇒ Δpos = _vel / 6.0
    ¬is_first_tick ⇒ Δvel = (0.0 - _pos - _vel / 2.0) / 6.0`,
  "collatz · the 3n+1 orbit (FSM)":
`-- The Collatz map: halve n if even, else 3n+1. A wild integer orbit that always falls to 1.
-- (No modulo operator yet, so even-ness is 2·(n/2) = n via integer division.)
fsm collatz
    n ∈ Int
    is_first_tick ⇒ n = 27
    ¬is_first_tick ⇒ n = (_n ≤ 1 ? 1 : (2 * (_n / 2) = _n ? _n / 2 : 3 * _n + 1))`,
  "random walk · nondeterministic drift (FSM)":
`-- Each tick the walker steps freely in x and y. The free dx/dy make it nondeterministic;
-- the occupancy_heatmap shows where it dwells, the reachability_tree shows the fan.
fsm random_walk
    x ∈ Int
    y ∈ Int
    dx ∈ Int
    dy ∈ Int
    -1 ≤ dx ≤ 1
    -1 ≤ dy ≤ 1
    is_first_tick ⇒ (x = 0 ∧ y = 0)
    ¬is_first_tick ⇒ Δx = dx
    ¬is_first_tick ⇒ Δy = dy`,
  "pick · a nondeterministic choice (FSM)":
`fsm pick
    count ∈ Int
    1 ≤ step ∈ Int ≤ 3
    is_first_tick ⇒ count = 0
    ¬is_first_tick ⇒ Δcount = step`,
  "N-queens · an algorithm as constraints (⊨ Solve)":
`-- No search algorithm: just state what a valid board IS, and the solver finds one.
-- Indented lines after a ⇒ (or a ∀ :) are a conjunction — all must hold.
claim queens
    col ∈ Seq(Int)
    #col = 4

    ∀ i ∈ {0..3} :
        0 ≤ col[i]
        col[i] ≤ 3

    ∀ i ∈ {0..3} :
        ∀ j ∈ {0..3} :
            i < j ⇒
                col[i] ≠ col[j]
                col[i] - col[j] ≠ i - j
                col[i] - col[j] ≠ j - i`,
  "graph coloring · 3-color a map (⊨ Solve)":
`-- Color six regions so no two neighbors share a color — the classic CSP, as constraints.
enum Hue = Red | Green | Blue

claim graph_coloring
    wa  ∈ Hue
    nt  ∈ Hue
    sa  ∈ Hue
    q   ∈ Hue
    nsw ∈ Hue
    v   ∈ Hue
    wa ≠ nt
    wa ≠ sa
    nt ≠ sa
    nt ≠ q
    sa ≠ q
    sa ≠ nsw
    sa ≠ v
    q  ≠ nsw
    nsw ≠ v`,
  "sum-pair · solve-for-X (⊨ Solve, pin x=3)":
`claim sum_pair
    x ∈ Int
    y ∈ Int
    0 ≤ x ≤ 10
    0 ≤ y ≤ 10
    x + y = 10`,

  // --- algorithms as constraints (run with ⊨ Solve — the solver replaces the algorithm) ---
  "topo sort · a DAG's linear order (⊨ Solve)":
`-- A DAG's edges as constraints; the solver finds a linear order respecting them.
-- No traversal, no visited-set — just "every edge points forward in the order".
type Edge(from, to ∈ Int)

claim toposort
    edges ∈ Seq(Edge)
    pos   ∈ Seq(Int)
    #edges = 5
    #pos   = 5

    edges[0] = Edge(0, 1)
    edges[1] = Edge(0, 2)
    edges[2] = Edge(1, 3)
    edges[3] = Edge(2, 3)
    edges[4] = Edge(3, 4)

    ∀ i ∈ {0..4} :
        0 ≤ pos[i]
        pos[i] ≤ 4
    ∀ i ∈ {0..4} :
        ∀ j ∈ {0..4} :
            i < j ⇒ pos[i] ≠ pos[j]
    ∀ e ∈ edges :
        pos[e.from] < pos[e.to]`,
  "4×4 sudoku · fill the grid (⊨ Solve)":
`-- 4×4 Sudoku: state the rules (each row, column, and 2×2 box holds 1..4 once)
-- and pin a few givens. The solver fills the rest — no backtracking written.
type Box(a, b, c, d ∈ Int)

claim sudoku
    cell  ∈ Seq(Int)
    boxes ∈ Seq(Box)
    #cell  = 16
    #boxes = 4

    ∀ i ∈ {0..15} :
        1 ≤ cell[i]
        cell[i] ≤ 4

    -- givens
    cell[0]  = 1
    cell[2]  = 3
    cell[8]  = 2
    cell[15] = 1

    -- rows distinct
    ∀ r ∈ {0..3} :
        ∀ a ∈ {0..3} :
            ∀ b ∈ {0..3} :
                a < b ⇒ cell[r * 4 + a] ≠ cell[r * 4 + b]
    -- columns distinct
    ∀ c ∈ {0..3} :
        ∀ a ∈ {0..3} :
            ∀ b ∈ {0..3} :
                a < b ⇒ cell[a * 4 + c] ≠ cell[b * 4 + c]
    -- the four 2×2 boxes, named by their member cells
    boxes[0] = Box(cell[0],  cell[1],  cell[4],  cell[5])
    boxes[1] = Box(cell[2],  cell[3],  cell[6],  cell[7])
    boxes[2] = Box(cell[8],  cell[9],  cell[12], cell[13])
    boxes[3] = Box(cell[10], cell[11], cell[14], cell[15])
    ∀ x ∈ boxes :
        x.a ≠ x.b
        x.a ≠ x.c
        x.a ≠ x.d
        x.b ≠ x.c
        x.b ≠ x.d
        x.c ≠ x.d`,
  "subset-sum · pick items hitting a target (⊨ Solve)":
`-- Subset-sum: choose a subset of these weights that totals exactly the target.
-- 'take' is a yes/no per item; the solver finds which items to take.
type Item(weight ∈ Int, take ∈ Bool)

claim subset_sum
    items ∈ Seq(Item)
    #items = 6
    target ∈ Int = 15

    items[0].weight = 3
    items[1].weight = 7
    items[2].weight = 1
    items[3].weight = 8
    items[4].weight = 4
    items[5].weight = 11

    -- the taken weights must total exactly the target
    chosen ∈ Int = (items[0].take ? 3 : 0) + (items[1].take ? 7 : 0) + (items[2].take ? 1 : 0) + (items[3].take ? 8 : 0) + (items[4].take ? 4 : 0) + (items[5].take ? 11 : 0)
    chosen = target`,
  "sort · output a sorted permutation (⊨ Solve)":
`-- Sorting as constraints: 'out' is the SAME multiset as 'input', but ascending.
-- No compare-and-swap; just "ordered" + "a permutation of the input".
claim sort_constraints
    input ∈ Seq(Int)
    out   ∈ Seq(Int)
    #input = 5
    #out   = 5

    input[0] = 30
    input[1] = 10
    input[2] = 50
    input[3] = 20
    input[4] = 40

    -- out is ascending
    ∀ (a, b) ∈ edges(out) :
        a ≤ b

    -- out is a permutation of input: each is a rearrangement of the other.
    -- (inputs are distinct, so multiset-equality = mutual element membership)
    ∀ i ∈ {0..4} :
        ∃ j ∈ {0..4} : out[j] = input[i]
    ∀ j ∈ {0..4} :
        ∃ i ∈ {0..4} : input[i] = out[j]`,

  // --- diagram-value demos (each FSM picked to make one underused view shine) ---
  "bistable · two basins of attraction (FSM, basin_map)":
`-- A random walk between two absorbing walls at 0 and 6 (gambler's ruin).
-- Each tick a free step ±1, unless already at a wall, where it sticks. From the
-- middle the walk can end at EITHER wall, so the reachable graph has two terminal
-- states. Open basin_map: it colors each reachable state by the wall it falls to.
fsm bistable
    x ∈ Int
    step ∈ Int
    -1 ≤ step ≤ 1
    is_first_tick ⇒ x = 3
    ¬is_first_tick ⇒
        0 ≤ x
        x ≤ 6
        Δx = (_x = 0 ? 0 : (_x = 6 ? 0 : step))`,
  "fixed point · a 1-D map's staircase (FSM, cobweb)":
`-- A 1-D contraction map: each tick x moves a quarter of the way to 40.
-- It converges monotonically to the fixed point. Open the cobweb view: the
-- red staircase climbs from the seed to where the map line meets y = x.
fsm fixedpoint
    x ∈ Int
    is_first_tick ⇒ x = 4
    ¬is_first_tick ⇒ x = _x + (40 - _x) / 4`,
  "four signals · a 4-variable system (FSM, scatter_matrix)":
`-- Four genuinely-carried sawtooths on coprime periods (11, 5, 7, 3). Each pair
-- sweeps a different lattice. Open scatter_matrix: every pairwise plane at once,
-- with each variable's distribution on the diagonal. (parallel_coords also fits.)
fsm fourvar
    a ∈ Int
    b ∈ Int
    c ∈ Int
    d ∈ Int
    is_first_tick ⇒ (a = 0 ∧ b = 0 ∧ c = 0 ∧ d = 0)
    ¬is_first_tick ⇒
        a = (_a ≥ 10 ? 0 : _a + 1)
        b = (_b ≥ 4  ? 0 : _b + 1)
        c = (_c ≥ 6  ? 0 : _c + 1)
        d = (_d ≥ 2  ? 0 : _d + 1)`,
  "digital block · clock + flags (FSM, timing_diagram)":
`-- A small synchronous digital block, all four signals genuinely carried tick-to-tick:
--   clk   — toggles every tick (the master clock)
--   clk2  — a divide-by-2 clock: toggles only on clk's rising edge
--   count — a 2-bit counter advancing each tick, wrapping at 3
--   pulse — a one-tick strobe, high only on the tick the counter wraps
-- Open timing_diagram: all four stack as waveforms on one time axis (a logic analyzer).
fsm timing
    clk   ∈ Bool
    clk2  ∈ Bool
    count ∈ Int
    pulse ∈ Bool
    is_first_tick ⇒ (clk = false ∧ clk2 = false ∧ count = 0 ∧ pulse = false)
    ¬is_first_tick ⇒
        clk = ¬_clk
        clk2 = (¬_clk ? ¬_clk2 : _clk2)
        count = (_count ≥ 3 ? 0 : _count + 1)
        pulse = (¬_pulse ∧ _count ≥ 3)`,
};

const $ = (s) => document.querySelector(s);

// --- Evident syntax-highlighting Ace mode -----------------------------------------
// A code editor with no language mode shows undifferentiated grey text. This Ace mode
// tokenizes Evident: keywords, the Unicode/ASCII operators, comments, strings, numbers,
// _prev reads, Type/Variant capitals, and booleans — mapped to dracula token classes.
ace.define("ace/mode/evident", [
  "require", "exports", "module",
  "ace/lib/oop", "ace/mode/text", "ace/mode/text_highlight_rules",
], function (require, exports) {
  const oop = require("ace/lib/oop");
  const TextMode = require("ace/mode/text").Mode;
  const TextHighlightRules = require("ace/mode/text_highlight_rules").TextHighlightRules;

  const KEYWORDS =
    "claim|type|enum|fsm|schema|import|assert|match|matches|subclaim|in|is" +
    "_first_tick|coindexed|edges";
  // The Unicode/ASCII operator glyphs. Escaped for use inside a character class.
  const OPS = "∈∉∀∃⇒⟸↦→⟨⟩≤≥≠Δ¬∧∨∪∩×·⊆∅=<>+\\-*/?:.,#|";

  function EvidentHighlightRules() {
    this.$rules = {
      start: [
        { token: "comment.line", regex: "--.*$" },
        { token: "string", regex: '"(?:\\\\.|[^"\\\\])*"' },
        { token: "constant.numeric", regex: "\\b\\d+(?:\\.\\d+)?\\b" },
        // booleans (lowercase) — capital True/False are unbound names, left as identifiers
        { token: "constant.language.boolean", regex: "\\b(?:true|false)\\b" },
        // keywords (word-boundary; is_first_tick handled by the regex alternation)
        { token: "keyword", regex: "\\b(?:" + KEYWORDS + ")\\b" },
        // previous-tick read: _foo
        { token: "variable.parameter", regex: "_[A-Za-z]\\w*\\b" },
        // Type name / enum Variant — Capitalized identifier
        { token: "entity.name.type", regex: "\\b[A-Z]\\w*\\b" },
        // plain identifiers
        { token: "identifier", regex: "\\b[a-z_]\\w*\\b" },
        // operators (Unicode + ASCII)
        { token: "keyword.operator", regex: "[" + OPS + "]" },
      ],
    };
  }
  oop.inherits(EvidentHighlightRules, TextHighlightRules);

  function Mode() {
    this.HighlightRules = EvidentHighlightRules;
    this.lineCommentStart = "--";
  }
  oop.inherits(Mode, TextMode);
  exports.Mode = Mode;
});

// --- editor construction ----------------------------------------------------------
const editor = ace.edit("code");
editor.setTheme("ace/theme/dracula");
editor.session.setMode("ace/mode/evident");
editor.session.setUseWrapMode(true);          // line wrapping on
editor.session.setTabSize(4);
editor.session.setUseSoftTabs(true);
editor.setOptions({
  fontSize: "14px",
  showPrintMargin: false,
  highlightActiveLine: true,
  useWorker: false,                            // no built-in linter; analyze() is our diagnostics
  newLineMode: "unix",
});
editor.renderer.setShowGutter(true);

// --- save / export / share: keeping more than one experiment (Task #213) ----------
// Three orthogonal escape-hatches for a buffer, each a pair of PURE helpers (no DOM/editor
// dependency) so they can be unit-tested headless:
//   • named slots   — a localStorage map of {name → source}, separate from the single-buffer
//                      auto-persist (evident-buffer). Survives reload; appears in #samples.
//   • export .ev    — a text/plain Blob download (the file leaves the browser).
//   • share link    — the source packed into the URL hash; a friend pastes the link and gets
//                      the program. Round-trip is lossless for unicode (∈ ⇒ Δ ∀).
const SLOTS_KEY = "evident-slots";
function loadSlots() {                              // corrupt / missing map → {} (never throw)
  try {
    const raw = localStorage.getItem(SLOTS_KEY);
    if (!raw) return {};
    const obj = JSON.parse(raw);
    if (!obj || typeof obj !== "object" || Array.isArray(obj)) return {};
    const out = {};                                // keep only string→string entries
    for (const k of Object.keys(obj)) if (typeof obj[k] === "string") out[k] = obj[k];
    return out;
  } catch (e) { return {}; }
}
function writeSlots(map) { try { localStorage.setItem(SLOTS_KEY, JSON.stringify(map)); } catch (e) {} }
function saveSlot(name, source) {                  // add/overwrite; returns the new map
  const map = loadSlots(); map[name] = source; writeSlots(map); return map;
}
function deleteSlot(name) {                         // remove one; returns the new map
  const map = loadSlots(); delete map[name]; writeSlots(map); return map;
}

// Share link: base64 of UTF-8 bytes, then URL-encoded. btoa wants a binary string, so the
// unescape(encodeURIComponent(…)) dance widens unicode to bytes first; decode reverses it.
// Any malformed input (bad base64, non-UTF-8) returns null — the caller falls back to a
// normal load instead of throwing.
function encodeShare(source) {
  return encodeURIComponent(btoa(unescape(encodeURIComponent(source))));
}
function decodeShare(token) {
  try {
    const src = decodeURIComponent(decodeURIComponent(token).replace(/^src=/, ""));
    return decodeURIComponent(escape(atob(src)));
  } catch (e) { return null; }
}
// Pull a shared program out of a location.hash like "#src=<token>"; null if absent/undecodable.
function sharedFromHash(hash) {
  const m = (hash || "").match(/^#src=(.+)$/);
  return m ? decodeShare(m[1]) : null;
}

// Persist the buffer across reloads — losing your work on an accidental refresh is the
// fastest way to lose a user's trust. A shared link (#src=…) takes precedence over the
// auto-persisted buffer: the whole point of the link is to override what's already there.
const SHARED = sharedFromHash(location.hash);
const SAVED = (() => { try { return localStorage.getItem("evident-buffer"); } catch (e) { return null; } })();
editor.setValue(SHARED != null ? SHARED : (SAVED != null ? SAVED : DEFAULT_PROGRAM), -1);   // -1 = cursor to start
// The buffer is saved on every edit (scheduleAnalyze), but a refresh during the 350ms analyze
// debounce — or before the first edit — could miss the latest keystrokes; flush on unload too so
// reload NEVER loses work (Marek #174).
window.addEventListener("beforeunload", () => {
  try { localStorage.setItem("evident-buffer", editor.getValue()); } catch (e) {}
});

// --- auto-indent on Enter ---------------------------------------------------------
// Evident is indentation-sensitive. On Enter: copy the current line's leading whitespace,
// and add one level after a block opener (an fsm/claim/type/enum/schema header, or a line
// ending in ⇒) — so a hand-typed body lands correctly indented instead of at column 0
// (which the parser rejects).
editor.commands.addCommand({
  name: "evidentNewline",
  bindKey: { win: "Enter", mac: "Enter" },
  exec: function (ed) {
    const cursor = ed.getCursorPosition();
    const line = ed.session.getLine(cursor.row);
    const indent = (line.match(/^[ \t]*/) || [""])[0];
    const opener = /^\s*(fsm|claim|type|enum|schema)\b/.test(line) || /⇒\s*$/.test(line);
    ed.insert("\n" + indent + (opener ? "    " : ""));
  },
});

// --- typable-token input (backslash + bare mnemonic auto-replacement) -------------
// Driven off session 'change'. We inspect the text just inserted and the word/op-pair
// immediately preceding the cursor, then splice in the glyph. Splices go through a single
// undo group with the triggering keystroke so Ctrl-Z reverts one replacement at a time.
let _replacing = false;        // guard against re-entrancy from our own splice
function applyTokenInput(delta) {
  if (_replacing) return;
  if (!delta || delta.action !== "insert") return;
  const inserted = delta.lines.length === 1 ? delta.lines[0] : null;
  if (!inserted || inserted.length !== 1) return;     // only single-char keystrokes trigger
  const ch = inserted;
  const row = delta.end.row, col = delta.end.column;  // cursor sits just after the inserted char
  const line = editor.session.getLine(row);
  const before = line.slice(0, col);                  // text up to and including the trigger char

  // (1) ASCII operator pair — convert the instant the 2nd char lands.
  const pair = before.slice(-2);
  if (OP_PAIRS[pair]) {
    spliceReplace(row, col - 2, col, OP_PAIRS[pair]);
    return;
  }

  // (2) backslash LaTeX input: \word + a non-letter committed it.
  if (!/[a-zA-Z\\]/.test(ch)) {
    const bs = before.match(/\\([a-zA-Z]+)(.)$/);
    if (bs && UNI[bs[1]]) {
      // replace "\word<trigger>" with "<glyph><trigger>"
      spliceReplace(row, col - bs[0].length, col, UNI[bs[1]] + bs[2]);
      return;
    }
  }

  // (3) bare word mnemonic — convert when a non-word char follows a COMPLETE word that is
  //     a mnemonic. Word-boundary safe: the char before the word must be a non-word char
  //     (or start of line), so `Int`/`min`/`Coining` never convert — only a standalone word.
  if (!/[A-Za-z0-9_]/.test(ch)) {
    const wm = before.match(/(^|[^A-Za-z0-9_])([A-Za-z]+)(.)$/);
    if (wm && WORD_MNEMONICS[wm[2]]) {
      const wordStart = col - wm[3].length - wm[2].length;   // start of the matched word
      // Replace "word<trigger>" with "<glyph><trigger>" (keep the boundary char and land
      // the cursor AFTER it) — otherwise the cursor sits before the trigger space and the
      // next keystroke wedges between glyph and space (`in `+`Int` → `∈Int `).
      spliceReplace(row, wordStart, col, WORD_MNEMONICS[wm[2]] + wm[3]);
    }
  }
}

// Replace the [startCol, endCol) range on `row` with `text`, keeping the cursor after the
// inserted text and the operation in the same undo group as the triggering keystroke (so a
// single Ctrl-Z reverts exactly one replacement).
function spliceReplace(row, startCol, endCol, text) {
  _replacing = true;
  const Range = ace.require("ace/range").Range;
  editor.session.replace(new Range(row, startCol, row, endCol), text);
  editor.moveCursorTo(row, startCol + text.length);
  _replacing = false;
}

editor.session.on("change", (delta) => {
  applyTokenInput(delta);
  scheduleAnalyze();
});

// --- hover-to-learn glossary ------------------------------------------------------
// The notation is unfamiliar and several constructs are sugar whose expansion IS the
// semantics. Hover a keyword/operator/_prev/Bool to learn it — without leaving for docs.
const GLOSSARY = {
  fsm: "fsm — a state machine: a claim that carries state across ticks. _x reads the previous tick; x = … writes this tick (a difference equation).",
  claim: "claim — a predicate / relation. Also how you write tests and reusable constraint modules. Run ⊨ Solve to get a witness.",
  type: "type — a record/struct with local invariants. A noun you instantiate, e.g. type IVec2(x, y ∈ Int).",
  enum: "enum — a tagged union; variants may carry payloads and recurse, e.g. enum Result = Ok(Int) | Err(String).",
  schema: "schema — a named set defined by membership conditions (synonym for type).",
  is_first_tick: "is_first_tick — Bool, true only on the FSM's first tick. Used to seed the initial state.",
  is_second_tick: "is_second_tick — Bool, true only on the SECOND tick. Sets the 2nd initial condition for a ΔΔ (second-order) model.",
  "ΔΔ": "ΔΔ  second difference — 'ΔΔx' = x − 2·_x + __x (needs two ticks of history, __x). Lets a 2nd-order system, e.g. an oscillator, be written in ONE variable — the runtime carries velocity as history.   type \\Delta\\Delta",
  match: "match — pattern-match an enum value across its variants.",
  subclaim: "subclaim — a named nested claim, scoped to its parent's variables.",
  "∈": "∈  membership / typing — 'x ∈ Int' declares x has type Int.   type \\in",
  "⇒": "⇒  implies — 'A ⇒ B' means: if A then B.   type \\imp",
  "⟸": "⟸  reverse-implies (dispatch) — 'A ⟸ B' means A applies when B.   type \\when",
  "∀": "∀  for-all — '∀ x ∈ s : P' means P holds for every x in s.   type \\all",
  "∃": "∃  there-exists.   type \\exists",
  "Δ": "Δ  forward difference — 'Δx' = x − _x (this tick minus last). 'Δx = 1' means x rises by 1 each tick.   type \\Delta",
  "↦": "↦  maps-to / rename, e.g. (slot ↦ value).   type \\mapsto",
  "≤": "≤  less-than-or-equal.   type \\le", "≥": "≥  greater-than-or-equal.   type \\ge",
  "≠": "≠  not-equal.   type \\ne", "¬": "¬  logical not.   type \\neg",
  "∧": "∧  logical and.   type \\and", "∨": "∨  logical or.   type \\or",
  "⟨": "⟨ ⟩  sequence literal ⟨a, b, c⟩.   type \\langle \\rangle",
  "⟩": "⟨ ⟩  sequence literal ⟨a, b, c⟩.   type \\langle \\rangle",
};
const gloss = document.createElement("div");
gloss.id = "gloss"; gloss.hidden = true; document.body.appendChild(gloss);
function glossFor(t) {
  if (GLOSSARY[t]) return GLOSSARY[t];
  if (t && t.startsWith("__")) return `${t} — two-ticks-ago read: the value of ${t.slice(2)} two ticks back (the history a ΔΔ second-order model carries).`;
  if (t && t[0] === "_") return `${t} — previous-tick read: the value of ${t.slice(1)} on the prior tick.`;
  if (t === "true" || t === "false") return `${t} — Boolean literal (lowercase). Capital True/False is an unbound name — a silent bug.`;
  return null;
}
// Ace renders tokens as spans with ace_<type> classes inside the text layer. We resolve
// the token under the cursor via the session tokenizer (precise) and show a gloss for
// keyword/operator/_prev/boolean tokens.
const editorEl = $("#code");
editorEl.addEventListener("mousemove", (e) => {
  const pos = editor.renderer.screenToTextCoordinates(e.clientX, e.clientY);
  if (!pos) { gloss.hidden = true; return; }
  const tok = editor.session.getTokenAt(pos.row, pos.column + 1);
  if (tok) {
    // Don't gate on Ace's token TYPE — the Evident mode classifies the unicode operators
    // inconsistently, so hovering ∈/⇒/Δ in the editor did nothing despite a glossary entry
    // (Marek/Sam #45). glossFor() returns null for anything not in the glossary, so trying it on
    // every token is safe and lets operators/keywords/_prev all teach themselves.
    const g = glossFor((tok.value || "").trim());
    if (g) {
      gloss.textContent = g; gloss.hidden = false;
      gloss.style.left = Math.min(e.clientX + 12, window.innerWidth - 380) + "px";
      gloss.style.top = (e.clientY + 18) + "px";
      return;
    }
  }
  gloss.hidden = true;
});
editorEl.addEventListener("mouseleave", () => { gloss.hidden = true; });

// --- concept hover in the banner -------------------------------------------------
// The model-shape banner uses words a newcomer hasn't met ("Driven pipeline", "fixed point",
// "inductive invariant"). The editor glossary can't reach them — they're in the banner, not the
// source — so annotate the banner text: wrap each known concept in a hoverable span explained by
// the SAME #gloss tooltip (Sam #163/#165).
const CONCEPTS = {
  "inductive invariant": "a bound z3 PROVED is closed under the transition: true now ⇒ true next tick ⇒ true forever. A proof, not a sample.",
  "Driven pipeline": "a deterministic recurrence: one independent variable (the clock/driver) advances on its own; the rest are computed from it.",
  "Driven": "a deterministic recurrence: one variable advances on its own clock; the others follow from it.",
  "fixed point": "a state the transition maps to itself — reach it and the machine stays. The equilibrium of the dynamics.",
  "Nondeterministic": "from some state there are ≥2 legal next states — a free choice fans the future out.",
  "Cyclic": "the machine revisits states forever in a loop — eventually periodic, with no fixed point.",
  "Unbounded": "a variable grows without limit — the reachable set never closes.",
  "Unstable": "a fixed point the dynamics move AWAY from — a tiny nudge and the orbit diverges.",
  "reachable": "the states the machine can actually enter from its start, found by SOLVING the transition — not guessed.",
  "recurrence": "each tick's value is defined from the previous tick(s): x = f(_x).",
};
const _CONCEPT_KEYS = Object.keys(CONCEPTS).sort((a, b) => b.length - a.length);  // longest first
function annotateConcepts(text) {
  // Collect non-overlapping concept matches on the ORIGINAL text, then wrap in ONE left-to-right
  // pass. A previous version did iterated `.replace` over the accumulating HTML, so a later key
  // ("recurrence") matched the same word INSIDE an earlier span's data-gloss attribute and nested a
  // broken span — leaking a stray `">` into the banner (Sam #185). Building once from the source
  // text makes inserted markup unreachable to the matcher.
  const esc = escapeHtml(text);
  const spans = [];                                 // {s, e, k} on `esc`, longest key wins
  for (const k of _CONCEPT_KEYS) {                  // _CONCEPT_KEYS is longest-first
    const re = new RegExp("(?<![\\w-])(" + k.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + ")(?![\\w-])", "g");
    let m;
    while ((m = re.exec(esc)) !== null) {
      const s = m.index, e = s + m[0].length;
      if (!spans.some((sp) => s < sp.e && e > sp.s)) spans.push({ s, e, k });   // skip overlaps
    }
  }
  spans.sort((a, b) => a.s - b.s);
  let out = "", pos = 0;
  for (const sp of spans) {
    out += esc.slice(pos, sp.s)
         + `<span class="concept" data-gloss="${escapeHtml(CONCEPTS[sp.k])}">${esc.slice(sp.s, sp.e)}</span>`;
    pos = sp.e;
  }
  return out + esc.slice(pos);
}
document.addEventListener("mouseover", (e) => {
  const c = e.target.closest && e.target.closest(".concept");
  if (c && c.dataset.gloss) {
    gloss.textContent = c.dataset.gloss; gloss.hidden = false;
    gloss.style.left = Math.min(e.clientX + 12, window.innerWidth - 380) + "px";
    gloss.style.top = (e.clientY + 18) + "px";
  }
});
document.addEventListener("mouseout", (e) => {
  if (e.target.closest && e.target.closest(".concept")) gloss.hidden = true;
});

// --- per-view captions: "what am I looking at?" ----------------------------------
// A newcomer can decode solution_space, but "morse graph" / "nullcline field" / "chord diagram"
// are just names (Sam #189). One faithful line per view, derived from what the renderer ACTUALLY
// draws (viz/render_<view>.py docstrings) — shown two ways: a hover gloss on each tab (reusing the
// #gloss delegation) AND a caption line under the rendered diagram. Shape: "shows X · read it as Y
// · tells you Z". Must match ALL_VIEWS in ide/web/server.py exactly (coverage is checked).
const VIEW_CAPTIONS = {
  solution_space:
    "shows the SOLVED boundary of the program, not one run · read it as each variable's full range (left) + the feasible region of the two principal vars (right) · tells you what states are possible at all, with fixed points marked.",
  time_series:
    "shows one trajectory (~60 ticks) from the initial state · read it as every state variable plotted against tick number on stacked tracks (numeric=line, bool/enum=step) · tells you how each value evolves over time.",
  state_graph:
    "shows the reachable state-transition graph · read it as nodes=states, arrows=transitions of state=f(_state), terminal/absorbing states ringed · tells you every state the machine can enter and how they connect.",
  phase_portrait:
    "shows the difference-equation vector field · read it as each point's displacement successor(p)−p as an arrow, colored by step magnitude, faceted per categorical value · tells you which way the dynamics flow across value-space.",
  reachability_tree:
    "shows the breadth-first reachability tree from the initial state · read it as each node at its depth = shortest-path length from the start, keeping only first-discovery edges · tells you how many steps it takes to reach each state.",
  morse_graph:
    "shows the recurrence skeleton — the condensation DAG of the reachable graph · read it as one node per strongly-connected component (cycle), classified attractor/repeller/transient · tells you where the dynamics get trapped vs pass through.",
  occupancy_heatmap:
    "shows where the system spends its time · read it as a 2-D histogram of many-seed/many-step visited points over two axes, brightness = visit density (log) · tells you the occupied region / attractor of state-space.",
  timing_diagram:
    "shows one ~40-tick run as EE-style waveforms · read it as one stacked track per variable (bool/enum=digital edges, numeric=analog line) ordered most-informative on top · tells you when each signal changes relative to the others.",
  transition_matrix:
    "shows the transition relation as an adjacency-matrix heatmap · read it as cell (i,j) lit iff state i → state j, states ordered so the top categorical forms blocks · tells you whether transitions stay within a mode (block-diagonal) or switch.",
  basin_map:
    "shows the basins of attraction · read it as a 2-axis projection of start states colored by WHICH terminal (fixed point / cycle / terminal SCC) each eventually settles into · tells you where each starting state ends up.",
  orbit_scatter:
    "shows one trajectory as discrete unconnected dots in two state axes · read it as each dot = one tick's state, gaps = the jump the equation makes; categorical on color, time gradient when none · tells you the orbit's shape (loop=cycle, pile-up=fixed point).",
  scatter_matrix:
    "shows pairwise projections of all state variables · read it as an N×N grid of scatter panels (one per variable pair), hued by the top categorical var · tells you which variables correlate or separate across the reachable set.",
  parallel_coords:
    "shows the reachable state set as polylines (Inselberg) · read it as each state a line crossing every variable's axis at its value, hued by the top categorical · tells you which value-combinations cluster per class.",
  chord_diagram:
    "shows transition flow on a circular categorical axis · read it as nodes = values of the top categorical (room→room, mode→mode), arc width/opacity = transition count, arc hue = a second categorical · tells you how much flow goes between which categories.",
  nullcline_field:
    "shows the qualitative phase-plane sign field over two numeric axes · read it as the plane shaded by the sign of each component's change, with nullclines (zero-change curves) overlaid; their crossings are fixed points · tells you which way each variable is pushed everywhere.",
  fixedpoint_map:
    "shows where the system comes to rest · read it as a 2-axis projection with fixed points as large markers, short cycles as arrowed loops, other sampled states as faint dots · tells you the attractors standing out against the basin.",
  cobweb:
    "shows a 1-D map x_n → x_{n+1} as a cobweb plot · read it as both axes the same scalar, staircasing between the map curve and the diagonal; faceted per categorical mode · tells you whether iterating the scalar converges, cycles, or diverges.",
};

// Extend the #gloss delegation to tabs too: a hover on a view tab shows its caption.
document.addEventListener("mouseover", (e) => {
  const t = e.target.closest && e.target.closest("#tabs .tab");
  if (t && t.dataset.gloss) {
    gloss.textContent = t.dataset.gloss; gloss.hidden = false;
    gloss.style.left = Math.min(e.clientX + 12, window.innerWidth - 380) + "px";
    gloss.style.top = (e.clientY + 18) + "px";
  }
});
document.addEventListener("mouseout", (e) => {
  if (e.target.closest && e.target.closest("#tabs .tab")) gloss.hidden = true;
});

// --- inline error line marker -----------------------------------------------------
// Tint the offending line. Ace marks a line via a full-width marker; the simplest robust
// approach is a gutter-decoration + a row marker class (.ace_error-line) on that row.
let _errLine = null;
function clearErrorLine() {
  if (_errLine != null) {
    editor.session.removeGutterDecoration(_errLine, "error-gutter");
    if (_errMarker != null) { editor.session.removeMarker(_errMarker); _errMarker = null; }
    _errLine = null;
  }
}
let _errMarker = null;
// Mark the offending line. Prefer the structured {line, col} from /api/analyze
// (parser now emits it); fall back to scraping "line N" out of the message text.
function markErrorLine(err, loc) {
  clearErrorLine();
  let ln = null;
  if (loc && Number.isInteger(loc.line)) {
    ln = loc.line - 1;
  } else {
    const m = (err || "").match(/line (\d+)/i);
    if (m) ln = parseInt(m[1], 10) - 1;
  }
  if (ln != null && ln >= 0 && ln < editor.session.getLength()) {
    const Range = ace.require("ace/range").Range;
    _errMarker = editor.session.addMarker(
      new Range(ln, 0, ln, Infinity), "ace-error-line", "fullLine");
    editor.session.addGutterDecoration(ln, "error-gutter");
    _errLine = ln;
  }
}

// --- dropped-constraint line markers ----------------------------------------------
// A DROPPED constraint is Evident's signature silent bug: the line parsed, but couldn't
// translate to a Z3 Bool, so it was discarded — the variable it constrained is left FREE,
// and the model is under-constrained while looking valid. Surface that AT the line the
// user wrote it. Distinct AMBER style from the red parse-error marker (a parse error
// blocks; a dropped constraint runs but silently lies). The gutter cell carries an Ace
// warning annotation whose tooltip = the desugared dropped-constraint text.
let _droppedRows = [];
function clearDroppedLines() {
  for (const d of _droppedRows) {
    editor.session.removeMarker(d.marker);
    editor.session.removeGutterDecoration(d.row, "warn-gutter");
  }
  _droppedRows = [];
  editor.session.clearAnnotations();
}
// locs: 1-based source lines; warnings: the raw `warning: dropped …` block (for tooltips).
function markDroppedLines(locs, warnings) {
  clearDroppedLines();
  if (!Array.isArray(locs) || !locs.length) return;
  const Range = ace.require("ace/range").Range;
  const pretties = (warnings || "")
    .split("\n")
    .map((l) => (l.match(/couldn't translate to Bool\):\s*(.+)$/) || [])[1])
    .filter(Boolean);
  const annotations = [];
  locs.forEach((line, i) => {
    const row = line - 1;
    if (!Number.isInteger(row) || row < 0 || row >= editor.session.getLength()) return;
    const marker = editor.session.addMarker(
      new Range(row, 0, row, Infinity), "ace-warn-line", "fullLine");
    editor.session.addGutterDecoration(row, "warn-gutter");
    _droppedRows.push({ row, marker });
    annotations.push({
      row, column: 0, type: "warning",
      text: pretties[i]
        ? "dropped constraint (left FREE — not translated to a Z3 Bool):\n  " + pretties[i]
        : "dropped constraint — couldn't translate to a Z3 Bool (variable left free)",
    });
  });
  if (annotations.length) editor.session.setAnnotations(annotations);
}

// --- the live loop ----------------------------------------------------------------
let timer = null, activeView = null, lastSource = "", _dimTimer = null, _elapsedTimer = null;

// --- run-history + pin/compare (tasks #209, #207) ---------------------------------
// `history` is a ring buffer of the last good analyses (newest first), so you can flip
// back to "what did this look like 3 edits ago" (#209). `pinnedA` holds one snapshot
// captured by the 📌 button so the live result renders BESIDE it (#207). `pastView`,
// when set, means we're looking at a past snapshot read-only — the next edit returns live.
const HISTORY_CAP = 8;
let history = [];
let pinnedA = null;
let pastView = null;
let currentSlotName = null;   // the active saved-slot name; overrides the derived #fname (Task #213)

// Push a snapshot onto a newest-first ring buffer, capping length. Pure (returns the
// array) so it's unit-testable headless; mutates in place for the module array.
function pushHistory(arr, snap, cap) {
  arr.unshift(snap);
  if (arr.length > cap) arr.length = cap;
  return arr;
}

// Human "relative age" of a past timestamp vs now. Pure — unit-tested headless.
function relativeAge(deltaMs) {
  const s = Math.max(0, Math.floor(deltaMs / 1000));
  if (s < 5) return "just now";
  if (s < 60) return s + "s ago";
  const m = Math.floor(s / 60);
  if (m < 60) return m + "m ago";
  const h = Math.floor(m / 60);
  return h + "h ago";
}

function setStatus(text, cls) { const s = $("#status"); s.textContent = text; s.className = cls || "dim"; }

// Translate parser jargon into something a newcomer can act on. The raw error stays
// (it's precise); we append a plain-language hint for the common footguns.
// Rust lexer token names → the literal the user actually typed (Sam #195: "got Eq" is jargon).
const _TOKEN_NAMES = {
  Eq: "'='", Lt: "'<'", Gt: "'>'", Le: "'≤'", Ge: "'≥'", Ne: "'≠'", Plus: "'+'", Minus: "'-'",
  Star: "'*'", Slash: "'/'", LParen: "'('", RParen: "')'", LBrace: "'{'", RBrace: "'}'",
  Colon: "':'", Comma: "','", Dot: "'.'", Implies: "'⇒'", In: "'∈'", Forall: "'∀'",
  Newline: "a line break", Indent: "indentation", Dedent: "a dedent", Eof: "end of input",
};
function humanizeError(err) {
  let hint = "";                                     // pick the hint from the RAW message first
  if (/got Ident\(/.test(err) || /expected schema\/claim\/type\/import\/enum/i.test(err)) {
    hint = "\n\n→ This usually means a body line isn't indented. Indent declarations and "
         + "constraints 4 spaces under their fsm/claim/type.";
  } else if (/\bgot Eq\b/.test(err)) {
    hint = "\n\n→ Unexpected '=' — Evident uses a single '=' for equality (there is no '=='); "
         + "check for a doubled '= =', or an '=' where an expression was expected.";
  } else if (/couldn't translate to Bool/i.test(err)) {
    hint = "\n\n→ A constraint was dropped — often a typo'd or undeclared name, or a capital "
         + "True/False (Evident uses lowercase true/false).";
  } else if (/lex error|unexpected character/i.test(err)) {
    hint = "\n\n→ An unrecognized character. For operators, type a backslash word "
         + "(e.g. \\in → ∈, \\implies → ⇒, \\Delta → Δ).";
  }
  // then humanize the token names for display
  err = err.replace(/got Ident\("([^"]*)"\)/g, "got '$1'")
           .replace(/\bgot ([A-Z]\w+)\b/g, (m, t) => _TOKEN_NAMES[t] ? "got " + _TOKEN_NAMES[t] : m);
  return err + hint;
}

// The SOLVER-computed structure of the whole model (not a single run): a verdict, the
// rigorous fixed points / equilibria (states the solver proves map to themselves), and the
// exact boundary of the solution space (min..max each variable spans over the reachable set).
const VERDICTS = {
  terminates:       ["✓", "Terminates", "the orbit converges to a fixed point"],
  cyclic:           ["↻", "Cyclic", "revisits states forever — no fixed point"],
  nondeterministic: ["⑂", "Nondeterministic", "a free choice fans the future out"],
  unstable:         ["⚠", "Unstable equilibrium", "a fixed point exists, but the orbit diverges from it"],
  unbounded:        ["→", "Unbounded", "grows without settling"],
  settles:          ["·", "Settles", ""],
};
function renderStructure(s) {
  const el = $("#structure");
  // the invariant checker only makes sense for an FSM with a reachable set — not a raw claim
  $("#invariant").hidden = !s || !!s.claim;
  if (!s) { el.hidden = true; return; }
  el.hidden = false;
  const [icon, name, note] = VERDICTS[s.verdict] || ["·", s.verdict, ""];
  // title= tooltips teach the verification concepts in place — the words appear in the panel,
  // not the editor, so the editor glossary can't reach them (Sam #163).
  const vhelp = "the model's GLOBAL behaviour, solved from the transition relation over the whole "
    + "reachable set — not one simulated run.";
  let html = `<span class="verdict v-${s.verdict}" title="${vhelp}">${icon} ${name}</span>`
    + (note ? `<span class="dim">${note}</span>` : "");
  if (s.fixed_points && s.fixed_points.length) {
    const fp = s.fixed_points.slice(0, 3).map(
      (f) => "(" + Object.entries(f).map(([k, v]) => `${k}=${v}`).join(", ") + ")").join("  ");
    const more = s.fixed_points.length > 3 ? ` +${s.fixed_points.length - 3}` : "";
    const label = s.verdict === "nondeterministic" ? "rest states" : "fixed point";
    const fhelp = "a REACHABLE state the system maps to itself — once here it stays. Found by "
      + "solving T(s,s), then intersected with the reachable set so it's never a phantom.";
    html += `<span class="struct-fp" title="${fhelp}">● ${label}: ${fp}${more}</span>`;
  }
  const b = s.bounds || {}, keys = Object.keys(b);
  if (keys.length) {
    const bstr = keys.map((k) => `${k} ∈ [${b[k][0]}, ${b[k][1]}]`).join("   ");
    const bhelp = "the exact range each variable spans over the solution space — z3-proven "
      + "(Optimize over the unrolled transition), not the min/max of one run.";
    html += `<span class="struct-bounds" title="${bhelp}">⊏ boundary${s.capped ? " (≥, capped)" : ""}: ${bstr}</span>`;
  }
  el.innerHTML = html;
}

// Interactive diagram overlay (#184): drop transparent hover targets over the rendered
// solution_space scatter — hover → tooltip of that point's full state; click → pin it until
// the next hover. fx/fy are figure fractions from the top-left, so they map directly to a
// wrapper sized exactly to the image. No-op (and clears) when there are no points.
function fmtState(st) {
  return Object.entries(st || {})
    .map(([k, v]) => `${k}=${v}`).join("  ");
}
function overlayPoints(wrap, points) {
  if (!wrap) return;
  let pinned = false;
  const show = (txt, x, y) => {
    gloss.textContent = txt; gloss.hidden = false;
    gloss.style.left = Math.min(x + 12, window.innerWidth - 380) + "px";
    gloss.style.top = (y + 18) + "px";
  };
  const hide = () => { if (!pinned) gloss.hidden = true; };
  if (!points || !points.length) return;
  points.forEach((p) => {
    if (typeof p.fx !== "number" || typeof p.fy !== "number") return;
    const t = document.createElement("div");
    t.className = "pt-target";
    t.style.left = (p.fx * 100) + "%";
    t.style.top = (p.fy * 100) + "%";
    const txt = fmtState(p.state);
    t.title = txt;       // native tooltip fallback
    t.addEventListener("mouseenter", (e) => { pinned = false; show(txt, e.clientX, e.clientY); });
    t.addEventListener("mousemove", (e) => { if (!pinned) show(txt, e.clientX, e.clientY); });
    t.addEventListener("mouseleave", hide);
    t.addEventListener("click", (e) => {
      e.stopPropagation(); pinned = true; show(txt, e.clientX, e.clientY);
    });
    wrap.appendChild(t);
  });
}

function paint(data, ms) {
  clearInterval(_elapsedTimer);                          // stop the elapsed ticker — result is in
  $("#latency").textContent = ms != null ? `${ms} ms` : "";
  gloss.hidden = true;                                  // clear any pinned overlay tooltip from the
                                                       // previous program — a ghost pin must not
                                                       // float over the new diagram (Marek #172).
  $("#banner").classList.remove("recomputing");        // analysis returned — undim
  $("#structure").classList.remove("recomputing");
  $("#view").classList.remove("recomputing");
  // Tint each dropped-constraint line in the editor, on every result (ok / error / claim
  // alike) — the silent bug surfaces AT the line, not just in the console banner. Cleared
  // here too: an empty/absent dropped_locs wipes the previous run's amber markers.
  markDroppedLines(data.dropped_locs, data.warnings);
  const view = $("#view"), warn = $("#warnings");
  $("#view-caption").textContent = "";                   // clear the per-view caption on any result;
                                                         // the OK path below re-sets it for the new view.
  if (!data.ok) {
    exitCompareModes();                                // never a two-up / past view over an error or claim
    $("#structure").hidden = true;
    $("#invariant").hidden = true;                     // no reachable set → no verify row
    $("#inv-result").textContent = "";
    $("#tabs").innerHTML = "";                          // no current valid view — don't leave the
                                                       // 16-tab strip inviting clicks over a stale
                                                       // / empty diagram (Marek #147/#148).
    // a pure claim (no FSM) isn't an error — it's a solve target, not a thing to visualize
    if (/no fsm schemas? found/i.test(data.error || "")) {
      setStatus("claim — use Solve", "ok");
      $("#errors").hidden = true; warn.hidden = true;
      view.classList.remove("stale");
      view.innerHTML = '<div class="ph">No state machine to visualize.<br>'
        + 'Press <b>⊨ Solve</b> (top bar) to run this claim → a witness, or UNSAT.</div>';
      $("#banner").className = "live";
      $("#banner").textContent = "◆ a claim (a relation) — solve it for a witness assignment";
      $("#honesty").innerHTML = '<span class="dim">⊨ Solve runs the constraints → SAT (with a witness) or UNSAT</span>';
      clearErrorLine();
      return;
    }
    setStatus("error", "err");
    $("#errors").hidden = false;
    $("#errors").textContent = humanizeError(data.error || "analysis failed");
    markErrorLine(data.error, data.error_loc);     // highlight the offending line in the gutter
    // the diagram on screen is from a PREVIOUS good run — mark it stale; never show
    // green reachable-state stats next to a red parse error.
    view.classList.add("stale");
    $("#banner").className = "stale";
    $("#banner").textContent = "▷ source has an error — fix it to refresh the analysis";
    $("#honesty").innerHTML = data.dropped
      ? `<span class="dropped">⚠ ${data.dropped} dropped constraint(s)</span><span class="dim">diagram stale — fix the error</span>`
      : `<span class="dim">diagram stale — fix the error above</span>`;
    warn.hidden = !(data.dropped && data.warnings);
    if (!warn.hidden) warn.textContent = data.warnings;
    return;
  }
  view.classList.remove("stale");
  $("#errors").hidden = true;
  clearErrorLine();
  setStatus("ok", "ok");
  $("#banner").className = "live";
  $("#banner").innerHTML = "◆ " + annotateConcepts(data.banner);
  renderStructure(data.structure);
  activeView = data.view;

  // tabs
  const tabs = $("#tabs");
  tabs.innerHTML = "";
  tabs.setAttribute("role", "tablist");
  (data.views || []).forEach((v, i) => {
    // Real tabs: keyboard- and screen-reader-navigable, not bare clickable divs (Marek/Ana #31).
    // Roving tabindex — only the active tab is in the tab order; ←/→ move focus between tabs.
    const el = document.createElement("div");
    el.className = "tab" + (v === activeView ? " on" : "");
    el.textContent = v.replace(/_/g, " ");
    el.setAttribute("role", "tab");
    el.setAttribute("aria-selected", v === activeView ? "true" : "false");
    el.tabIndex = v === activeView ? 0 : -1;
    if (VIEW_CAPTIONS[v]) el.dataset.gloss = VIEW_CAPTIONS[v];   // hover a tab → its "what am I looking at?" gloss
    el.onclick = () => run(v);
    el.onkeydown = (e) => {
      if (e.key === "Enter" || e.key === " ") { e.preventDefault(); run(v); }
      else if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
        e.preventDefault();
        const els = [...tabs.children];
        els[(i + (e.key === "ArrowRight" ? 1 : els.length - 1)) % els.length].focus();
      }
    };
    tabs.appendChild(el);
  });

  // We're back to a live result — leave any read-only "past run" mode.
  pastView = null;
  // the rendered view: single live picture, or — when something is pinned — two-up (#207).
  renderLiveView(view, data);

  // the one-line "what am I looking at?" caption under the diagram — set on every render, cleared
  // when the view has no caption (so a stale caption never lingers under a different picture).
  $("#view-caption").textContent = (data.png && VIEW_CAPTIONS[data.view]) ? VIEW_CAPTIONS[data.view] : "";

  // run-history (#209): snapshot only SUCCESSFUL, drawable results — never errors / claims /
  // backend-down, and never a result with no png (nothing to thumbnail).
  if (data.png) {
    pushHistory(history, {
      ts: Date.now(), fname: $("#fname").textContent, banner: data.banner || data.view || "",
      view: data.view, png: data.png, points: data.points || [], ms,
    }, HISTORY_CAP);
  }
  renderHistory();

  // the honesty line (branching ×N surfaces nondeterminism right next to the stats)
  const dropCls = data.dropped ? "dropped" : "clean";
  const dropTxt = data.dropped ? `⚠ ${data.dropped} dropped constraint(s)` : "✓ 0 dropped constraints";
  const branch = data.branching >= 2 ? ` · branching ×${data.branching}` : "";
  const nStates = data.capped ? `≥${data.states} (capped sample)` : `${data.states}`;
  $("#honesty").innerHTML =
    `<span class="${dropCls}">${dropTxt}</span>` +
    `<span class="dim">${nStates} reachable states · ${data.edges} transitions${branch}</span>` +
    `<span class="dim">vars: ${(data.vars || []).join(", ")}</span>`;

  // which constraint(s) vanished — the actual dropped text, not just a count, with a
  // did-you-mean for the capital-True/False footgun.
  warn.hidden = !(data.dropped && data.warnings);
  if (!warn.hidden) {
    const tf = /\bTrue\b|\bFalse\b/.test(data.warnings)
      ? "→ Booleans are lowercase in Evident: use true / false, not True / False.\n\n" : "";
    warn.textContent = tf + data.warnings;
  }
}

// One picture as a `.view-wrap` (image + optional hover overlay), or a placeholder when the
// program has no view. Shared by the single-view and two-up (#207) paths.
function viewPane(data, withOverlay) {
  if (!data.png) return `<div class="ph">no view for this program</div>`;
  const pane = document.createElement("div");
  pane.className = "view-wrap";
  pane.innerHTML = `<img alt="${data.view}" src="data:image/png;base64,${data.png}">`;
  if (withOverlay) overlayPoints(pane, data.points || []);
  return pane;
}

// Render the live result into #view. Single picture normally; two-up (pinned A · live B) once
// the 📌 button has captured a snapshot (#207). Only the live B pane carries the #184 overlay.
function renderLiveView(view, data) {
  view.innerHTML = "";
  if (!pinnedA) {
    const pane = viewPane(data, true);
    if (typeof pane === "string") view.innerHTML = pane; else view.appendChild(pane);
    return;
  }
  const row = document.createElement("div");
  row.className = "compare-row";
  row.appendChild(comparePane("A · pinned", pinnedA.banner, viewPane(pinnedA, false), true));
  row.appendChild(comparePane("B · live", data.banner || data.view || "", viewPane(data, true), false));
  view.appendChild(row);
}

// One labelled column of the two-up compare. `ghost` dims the pinned A so the live B reads as
// the current picture. The A column carries an ✕ to unpin.
function comparePane(label, caption, body, ghost) {
  const col = document.createElement("div");
  col.className = "compare-pane" + (ghost ? " ghost" : "");
  const head = document.createElement("div");
  head.className = "compare-label";
  head.textContent = label;
  if (ghost) {
    const x = document.createElement("span");
    x.className = "compare-unpin"; x.textContent = "✕"; x.title = "unpin A — back to single live view";
    x.onclick = () => setPinned(null);
    head.appendChild(x);
  }
  col.appendChild(head);
  if (typeof body === "string") { const ph = document.createElement("div"); ph.innerHTML = body; col.appendChild(ph); }
  else col.appendChild(body);
  const cap = document.createElement("div");
  cap.className = "compare-cap dim"; cap.textContent = caption;
  col.appendChild(cap);
  return col;
}

// The history strip (#209): up to HISTORY_CAP thumbnails, newest first. Click → read-only past
// view. Empty strip when there's no history. The current past-view thumb (if any) is outlined.
function renderHistory() {
  const strip = $("#history");
  if (!strip) return;
  strip.innerHTML = "";
  if (!history.length) return;
  const now = Date.now();
  history.forEach((snap, i) => {
    if (!snap.png) return;            // skip a snapshot with no picture (degrade gracefully)
    const age = relativeAge(now - snap.ts);
    const thumb = document.createElement("img");
    thumb.className = "hist-thumb" + (pastView === snap ? " on" : "");
    thumb.src = `data:image/png;base64,${snap.png}`;
    thumb.alt = snap.view;
    thumb.title = `${snap.banner}  ·  ${age}`;
    thumb.onclick = () => viewPastRun(snap);
    strip.appendChild(thumb);
  });
}

// Open a past snapshot read-only in #view (#209). A note says how long ago + how to return; the
// next edit / analyze (paint clears pastView) bounces back to live.
function viewPastRun(snap) {
  if (!snap || !snap.png) return;
  pastView = snap;
  const view = $("#view");
  view.classList.remove("stale", "recomputing");
  const age = relativeAge(Date.now() - snap.ts);
  view.innerHTML = `<div class="past-wrap"><div class="past-note">⟲ past run (${age}) — edit to return to live</div>`
    + `<div class="view-wrap"><img alt="${snap.view}" src="data:image/png;base64,${snap.png}"></div></div>`;
  $("#view-caption").textContent = snap.banner || "";
  renderHistory();   // re-outline the active thumbnail
}

// 📌 toggle (#207): capture the most-recent live result as A, or unpin if already pinned. We pin
// the newest history snapshot (it mirrors the current live result), so A is a real drawable run.
function togglePin() {
  if (pinnedA) { setPinned(null); return; }
  const snap = history.find((s) => s.png);
  if (!snap) return;                 // nothing drawable to pin yet — no-op
  setPinned(snap);
}

function setPinned(snap) {
  pinnedA = snap;
  const btn = $("#pin-btn");
  if (btn) { btn.classList.toggle("on", !!snap); btn.textContent = snap ? "📌 unpin" : "📌 pin"; }
  // re-render the live view in the new layout, using the freshest history snapshot as B.
  if (!pastView && history.length) renderLiveView($("#view"), history[0]);
}

// On error / claim / backend-down we must not leave a two-up or a past view over a dead/changed
// backend (degrade gracefully). Drop back to single-view mode; history itself is preserved.
function exitCompareModes() {
  pastView = null;
  if (pinnedA) setPinned(null);
}

// The backend (solver) is unreachable OR returned an error status — it crashed or was stopped.
// NEVER leave the prior picture/verdict looking live (Ana #202, Marek #206): mark everything stale,
// hide the verdict, and say so loudly so a stale diagram is never mistaken for the current program's.
function backendDown(detail) {
  clearTimeout(_dimTimer); clearInterval(_elapsedTimer);
  exitCompareModes();                                    // don't show a stale two-up / past view over a dead backend
  setStatus("backend down", "err");
  $("#banner").className = "stale";
  $("#banner").textContent = "⚠ backend unavailable — the solver isn't responding";
  $("#structure").hidden = true; $("#invariant").hidden = true;
  $("#tabs").innerHTML = "";
  $("#view-caption").textContent = "";                   // no live diagram → no caption
  // BLANK the diagram entirely — a greyed-but-plausible picture (with its old title) can still read
  // as a believable lie when the backend is dead (Marek #177). Replace it with a clear placeholder.
  $("#view").classList.remove("recomputing", "stale");
  $("#view").innerHTML = '<div class="ph">⚠ backend unreachable — no live diagram.<br>Restart the server, then edit to refresh.</div>';
  $("#errors").hidden = false;
  $("#errors").textContent = "Could not reach the backend (it may have crashed or been stopped). "
    + "The picture above is stale. Restart it:\n\n    ./ide/web/run.sh   (or  python3 ide/web/server.py)\n\n(" + detail + ")";
}

async function run(view) {
  const source = editor.getValue();
  lastSource = source;
  // A saved-slot name (set on Save / on opening a slot) wins over the derived declaration
  // name — the user named this buffer, so honor it. Cleared when a sample/slot loads fresh.
  if (currentSlotName) {
    $("#fname").textContent = currentSlotName.replace(/\.ev$/, "") + ".ev";
  } else {
    const nm = source.match(/^\s*(?:fsm|claim|type|schema)\s+([A-Za-z_]\w*)/m);
    $("#fname").textContent = (nm ? nm[1] : "untitled") + ".ev";
  }
  setStatus("computing…", "busy");
  // Immediately mark the derived panels recomputing — the PREVIOUS program's Structure verdict,
  // verify result and solve witness must NEVER read as current while a new analysis runs, on a
  // switch / edit / error alike (Marek #64/#91/#93). paint() repaints or hides them on result.
  $("#banner").classList.add("recomputing");
  $("#structure").classList.add("recomputing");
  $("#view").classList.add("recomputing");                 // dim the OLD picture, not just the banner
  $("#inv-result").textContent = "";                       // last verify result is stale on any edit
  clearTrace();                                            // …and so is the counterexample scrubber
  if (!$("#solve").hidden) {                                // stale witness/UNSAT under a changed source
    $("#solve-head").innerHTML = '<span class="dim">source changed — press re-solve</span>';
    $("#solve-body").classList.add("stale");               // grey the board too, like #view (Sam #211)
  }
  const t0 = performance.now();
  // A live elapsed counter so a multi-second solve (real-valued / high-fanout FSMs run 1–8s) reads
  // as WORKING, not frozen (Ana/Marek #202). Only kicks in after 400ms so fast analyses don't flicker.
  clearInterval(_elapsedTimer);
  _elapsedTimer = setInterval(() => {
    const s = (performance.now() - t0) / 1000;
    if (s > 0.4) setStatus(`solving… ${s.toFixed(1)}s`, "busy");
  }, 100);
  try {
    const res = await fetch("/api/analyze", {
      method: "POST", headers: { "content-type": "application/json" },
      // A source edit (run() with no view) sends null so the server RE-RECOMMENDS the
      // lead view for what was just written — otherwise a tab click pins the view and a
      // later edit that turns the machine nondeterministic keeps showing a flat line.
      // A tab click (run("phase_portrait")) passes its view explicitly and is honored.
      body: JSON.stringify({ source, view: view || null }),
    });
    // A 500 RESOLVES the fetch (only a network drop rejects it), so without this check an HTTP
    // error would fall through and silently leave the prior picture looking live (Marek #206).
    if (!res.ok) { backendDown(`the solver returned HTTP ${res.status} — it likely crashed on that input`); return; }
    const data = await res.json();
    paint(data, Math.round(performance.now() - t0));
  } catch (e) {
    backendDown(String(e));
  }
}

// Persist + debounced analyze, driven from the single session 'change' handler above.
function scheduleAnalyze() {
  try { localStorage.setItem("evident-buffer", editor.getValue()); } catch (e) {}
  clearTimeout(timer); timer = setTimeout(() => run(), 350);
}

// --- solve/query: run a claim → SAT witness or UNSAT; pin vars for solve-for-X --------
function parsePins(s) {
  const given = {};
  (s || "").split(",").forEach((pair) => {
    const eq = pair.indexOf("=");
    if (eq > 0) { const k = pair.slice(0, eq).trim(); if (k) given[k] = pair.slice(eq + 1).trim(); }
  });
  return given;
}

function escapeHtml(s) { return (s || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;"); }

function renderSolve(d, given) {
  const head = $("#solve-head"), body = $("#solve-body");
  body.classList.remove("stale");                          // fresh result — undim (Sam #211)
  const pinned = Object.keys(given || {});
  if (!d.ok) { head.innerHTML = `<span class="bad">✕ ${escapeHtml(d.error || "query failed")}</span>`; body.innerHTML = ""; return; }

  // enumeration — a list of distinct witnesses, with exhaustive/▸limit honesty
  if (d.solutions) {
    const n = d.count != null ? d.count : d.solutions.length;
    if (!n) { head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${d.claim || "claim"}</b> has no solutions`; body.innerHTML = ""; return; }
    head.innerHTML = `<span class="sat">⊨ ${d.complete ? "all " + n : "≥ " + n}</span> distinct witness${n === 1 ? "" : "es"} of <b>${d.claim || "claim"}</b>`
      + (d.complete ? ` <span class="dim">(complete — the solver exhausted the space)</span>`
                    : ` <span class="dim">(showing ${n}; stopped at the limit, more may exist)</span>`);
    body.innerHTML = d.solutions.map((s, i) =>
      `<div class="sol"><span class="dim">#${i + 1}</span> `
      + Object.keys(s).sort().map((k) => `${k}=${escapeHtml(JSON.stringify(s[k]))}`).join("&nbsp;&nbsp;") + `</div>`).join("");
    return;
  }

  // UNSAT — with a delta-debugged core (which constraints conflict)
  if (d.satisfied === false) {
    head.innerHTML = `<span class="unsat">⊭ UNSAT</span> — <b>${d.claim || "claim"}</b> has no solution`
      + (pinned.length ? ` <span class="dim">with ${pinned.join(", ")} pinned</span>` : "");
    body.innerHTML = (d.core && d.core.length)
      ? `<div class="dim">conflicting core — removing any one of these makes it solvable:</div>`
        + `<table>${d.core.map((c) => `<tr><td class="k">line ${c.line}</td><td class="v">${escapeHtml(c.text)}</td></tr>`).join("")}</table>`
      : `<span class="dim">no assignment satisfies the constraints${pinned.length ? " under those pins — try different ones." : "."}</span>`;
    return;
  }

  // single SAT witness
  head.innerHTML = `<span class="sat">⊨ SAT</span> — <b>${d.claim || "claim"}</b> has a witness`
    + (pinned.length ? ` <span class="dim">(pinned: ${pinned.join(", ")})</span>` : "");
  const keys = Object.keys(d.bindings || {}).sort();
  if (!keys.length) { body.innerHTML = '<span class="dim">satisfiable (no free variables to report)</span>'; return; }
  // Domain picture(s): any Seq binding draws as a board / cell strip ABOVE the raw table
  // (Task #68) — a beginner can't read positional arrays as a solution.
  const src = (typeof editor !== "undefined") ? editor.getValue() : "";
  // A var that draws as a domain picture (board / grid / record table) is shown ONLY as that
  // picture — the raw JSON row underneath read like a debug dump (Marek #204). Scalars (no
  // picture) keep their row; the picture IS the source of truth for the rest.
  const vizByKey = {};
  keys.forEach((k) => { const v = seqViz(k, d.bindings[k], src); if (v) vizByKey[k] = v; });
  const viz = keys.map((k) => vizByKey[k]).filter(Boolean).join("");
  const rawKeys = keys.filter((k) => !vizByKey[k]);
  body.innerHTML = (viz ? `<div class="viz-wrap">${viz}</div>` : "")
    + (rawKeys.length ? `<table>${rawKeys.map((k) => `<tr><td class="k">${k}${pinned.includes(k) ? " 📌" : ""}</td>`
        + `<td class="v">${escapeHtml(JSON.stringify(d.bindings[k]))}</td></tr>`).join("")}</table>` : "");
}

// --- domain-picture rendering for Seq witnesses (Task #68 / #196) -----------------
// A Seq witness is hard to read as a flat array. Draw it as the domain shape it is:
//   • record-Seq (array of objects)            → a small TABLE, one row per element
//   • sudoku-shaped Int-Seq (length K², 1..K)  → a K×K filled grid
//   • N-queens column-Seq (permutation + name) → a chessboard with pieces
//   • anything else                            → the honest index→value cell strip
// Shapes are detected from the witness itself plus the source (`#name = N`), generically —
// no sample names are hardcoded.
function seqViz(name, val, source) {
  if (!Array.isArray(val) || !val.length) return null;
  const n = val.length;

  // record-Seq: every element is a plain object (a record). Render columns = field names.
  if (val.every((v) => v && typeof v === "object" && !Array.isArray(v))) {
    return recordTable(name, val);
  }

  // only primitive-Int seqs get a numeric picture; mixed/non-int seqs fall through.
  if (!val.every((v) => typeof v === "number" && Number.isInteger(v))) return null;

  // A queens board needs TWO honest signals, not just "values in 0..N-1" — that also matches a
  // topological order (pos=[0,1,2,3,4]) and a sudoku row, which would draw a wrong chessboard
  // (Marek #68/#92). Require: a queens-like variable NAME *and* a true permutation of 0..N-1
  // (one queen per row AND column).
  const queensName = /^(col|cols|queen|queens|row|rows|board)$/.test(name.toLowerCase());
  const isPermutation = n >= 4 && new Set(val).size === n && val.every((v) => v >= 0 && v < n);
  if (queensName && isPermutation) return queensBoard(name, val);

  // sudoku-shaped: a flat Int-Seq whose length is a perfect square K² (4, 9, 16, 25),
  // with every value a single symbol in 1..K (or 0..K-1). Reshape it into the K×K grid the
  // values already imply — Sam shouldn't reshape 16 index=value lines in his head.
  const k = Math.round(Math.sqrt(n));
  if (k >= 2 && k * k === n) {
    const min = Math.min(...val), max = Math.max(...val);
    const oneBased = min >= 1 && max <= k;        // 1..K (the canonical sudoku numbering)
    const zeroBased = min >= 0 && max <= k - 1;   // 0..K-1
    if (oneBased || zeroBased) return sudokuGrid(name, val, k);
  }

  return cellStrip(name, val);
}

// One row per element, one column per record field. Replaces a raw-JSON array of objects with a
// scannable table (subset-sum's {weight, take} items, toposort's {from, to} edges, sudoku boxes).
function recordTable(name, rows) {
  // union of field names across rows, in first-seen order (rows are homogeneous in practice).
  const cols = [];
  rows.forEach((r) => Object.keys(r).forEach((c) => { if (!cols.includes(c)) cols.push(c); }));
  const fmt = (v) =>
    typeof v === "boolean" ? (v ? "✓" : "·")
      : (v && typeof v === "object") ? escapeHtml(JSON.stringify(v))
      : escapeHtml(String(v));
  const head = `<tr><th>#</th>${cols.map((c) => `<th>${escapeHtml(c)}</th>`).join("")}</tr>`;
  const trs = rows.map((r, i) =>
    `<tr><td class="rt-i">${i}</td>`
    + cols.map((c) => `<td>${c in r ? fmt(r[c]) : ""}</td>`).join("") + `</tr>`).join("");
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} `
    + `<span class="dim">(${rows.length} × {${cols.map(escapeHtml).join(", ")}})</span></div>`
    + `<table class="rec-table">${head}${trs}</table></div>`;
}

// A flat Int-Seq reshaped into the K×K grid its values imply (sudoku / latin-square style).
function sudokuGrid(name, vals, k) {
  let cells = "";
  for (let r = 0; r < k; r++) {
    for (let c = 0; c < k; c++) {
      // subgrid shading when K is itself a perfect square (4→2×2 boxes, 9→3×3) — purely visual.
      const sub = Math.round(Math.sqrt(k));
      const boxed = sub * sub === k && (Math.floor(r / sub) + Math.floor(c / sub)) % 2 === 1;
      cells += `<div class="scell${boxed ? " box" : ""}">${escapeHtml(String(vals[r * k + c]))}</div>`;
    }
  }
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} — ${k}×${k} grid`
    + ` <span class="dim">(${escapeHtml(name)}[r·${k}+c] → cell at row r, col c)</span></div>`
    + `<div class="sgrid" style="grid-template-columns:repeat(${k},1fr)">${cells}</div></div>`;
}

// `#name = N` in the source → N (the pinned Seq length), else null.
function pinnedLen(source, name) {
  const m = (source || "").match(new RegExp("#\\s*" + name.replace(/[.*+?^${}()|[\\]\\\\]/g, "\\$&") + "\\s*=\\s*(\\d+)"));
  return m ? parseInt(m[1], 10) : null;
}

function queensBoard(name, cols) {
  const n = cols.length;
  let cells = "";
  for (let r = 0; r < n; r++) {
    for (let c = 0; c < n; c++) {
      const dark = (r + c) % 2 === 1;
      const q = cols[r] === c;
      cells += `<div class="qsq${dark ? " dark" : ""}${q ? " q" : ""}">${q ? "♛" : ""}</div>`;
    }
  }
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} — ${n}×${n} board`
    + ` <span class="dim">(row i → queen at column ${escapeHtml(name)}[i])</span></div>`
    + `<div class="qboard" style="grid-template-columns:repeat(${n},1fr)">${cells}</div></div>`;
}

function cellStrip(name, arr) {
  const cells = arr.map((v, i) =>
    `<div class="cell"><div class="cell-idx">${i}</div><div class="cell-val">${escapeHtml(String(v))}</div></div>`).join("");
  return `<div class="viz"><div class="viz-label">${escapeHtml(name)} `
    + `<span class="dim">(index → value)</span></div><div class="strip">${cells}</div></div>`;
}

async function solve(enumerate) {
  const source = editor.getValue();
  const given = parsePins($("#solve-given").value);
  // Name the claim explicitly so the solver doesn't choke on "ambiguous" when the file also
  // declares a type/enum (e.g. toposort's `type Edge` + `claim toposort`).
  const cm = source.match(/^\s*claim\s+([A-Za-z_]\w*)/m);
  const claim = cm ? cm[1] : null;
  $("#solve").hidden = false;
  $("#solve-head").innerHTML = `<span class="dim">${enumerate ? "enumerating…" : "solving…"}</span>`;
  $("#solve-body").innerHTML = "";
  try {
    const res = await fetch("/api/solve", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source, claim, given, enumerate: !!enumerate, limit: 20 }),
    });
    renderSolve(await res.json(), given);
  } catch (e) {
    $("#solve-head").innerHTML = `<span class="bad">solve failed: ${e}</span>`;
  }
}

$("#solve-btn").onclick = () => solve(false);
$("#solve-resolve").onclick = () => solve(false);

// Export the SMT-LIB encoding (Ana #200): copy to clipboard so you can re-run it in z3 / paste it
// into notes; fall back to a .smt2 download where the clipboard is blocked.
$("#smtlib-btn").onclick = async () => {
  setStatus("exporting SMT-LIB…", "busy");
  try {
    const res = await fetch("/api/smtlib", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source: editor.getValue() }),
    });
    if (!res.ok) { backendDown(`the solver returned HTTP ${res.status}`); return; }
    const d = await res.json();
    if (!d.ok) { setStatus("✕ " + (d.error || "export failed"), "err"); return; }
    try {
      await navigator.clipboard.writeText(d.smtlib);
      setStatus("SMT-LIB copied to clipboard ✓", "ok");
    } catch (_) {                                       // clipboard blocked → download instead
      const a = document.createElement("a");
      a.href = URL.createObjectURL(new Blob([d.smtlib], { type: "text/plain" }));
      a.download = ($("#fname").textContent || "model").replace(/\.ev$/, "") + ".smt2";
      a.click(); URL.revokeObjectURL(a.href);
      setStatus("SMT-LIB downloaded ✓", "ok");
    }
  } catch (e) { setStatus("✕ " + e, "err"); }
};
$("#solve-all").onclick = () => solve(true);
if ($("#pin-btn")) $("#pin-btn").onclick = () => togglePin();

// --- save / export / share actions (Task #213) ------------------------------------
// "Save as…" prompts for a slot name, stores {name → source} in the evident-slots map, makes
// it the active buffer name, and re-renders the dropdown so it's immediately re-openable.
function saveAsPrompt() {
  const suggested = currentSlotName || ($("#fname").textContent || "untitled.ev").replace(/\.ev$/, "");
  const name = (window.prompt("Save program as:", suggested) || "").trim();
  if (!name) return;
  saveSlot(name, editor.getValue());
  currentSlotName = name;
  $("#fname").textContent = name.replace(/\.ev$/, "") + ".ev";
  refreshSamplesMenu();
  setStatus("saved “" + name + "” ✓", "ok");
}
function deletePrompt() {
  const slots = loadSlots(), keys = Object.keys(slots).sort();
  if (!keys.length) { setStatus("no saved programs to delete", "dim"); return; }
  const name = (window.prompt("Delete which saved program?\n" + keys.join(", "), keys[0]) || "").trim();
  if (!name) return;
  if (slots[name] == null) { setStatus("no saved program named “" + name + "”", "err"); return; }
  deleteSlot(name);
  if (currentSlotName === name) currentSlotName = null;
  refreshSamplesMenu();
  setStatus("deleted “" + name + "” ✓", "ok");
}
function exportEv() {
  const name = ($("#fname").textContent || "model").replace(/\.ev$/, "") || "model";
  const a = document.createElement("a");
  a.href = URL.createObjectURL(new Blob([editor.getValue()], { type: "text/plain" }));
  a.download = name + ".ev";
  a.click(); URL.revokeObjectURL(a.href);
  setStatus("exported " + name + ".ev ✓", "ok");
}
async function copyShareLink() {
  const url = location.origin + location.pathname + "#src=" + encodeShare(editor.getValue());
  try {
    await navigator.clipboard.writeText(url);
    setStatus("share link copied ✓", "ok");
  } catch (_) {                                      // clipboard blocked → put it in the hash so it's at least visible
    location.hash = "src=" + encodeShare(editor.getValue());
    setStatus("share link in the address bar — copy the URL", "ok");
  }
}
if ($("#save-btn"))   $("#save-btn").onclick   = () => saveAsPrompt();
if ($("#export-btn")) $("#export-btn").onclick = () => exportEv();
if ($("#share-btn"))  $("#share-btn").onclick  = () => copyShareLink();

// Assert-and-check a safety invariant over the reachable set — verify `var op value` holds on
// EVERY reachable state (a proof when the set is finite & fully explored), or get a reachable
// counterexample. The other half of the relational pitch: not just "watch", but "prove".
const _INV_RE = /^\s*([A-Za-z_]\w*(?:\.\w+)?)\s*(<=|>=|!=|<|>|=|≤|≥|≠)\s*(.+?)\s*$/;
// two-sided range — lo (<|≤) var (<|≤) hi — the canonical invariant shape (Marek #156)
const _INV_RANGE = /^\s*(-?\d+(?:\.\d+)?)\s*(<=|<|≤)\s*([A-Za-z_]\w*(?:\.\w+)?)\s*(<=|<|≤)\s*(-?\d+(?:\.\d+)?)\s*$/;
function _coerce(s) {
  if (/^-?\d+$/.test(s)) return parseInt(s, 10);
  if (/^-?\d*\.\d+$/.test(s)) return parseFloat(s);
  if (s === "true" || s === "false") return s === "true";
  return s;
}
async function _checkOne(varName, op, value) {
  const res = await fetch("/api/invariant", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ source: editor.getValue(), var: varName, op, value }),
  });
  return res.json();
}
async function checkInvariant() {
  const out = $("#inv-result");
  clearTrace();                              // a new check invalidates the old scrubber
  const raw = $("#inv-prop").value.trim();
  if (!raw) { out.textContent = ""; return; }
  // LIVENESS first: P ⤳ Q (leads-to), or ◇/eventually Q — routed to the temporal checker (#142).
  const lt = raw.split(/\s*(?:⤳|~>|\bleads to\b)\s*/);
  if (lt.length === 2) {
    const P = lt[0].match(_INV_RE), Q = lt[1].match(_INV_RE);
    if (!P || !Q) { out.className = "bad"; out.textContent = "✕ leads-to: write  P ⤳ Q  (e.g. mode = Coining ⤳ mode = Idle)"; return; }
    return runTemporal(out, { var: Q[1], op: Q[2], value: _coerce(Q[3]), modality: "leads_to",
                              p_var: P[1], p_op: P[2], p_value: _coerce(P[3]) });
  }
  const ev = raw.match(/^\s*(?:◇|eventually)\s+(.+)$/i);
  if (ev) {
    const Q = ev[1].match(_INV_RE);
    if (!Q) { out.className = "bad"; out.textContent = "✕ eventually: write  ◇ var op value  (e.g. ◇ done = true)"; return; }
    return runTemporal(out, { var: Q[1], op: Q[2], value: _coerce(Q[3]), modality: "eventually" });
  }
  // SAFETY (□): a two-sided range becomes TWO predicates (var ≥ lo ∧ var ≤ hi); else a single comparison.
  let preds;
  const rg = raw.match(_INV_RANGE);
  if (rg) {
    const [, lo, lop, varName, hop, hi] = rg;
    preds = [[varName, lop === "<" ? ">" : ">=", _coerce(lo)], [varName, hop, _coerce(hi)]];
  } else {
    const mt = raw.match(_INV_RE);
    if (!mt) { out.className = "bad"; out.textContent = "✕ write  var op value  (e.g. count ≤ 5  or  0 ≤ x ≤ 6)"; return; }
    preds = [[mt[1], mt[2], _coerce(mt[3])]];
  }
  out.className = "dim"; out.textContent = "checking…";
  try {
    let checked = 0, exhaustive = true; const texts = [];
    for (const [varName, op, value] of preds) {
      const d = await _checkOne(varName, op, value);
      if (!d.ok) { out.className = "bad"; out.textContent = "✕ " + (d.error || "check failed"); return; }
      texts.push(d.predicate); checked = Math.max(checked, d.checked || 0); exhaustive = exhaustive && d.exhaustive;
      if (!d.holds) {
        const cex = Object.entries(d.counterexample || {}).map(([k, v]) => `${k}=${v}`).join(", ");
        const tr = _fmtTrace(d.trace);
        out.className = "bad";
        out.textContent = `✗ violated (${d.predicate}) — counterexample  ${cex}` + (tr ? `   ·   trace: ${tr}` : "");
        if (d.trace && d.trace.length >= 2) showTrace(d.trace, d.predicate);
        return;
      }
    }
    out.className = "good";
    out.textContent = (exhaustive ? "✓ proven" : "✓ holds (bounded)")
      + ` — ${texts.join(" ∧ ")} on all ${checked} reachable states`;
  } catch (e) { out.className = "bad"; out.textContent = "✕ " + e; }
}
// A counterexample run as a compact trace: init → … → the offending state (Ana #173/#175).
function _fmtTrace(trace) {
  if (!trace || trace.length < 2) return "";
  const steps = trace.map((s) => Object.entries(s).map(([k, v]) => `${k.split(".").pop()}=${v}`).join(" "));
  return steps.length > 8 ? steps.slice(0, 4).join(" → ") + " → … → " + steps[steps.length - 1]
                          : steps.join(" → ");
}

// --- scrubbable counterexample trace (TLA+-Toolbox style, Ana #198/#120/#175) ----------
// The trace array is the BFS path init→violation (safety) or the dodging/lasso run (liveness).
// Step through it one state at a time, reading the FULL assignment at each step — not the
// one-line collapse. Pure helpers (_traceClamp / _traceStepLabel / _traceStateLine) carry the
// index + format logic so they're unit-testable without a DOM.
function _traceClamp(i, n) { return i < 0 ? 0 : (i > n - 1 ? n - 1 : i); }
function _traceStepLabel(i, n) { return `step ${i + 1} / ${n}`; }       // 1-based for humans
function _traceStateLine(state) {
  return Object.entries(state || {}).map(([k, v]) => `${k.split(".").pop()} = ${v}`).join("   ");
}
const _trace = { states: [], i: 0, label: "" };
function clearTrace() {
  _trace.states = []; _trace.i = 0; _trace.label = "";
  const el = $("#inv-trace"); el.hidden = true; el.innerHTML = "";
}
function _renderTrace() {
  const el = $("#inv-trace"), n = _trace.states.length;
  if (n < 2) { el.hidden = true; el.innerHTML = ""; return; }
  const i = _trace.i, last = i === n - 1;
  el.hidden = false;
  el.innerHTML = "";
  const head = document.createElement("div"); head.className = "trace-head";
  if (_trace.label) { const lab = document.createElement("span"); lab.className = "trace-label"; lab.textContent = _trace.label; head.appendChild(lab); }
  const prev = document.createElement("button"); prev.className = "trace-nav"; prev.textContent = "◀"; prev.disabled = i === 0;
  prev.onclick = () => { _trace.i = _traceClamp(_trace.i - 1, n); _renderTrace(); };
  const step = document.createElement("span"); step.className = "trace-step"; step.textContent = _traceStepLabel(i, n);
  const next = document.createElement("button"); next.className = "trace-nav"; next.textContent = "▶"; next.disabled = last;
  next.onclick = () => { _trace.i = _traceClamp(_trace.i + 1, n); _renderTrace(); };
  head.appendChild(prev); head.appendChild(step); head.appendChild(next);
  if (last) { const flag = document.createElement("span"); flag.className = "trace-flag"; flag.textContent = "● violation here"; head.appendChild(flag); }
  el.appendChild(head);
  const line = document.createElement("div");
  line.className = "trace-state" + (last ? " bad" : "");
  line.textContent = _traceStateLine(_trace.states[i]);
  el.appendChild(line);
}
// Open the stepper on a fresh trace, parked at the violating (final) step.
function showTrace(trace, label) {
  if (!trace || trace.length < 2) { clearTrace(); return; }
  _trace.states = trace; _trace.i = trace.length - 1; _trace.label = label || "";
  _renderTrace();
}
// Liveness check (◇ / ⤳) against /api/temporal, with the dodging-run trace on failure.
async function runTemporal(out, body) {
  clearTrace();
  out.className = "dim"; out.textContent = "checking…";
  try {
    const res = await fetch("/api/temporal", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source: editor.getValue(), ...body }),
    });
    const d = await res.json();
    if (!d.ok) { out.className = "bad"; out.textContent = "✕ " + (d.error || "check failed"); return; }
    if (d.holds) {
      out.className = "good";
      out.textContent = (d.exhaustive ? "✓ proven" : "✓ holds (bounded)")
        + ` — ${d.predicate} on all ${d.checked} reachable states`;
    } else {
      const tr = _fmtTrace(d.trace);
      out.className = "bad";
      out.textContent = `✗ violated — ${d.predicate}; a run dodges it forever`
        + (tr ? `:  ${tr}` : "");
      if (d.trace && d.trace.length >= 2) showTrace(d.trace, "a run that dodges it forever");
    }
  } catch (e) { out.className = "bad"; out.textContent = "✕ " + e; }
}
$("#inv-btn").onclick = checkInvariant;
// The ⊢ verify box accepts the SAME typable shortcuts as the editor — a newcomer who learned
// `\ge → ≥` / `>=` shouldn't get bounced when they reuse it here (Sam #212/#160).
function expandFieldSymbols(el) {
  const v = el.value, pos = el.selectionStart;
  const before = v.slice(0, pos);
  let start = -1, rep = "";
  const bs = before.match(/\\([a-zA-Z]+)([^a-zA-Z])$/);     // \word + a just-typed non-letter
  if (bs && UNI[bs[1]]) { start = pos - bs[0].length; rep = UNI[bs[1]] + bs[2]; }
  else if (OP_PAIRS[before.slice(-2)]) { start = pos - 2; rep = OP_PAIRS[before.slice(-2)]; }
  if (start >= 0) {
    el.value = v.slice(0, start) + rep + v.slice(pos);
    el.setSelectionRange(start + rep.length, start + rep.length);
  }
}
$("#inv-prop").addEventListener("input", () => expandFieldSymbols($("#inv-prop")));
$("#solve-given").addEventListener("input", () => expandFieldSymbols($("#solve-given")));
$("#inv-prop").addEventListener("keydown", (e) => { if (e.key === "Enter") checkInvariant(); });
$("#solve-close").onclick = () => { $("#solve").hidden = true; };
$("#solve-given").addEventListener("keydown", (e) => { if (e.key === "Enter") solve(false); });

// --- samples menu: open a worked example, or one of your saved programs ------------
// Built-in samples and the user's saved slots share this one dropdown; saved slots live under
// a "── my saved ──" optgroup with slot:-prefixed values so they can't collide with a sample
// name. refreshSamplesMenu() rebuilds it after a save/delete so a just-saved program appears.
const sel = $("#samples");
function refreshSamplesMenu() {
  const slots = loadSlots();
  const slotKeys = Object.keys(slots).sort();
  let html = '<option value="">open sample…</option>' +
    Object.keys(SAMPLES).map((k) => `<option value="${escapeHtml(k)}">${escapeHtml(k)}</option>`).join("");
  if (slotKeys.length) {
    html += '<optgroup label="── my saved ──">' +
      slotKeys.map((k) => `<option value="slot:${escapeHtml(k)}">${escapeHtml(k)}</option>`).join("") +
      '</optgroup>';
  }
  sel.innerHTML = html;
}
refreshSamplesMenu();
// Load a program into the editor and clear every per-program panel that must not bleed across
// (pin, solve board, verify assertion + result, counterexample scrubber). Shared by samples,
// slots, and the shared-link loader.
function loadProgram(source, slotName) {
  currentSlotName = slotName || null;
  editor.setValue(source, -1);
  $("#solve-given").value = "";   // a fresh program must not inherit the last pin…
  $("#solve").hidden = true;       // …nor leave a stale UNSAT/witness over the new program
  $("#inv-prop").value = "";       // …nor a stale verify assertion (Sam #107)
  $("#inv-result").textContent = "";
  clearTrace();
  run();
}
sel.onchange = () => {
  const v = sel.value;
  if (v.startsWith("slot:")) {
    const name = v.slice(5), slots = loadSlots();
    if (slots[name] != null) loadProgram(slots[name], name);
  } else if (SAMPLES[v]) {
    loadProgram(SAMPLES[v], null);
  }
  sel.value = "";          // reset the label so the same entry can be re-opened
};

// --- symbol palette / cheat-sheet (Task #62) --------------------------------------
// A blank-editor newcomer can't discover how to type ∈/⇒/Δ — the hover glossary only
// fires on glyphs already in the buffer. This popover lists every typable operator with
// BOTH mnemonics; clicking a row inserts the glyph at the cursor. Esc dismisses it.
const PALETTE = [
  ["∈", "membership / typing",  "\\in  or  in"],
  ["⇒", "implies",              "\\imp / =>  or  implies"],
  ["⟸", "reverse-implies",      "\\when  or  when"],
  ["∀", "for-all",              "\\all  or  forall"],
  ["∃", "there-exists",         "\\exists  or  exists"],
  ["¬", "not",                  "\\neg  or  not"],
  ["∧", "and",                  "\\and  or  and"],
  ["∨", "or",                   "\\or  or  or"],
  ["≤", "less-or-equal",        "\\le  or  <="],
  ["≥", "greater-or-equal",     "\\ge  or  >="],
  ["≠", "not-equal",            "\\ne  or  !="],
  ["Δ", "forward difference",   "\\Delta  or  delta"],
  ["↦", "maps-to / rename",     "\\mapsto"],
  ["→", "to",                   "\\to"],
  ["⟨", "seq-literal open",     "\\langle"],
  ["⟩", "seq-literal close",    "\\rangle"],
  ["∪", "set union",            "\\cup"],
  ["∩", "set intersection",     "\\cap"],
];
const palette = document.createElement("div");
palette.id = "palette"; palette.hidden = true;
palette.innerHTML =
  '<div class="palette-head">type these operators — click a row to insert it'
  + ' <span class="dim">(Esc closes)</span></div>'
  + PALETTE.map(([g, name, mn], i) =>
      `<div class="palette-row" data-i="${i}">`
      + `<span class="palette-glyph">${escapeHtml(g)}</span>`
      + `<span class="palette-name">${escapeHtml(name)}</span>`
      + `<span class="palette-mn dim">${escapeHtml(mn)}</span></div>`).join("");
document.body.appendChild(palette);

function insertGlyph(glyph) {
  editor.session.insert(editor.getCursorPosition(), glyph);
  editor.focus();
}
function togglePalette(show) {
  const open = show != null ? show : palette.hidden;
  palette.hidden = !open;
  $("#symbols-btn").classList.toggle("on", open);
}
palette.addEventListener("click", (e) => {
  const row = e.target.closest(".palette-row");
  if (!row) return;
  insertGlyph(PALETTE[+row.dataset.i][0]);   // keep the palette open for multi-insert
});
$("#symbols-btn").onclick = (e) => { e.stopPropagation(); togglePalette(); };
// dismiss: Esc anywhere, or a click outside the popover/button
document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !palette.hidden) togglePalette(false); });
document.addEventListener("click", (e) => {
  if (!palette.hidden && !palette.contains(e.target) && e.target.id !== "symbols-btn") togglePalette(false);
});

// --- command palette + keyboard shortcuts (Task #182) -----------------------------
// The app was mouse-only for everything but typing — every critic reached for a Cmd-K
// command menu and shortcuts for Solve / comment-toggle. This adds a centered, fuzzy-
// filtered command overlay (↑/↓ Enter Esc) plus three global chords. Commands are built
// FRESH on each open so the live #tabs views and any missing target element are honored;
// a command whose target is gone is simply skipped, never throws.

const $$ = (s) => Array.from(document.querySelectorAll(s));

// @pure-helpers-start  (sliced + eval'd by the headless node test — keep self-contained)
// Subsequence fuzzy match: every char of `q` appears in `label` in order (case-insensitive).
// Returns the matched index list (for highlighting) or null when it doesn't match. An empty
// query matches everything with an empty (no-highlight) index list.
function fuzzyMatch(label, q) {
  const text = String(label).toLowerCase();
  const query = String(q || "").toLowerCase().replace(/\s+/g, "");
  if (!query) return [];
  const idx = [];
  let j = 0;
  for (let i = 0; i < text.length && j < query.length; i++) {
    if (text[i] === query[j]) { idx.push(i); j++; }
  }
  return j === query.length ? idx : null;
}

// Line-comment toggle over a block of source lines (the rows the selection spans). If EVERY
// non-blank line already starts with a `--` comment prefix, strip it; otherwise add `-- `.
// Blank lines are untouched. Pure: takes/returns an array of strings, so it's unit-testable
// without Ace. Mirrors the Evident `-- ` comment convention.
function toggleCommentLines(lines) {
  const prefix = "-- ";
  const codeLines = lines.filter((l) => l.trim() !== "");
  const allCommented = codeLines.length > 0
    && codeLines.every((l) => /^\s*-- ?/.test(l));
  return lines.map((l) => {
    if (l.trim() === "") return l;
    if (allCommented) return l.replace(/^(\s*)-- ?/, "$1");
    return l.replace(/^(\s*)/, "$1" + prefix);
  });
}
// @pure-helpers-end

// Apply the comment toggle to the editor's currently selected rows (or the cursor's line).
function toggleCommentSelection() {
  const sess = editor.session;
  const range = editor.getSelectionRange();
  const startRow = range.start.row;
  // a selection ending exactly at column 0 of a later row shouldn't pull in that empty next line
  const endRow = range.end.column === 0 && range.end.row > startRow
    ? range.end.row - 1 : range.end.row;
  const rows = [];
  for (let r = startRow; r <= endRow; r++) rows.push(sess.getLine(r));
  const out = toggleCommentLines(rows);
  const Range = ace.require("ace/range").Range;
  for (let r = startRow; r <= endRow; r++) {
    const cur = sess.getLine(r), next = out[r - startRow];
    if (next !== cur) sess.replace(new Range(r, 0, r, cur.length), next);
  }
  editor.focus();
}

// Build the live command list. Each command: { label, run }. Targets are resolved at build
// time; a command whose target element is missing is omitted so .run never throws.
function buildCommands() {
  const cmds = [];
  const clickIf = (id) => { const el = $(id); if (el) el.click(); };
  // open a sample — same path as the #samples select onchange
  Object.keys(SAMPLES).forEach((name) => {
    cmds.push({ label: "Open sample: " + name, run: () => loadProgram(SAMPLES[name], null) });
  });
  // open one of your saved programs
  const slots = loadSlots();
  Object.keys(slots).sort().forEach((name) => {
    cmds.push({ label: "Open saved: " + name, run: () => loadProgram(slots[name], name) });
  });
  cmds.push({ label: "Save as… — keep this program in a named slot", run: () => saveAsPrompt() });
  if (Object.keys(slots).length) cmds.push({ label: "Delete saved…", run: () => deletePrompt() });
  cmds.push({ label: "Export .ev — download this buffer as a file", run: () => exportEv() });
  cmds.push({ label: "Copy share link — a URL that loads this program", run: () => copyShareLink() });
  cmds.push({ label: "Solve claim — ⊨ witness or UNSAT", run: () => solve(false) });
  if ($("#smtlib-btn")) cmds.push({ label: "Copy SMT-LIB encoding", run: () => clickIf("#smtlib-btn") });
  if ($("#pin-btn")) cmds.push({ label: pinnedA ? "Unpin compare (A)" : "Pin this result — compare next beside it", run: () => togglePin() });
  if ($("#symbols-btn")) cmds.push({ label: "Symbols palette — how to type ∈ ⇒ Δ", run: () => togglePalette(true) });
  if ($("#tour-btn")) cmds.push({ label: "Guided tour", run: () => startTour() });
  // one command per live view tab (the #tabs strip is rebuilt by paint())
  $$("#tabs .tab").forEach((tab) => {
    const view = tab.textContent.trim().replace(/ /g, "_");
    cmds.push({ label: "View: " + tab.textContent.trim(), run: () => run(view) });
  });
  cmds.push({ label: "Verify — focus the ⊢ property field", run: () => { const f = $("#inv-prop"); if (f) f.focus(); } });
  return cmds;
}

// --- the overlay DOM ---
const cmdk = document.createElement("div");
cmdk.id = "cmdk"; cmdk.hidden = true;
cmdk.innerHTML =
  '<div id="cmdk-box">'
  + '<input id="cmdk-input" placeholder="Type a command…  (open a sample, solve, switch view)" autocomplete="off" spellcheck="false">'
  + '<div id="cmdk-list"></div>'
  + '<div id="cmdk-foot" class="dim">⌘K commands · ⌘⏎ solve · ⌘/ comment · ↑↓ move · ⏎ run · Esc close</div>'
  + '</div>';
document.body.appendChild(cmdk);
const cmdkInput = $("#cmdk-input"), cmdkList = $("#cmdk-list");
let cmdkCommands = [], cmdkFiltered = [], cmdkActive = 0;

function highlightLabel(label, idx) {
  if (!idx || !idx.length) return escapeHtml(label);
  let out = "", set = new Set(idx);
  for (let i = 0; i < label.length; i++) {
    const c = escapeHtml(label[i]);
    out += set.has(i) ? `<b>${c}</b>` : c;
  }
  return out;
}

function renderCmdk() {
  const q = cmdkInput.value;
  cmdkFiltered = [];
  cmdkCommands.forEach((c) => {
    const m = fuzzyMatch(c.label, q);
    if (m !== null) cmdkFiltered.push({ cmd: c, idx: m });
  });
  if (cmdkActive >= cmdkFiltered.length) cmdkActive = Math.max(0, cmdkFiltered.length - 1);
  if (!cmdkFiltered.length) {
    cmdkList.innerHTML = '<div class="cmdk-empty dim">no matching command</div>';
    return;
  }
  cmdkList.innerHTML = cmdkFiltered.map((f, i) =>
    `<div class="cmdk-row${i === cmdkActive ? " on" : ""}" data-i="${i}">${highlightLabel(f.cmd.label, f.idx)}</div>`
  ).join("");
  const on = cmdkList.querySelector(".cmdk-row.on");
  if (on) on.scrollIntoView({ block: "nearest" });
}

function runCmdk(i) {
  const f = cmdkFiltered[i];
  closeCmdk();
  if (f) { try { f.cmd.run(); } catch (e) { /* a stale target — never let the palette throw */ } }
}

function openCmdk() {
  togglePalette(false);                 // don't stack the symbols popover under the modal
  cmdkCommands = buildCommands();
  cmdkInput.value = ""; cmdkActive = 0;
  cmdk.hidden = false;
  renderCmdk();
  cmdkInput.focus();
}

function closeCmdk() { cmdk.hidden = true; editor.focus(); }
function cmdkOpen() { return !cmdk.hidden; }
function toggleCmdk() { cmdkOpen() ? closeCmdk() : openCmdk(); }

cmdkInput.addEventListener("input", () => { cmdkActive = 0; renderCmdk(); });
cmdkInput.addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown") { e.preventDefault(); cmdkActive = Math.min(cmdkActive + 1, cmdkFiltered.length - 1); renderCmdk(); }
  else if (e.key === "ArrowUp") { e.preventDefault(); cmdkActive = Math.max(cmdkActive - 1, 0); renderCmdk(); }
  else if (e.key === "Enter") { e.preventDefault(); runCmdk(cmdkActive); }
  else if (e.key === "Escape") { e.preventDefault(); closeCmdk(); }
});
cmdkList.addEventListener("click", (e) => {
  const row = e.target.closest(".cmdk-row");
  if (row) runCmdk(+row.dataset.i);
});
cmdk.addEventListener("mousedown", (e) => { if (e.target === cmdk) closeCmdk(); });  // backdrop click closes
if ($("#cmdk-hint")) $("#cmdk-hint").onclick = (e) => { e.stopPropagation(); openCmdk(); };

// --- editor-scoped chords (Ace owns keystrokes while the editor is focused) ---
editor.commands.addCommand({
  name: "cmdkPalette", bindKey: { win: "Ctrl-K", mac: "Command-K" },
  exec: () => toggleCmdk(),
});
editor.commands.addCommand({
  name: "cmdkSolve", bindKey: { win: "Ctrl-Enter", mac: "Command-Enter" },
  exec: () => solve(false),
});
editor.commands.addCommand({
  name: "cmdkComment", bindKey: { win: "Ctrl-/", mac: "Command-/" },
  exec: () => toggleCommentSelection(),
});

// --- global chords (fire when focus is OUTSIDE the editor / inputs) ---
// Cmd-K toggles the palette from anywhere. Cmd-Enter / Cmd-/ also work globally, but a plain
// keystroke must NEVER fire a shortcut while you're typing in an input or the editor (those
// paths are handled by the Ace commands / the palette's own keydown above).
document.addEventListener("keydown", (e) => {
  const mod = e.metaKey || e.ctrlKey;
  if (!mod) return;
  const k = e.key.toLowerCase();
  if (k === "k") { e.preventDefault(); toggleCmdk(); return; }
  if (cmdkOpen()) return;                       // the palette's own input owns the rest
  const inField = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement
    || (e.target.closest && e.target.closest("#code"));   // Ace already handles its own chords
  if (inField) return;
  if (k === "enter") { e.preventDefault(); solve(false); }
  else if (k === "/") { e.preventDefault(); toggleCommentSelection(); }
});

// --- guided first-run walkthrough — coachmark tour (Task #164) ---------------------
// A newcomer lands on a loaded sample but no "try this" path. This is a ~4-step lap:
// each step spotlights one target element (translucent backdrop + a ring) and shows a
// small card naming the panel and the one thing to try. Auto-runs ONCE on first visit;
// the "? tour" button replays it. Targets are looked up live; a missing one is skipped.
const TOUR_FLAG = "evident-tour-done";
const SUDOKU_SAMPLE = "4×4 sudoku · fill the grid (⊨ Solve)";
const TOUR_STEPS = [
  { sel: "#editor-pane", title: "1 · The editor",
    body: "Write constraints here. This is a live counter — try changing "
      + "<code>count = 0</code> to <code>count = 3</code>. Operators autocomplete: "
      + "type <code>\\le</code> → ≤." },
  { sel: "#banner", title: "2 · The dynamics panel",
    body: "Every edit re-solves instantly. The banner names your model's SHAPE; the "
      + "diagram below shows the solved behavior — hover the dots to inspect a state." },
  { sel: "#honesty", title: "3 · The honesty line",
    body: "Evident never hides a problem: a dropped constraint is flagged here AND "
      + "marked amber on its line in the editor. That's the silent bug, surfaced." },
  { sel: "#solve-btn", title: "4 · ⊨ Solve",
    body: "Some programs are claims, not machines — press ⊨ Solve for a witness "
      + "assignment (or UNSAT). Try it on the sudoku sample." },
];

let tourIdx = 0;
let tourEls = null;

function buildTourDom() {
  if (tourEls) return tourEls;
  const backdrop = document.createElement("div");
  backdrop.id = "tour-backdrop"; backdrop.hidden = true;
  const ring = document.createElement("div");
  ring.id = "tour-ring"; ring.hidden = true;
  const card = document.createElement("div");
  card.id = "tour-card"; card.hidden = true;
  backdrop.appendChild(ring); backdrop.appendChild(card);
  document.body.appendChild(backdrop);
  // a backdrop click (outside the card) closes the tour without marking it "seen"-only
  backdrop.addEventListener("click", (e) => { if (e.target === backdrop) endTour(); });
  tourEls = { backdrop, ring, card };
  return tourEls;
}

// place the ring over `target` and the card just below/above it, clamped to the viewport.
function positionTour(target) {
  const { ring, card } = tourEls;
  const r = target.getBoundingClientRect();
  const pad = 4;
  ring.style.top = (r.top - pad) + "px";
  ring.style.left = (r.left - pad) + "px";
  ring.style.width = (r.width + pad * 2) + "px";
  ring.style.height = (r.height + pad * 2) + "px";
  const cw = card.offsetWidth || 300, ch = card.offsetHeight || 140;
  let top = r.bottom + 12;
  if (top + ch > window.innerHeight - 8) top = Math.max(8, r.top - ch - 12);
  let left = r.left;
  left = Math.min(left, window.innerWidth - cw - 8);
  left = Math.max(8, left);
  card.style.top = top + "px";
  card.style.left = left + "px";
}

function renderTourStep() {
  const total = TOUR_STEPS.length;
  // skip forward over any step whose target is missing from the DOM
  while (tourIdx < total && !$(TOUR_STEPS[tourIdx].sel)) tourIdx++;
  if (tourIdx >= total) { endTour(); return; }
  const step = TOUR_STEPS[tourIdx];
  const target = $(step.sel);
  if (!target) { endTour(); return; }
  const { ring, card } = tourEls;
  const last = tourIdx === total - 1;
  card.innerHTML =
    `<div class="tour-title">${step.title}</div>`
    + `<div class="tour-body">${step.body}</div>`
    + `<div class="tour-foot">`
    + `<span class="tour-step">${tourIdx + 1} / ${total}</span>`
    + `<span class="sp"></span>`
    + `<span class="tour-skip" data-act="skip">Skip</span>`
    + (tourIdx > 0 ? `<button data-act="back">Back</button>` : "")
    + `<button class="primary" data-act="next">${last ? "Done" : "Next"}</button>`
    + `</div>`;
  ring.hidden = false; card.hidden = false;
  positionTour(target);
}

function startTour() {
  buildTourDom();
  togglePalette(false);            // don't let the palette overlap the tour
  tourIdx = 0;
  tourEls.backdrop.hidden = false;
  renderTourStep();
}

function endTour() {
  try { localStorage.setItem(TOUR_FLAG, "1"); } catch (_) {}
  if (!tourEls) return;
  tourEls.backdrop.hidden = true;
  tourEls.ring.hidden = true;
  tourEls.card.hidden = true;
}

function tourActive() { return tourEls && !tourEls.backdrop.hidden; }

// card-button delegation
document.addEventListener("click", (e) => {
  const btn = e.target.closest("[data-act]");
  if (!btn || !tourActive()) return;
  const act = btn.dataset.act;
  if (act === "skip") { endTour(); return; }
  if (act === "back") { tourIdx = Math.max(0, tourIdx - 1); renderTourStep(); return; }
  if (act === "next") {
    if (tourIdx >= TOUR_STEPS.length - 1) {
      // last step "Done": open the sudoku sample so ⊨ Solve has something to chew on
      if (SAMPLES[SUDOKU_SAMPLE]) loadProgram(SAMPLES[SUDOKU_SAMPLE], null);
      endTour();
    } else { tourIdx++; renderTourStep(); }
  }
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && tourActive()) { e.stopPropagation(); endTour(); }
});
window.addEventListener("resize", () => {
  if (!tourActive()) return;
  const t = $(TOUR_STEPS[tourIdx].sel);
  if (t) positionTour(t);
});
$("#tour-btn").onclick = (e) => { e.stopPropagation(); startTour(); };

// auto-run once on first visit
function maybeAutoTour() {
  let seen = false;
  try { seen = localStorage.getItem(TOUR_FLAG) === "1"; } catch (_) { seen = true; }
  if (!seen) startTour();
}

// kick off
run();
maybeAutoTour();
// If we loaded a program from a shared link, say so — but only after run()'s "computing…"
// settles, so the message isn't immediately clobbered. Subtle, dismissed by the next action.
if (SHARED != null) {
  setTimeout(() => { setStatus("loaded from a shared link ✓", "ok"); }, 900);
}
