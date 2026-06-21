# Evident Web IDE — goal-loop log

Round-by-round record of the "build the IDE until all three critics SHIP" loop. Each
entry: the change made, the three critics' verdict lines, and whether Iris ran. Newest
at the bottom. Recordings (timestamped per run) live in `ide/critics/recordings/`.

Goal: `ide-critic` (Marek), `ide-critic-newcomer` (Sam), `ide-critic-expert` (Ana) all
return `VERDICT: SHIP` against the dev server at http://localhost:5173.

---

## Round 0 — M0 baseline (pre-loop)

**Build:** M0 — live write→see loop, model-shape banner, dropped-constraint honesty
line, three view tabs (time_series / state_graph / phase_portrait), `\in`→∈ Unicode input.

- **Marek (ide-critic): NEEDS_WORK** — immediacy 4 · diagram-helps 4 · directness 2 ·
  honesty 3 · first-run 3 · recovery 4 · promises 3.
  - BLOCKER: editor auto-indent over-indents continuation lines → the indentation-sensitive
    parser rejects every multi-line program ("can't get past line 2 of hello-world").
  - MAJORS: nondeterminism hidden (time_series paints a 400-state machine as one witness
    line — the headline feature failing); auto-select picks an empty N/A over the live
    view; stale chart shown beside a red error; dropped-count has no pointer to the line.
  - NITS: title bar always "counter.ev"; clipped legends; 2277 ms on a 400-state solve, no
    progress indicator.
  - DELIGHTS (do not regress): honest N/A cards; the orbit phase portrait; runaway
    under-constraining visible; Unicode input; precise, actionable errors.
- **Sam (ide-critic-newcomer):** not yet run → round 1.
- **Ana (ide-critic-expert):** not yet run → round 1.
- **Iris:** not yet run (seed the backlog at round 1 start).

**Next (round 1):** fix the auto-indent blocker first, then the majors (nondeterminism
visibility, auto-select, stale-on-error, dropped-count provenance); run the full panel.

---

## Round 1 — kill the auto-indent blocker + make nondeterminism visible

**Build (commit 59e993c):** editor Enter → bare newline (ends the indent accumulation);
nondeterminism banner ("up to N successors") + honesty "branching ×N" + auto-select
reachability_tree; stale-on-error overlay; dropped-constraint provenance panel; dynamic
title; views 3→6. Iris round-1 backlog batch (14 proposals); promoted "the nondeterminism
badge." Critics run sequentially (single shared browser MCP — parallel would clobber the
shared page).

- **Marek (ide-critic): NEEDS_WORK** — immediacy 3 · diagram-helps 5 · directness 3 ·
  honesty 4 · first-run 4 · recovery 5 · promises 3. **Zero blockers** — auto-indent is
  dead (typed a 2-level nested program from scratch, it parsed). Majors: banner
  misclassifies the counter as "relational cycle" while its own diagram says
  "deterministic"; latency 1–2.6 s on unbounded machines; auto-select weak (state_graph
  noodle for a 1-D ramp; reachability_tree hides the back-edge on the cyclic machine).
  Promised-missing: morse_graph (6th tab never emitted), the full 16, interactivity,
  diagnostics, solve. Delights: dropped-constraint panel, reachability_tree-as-bug-detector,
  stale-on-error, faithfulness labels.
- **Sam (newcomer): NEEDS_WORK** — immediacy 5 · diagram-helps 5 · directness 4 ·
  honesty 5 · first-run 4 · recovery 2 · promises 4. **Blocker:** the indentation error is
  cruel — `count ∈ Int` at col 0 → "expected schema/claim/…, got Ident(count)"; a newcomer
  is dead on line 2 with no "indent under `counter`" hint. Majors: no in-tool
  learning/tooltips; the "relational cycle" banner confuses; only one sample reachable (no
  examples menu). Delights: Unicode input, the bug-hunt runaway, honesty line (capital
  `True`), N/A cards, stale-on-error.
- **Ana (expert): NEEDS_WORK** — immediacy 4 · diagram-helps 4 · directness 2 · honesty 4 ·
  first-run 4 · recovery 4 · promises 2. **Blockers:** (1) the banner calls a driven clock
  "a co-determining cycle" — a soundness failure in the headline analysis; (2) auto-select
  is broken on edit — `app.js` sends a sticky `activeView`, defeating `_recommend`, so the
  nondeterministic `pick` defaults to time_series and the fan is invisible. Majors: **no
  solve/query** (a pure `claim` errors "no fsm schemas found") — can watch dynamics but not
  interrogate the model; the honesty line says a flat "400 states" with no "capped"
  qualifier. Delights: the dropped-constraint surface, renderer faithfulness/self-labeling.

**Convergent (all three):** the model-shape banner misclassifies self-carried/driven state
as a relational cycle. → round 2's #1 fix.

**Next (round 2):** (1) banner classifier — a self-carry `var ← _var` is not a cycle; a
lone varying clock is the *most* driven shape; (2) auto-select — drop the sticky activeView
on source edits + smarter `_recommend` (time_series for 1-D ramps, state_graph for cyclic,
reachability_tree for branching); (3) newcomer indentation-error translation; (4) capped
labeling on the honesty line + lower live tick budget + a "sampling…" spinner; (5)
morse_graph `render()` wrapper; (6) a samples menu; (7) the ⇒ Unicode shortcut. Deferred to
round 3: **solve/query** (Ana's central gap) and in-tool learning/tooltips.
