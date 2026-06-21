# Evident Web IDE — Spec (draft)

Status: draft for discussion. Companion to `docs/design/observability.md`,
`docs/design/state-space-diagrams.md`, `docs/visualizations/`, and
`docs/design/web-server-exploration.md`.

## 1. Thesis — the diagram is co-equal with the editor

In a normal IDE the **text is the program** and everything else (debugger, output)
is secondary. In Evident the text is a *constraint system*; what it actually MEANS —
the set of satisfying assignments, the dynamics of the FSM it induces — is invisible
in the source. We have built sixteen ways to see it. So the IDE's organizing idea:

> **You write constraints on the left; you watch the dynamics they induce on the
> right, live.** The visualization is not a preview pane — it is the truth the text
> hides, rendered as you type.

What the picture shows that the text cannot:

- **Under-constraining** — where the solver has freedom you didn't intend (the honest
  looseness: negative regions, post-halt drift, the nondeterministic fan). Evident's
  worst bugs are *silent* (a dropped constraint, a vacuous claim); the diagram makes
  them visible.
- **Model shape** — driven pipeline vs. genuine relation vs. cyclic machine, read off
  the independence analysis (`independence()` / `independence_structural()`).
- **The test itself** — "does my FSM cycle / terminate / reach the goal" is a picture,
  not an assertion you must hand-write.

Tagline: **write the constraints, see the consequences.**

## 2. Why now (this is mostly wiring, not a from-scratch build)

The hard parts already exist:

| Need | Already have |
|---|---|
| Parse / typecheck / solve | the Rust runtime (`evident test / export / effect-run`) |
| Transition IR + state schema | `evident export` → SMT-LIB + JSON schema (roles, kinds) |
| Model-semantics layer | `viz/evident_viz.py`: `reachable, trajectory, successor(s), initial_state, state_vars, independence, axis_bounds, assign_channels, facet_var, change_rates` |
| 16 renderers (PNG export path) | `viz/render_*.py` |
| Client-side reimplementation spec | `docs/visualizations/*.md` — written deliberately language-agnostic |
| Variable selection + axis/channel mapping + independence | done and reviewer-validated this cycle |

`evident_viz.py` IS the backend's model-semantics API; the renderers ARE the export
path; the method docs ARE the client-rendering spec. The IDE wires these to a frontend.

## 3. Layout

```
┌───────────┬───────────────────────────────┬──────────────────────────────┐
│ Outline   │  Editor (CodeMirror 6)         │  Dynamics panel              │
│ - types   │  - Unicode input (\in → ∈)     │  ┌─ model-shape banner ────┐ │
│ - claims  │  - live diagnostics + footgun  │  │ "driven: cursor →"      │ │
│ - the fsm │    detectors                   │  └─────────────────────────┘ │
│           │  - hover types / vacuity       │  ┌─ recommended view ──────┐ │
│ files     │  - ⟦solve⟧ / ⟦run⟧ codelens    │  │ (selector's pick, large)│ │
│           │                                │  └─────────────────────────┘ │
│           │                                │  [ gallery thumbnails strip ]│
├───────────┴───────────────────────────────┴──────────────────────────────┤
│ Solver console: run claim → SAT/witness | UNSAT/core · solve-for-X · pins  │
│ Tick transport:  ⏮ ◀ ▶ ⏭  ●──────○──────  (when an FSM is loaded)          │
└────────────────────────────────────────────────────────────────────────────┘
```

## 4. The editor (Evident-specific)

- **Unicode input method (highest-value affordance).** LaTeX-style: `\in`→∈,
  `\forall`→∀, `\mapsto`→↦, `\implies`→⇒, `\langle`/`\rangle`→⟨⟩, `\neq`→≠, `\leq`→≤,
  `\Delta`→Δ, `\in=`→∈ chained-membership, etc. Evident's notation is its identity;
  typing it must be effortless.
- **Syntax highlighting** for Unicode operators, word-keywords, claim/type/enum names.
- **Live diagnostics from the language server**, including the **silent-bug detectors**
  that catch Evident's documented footguns inline, each with the exact fix:
  - dropped constraint / `True` vs `true` / unbound-name-silently-dropped
  - precedence traps (`⇒` binds tighter than `∧`; `=` tighter than comparisons)
  - `match` on a field-access scrutinee (silently dropped)
  - partial lookup tables (Z3 non-determinism)
  - parallel-Seq drift (`#a = #b` smell), index-in-interface leak
- **Hover**: inferred type of an expression; a claim's signature; **is this constraint
  vacuously satisfiable?** (the under-constraining detector, inline).
- **Completion**: claim/type/field/enum-variant names; subclaim dispatch.
- **Codelens**: `⟦run⟧` on any `claim` → SAT witness inline; `⟦solve X⟧` on a var.

## 5. Dynamics panel (the heart)

Always leads with what the selector + independence analysis recommend:

1. **Model-shape banner** (one line, from `independence*()`):
   - "Driven pipeline — independent variable `cursor`; `sum`, `done` computed from it"
   - "Genuinely relational — no driver (a cycle)"
   - "Nondeterministic — the free input is the choice, not a state variable"
2. **Recommended view** large — the selector's axis pair (driver→X), channel mapping
   applied; honest **N/A card** when a view doesn't fit the program.
3. **Gallery strip** — the other applicable views as thumbnails; click to promote.

**Interactivity is the reason to go past static PNGs:**

- **time_series / timing_diagram**: a playhead tied to the tick transport. Scrubbing
  moves the FSM state and highlights the matching point in *every other view*.
