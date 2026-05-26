# Plans Index

Roadmap to a ≤11,000-line Rust runtime, with FSM-driven effect
dispatch and a generic FFI primitive replacing today's plugin
architecture.

Start here: [`roadmap.md`](roadmap.md).

Then per-phase plans (each phase has its own README plus per-task
files):

- [`01-ffi-effects/`](01-ffi-effects/) — FFI primitive + effect dispatcher (sequential)
- [`02-plugin-migrations/`](02-plugin-migrations/) — Move SDL/audio/shader/stdio to Evident libraries (parallel)
- [`03-language-prereqs/`](03-language-prereqs/) — Recursive claims, unbounded output, enum bindings (sequential)
- [`04-codegen-libraries/`](04-codegen-libraries/) — GLSL/SMT-LIB/reporters/passes ports (parallel)
- [`05-final-trim/`](05-final-trim/) — Cut to 11K (sequential)

Progress tracker: [`PROGRESS.md`](PROGRESS.md).

Adjacent design docs (not phased, but on deck):

- [`../design/fsms-as-functions.md`](../design/fsms-as-functions.md) — **the capstone**: `fsm` is a *function* (composes by application to completion, `result = F(init)`), `claim` is a *constraint* (composes by conjunction); nesting an fsm = spec-over-implementation, recovering the whole-output guarantee flat FSMs gave up; the condensability→guarantee spectrum (dissolve / forward-execute / CEGAR); effects percolate up (the child is a pure function returning effects as data); the three tiers as one idea. Supersedes the separate `run`/`halts_within` framings.
- [`../design/fsms-as-functions-impl.md`](../design/fsms-as-functions-impl.md) — the **turnkey implementation spec** for the capstone: pins the four open edges (keep `run()` as a one-release alias; generalize the world-only terse rewrite to *any* state var, emitting the internal `state, state_next` pair; lower `result = F(init)` → `RunFsm` at load time by `F`'s keyword; ban source-level `state_next`). Scopes **universal `_state`** as the first mergeable step (fires after REVIVE-inject/desugar — it rewrites `unify_world_syntax`/`inject_prev_tick_decls`), the corpus migration that sweeps the REVIVE passes, and the ordered session sequence ending in a CLAUDE.md rewrite to the one consistent story.
- [`../design/loop-functionizer.md`](../design/loop-functionizer.md) — wrap the step-functionizer in a native run-to-halt loop over an explicit work-stack; self-hosts the tree-walk passes (`subscriptions`/`validate`/`pretty`) without adding recursion to the language — the port shape that finally inverts the self-hosting LOC count.
- [`../design/selection-policy.md`](../design/selection-policy.md) — the selection-policy axis (determine / witness / defer) that unifies the functionizers, plus the design for the missing **defer** strategy: the residual (partial) functionizer.
- [`../design/nested-fsm-strategies.md`](../design/nested-fsm-strategies.md) — running one FSM to completion *as a value* inside another (`run(F, init)`): the three-tier strategy selector (symbolic-unroll→JIT / loop-functionizer / blocking-interpret) mirroring the functionizer fall-through, with blocking-interpret the always-correct baseline and the equivalence oracle the faster tiers are validated against.
- [`../design/minimal-runtime-implementor-contract.md`](../design/minimal-runtime-implementor-contract.md) — the north-star answer to "what must someone write to implement an Evident runtime?": the two-bucket principle (irreducible kernel vs self-hosted stdlib vs optional accelerator), the kernel enumerated and budgeted (front end + solver FFI + effect FFI + scheduler-with-recursive-enum-tree-walk), tree-walking as an enum-generic *kernel* capability (not an AST walker, not a tier-2 optimization), and the conformance contract.
- [`../design/compiled-fn-disk-cache.md`](../design/compiled-fn-disk-cache.md) — a `__pycache__`-style disk cache for the AOT pipeline. The honest finding: native code and `Z3Program` (`Dynamic<'ctx>` handles) don't serialize, so — like `.pyc` caching bytecode, not machine code — cache the expensive *results*: the tier-3-interpreted pass output (v1: `subscriptions` access-sets, keyed by claim-body hash; amortizes ZZ's +0.18s setup) and the simplified-SMT-LIB form (re-parse + cheap re-JIT). Version-tag + source-hash invalidation, `EVIDENT_CACHE` location (WW-resolver shape), a hit-equals-recompute correctness gate; native-code persistence deferred (the JIT is the cheap stage).

- [`../research/fsm-behavioral-constraints.md`](../research/fsm-behavioral-constraints.md) — research report on "parent constraint-model constrains a child FSM over its whole run": names the problem (model checking + synthesis), surveys BMC / k-induction / IC3-PDR / CEGAR / CHC, deep-dives **CHC + Z3's Spacer** with the Horn encoding and a worked countdown example, and — the load-bearing gate — an **actual inspection of the installed `z3-0.12.1` + `z3-sys-0.8.1` sources** with verdicts: **Fixedpoint/CHC is reachable via raw `z3-sys` FFI + a thin wrapper** (full API bound, `lib.rs:6215+`; Spacer in the linked libz3 4.12.1; `raw_ctx` bridge already shipping in `string_ops.rs`), and the **user-propagator is NOT bound by the crate** (present in libz3, absent from z3-sys; needs a hand-rolled extern block + raw 2-field `Solver` access). Recommends building on CHC/Spacer with BMC+k-induction as the bounded fallback and CEGAR for the recursive case where Z3 is an unsound oracle.

Cross-cutting inventory: [`../design/self-hosting-inventory.md`](../design/self-hosting-inventory.md) — every `runtime/src/**/*.rs` file classified by tier + prioritized port order.

## Working on a task

1. Read the task's plan file end-to-end.
2. If the plan is unclear or stale, push back: ask for clarification or
   a plan revision before writing code.
3. Execute. Stick to the plan's "Files touched" list. If you need to
   change a file outside the list, update the plan and commit the
   plan change separately.
4. Verify the acceptance checklist.
5. Commit with a referencing footer: `Plan: docs/plans/01-ffi-effects/02-effect-types.md`.
6. Update `PROGRESS.md` with current LOC and which task landed.

## Worktree-based parallel execution

For Phases 2 and 4, multiple tasks can run in parallel via the Agent
tool with `isolation: "worktree"`. The orchestrating session sends a
single message with one Agent call per task; each agent works in an
isolated worktree on its own branch. After completion, branches merge
to main with conflict resolution at the merge gate.

Example (Phase 2 launch):
```
[Agent for 2.1 stdio  → branch phase-2-stdio]
[Agent for 2.2 sdl    → branch phase-2-sdl]
[Agent for 2.3 audio  → branch phase-2-audio]
[Agent for 2.4 shader → branch phase-2-shader]
```

After all four complete, a non-isolated agent runs Phase 2.5 to remove
the plugin abstraction.

## Acceptance discipline

Every task has a checklist in its plan file. Don't mark a task done
until the checklist is met. The 11K target is the only hard success
criterion for the overall roadmap; if a phase undershoots, escalate
rather than fudge.
