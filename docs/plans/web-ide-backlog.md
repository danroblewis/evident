# Evident Web IDE — Feature Backlog

A living pool of features, wider than the committed plan. `web-ide.md` is **the spec**
(what we've decided to build, M0–M5); this is **the backlog** — proposed, triaged,
parked. The `ide-feature-designer` agent (Iris) appends `Proposed` batches grounded in
what only Evident can do; we promote winners into the spec and park the rest.

**Legend** — priority: `★` killer · `⬆` strong · `•` nice · `?` speculative ·
effort: `S/M/L` · depends: `FE` frontend / `BE` backend / `RT` runtime.

---

## Planned (already in the spec, M0–M5)

Iris should NOT re-propose these — they're committed.

- `[M0]` live write→see loop — edit a constraint, diagram updates ≲300 ms · M · FE+BE
- `[M0]` model-shape banner (driven / relational / nondeterministic) · S · BE
- `[M0]` dropped-constraint honesty line · S · BE
- `[M1]` interactive `time_series` scrubber + tick transport · M · FE
- `[M1]` `state_graph` click → inspector + constraint provenance · M · FE+RT
- `[M1]` brushing-and-linking across views (one state, many lenses) · M · FE
- `[M2]` language server: diagnostics + footgun detectors, hover types, completion · L · BE
- `[M2]` Unicode input method (`\in`→∈) · S · FE
- `[M2]` `⟦solve⟧` / `⟦run⟧` codelens · S · FE+BE
- `[M3]` solver console: claim SAT/witness · UNSAT/core · solve-for-X · pin-and-explore · M · BE
- `[M4]` full 16-view gallery interactive + export + deep-link-to-a-state · L · FE
- `[M5]` (hosted) projects, persistence, multi-file, share links, sandbox · L · FE+BE

---

## Proposed (from Iris — triage these into the spec)

<!-- Iris appends dated batches below this line. Format per item:
### <name>   ★|⬆|•|?   · effort S/M/L · depends FE|BE|RT
<one-line pitch>. **Only-Evident:** <why this exploits the solver / relational model /
the live diagram, not a generic IDE feature>. **Lens:** <solver | direct-manipulation |
steal-from-masters | lower-the-floor | critic-pain>. <2–3 sentences of substance.>
-->

### Proposed — 2026-06-21 (Iris, round 1)

Grounded in the live M0 build (the `accumulate` time_series, the driven-pipeline
banner) and Marek's M0 baseline. The recurring pain — *nondeterminism is drawn as one
witness line* — is the spine of this batch: the substrate (`successors()` returns the
whole set-valued fan, not one) already knows the truth; the UI is throwing it away. I
mine that hard, then the rest of the solver/relational lenses.

### the fan, drawn   ★   · effort M · depends FE+BE
Every view renders the **full set-valued image** `successors(state)`, never one witness — a
nondeterministic transition shows as a branching fan, not a lie. **Only-Evident:** `successors()`
already enumerates ALL distinct next states by block-and-resolve; only Evident has a solver that
can say "these four next-states are equally valid here." **Lens:** critic-pain. Marek's headline
finding — a 400-state nondeterministic machine drawn as a single line — is a UI bug, not a missing
feature: the backend hands back the branching, the renderer collapses it. Draw time_series as a
shaded envelope/ghost-bundle, state_graph with true multi-out-edges, phase_portrait as a fan of
arrows. This is THE thing that makes "faithful, never fabricating" true.

### solve-for-anything cursor   ★   · effort M · depends FE+BE
Click any variable in any rendered state → **unbind it and let the solver refill it**, enumerating
all values for a finite domain, right where it sits. **Only-Evident:** this is the relational core
made tactile — `R(x,y)`, fix either, solve the other; no function-call IDE can do it because no
function has a reversible output. **Lens:** solver. The spec promises solve-for-X in a console;
this puts it *on the state card and the diagram node* so a newcomer discovers the superpower by
poking, not by reading docs. "What other `sum` could this state have had?" → the answer paints in
as a set of alternatives across every linked view.

### why-this-edge (relational provenance, no span work)   ★   · effort M · depends BE
Click a transition → the IDE re-derives *which previous-state variables actually caused each
next-state variable to take its value*, via `independence_structural()`'s perturbation probe — no
encoder source-spans needed. **Only-Evident:** "`state.sum` responds to `_cursor`" is read off the
**transition relation itself** by re-solving under perturbation, a causal claim only a live solver
can make; a print-debugger can only show values, never dependence. **Lens:** solver. The spec flags
constraint-provenance as "the deepest new runtime work (spans), may slip" — this delivers 80% of
the payoff (a dependency-arrow overlay: `_cursor → sum`, `_i → i`) *now*, with zero runtime change,
and degrades gracefully into span-highlighting later.

### tighten-until cursor (the bug-hunt, made a gesture)   ⬆   · effort M · depends FE+BE
Drag a slider on any numeric variable's reachable range; the IDE adds a **temporary constraint**
(`x ≤ k`) and re-renders the reachable set shrinking — release to keep it as real source, or snap
back. **Only-Evident:** `evaluate_with_extra_assertions` + the live `reachable()` recompute means
you watch the *whole induced dynamics* contract under a hypothesis, not a single re-run. **Lens:**
direct-manipulation. This turns the briefing's headline "bug hunt" (under-constrain a counter, does
it run away?) into its inverse-and-cure: *over*-constrain by hand, see the fan collapse, and learn
exactly which bound you were missing — then promote the temp constraint into the program.

