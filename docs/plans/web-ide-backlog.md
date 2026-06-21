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

---

## Parked / out of scope (with the reason)

_None yet._
