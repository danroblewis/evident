# Plan — split-plan: design the split of the current runtime (transpiler + isolated engine)

**Mission.** A decisive design doc for cutting the EXISTING runtime along the
SMT-LIB seam into (1) an Evident→SMT-LIB+metadata transpiler and (2) an isolated
Z3-FSM-engine — and an honest comparison against the `new-runtime` greenfield,
so the build-vs-rewrite decision is evidence-based.

**Deliverable:** `docs/design/runtime-split.md` (+ a `docs/plans/README.md`
line). Docs-only — read the source freely, change no `.rs`.

## Orchestration protocol
Phases in order. The SURVEY phase fans out parallel subagents (`general-purpose`,
`sonnet`), one per source cluster, each producing a structured classification;
you collate into the design doc. Later phases are mostly synthesis (1–2 subagents
+ your integration). Commit the doc incrementally per phase.

## Phase 1 — Source survey (parallel, the bulk)
Goal: classify every `runtime/src` module as **front-end** (Evident→AST→SMT-LIB),
**engine** (SMT-LIB→solve-loop→effects), or **entangled** (resists a clean cut),
with a one-line "why" + the seam difficulty. Subagents, 1 per cluster (parallel):
  - `core/` + `lexer.rs` + `parser/`
  - `translate/` (esp. encode/decode_ast, exprs, smtlib.rs — flag the in-memory-
    Z3-AST vs SMT-LIB-text coupling)
  - `effect_loop/` + `subscriptions.rs`
  - `functionize/` + `z3_eval.rs`
  - `runtime/` (load, query, inject, desugar, …)
  - `ffi.rs` + `fti.rs` + `event_sources/` + `chc.rs`
Each writes `docs/design/split-survey/<cluster>.md`: per-file front-end/engine/
entangled + why + seam notes.
**Gate:** a collated classification table. Commit.

## Phase 2 — The interface
Goal: the exact SMT-LIB + metadata contract between the two halves (reconcile
with `runtime-contract/FORMAT.md` if present). What the transpiler emits, what
the engine consumes, how FSM structure (state/effects/given/halt) is conveyed.
**Gate:** an "Interface" section in `runtime-split.md`. Commit.

## Phase 3 — Migration plan
Goal: an ordered, each-step-`./test.sh`-green sequence to extract the engine
behind the SMT-LIB interface without a flag-day (which modules move first, what
shims bridge during transition, where the `translate` in-memory-AST coupling
must be broken). **Gate:** a "Migration" section. Commit.

## Phase 4 — Comparison + recommendation (decisive)
Goal: split vs `new-runtime` greenfield. Where the split wins (reuses hard-won
engine semantics; incremental; lower risk) vs loses (inherits legacy
fragility/entanglement; the translate-layer coupling; the leaked-context/
thread_local issues). Cross-reference `new-runtime`'s README + the
`behavior-contract` if landed. **A decisive recommendation:** split, greenfield,
or hybrid (greenfield the engine, keep+adapt the front-end). 
**Gate:** the doc is complete + recommendation stated. Final commit + push
`session-split-plan`. **DO NOT merge to main.**

## Honest notes
- The recommendation must be decisive and evidence-grounded — this doc exists to
  settle the build-vs-rewrite question, not to hedge.
- The most likely honest answer (flag if so): "greenfield the engine (it's
  cleaner from scratch), but reuse the front-end transpiler" — i.e., the split
  *interface* matters even if we don't literally cut the legacy. Say what you find.