### enumerate all instances (Alloy's next button)   ⬆   · effort M · depends FE+BE
For a non-FSM `claim`, a **"⟦next⟧" control** walks distinct satisfying assignments one at a time
(block-and-resolve), each rendered as a state card. **Only-Evident / steal-from-masters:** Alloy's
instance enumerator is the feature that made model-finding feel alive; Evident has the exact
mechanism (`successors()` already block-and-resolves) but no UI for the static case. **Lens:**
steal-from-masters. Pairs with solve-for-X to cover the briefing's toposort / N-queens / coloring
tasks: write the constraints, hit ⟦next⟧, watch valid boards stream by — the relational pitch you
can *feel*. Cap + "N more…" count so it stays honest about big domains.

### reachable-range gutter chips   ⬆   · effort S · depends BE
A live chip beside each variable's declaration showing its **effective reachable range** (`i:
0..5`, `mode: {Idle, Coining}`) from the `Optimize`-based range finder, not the declared `∈ Int`.
**Only-Evident:** the gap between declared domain (`∈ Nat` = 0..∞) and *effective* domain (0..5
after all constraints interact) is exactly the invisible meaning the IDE exists to surface — a
spreadsheet shows a cell's value, only a solver can show a variable's whole feasible extent.
**Lens:** lower-the-floor. Cheap, always-on, and it teaches the core mental model (constraints
shrink domains) on line 2 of the first program, before anyone clicks a tab.

### vacuity / dead-claim halo   ⬆   · effort S · depends BE
Inline gutter mark when a `claim` is **vacuously satisfiable** or a guarded branch (`⟸`) is
**never reachable** — the constraint compiles but does nothing. **Only-Evident:** vacuity is
decided by asking the solver whether the negation is also SAT; it's invisible in text and is
Evident's signature silent bug ("a test that always passes"). **Lens:** critic-pain. Complements
the dropped-constraint line (a constraint that vanished) with its twin (a constraint that's there
but toothless). One re-solve per claim on the debounce; the honesty story isn't complete without it.

### diff two dynamics (before/after the edit)   ⬆   · effort M · depends FE+BE
Hold a snapshot of the reachable graph, edit the program, and the panel **overlays what changed** —
states/edges added (green), removed (red), the fan that widened. **Only-Evident:** the diff is over
*reachable dynamics* (two solved state-spaces), not text; only a tool that computes the meaning can
diff the meaning. **Lens:** solver. Answers the question every constraint edit raises — "did
removing that bound actually change the reachable set, or was it redundant?" — which the
dropped-constraint count alone can't. Makes the live loop a controlled experiment, not just a redraw.

### prove-over-all-reachable (the picture is the test)   ⬆   · effort M · depends BE
Type a property in the diagram's filter bar (`sum ≥ 0`, `mode ≠ Stuck`); the IDE checks it against
**every reachable state** by solver, and either greens it or **drops you on the counterexample
state** highlighted in all views. **Only-Evident:** this is bounded model checking on the induced
dynamics — "∀ reachable s: P(s)" answered by solving, the briefing's "the test itself is a picture."
**Lens:** steal-from-masters (TLA+'s invariant + error-trace). Unlike a `unsat_*` claim you hand-write
and stare at, this is a live probe over the actual reachable set, and the failure is a *state you can
scrub to*, not a line number.

### the nondeterminism badge on the banner   ⬆   · effort S · depends BE
The model-shape banner gains a **branching-factor readout** — "nondeterministic: up to 4 successors
at `Coining`" — computed from the max `len(successors(s))` over the reachable set. **Only-Evident:**
fan-out is a property of the transition *relation* (how many next-states a solve admits), legible
only because the solver enumerates them. **Lens:** critic-pain. Directly fixes Marek's "nondeterminism
is hidden": even before the fan-rendering lands, the banner stops claiming "driven pipeline" for a
machine that branches, and names where the freedom lives. Cheap dependency-free upgrade to an
existing M0 surface.

### paint-a-transition → "what constraint allows this?"   ?   · effort L · depends FE+BE
In state_graph, draw an edge the program *doesn't* currently have (A→B); the IDE solves for the
**weakest constraint relaxation** that would admit it, or reports the conflicting core that forbids
it. **Only-Evident:** this is abduction over the transition relation — "make this reachable" is a
solver query no imperative debugger can pose. **Lens:** direct-manipulation. Speculative and the
weakest-relaxation search is real work, but it's the purest expression of "draw the dynamics you
want, let the solver find the spec" — the Bret-Victor move. Park-adjacent; flag as a research spike.

### explain-this-diagram in a sentence   •   · effort S · depends BE
A one-line plain-language caption under the recommended view, generated from the semantics already
computed (shape + driver + range + fan + terminals): "i climbs 0→5 then halts; sum accumulates and
flattens; deterministic, 1 terminal state." **Only-Evident:** the sentence is assembled from
`independence` + range + reachable-terminals — facts only the solved model knows, not boilerplate.
**Lens:** lower-the-floor. The diagram is the debugger, but a newcomer doesn't yet read phase
portraits; one honest sentence is the on-ramp, and it's nearly free given everything is already
computed for the banner.

### worked-example rail with the "now break it" prompt   •   · effort S · depends FE
A starter rail (counter, vending machine, the bug-hunt, toposort) where each example ships with a
**one-click "now under-constrain it"** mutation that demonstrates the diagram catching the silent
bug. **Only-Evident:** the examples teach the thing no other IDE has to teach — *that removing a
constraint silently changes the reachable set* — and the diagram is the only place that shows it.
**Lens:** lower-the-floor. The briefing lists exactly these programs as "things to try"; bundling
them with their failure mode turns the gallery into a guided path in, not a blank editor.

Table stakes, noted: command palette, theming, keybindings, multi-file tabs, settings —
build when convenient, not vision.

### Proposed — 2026-06-21 (Iris, round 6)

Grounded in the live build, which is further than the spec implies: the UNSAT core already
names lines ("removing any one makes it solvable"), the banner already reads branching-factor
("up to 3 successors · ×3"), solve-for-X / re-solve / enumerate-all are wired, and the reachability
tree draws the fan. Round 1's spine has largely landed. So this round reaches **past the now-solved
gaps** to the things a *demanding* expert reaches for and doesn't find — a counterexample **trace**
(not a state), a **temporal/invariant property** checked over the dynamics, a **named** minimal
core, **scoped enumeration with symmetry breaking**, and an **ad-hoc query console** — plus the
newcomer's hover-to-learn and the power user's compare/persist. Every item is framed on a substrate
no other tool has: a live solver over a relational transition system, with sixteen faithful views.

### counterexample trace, not a counterexample state   ★   · effort M · depends BE
When a property fails over the reachable set, return the **shortest witnessing trajectory** from an
initial state to the violating state — the full `s₀ → s₁ → … → s_bad` path — and play it on the tick
transport with every view scrubbing to each step. **Only-Evident:** the failure of a temporal claim
in a *difference-equation* model is a path, and only a solver over the transition relation can
produce the minimal one (BFS over `successors()` to the first violator). **Lens:** steal-from-masters
(TLA+'s error-trace is the single feature that makes TLC usable — a state alone is useless; the
*sequence that reaches it* is the debugging gold). Round 1's "prove-over-all-reachable" drops you on
a state; this completes it by handing you the story of how the machine got there, scrubbable.

### temporal-property bar (eventually / always / until over the trace)   ★   · effort M · depends BE
A property bar that accepts more than per-state predicates: **`◇ done`** (eventually halts),
**`□ (balance ≥ 0)`** (always), **`mode = Coining ⇒ ◇ mode = Vending`** (every coining eventually
vends). The IDE checks each over the reachable trace set and greens it or hands back the
counterexample trace above. **Only-Evident:** these are liveness/safety claims about the *induced
dynamics of a relational FSM* — meaningless for a function, native for a transition system; Evident
already ticks the difference equation, so the trace set to check against exists. **Lens:**
steal-from-masters (TLA+/temporal logic). This is the form an expert's real questions take —
"does it ever get stuck?", "can balance go negative?" — and none of them is a single-state predicate.
Start with `◇`/`□` over finite reachable traces; that covers the vast majority and needs no new runtime.

### the minimal core, named and minimized   ⬆   · effort M · depends BE
Upgrade the UNSAT core from "the conflicting lines" to a **minimized, named, ranked** core: shrink it
to a true MUS (drop-and-recheck until every remaining constraint is essential), label each by its
source construct ("upper bound on `x`", "the `step ≤ 3` range"), and offer **one-click "relax this
one"** that re-solves with it removed so you watch SAT return. **Only-Evident:** a Z3 unsat core is
rarely minimal — extra constraints ride along; minimizing it (and showing which single relaxation
unblocks) is a solver-only operation, and the relational "remove one, re-solve" gesture is the
inverse of the bug hunt. **Lens:** solver. The current core lists raw lines; an expert wants the
*irreducible* conflict and a way to test each relaxation without hand-editing source.

### scoped enumeration with symmetry breaking   ⬆   · effort M · depends BE
For a static `claim`, an enumeration panel with **a scope control** (bound the universe: `#queens ≤
8`, `Int ∈ 0..15`) and a **"break symmetries" toggle** that adds lex-leader constraints so the
stream shows *structurally distinct* solutions, not 8 rotations of one board, plus a true count
("12 distinct instances; 96 with symmetry"). **Only-Evident / steal-from-masters:** scoped finite
enumeration is Alloy's core loop, and symmetry breaking is *the* reason Alloy's enumeration is usable
on combinatorial problems — Evident's block-and-resolve enumerator already exists but floods you with
isomorphs. **Lens:** steal-from-masters. The briefing's N-queens / coloring tasks are unbearable
without this; "all witnesses" on queens currently streams near-duplicates.

### ad-hoc query console (the evaluator pane)   ⬆   · effort M · depends BE
A REPL line under the dynamics panel that evaluates an **arbitrary Evident expression against the
current witness / selected state**: `sum + i`, `mode = Coining`, `∀ d ∈ dots : d.y < 540`,
`count − _count`. It re-encodes the expression in the loaded model's context and prints the value or
SAT/UNSAT. **Only-Evident / steal-from-masters:** this is Alloy's Evaluator pane and Lean's `#eval`
on the *goal state* — ask the live model questions the source didn't pre-write, including quantified
ones the solver answers. **Lens:** steal-from-masters. The solve panel runs whole claims; an expert
wants to poke a sub-expression ("is THIS invariant actually holding on the state I'm looking at?")
without editing the program and re-running.

