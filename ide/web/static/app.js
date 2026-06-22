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

    is_first_tick ⇒ (mode = Idle ∧ balance = 0 ∧ stock = 3 ∧ vault = 0)

    (¬is_first_tick ∧ act = InsertCoin ∧ _balance < 5) ⇒ (mode = Coining ∧ balance = _balance + 1 ∧ stock = _stock ∧ vault = _vault)
    (¬is_first_tick ∧ act = InsertCoin ∧ _balance ≥ 5) ⇒ (mode = Coining ∧ balance = _balance ∧ stock = _stock ∧ vault = _vault)
    (¬is_first_tick ∧ act = Purchase ∧ _balance ≥ 3 ∧ _stock > 0) ⇒ (mode = Dispensing ∧ balance = _balance - 3 ∧ stock = _stock - 1 ∧ vault = _vault + 3)
    (¬is_first_tick ∧ act = Purchase ∧ (_balance < 3 ∨ _stock = 0)) ⇒ (mode = Idle ∧ balance = _balance ∧ stock = _stock ∧ vault = _vault)
    (¬is_first_tick ∧ act = Cancel)  ⇒ (mode = Refunding ∧ balance = 0 ∧ stock = _stock ∧ vault = _vault)
    (¬is_first_tick ∧ act = Service) ⇒ (mode = Servicing ∧ balance = _balance ∧ stock = 3 ∧ vault = 0)`,
  "traffic light · a cyclic state machine (FSM)":
`enum Light = Red | Green | Yellow

