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