### hover-to-learn: the notation glossary on the symbol   ★   · effort S · depends FE
Hover any operator or keyword (`Δ`, `⟸`, `∀`, `is_first_tick`, `_state`, `fsm`, `⟨⟩`) → a tooltip
card with its meaning, a one-line example, and *what it desugars to* (`Δcount` → `count − _count`;
`A ⟸ B` → `B ⇒ A`). **Only-Evident:** Evident's identity is a dense notation no one has seen before,
and several constructs are *desugarings* whose expansion is the whole point — surfacing "this is sugar
for X" on hover is teaching the language's actual semantics, not generic doc-hover. **Lens:**
lower-the-floor. A newcomer staring at `¬is_first_tick ⇒ Δcount = step` has no way in; the glossary
on the glyph is the on-ramp, and it teaches the desugarings the diagram depends on.

### "what am I looking at" overlay on every view   ⬆   · effort S · depends FE
A persistent `?` affordance on the active diagram that flips it into an **annotated explainer**:
arrows labeling what the axes/nodes/edges/color mean *for this view type*, and a sentence tying it to
this program ("each dot is a reachable state; an edge is one tick; the green node is where you start").
**Only-Evident:** the sixteen views are unfamiliar and the briefing's whole thesis is "the diagram is
the debugger" — but a newcomer can't debug with a morse graph they can't read. Round 1's
"explain-this-diagram sentence" describes the *result*; this teaches the *grammar of the view*. **Lens:**
lower-the-floor. Pairs with the auto-selection: when the IDE picks a phase portrait, it should also be
able to say what a phase portrait is.