fsm traffic
    light ∈ Light
    timer ∈ Int
    is_first_tick ⇒ (light = Red ∧ timer = 0)
    (¬is_first_tick ∧ _timer ≥ 2) ⇒ timer = 0
    (¬is_first_tick ∧ _timer < 2) ⇒ Δtimer = 1
    (¬is_first_tick ∧ _timer < 2) ⇒ light = _light
    (¬is_first_tick ∧ _timer ≥ 2 ∧ _light = Red)    ⇒ light = Green
    (¬is_first_tick ∧ _timer ≥ 2 ∧ _light = Green)  ⇒ light = Yellow
    (¬is_first_tick ∧ _timer ≥ 2 ∧ _light = Yellow) ⇒ light = Red`,
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

// Persist the buffer across reloads — losing your work on an accidental refresh is the
// fastest way to lose a user's trust.
const SAVED = (() => { try { return localStorage.getItem("evident-buffer"); } catch (e) { return null; } })();
editor.setValue(SAVED != null ? SAVED : DEFAULT_PROGRAM, -1);   // -1 = cursor to start

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
    const ty = tok.type || "";
    const isHoverable = /keyword|operator|variable\.parameter|boolean/.test(ty);
    if (isHoverable) {
      const g = glossFor((tok.value || "").trim());
      if (g) {
        gloss.textContent = g; gloss.hidden = false;
        gloss.style.left = Math.min(e.clientX + 12, window.innerWidth - 380) + "px";
        gloss.style.top = (e.clientY + 18) + "px";
        return;
      }
    }
  }
  gloss.hidden = true;
});
editorEl.addEventListener("mouseleave", () => { gloss.hidden = true; });

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
function markErrorLine(err) {
  clearErrorLine();
  const m = (err || "").match(/line (\d+)/i);
  if (m) {
    const ln = parseInt(m[1], 10) - 1;
    if (ln >= 0 && ln < editor.session.getLength()) {
      const Range = ace.require("ace/range").Range;
      _errMarker = editor.session.addMarker(
        new Range(ln, 0, ln, Infinity), "ace-error-line", "fullLine");
      editor.session.addGutterDecoration(ln, "error-gutter");
      _errLine = ln;
    }
  }
}

// --- the live loop ----------------------------------------------------------------
let timer = null, activeView = null, lastSource = "", _dimTimer = null;

function setStatus(text, cls) { const s = $("#status"); s.textContent = text; s.className = cls || "dim"; }

// Translate parser jargon into something a newcomer can act on. The raw error stays
// (it's precise); we append a plain-language hint for the common footguns.
function humanizeError(err) {
  let hint = "";
  if (/got Ident\(/.test(err) || /expected schema\/claim\/type\/import\/enum/i.test(err)) {
    hint = "\n\n→ This usually means a body line isn't indented. Indent declarations and "
         + "constraints 4 spaces under their fsm/claim/type.";
  } else if (/couldn't translate to Bool/i.test(err)) {
    hint = "\n\n→ A constraint was dropped — often a typo'd or undeclared name, or a capital "
         + "True/False (Evident uses lowercase true/false).";
  } else if (/lex error|unexpected character/i.test(err)) {
    hint = "\n\n→ An unrecognized character. For operators, type a backslash word "
         + "(e.g. \\in → ∈, \\implies → ⇒, \\Delta → Δ).";
  }
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
  // the invariant checker only makes sense when there's a reachable set (an FSM with structure)
  $("#invariant").hidden = !s;
  if (!s) { el.hidden = true; return; }
  el.hidden = false;
  const [icon, name, note] = VERDICTS[s.verdict] || ["·", s.verdict, ""];
  let html = `<span class="verdict v-${s.verdict}">${icon} ${name}</span>`
    + (note ? `<span class="dim">${note}</span>` : "");
  if (s.fixed_points && s.fixed_points.length) {
    const fp = s.fixed_points.slice(0, 3).map(
      (f) => "(" + Object.entries(f).map(([k, v]) => `${k}=${v}`).join(", ") + ")").join("  ");
    const more = s.fixed_points.length > 3 ? ` +${s.fixed_points.length - 3}` : "";
    const label = s.verdict === "nondeterministic" ? "rest states" : "fixed point";
    html += `<span class="struct-fp">● ${label}: ${fp}${more}</span>`;
  }
  const b = s.bounds || {}, keys = Object.keys(b);
  if (keys.length) {
    const bstr = keys.map((k) => `${k} ∈ [${b[k][0]}, ${b[k][1]}]`).join("   ");
    html += `<span class="struct-bounds">⊏ boundary${s.capped ? " (≥, capped)" : ""}: ${bstr}</span>`;
  }
  el.innerHTML = html;
}

function paint(data, ms) {
  $("#latency").textContent = ms != null ? `${ms} ms` : "";
  $("#banner").classList.remove("recomputing");        // analysis returned — undim
  $("#structure").classList.remove("recomputing");
  $("#view").classList.remove("recomputing");
  const view = $("#view"), warn = $("#warnings");
  if (!data.ok) {
    $("#structure").hidden = true;
    $("#invariant").hidden = true;                     // no reachable set → no verify row
    $("#inv-result").textContent = "";
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
    markErrorLine(data.error);                     // highlight the offending line in the gutter
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
  $("#banner").textContent = "◆ " + data.banner;
  renderStructure(data.structure);
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

async function run(view) {
  const source = editor.getValue();
  lastSource = source;
  const nm = source.match(/^\s*(?:fsm|claim|type|schema)\s+([A-Za-z_]\w*)/m);
  $("#fname").textContent = (nm ? nm[1] : "untitled") + ".ev";
  setStatus("computing…", "busy");
  // Immediately mark the derived panels recomputing — the PREVIOUS program's Structure verdict,
  // verify result and solve witness must NEVER read as current while a new analysis runs, on a
  // switch / edit / error alike (Marek #64/#91/#93). paint() repaints or hides them on result.
  $("#banner").classList.add("recomputing");
  $("#structure").classList.add("recomputing");
  $("#view").classList.add("recomputing");                 // dim the OLD picture, not just the banner
  $("#inv-result").textContent = "";                       // last verify result is stale on any edit
  if (!$("#solve").hidden)                                  // stale witness/UNSAT under a changed source
    $("#solve-head").innerHTML = '<span class="dim">source changed — press re-solve</span>';
  const t0 = performance.now();
  try {
    const res = await fetch("/api/analyze", {
      method: "POST", headers: { "content-type": "application/json" },
      // A source edit (run() with no view) sends null so the server RE-RECOMMENDS the
      // lead view for what was just written — otherwise a tab click pins the view and a
      // later edit that turns the machine nondeterministic keeps showing a flat line.
      // A tab click (run("phase_portrait")) passes its view explicitly and is honored.
      body: JSON.stringify({ source, view: view || null }),
    });
    const data = await res.json();
    paint(data, Math.round(performance.now() - t0));
  } catch (e) {
    setStatus("server error", "err");
    $("#errors").hidden = false;
    $("#errors").textContent = "could not reach the backend: " + e;
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
  const viz = keys.map((k) => seqViz(k, d.bindings[k], src)).filter(Boolean).join("");
  body.innerHTML = (viz ? `<div class="viz-wrap">${viz}</div>` : "")
    + `<table>${keys.map((k) => `<tr><td class="k">${k}${pinned.includes(k) ? " 📌" : ""}</td>`
        + `<td class="v">${escapeHtml(JSON.stringify(d.bindings[k]))}</td></tr>`).join("")}</table>`;
}

// --- domain-picture rendering for Seq witnesses (Task #68) ------------------------
// A Seq(Int) witness is hard to read as an array. Draw it: an N-queens board when the
// values look like one column-per-row placement (length N, every value in 0..N-1), else
// a simple labeled index→value cell strip.
function seqViz(name, val, source) {
  if (!Array.isArray(val) || !val.length) return null;
  // only primitive-Int seqs get a picture; arrays of records/objects keep the raw table
  if (!val.every((v) => typeof v === "number" && Number.isInteger(v))) return null;
  const n = val.length;
  // A queens board needs TWO honest signals, not just "values in 0..N-1" — that also matches a
  // topological order (pos=[0,1,2,3,4]) and a sudoku row (cell, 16 values in 0..3), which both
  // drew a wrong chessboard (Marek #68/#92). Require: a queens-like variable NAME *and* a true
  // permutation of 0..N-1 (one queen per row AND column). Everything else gets the honest strip.
  const queensName = /^(col|cols|queen|queens|row|rows|board)$/.test(name.toLowerCase());
  const isPermutation = n >= 4 && new Set(val).size === n && val.every((v) => v >= 0 && v < n);
  if (queensName && isPermutation) return queensBoard(name, val);
  return cellStrip(name, val);
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
$("#solve-all").onclick = () => solve(true);

// Assert-and-check a safety invariant over the reachable set — verify `var op value` holds on
// EVERY reachable state (a proof when the set is finite & fully explored), or get a reachable
// counterexample. The other half of the relational pitch: not just "watch", but "prove".
const _INV_RE = /^\s*([A-Za-z_]\w*(?:\.\w+)?)\s*(<=|>=|!=|<|>|=|≤|≥|≠)\s*(.+?)\s*$/;
async function checkInvariant() {
  const out = $("#inv-result");
  const raw = $("#inv-prop").value.trim();
  if (!raw) { out.textContent = ""; return; }
  const mt = raw.match(_INV_RE);
  if (!mt) { out.className = "bad"; out.textContent = "✕ write  var op value  (e.g. count ≤ 5)"; return; }
  const [, varName, op, valStr] = mt;
  let value = valStr;
  if (/^-?\d+$/.test(valStr)) value = parseInt(valStr, 10);
  else if (/^-?\d*\.\d+$/.test(valStr)) value = parseFloat(valStr);
  else if (valStr === "true" || valStr === "false") value = (valStr === "true");
  out.className = "dim"; out.textContent = "checking…";
  try {
    const res = await fetch("/api/invariant", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ source: editor.getValue(), var: varName, op, value }),
    });
    const d = await res.json();
    if (!d.ok) { out.className = "bad"; out.textContent = "✕ " + (d.error || "check failed"); return; }
    if (d.holds) {
      out.className = "good";
      out.textContent = (d.exhaustive ? "✓ proven" : "✓ holds (bounded)")
        + ` — ${d.predicate} on all ${d.checked} reachable states`;
    } else {
      const cex = Object.entries(d.counterexample || {}).map(([k, v]) => `${k}=${v}`).join(", ");
      out.className = "bad"; out.textContent = `✗ violated — counterexample  ${cex}`;
    }
  } catch (e) { out.className = "bad"; out.textContent = "✕ " + e; }
}
$("#inv-btn").onclick = checkInvariant;
$("#inv-prop").addEventListener("keydown", (e) => { if (e.key === "Enter") checkInvariant(); });
$("#solve-close").onclick = () => { $("#solve").hidden = true; };
$("#solve-given").addEventListener("keydown", (e) => { if (e.key === "Enter") solve(false); });

// --- samples menu: open a worked example -----------------------------------------
const sel = $("#samples");
sel.innerHTML = '<option value="">open sample…</option>' +
  Object.keys(SAMPLES).map((k) => `<option value="${k}">${k}</option>`).join("");
sel.onchange = () => {
  if (SAMPLES[sel.value]) {
    editor.setValue(SAMPLES[sel.value], -1);
    $("#solve-given").value = "";   // a fresh sample must not inherit the last pin…
    $("#solve").hidden = true;       // …nor leave a stale UNSAT/witness over the new program
    $("#inv-prop").value = "";       // …nor a stale verify assertion (Sam #107)
    $("#inv-result").textContent = "";
    run();
  }
  sel.value = "";          // reset the label so the same sample can be re-opened
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

// kick off
run();