- **state_graph / morse_graph / reachability_tree**: click a node → its full state in
  an inspector + **constraint provenance** (the editor highlights the source
  constraints satisfied on the incoming transition).
- **phase_portrait / orbit_scatter / occupancy**: hover → the state; brush a region →
  filter / select.
- **Brushing-and-linking**: a selected state is highlighted across all views at once —
  one state, many lenses. This is the payoff the static gallery can't give.

## 6. Solver console (Evident's relational superpower)

Because programs are relations, the IDE does what an imperative debugger cannot:

- **Run a claim** → SAT (witness assignment rendered as a state card) or UNSAT (the
  minimal **unsat core** highlighted in the editor — *which constraints conflict*).
- **Solve for X** — unbind any variable; the solver fills it. Finite domain →
  enumerate the satisfying values.
- **Pin & explore** — pin some fields, see the reachable set under those pins
  (`evaluate_with_extra_assertions`).
- **Why UNSAT** — minimal core → highlighted constraints.

## 7. Architecture

```
Browser (React + TypeScript)
  ├─ CodeMirror 6 editor  ─── LSP over WebSocket ───────────────┐
  ├─ Dynamics panel  (D3 / Canvas interactive views + PNG fallback)
  └─ Solver console                                             │
                                                                ▼
Backend service (FastAPI / Python)
  ├─ Evident Language Server   diagnostics · hover · completion · footgun detectors
  ├─ Model-Semantics API       reachable · trajectory · successor(s) · selector ·
  │                            independence · axis_bounds · channels   → evident_viz.py
  ├─ Render service            the 16 matplotlib renderers → PNG/SVG (export + fallback)
  └─ Runtime bridge            → Rust `evident` CLI (load · export · test · effect-run)
```

Payloads are small — reachable sets are 5–400 states — so the data contract ships
structured JSON the client renders, with PNG as the long-tail fallback.

**Model-semantics API (sketch):**

- `POST /session { source }` → `{ schema, fsm, diagnostics, modelShape }`
- `GET  /dynamics` → `{ states[], edges[], trajectory[], initial, terminals[],
                        selector:{axisPair, channels, facet}, independence }`
- `POST /successor {state}` · `POST /successors {state}`
- `POST /query  { claim }` → `{ sat, witness | unsatCore }`
- `POST /solve  { pins, free }` → `{ assignments[] }`
- `GET  /render/{type}.png`  (fallback / export)
- **WebSocket**: on edit (debounced ~300 ms) recompute diagnostics + dynamics, push a
  diff; the panel animates the change.

The Language Server is the right long-term home for editor features; v0 can shortcut
through the same FastAPI process and grow into a real LSP server.

## 8. Rendering strategy — the one real build decision

- **(A) Server PNGs only** — reuse matplotlib as-is. Ships in days, zero viz rewrite,
  but static (no hover/scrub/link). A v0 proof-of-concept.
- **(B) Fully client-side** — reimplement all 16 in D3/Canvas from the method docs.
  The real product; biggest build.
- **(C) Hybrid — RECOMMENDED.** Backend always serves the JSON data; the client
  renders the **high-value interactive views natively** (time_series, state_graph,
  phase_portrait, occupancy, morse) and PNG-streams the long tail (chord,
  scatter_matrix, parallel_coords) until ported. Ship value early; go interactive
  view-by-view. The method docs make each port a bounded task.

## 9. Phasing

- **M0 — Playground proof.** Editor (highlighting + Unicode input, no LSP yet) +
  "Run" → server renders the selector's recommended view as PNG + the model-shape
  banner. Proves the write→see loop.
- **M1 — Live + interactive core.** WebSocket live recompute; native interactive
  `time_series` (scrubber) + `state_graph` (click-through) + tick transport;
  brushing-and-linking between them.
- **M2 — Language server.** Diagnostics incl. footgun detectors, hover types,
  vacuity hints, completion, the `⟦solve⟧`/`⟦run⟧` codelens.
- **M3 — Solver console.** Claims (SAT/witness, UNSAT/core), solve-for-X, pin-and-explore.
- **M4 — Rest of the gallery interactive**, export, deep-link to a state.
- **M5 — (if hosted) projects, persistence, multi-file, share links, sandboxed runtime.**

## 10. Open decisions (need a call before M0)

1. **Local-first or hosted?** Local desktop-ish tool (runtime on your machine —
   simplest, no sandboxing) vs cloud playground (zero-install, shareable — needs a
   sandboxed runtime + auth). *Recommend: local-first first; hosted playground later.*
2. **Primary purpose:** an exploration/teaching **playground** (single program,
   share links, the gallery) vs serious **multi-file development** (projects, VCS,
   perf). *Recommend: playground-grade polish on one program first.*
3. **Rendering:** confirm the **hybrid (C)**.
4. **Backend:** Python FastAPI (reuses everything, fastest) vs a server mode built
   into the Rust runtime (cleaner long-term, slower now). *Recommend: Python now;
   fold into a Rust `evident serve` later.*

## 11. Risks / unknowns

- Live recompute latency on every keystroke — debounce + incremental; `reachable()`
  is cheap (ms) for the small reachable sets we see, but pathological programs (lru's
  input fan-out, 5000-state cap) need a budget + "computing…" state.
- Constraint provenance (mapping a transition back to the source constraints that
  produced it) needs the encoder to retain source spans — currently it does not; this
  is the deepest new runtime work and may slip to a later milestone.
- The matplotlib render service is a heavyweight per-call subprocess; fine as fallback,
  not as the live path (hence hybrid).
