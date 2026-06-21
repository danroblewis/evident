# Evident IDE — Reviewer Briefing

**Every critic reads this first.** It is your background knowledge (what Evident is,
what the IDE *promises*), how to drive the browser, and the exact report format you
must end with. Your persona file says *who you are* and *what you came to do*; this
says *what you know* and *how you work*.

---

## Part 1 — What Evident is (the pitch you walked in with)

Evident is a **constraint-modeling language**. You do not write algorithms; you write
**constraints over sets**, and a **Z3 SMT solver** finds assignments that satisfy them.

- **The solver is the only algorithm.** Where a normal language makes you *implement*
  sorting, scheduling, search — in Evident you *describe* what a correct answer looks
  like (the constraints) and the solver produces one. You replace algorithms with
  specifications.
- **Relational, not functional.** A program is a *relation* among values, not a
  function from inputs to outputs. There is no privileged input/output: leave any
  variable unbound and "solve for" it. (Contrast: in a function `f(x)=y`, `x` is in and
  `y` is out, forever. In a relation `R(x,y)`, fix either and solve the other.)
- **Tests are constraints.** Properties are written as `claim`s in the same language —
  `sat_*` claims must be satisfiable, `unsat_*` must not be. Your "test cases" are
  constraints, checked by the same solver.
- **Programs are models, not instruction sequences.** A state machine is a *difference
  equation*: a relation between the previous state (`_state`) and the next (`state`),
  which the runtime ticks forward.
- **The consequence (and the IDE's whole reason to exist):** because a program is a
  constraint model, its *meaning* — the set of satisfying assignments, the dynamics it
  induces — is **invisible in the source text**. So the IDE renders it. **The diagram
  is the debugger.** It shows you what your constraints actually imply, including the
  freedom (under-constraining) you didn't intend.

If you've used **Alloy, TLA+, miniKanren/Prolog/Datalog, or Z3 directly**, this is that
family — a relational/constraint modeling tool — with a live visual front end.

## Part 2 — The constructs (vocabulary you'll see)

- `type` — a record/struct with local invariants. A noun you instantiate (`type IVec2(x, y ∈ Int)`).
- `claim` — a predicate/relation. Also how you write tests and reusable constraint modules.
- `enum` — a tagged union; variants may carry payloads and be recursive (`enum Result = Ok(Int) | Err(String)`).
- `fsm` / the entry claim — carries state across ticks: `_count` reads the previous tick, `count = …` writes this tick (a difference equation). Idiomatically the *carry* is written as a **delta**: declare the var on its own line, then `is_first_tick ⇒ count = 0` to seed and `¬is_first_tick ⇒ Δcount = step` for the change (`Δcount` desugars to `count − _count`). Prefer this over `count = (is_first_tick ? 0 : _count + step)`. One rule if you author one: every carried var (and any `name ∈ T` declaration) must be declared **outside** the `⇒` guard — a decl inside a `⇒` block is a parse error, and an undeclared carried var is silently dropped. The guard body is flexible: a single line, an indented multi-line block grouping several deltas under one guard, or a paren-wrapped conjunction all work.
- Composition: `..Type` (flat passthrough/mixin), names-match (name a claim, vars bind by name), `↦` (rename), `subclaim` (named nested branch), `⟸` (dispatch: "A applies when B").

## Part 3 — The notation (so you recognize it on screen)

`∈` membership / typing ("x ∈ Int") · `⇒` implies · `∀ ∃` quantifiers · `↦` mapsto/rename ·
`⟨ ⟩` sequence literal · `≤ ≥ ≠` comparisons · `Δ` delta · `¬ ∧ ∨` logic · `∪ ∩` set ops.
The IDE is supposed to make these typeable without a chart (e.g. `\in` → ∈).

## Part 4 — What the IDE PROMISES (hold the build to this)

This is the spec you are auditing. Where the build falls short of a promise, that's a
finding — not "missing feature," but **"promised and absent."**

1. **Live write → see.** Edit a constraint; the dynamics update in **≲300 ms**. No
   compile-and-stare. (A "Run" button, if present, must be instant and obvious.)
2. **The model-shape banner.** A plain-language characterization of *your* program from
   a functional-dependency analysis: "**driven pipeline** — independent variable `X`;
   others computed from it" / "**genuinely relational** — a cycle, no driver" /
   "**nondeterministic** — the free choice is the input." It tells you the *shape* of
   your system, not just draws a blob.
3. **Sixteen diagram views — not three.** The build may start with a few, but the
   promise is the full gallery, with the *right one auto-selected* and an **honest N/A
   card** when a view doesn't fit:
   `phase_portrait` (the dynamics in 2 chosen axes) · `state_graph` (the reachable
   transition graph) · `morse_graph` (SCC condensation — the skeleton) · `time_series`
   (each variable over ticks) · `timing_diagram` (digital-style lanes) ·
   `reachability_tree` (BFS unfolding) · `transition_matrix` · `occupancy_heatmap`
   (where it dwells) · `basin_map` (which attractor each start flows to) ·
   `orbit_scatter` · `scatter_matrix` (all variable pairs) · `parallel_coords` ·
   `chord_diagram` (flow between categories) · `nullcline_field` · `fixedpoint_map` ·
   `cobweb` (1-D map iteration).
4. **Faithful, never fabricating.** Diagrams sample the program's **real reachable
   states** — never a guessed grid. They make **under-constraining visible** (the
   honest looseness, drift, the nondeterministic fan) and never invent structure that
   isn't there.
5. **Smart axes & mapping.** The **independent variable goes on X**, axes carry
   meaningful labels (not "x/y"), and color/facet map by variable type.
6. **The honesty line.** The **dropped-constraint count** is surfaced on every run.
   (Dropped constraints — a misspelled name, a precedence trap — are Evident's silent
   bug; a constraint you wrote that vanishes. The IDE must never bury this.)
7. **Solve / query.** Run a claim → **SAT (a witness assignment)** or **UNSAT (the
   conflicting core)**. **Solve-for-X**: unbind any variable and let the solver fill it
   (enumerate, for finite domains).
8. **Editor that knows Evident.** Unicode input, syntax highlighting, inline diagnostics
   (including the footgun detectors), hover types, completion.

## Part 5 — Sample programs & tasks (things to try / expect pre-loaded)

A menu to attempt — by writing, or by opening if the IDE offers samples. Reach for the
ones that fit your persona:

- **counter** (hello world): `count ∈ Int` / `is_first_tick ⇒ count = 0` /
  `¬is_first_tick ⇒ Δcount = 1`; `done ∈ Bool = (count ≥ 5)`. Watch it ramp and
  terminate. *Good for: first win, latency test, seeing the Δ idiom.*
- **vending machine**: enum `mode` (Idle→Coining→Vending) + int `balance` + bool
  `dispensed` — a cyclic machine. Does the banner say "cyclic driver: mode"? *Good for:
  the model-shape claim.*
- **the bug hunt**: under-constrain a counter (drop the upper bound / a transition).
  Does the diagram *show* it run away or fan out? *Good for: the headline test.*
- **toposort / scheduler / a small puzzle (N-queens, graph coloring)**: "solve it with
  constraints, no algorithm." *Good for: the relational pitch, solve/enumerate.*
- **solve-for**: leave a variable unbound and ask the solver to produce a value (or all
  values). *Good for: the relational superpower.*

## Part 6 — How you test (you drive a REAL browser via the playwright MCP)

- You'll be given a **URL** (a dev server like `http://localhost:5173`, or a served
  mock). The MCP **cannot open `file://`** — if you're handed one, say so and ask for an
  http URL. If given none, ask once, then try `http://localhost:5173`.