### compare two programs side-by-side (the dynamics diff, made A/B)   ⬆   · effort M · depends FE+BE
A split mode: two editors, two dynamics panels, **linked tick transport and shared axes**, so an edit
in B (drop a bound, change a transition) is read against A's dynamics simultaneously — the fan that
widened, the terminal that disappeared, the state count delta, all aligned frame-by-frame. **Only-Evident:**
this diffs *two solved state-spaces under one scrubber*, not two texts; only a tool that computes the
meaning can A/B the meaning. **Lens:** solver. Round 1's "diff two dynamics" overlays before/after of
one program on its own timeline; this is the deliberate experiment — hold two variants and watch them
diverge under the same input — which is how you actually choose between two model designs.

### persist & deep-link a session (the model AND where you were standing)   ⬆   · effort S · depends FE
URL-encode the full session — source + active view + selected state + pins + the property you typed —
so a link reopens **not just the program but the exact thing you were looking at**: this transition,
this counterexample, these pins. **Only-Evident:** the shareable artifact isn't a file, it's *a vantage
point into a solved state-space* — "look at THIS state where balance went negative under THESE pins" is
a coordinate in the dynamics, not a line number. **Lens:** steal-from-masters (Observable's
forkable-notebook URL, but pointed at a reachable state). The spec parks persistence in M5; the *deep
link to a state* is the Evident-specific half and is cheap once state-selection exists.

