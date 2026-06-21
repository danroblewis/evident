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

---

## Round 2 — banner soundness + auto-select + samples + morse_graph

**Build (commit 77a870d):** banner classifier (lone self-carried clock → "Driven", not
"relational cycle"); auto-select drops the sticky activeView on edits + refined `_recommend`;
newcomer error translation (humanizeError); capped-sample labeling; morse_graph `render()`
wrapper (gallery now genuinely 6); samples menu (5 worked examples); ⇒ Unicode shortcut.

- **Marek (ide-critic): SHIP** ✅ — immediacy 4 · diagram-helps 5 · directness 4 · honesty 5 ·
  first-run 5 · recovery 5 · promises 4. "The banner stopped lying, the right view shows up,
  and the fan finally fans." Verified the banner fix is analysis-based (his own typed ramp
  classified right). Non-blocking majors: latency on unbounded (known); no editor auto-indent.
  Nit: banner doesn't say "cyclic" for vending.
- **Sam (newcomer): SHIP** ✅ — immediacy 4 · diagram-helps 5 · directness 4 · honesty 5 ·
  first-run 5 · recovery 5 · promises 4. "The door's open now... a win in under five minutes,
  made my classic indentation mistake, and the tool talked me back in without a manual."
  Non-blocking majors: hand-typed indentation friction (flip side of bare-newline); latency.
  Nit: vending should read "cycle".
- **Ana (expert): NEEDS_WORK** — immediacy 5 · diagram-helps 5 · directness 2 · honesty 5 ·
  first-run 5 · recovery 4 · promises 3. **Zero blockers** — both round-1 blockers
  adversarially verified fixed (a hand-built 2-var mutual cycle `a=¬_b, b=_a` still reads
  "relational", so the fix didn't over-correct). Majors: (1) banner flattens cyclic-recurrent
  (vending) and terminating-driven (counter) into one phrase — the Morse view knows ("cycle
  ×3") but the headline doesn't; (2) solve/query absent. Her SHIP spec: SAT witness + UNSAT
  core + solve-for-X/enumerate, plus the cyclic-banner fix. Delights: exact-vs-capped labeling,
  the actionable dropped-constraint detector, the terminating counter's honest self-loop.

**Status: 2 of 3 SHIP.** The editor auto-indent friction (Marek + Sam) is a deliberate
trade-off: the critics type *pre-indented* strings via the browser, so bare-newline Enter is
correct for them; any auto-indent re-introduces the round-1 accumulation blocker. Documented,
not "fixed."

**Next (round 3 — flip Ana):** (1) **cyclic-vs-terminating banner** — say "cyclic driver" when
the reachable graph has a recurrent SCC (≥2-node) [also Marek/Sam nit]; (2) **solve/query** —
`evident query` CLI (SAT witness via the existing rt.query engine) + `/api/solve` + a frontend
solve panel (run a claim → witness/UNSAT; solve-for-X by pinning vars). Deferred to round 4 if
needed: UNSAT core (needs runtime get_unsat_core) + enumeration (needs blocking clauses).

---

## Round 3 — solve/query (SAT witness + solve-for-X) + cyclic-vs-terminating banner

**Build (commit 9a498f5):** cyclic banner (recurrent-SCC detection); `evident query` CLI
(SAT witness via rt.query, ./test.sh green 252); `/api/solve`; the ⊨ Solve panel (run a claim →
witness/UNSAT, pin vars for solve-for-X); queens + sum-pair samples; pure claims invite Solve
instead of erroring. Only Ana re-run this round (Marek/Sam SHIPPED on round 2; round 3 is purely
additive — they'll be re-confirmed on round 4).

- **Ana (expert): NEEDS_WORK** — immediacy 4 · diagram-helps 5 · directness 3 · honesty 5 ·
  first-run 5 · recovery 4 · promises 4. **Zero blockers.** Up from round 2 (directness 2→3,
  promises 3→4). Adversarially confirmed: cyclic banner sound (counter's terminal self-loop NOT
  called cyclic, vending's 3-cycle IS), witness verifiable (`col=[1,3,0,2]`), solve-for-X real
  (`x=3→y=7`), UNSAT honest, no fabrication. Majors gating SHIP: (1) **UNSAT core** — UNSAT gives
  no conflicting subset ("compiler error with no line number"); (2) **enumeration** — one witness,
  can't walk all solutions (the Alloy use case). Nits: a seq-length-pin runtime quirk surfaced via
  her raw-API probe (a length-1 witness under `#col=4` — runtime encoding edge, not IDE
  fabrication; the pin-box path avoids it); the solve panel keeps a stale pin across sample switch.

**Next (round 4 — flip Ana, both backend-only, no runtime change):** (1) **UNSAT core** via
source-level delta-debugging (`/api/solve` removes each constraint line, re-queries; SAT-flipping
lines = the core); (2) **enumeration** via iterated source-level blocking (solve → append
`¬(witness)` → re-solve, up to a limit, with "showing k of N · complete/≥N" honesty); (3) clear
the pin box on sample load. Then re-run all three on round 4.