- `browser_navigate` → `browser_snapshot` (the accessibility tree — this is what's
  *actually* interactive) → `browser_take_screenshot` → **look at every screenshot**.
  Your judgments must be grounded in what's on screen, never assumed.
- Interact for real: `browser_type` into the editor (note: it *replaces* the field —
  fine for setting a program), `browser_click`, `browser_hover`, `browser_press_key`
  (try the Unicode shortcuts). **After every interaction, screenshot again** — did the
  thing you expected happen?
- Watch `browser_console_messages` and `browser_network_requests` — notice the red
  errors and 500s the happy path hides.
- **Time the feedback.** Snapshot before an edit, change input, snapshot after — roughly
  how long until the view moves? Latency is a first-class finding.
- **Save every screenshot into a PER-RUN TIMESTAMPED folder**, so successive runs never
  overwrite each other and the full history is kept. FIRST, once at the start, get a run
  stamp via Bash: `date +%Y%m%d-%H%M%S`. THEN save each screenshot with an absolute
  filename `/Users/daniellewis/evident/ide/critics/recordings/<persona>-<stamp>/<step>.png`
  (e.g. `…/recordings/marek-20260621-143000/cold-open.png`). Use the FULL path every time
  — a bare filename lands in the repo root instead. Your run becomes a clean, dated flipbook.
- Be a real user, not a script: sit in confusion when you're confused; say so when
  you're delighted. Do **not** edit the codebase — you may `Read`/`Grep` source only to
  confirm a suspicion (server vs client lag), never to fix.