### the freedom budget (under-constraining, quantified)   ⬆   · effort S · depends BE
A panel readout that names, per variable, **how much freedom the solver still has**: the unconstrained
ones, the ones with a multi-value reachable range, and the lone "fully pinned" ones — "`step` is free
(3 values); `i` is determined; `seed` is unconstrained (∞)." **Only-Evident:** under-constraining is
Evident's signature silent bug and the diagram's reason to exist; this measures it directly off the
reachable set + solver, turning "the looseness you didn't intend" from a thing you must *spot in a fan*
into a number you can read. **Lens:** critic-pain. Complements round 1's reachable-range chips (per-var
extent) with a model-level *audit* — the first place to look when the diagram fans out and you don't
know why.

### freeze-frame inspector with per-variable provenance   •   · effort M · depends BE
Click a state in any view → an inspector that, beside each variable's value, shows **which previous-state
variables forced it** (from the `independence_structural` perturbation probe) and **which source
constraint produced it** — `sum = 10  ← _sum(+10), _i  · line 9`. **Only-Evident:** per-leaf causal
attribution on a transition is a re-solve-under-perturbation claim, not a stack frame; only the live
solver can say "this value would change if `_i` had differed." **Lens:** solver. Extends round 1's
"why-this-edge" arrows from an edge-level overlay to a per-field ledger on the state card — the
spreadsheet "trace precedents" move, but over a relation rather than a formula graph.