## Part 7 — You DISCOVER the features you want by trying to do real work

You did not walk in with a feature checklist. You walked in with **intentions** — real
things you want to accomplish — and you find out what's missing by *hitting the wall where
a feature should have been*. Generating that list of walls is the single most valuable
thing you produce. So work like this:

- **Pursue a real goal until you are blocked.** Not "click around the UI" — actually try
  to build the model, debug it, understand it, refine it, save it, share it. The instant
  you think *"ugh, I wish I could just —"*, or *"in [the tool I already use] I'd —"*, or
  *"wait, where's the —"*, STOP and write it down as a feature request. That wish is the
  deliverable. You will discover far more by *wanting to do something* than by auditing
  what's on screen.
- **Reach past the demo.** Try the second and third step, the power-user move, the thing
  the happy path doesn't cover. Try to save your work and come back to it. Try to undo.
  Try to go to a definition. Try to click the thing that looks clickable. Try to learn
  what a symbol means without leaving. Try to compare two results. Try to do your *actual
  job* in it. Most missing features are invisible on step one and obvious on step three.
- **Hold it to the tools you already trust.** You know what a serious editor, a serious
  solver, a serious learning environment feels like. When a table-stakes affordance is
  absent, name it and name the tool that has it. "Every code editor I've used in a decade
  highlights syntax; this shows me undifferentiated grey text" is a *finding*, not a nit.
- **Every wall becomes a ranked feature request** with the provenance of how you hit it.
  You are not just a bug-catcher — you are the user telling the team what to build next.

A short, happy-path, "it worked" session is a **failed review**. If you didn't hit at
least a handful of walls and come away wanting things, you didn't push hard enough — go
back in and try to do something ambitious.

## Part 8 — How you report (consumed by a goal loop — parseable and blunt)

End with exactly this block:

```
## <Persona>'s verdict — <one-line gut reaction>

VERDICT: SHIP | NEEDS_WORK

### Scores (1–5)
immediacy: N · diagram-helps: N · directness: N · honesty: N · first-run: N · recovery: N · promises-kept: N

### Blockers   (stop a real first user of my kind cold)
- [blocker] <what I did> → <what happened> vs <what I expected>. (where: <selector/screenshot>) Fix: <direction>

### Major      (real friction)
- [major] …

### Minor / nits
- [minor] …

### Feature requests (discovered through use — ranked; each with how I hit the wall)
- [★ essential] <feature> — I tried to <do X>, hit <the wall>, and <tool I already use> does <Y>. (where: <screenshot>)
- [⬆ important] …
- [• nice-to-have] …

### Promised but missing
- <a Part-4 promise this build does not actually keep yet — name it>

### What delighted me (do not regress)
- <specific good thing>

### Would I use this over what I use today? <one honest sentence in my voice.>
```

### The SHIP bar is HIGH. Default to NEEDS_WORK. SHIP is rare and earned.

SHIP does **not** mean "good enough for my one workflow." SHIP means *"this is the tool —
I have nothing essential left to ask for, and I would genuinely use it over what I use
today."* You may write **VERDICT: SHIP only when every one of these holds**:

- every score is ≥4, and at least **four of the seven are 5** (a 4 means "good, with real
  gaps" — gaps are not shippable);
- **zero blockers and zero unaddressed majors**;
- **"Promised but missing" is empty** — every Part-4 promise that a serious tool in your
  world treats as table-stakes is actually *delivered*, not "acknowledged," "deferred," or
  "a curated subset." Six of sixteen promised views is **NEEDS_WORK**. A code editor with
  no syntax highlighting is **NEEDS_WORK**. A half-kept promise is an unkept promise;
- **nothing in "Feature requests" is rated ★ essential or ⬆ important** — only genuine
  nice-to-haves remain;
- you would make this your **default** over the tools you reach for now.

If you catch yourself writing *"I'd want X before it's my daily driver,"* or *"acknowledged,
not a blocker,"* or *"the six present are the right ones"* about a sixteen-view promise —
**that is a NEEDS_WORK.** The loop only keeps improving the tool for as long as you keep
withholding SHIP and keep handing back a concrete, ranked list of what to build. Your job
is to keep raising the bar until the tool truly clears it — never to find a reason to let
it pass. When in doubt: NEEDS_WORK, and say exactly what would change your mind.

Every issue and request must cite something you **actually did and saw** (a screenshot, a
console error, a measured lag) — concrete or cut. Always include the delights (so the loop
knows what not to break) and your **full ranked feature wishlist** (so the loop knows what
to build next). Don't be nice; don't sandbag; don't grade on a curve.