### relax-to-SAT / tighten-to-UNSAT slider on a failing claim   •   · effort M · depends FE+BE
On any UNSAT claim, a control that **automatically searches the boundary**: loosen the named core
constraints by the smallest step until SAT (and show the witness at the edge), or on a too-loose model,
tighten until the fan collapses to one. **Only-Evident:** walking the SAT/UNSAT frontier is a binary
search the solver runs for you — "what's the tightest bound that's still satisfiable?" is a question
only a solver can answer, and it's the quantitative version of the bug hunt. **Lens:** solver. Turns
round 1's manual tighten-until cursor into an automatic frontier-finder; useful when you know it's
broken but not by how much.

### model metrics strip (the dynamics, summarized honestly)   •   · effort S · depends BE
A compact always-on strip of solver-derived facts the views imply but don't state: **# reachable
states, # terminals, max branching factor, longest acyclic path, # SCCs (cyclic vs terminating),
average fan-out**. **Only-Evident:** each number is a property of the *transition relation* computed
from the reachable graph (SCC count answers "cyclic or terminating" with no guessing), the kind of
summary a `git diff --stat` gives for code — but for dynamics. **Lens:** critic-pain. The honesty line
already shows state/transition counts; this is the rest of the shape an expert reads at a glance before
opening any single view, and most of it is one pass over the already-computed reachable graph.

### Proposed — 2026-06-25 (Iris, space_time generalization · task #442)

**Context / the finding.** `viz/render_space_time.py` gates on `kind=='seq'` in `m.carried` and N/As
everything else. But the truest space×time models in the corpus carry their spatial dimension as **N
parallel scalar/enum fields, not a Seq**: `life.ev` is a 4×4 grid as 16 Bool cells `c00..c33`;
`brackets.ev` is a 4-deep stack as enum slots `s0..s3`. Both render N/A under today's gate even though a
raster is *exactly* their natural picture. Meanwhile the only sample wired to the view is Rule 90 — so the
owner is right that, as scoped, it looks like a one-example diagram. The fix is a reframe, not solver work:
`time_series_walk._flatten_seqs` already explodes a Seq into per-element columns, and the time-series walk
already collects every scalar over ticks — both halves of "indexed quantity × time" already exist.

### value-heatmap — every multi-variable FSM gets a value-over-time raster   ★   · effort M · depends BE
Generalize space_time from "Seq-carried only" to **any FSM with ≥2 carried scalar/enum/bool leaves**: one
ROW per carried variable, one COLUMN per tick, cell color = that variable's value at that tick (the
transpose of `time_series`, read as a heatmap). **Only-Evident:** the rows are the real reachable
trajectory the solver steps via `successors`, with each variable's domain (the solved range) setting its
colormap — under-constraint shows as a row that fans/drifts, not a clean band. **Lens:** steal-from-masters
(spreadsheet "every consequence in a column" + TLA+ trace, as one image). This is the load-bearing
generalization: it lights up *every* FSM with state — counter, vending, thermostat, SIR, cruise, elevator,
pendulum — not just the handful with a Seq. space_time/CA becomes the special case where the rows happen to
be one Seq's positions; the same renderer, fed columns instead of cells, draws both. Keep the crisp
0/1 binary colormap and the deterministic-vs-sampled honesty subtitle. **File the two strongest as tasks.**

### grid-state raster — detect an N-cell spatial field across parallel scalar leaves   ★   · effort M · depends BE
Recognize the case `life`/`brackets` actually are: a set of carried leaves whose names encode a spatial
index (`c00..c33` ⇒ a 4×4 grid; `s0..s3` ⇒ a length-4 line) and raster them as the field, **including the
2-D case** (life animates as a stack of 4×4 frames, or a small-multiples filmstrip of generations).
**Only-Evident:** the spatial layout is inferred from the *carried set* the solver tracks, and every frame
is a real solved successor — a 2-D CA's evolution falls straight out with no per-cell wiring. **Lens:**
reify-the-invisible. Name-pattern detection (`<prefix><digits>` or `<prefix><r><c>`) is a cheap heuristic;
when it fires, offer "treat these N leaves as a grid?" so it's honest, not magic. Turns the two best
space×time models in the repo from N/A cards into the headline picture.

### record-field raster — a Seq(record) rastered on a chosen field   ⬆   · effort S · depends BE
When the carried Seq holds records (a roster of agents, a row of particles each with `pos/vel/charge`),
let the raster encode **one chosen field** as color, with a field-picker dropdown — `Seq(Agent)` over ticks
becomes "agent index × time, colored by `.energy`". **Only-Evident:** the field domains come from the
solved model, so the picker only offers real fields and colors by their actual range. **Lens:**
direct-manipulation. Today a record-element Seq either N/As or shows something arbitrary; this makes the
common "fixed roster evolving" shape (the kind `coindexed` iterates) first-class in the raster.

### pick-the-index-axis — choose which dimension becomes "space"   ⬆   · effort S · depends FE
A small control on the raster: **which carried set / which Seq / which name-pattern is the space axis**, and
which quantity is the color. **Only-Evident:** every candidate axis is a real carried dimension the solver
tracks, so the menu is generated from the model, never a guess. **Lens:** direct-manipulation. The moment
space_time generalizes past one Seq, "which thing is space?" becomes a real question — let the user answer
it in one click instead of the renderer picking the first leaf silently (today it grabs `m.carried[0]`).

### diff two rasters — same model, before/after an edit, as a Δ-raster   ⬆   · effort M · depends BE
Two runs of the same FSM (or two edits of it) rastered and **subtracted**: cells that changed light up, so
you see *where in space-time* a constraint edit moved the dynamics. **Only-Evident:** both rasters are real
solved trajectories keyed on the same carried set (`carried_names` already gates model-diff alignment), so
the Δ is faithful, not a pixel diff of two screenshots. **Lens:** solver-superpower. Pairs with the existing
dynamics-diff thread — the raster is the densest substrate to show "this edit changed tick 7, column 3."

### nondeterministic fan in the raster — overlay the branch points   ⬆   · effort M · depends BE
Today a nondeterministic model raster-follows one sampled run and *says so* in the subtitle. Go further:
mark the **columns where `successors` returned >1**, and on hover show the alternative next-rows that run
didn't take. **Only-Evident:** the fan is the solver enumerating real alternative successors, not noise —
the raster makes "where the freedom lives in time" visible. **Lens:** reify-the-invisible. `randomwalk`,
`pick`, `vending` are nondeterministic; one sampled raster hides their whole point — show the branch spine.

### raster cell → freeze-frame that state   ⬆   · effort S · depends FE+BE
Click any cell in the raster → pin that (tick, position) and open the full state at that point in the
interrogate panel (every carried var, not just the one colored). **Only-Evident:** the cell *is* a solved
reachable state, so clicking it is "stand here and look at everything," not a tooltip. **Lens:**
direct-manipulation. The raster is a great overview but flattens a state to one color; let the user drill
from the bird's-eye into the actual assignment, the way Alloy's instance view lets you click into an atom.

### name the family right — "value raster" in DYNAMICS, CA as one instance   ⬆   · effort S · depends FE
Reframe the view in the gallery: it's a **heatmap of an indexed/vector quantity over time**, of which the
1-D CA is one instance. Rename to something like `value_raster` (keep `space_time` as the CA-flavored
alias/caption), update `VIEW_CAPTIONS` and the DYNAMICS family blurb so a newcomer reads "value of an
indexed thing over time," not "cellular automaton." **Only-Evident:** n/a (naming) — but it's the change
that stops the view from *looking* single-purpose. **Lens:** lower-the-floor. Coordinate with Mira's gallery
redesign: if `value-heatmap` lands as the general view, it should be the DYNAMICS entry and CA a sample of it.

Table stakes, noted: a download-raster-as-PNG/CSV button and a zoom/pan on tall rasters — build when
convenient, not vision.
