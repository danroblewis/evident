# Runtime split — cutting the current runtime along the SMT-LIB seam

> **Status:** design doc (2026-05). Docs-only. Decides whether — and how — to
> split the EXISTING Rust runtime into (1) an Evident→SMT-LIB+metadata
> **transpiler** (front-end) and (2) an isolated **Z3-FSM engine** (SMT-LIB →
> per-tick solve → effects), and compares that split against the `new-runtime`
> greenfield rewrite. Companion to
> [`smtlib-as-compile-target.md`](smtlib-as-compile-target.md) (the north star)
> and [`smt-lib-as-ir.md`](smt-lib-as-ir.md) (the IR mapping).

## The question

The north star ([`smtlib-as-compile-target.md`](smtlib-as-compile-target.md)) says
Evident should compile to **SMT-LIB text**, Z3 should run it, and the
functionizers should optimize the resulting AST regardless of source. That
implies a clean two-part architecture:

```
                          ┌─ the SMT-LIB seam ─┐
Evident source → AST → [ FRONT-END ] → SMT-LIB text + metadata → [ ENGINE ] → effects
                       transpiler                                  per-tick solve loop
                       (no Z3, no IO)                              (Z3 + state + dispatch)
```

Three sessions probe this from three angles:
- **`behavior-contract`** — capture the current engine's semantics as
  implementation-agnostic fixtures (`runtime-contract/`), the oracle any engine
  must pass.
- **`new-runtime`** — build a *greenfield* Rust engine whose input is SMT-LIB +
  metadata (`runtime-smt/`), unconstrained by the legacy.
- **`split-plan`** (this doc) — design how to cut the *existing* runtime along
  the same seam, and decide split-vs-greenfield on evidence.

This doc is the evidence. Phase 1 surveys the current source against the seam;
Phase 2 pins the interface; Phase 3 lays out an additive migration; Phase 4
delivers the decisive recommendation.

---

## Phase 1 — Source survey

Six parallel subagents classified every module in the seam-relevant clusters as
**front-end** (Evident→AST→SMT-LIB), **engine** (SMT-LIB→solve→effects),
**entangled** (resists a clean cut), or the refinements **metadata-producer** /
**straddle** / **shared-types**. Per-cluster detail lives in
[`split-survey/`](split-survey/); this section collates.

### Collated classification (by cluster)

| Cluster | Files | LOC | Front-end | Engine | Entangled / Straddle | Survey |
|---|---|---|---|---|---|---|
| `core/` + `lexer.rs` + `parser/` | 18 | ~2,900 | 12 (parser, lexer, ast, api, seq_helpers) | 2 (z3_program, z3_types) | 1 entangled (functionizer) + 1 shared (value) + 2 infra | [core-lexer-parser](split-survey/core-lexer-parser.md) |
| `translate/` | 35 | 13,131 | 12 (~2,650) | 11 (~2,050) | **12 entangled (~8,430)** | [translate](split-survey/translate.md) |
| `effect_loop/` + `subscriptions.rs` | 9 | 1,923 | — | 4 | 2 metadata-producer + 3 entangled | [effect_loop-subscriptions](split-survey/effect_loop-subscriptions.md) |
| `functionize/` + `z3_eval.rs` | 8 | ~4,500 | 0 | **8 (all engine)** | 0 | [functionize-z3eval](split-survey/functionize-z3eval.md) |
| `runtime/` (EvidentRuntime) | 16 | 3,902 | 7 | 1 | 4 straddle + 4 support | [runtime](split-survey/runtime.md) |
| `ffi.rs`/`fti.rs`/`event_sources/`/`chc.rs`/`effect_dispatch.rs` | 13 | 3,021 | 0 | 11 | 2 entangled (minor) | [ffi-fti-eventsources-chc](split-survey/ffi-fti-eventsources-chc.md) |

(The six clusters cover the seam-bearing core. Not separately surveyed:
`commands/` + `main.rs` (CLI dispatch, front-end-facing), `portable/` (the
self-hosted Evident passes — front-end), `fsm_unroll/` (nested-FSM symbolic
unroll — engine, same `Z3Program` weld as functionize), `pretty.rs`
(diagnostics), `decompose.rs`/`z3_profile.rs` (engine, noted by the
functionize survey). None change the picture below.)

### Where the seam actually falls

The six surveys converge on a sharp, consistent picture. The runtime divides
into **three** regions, not two — and the third is the whole story.

#### 1. The front-end is already clean and ready to cross as text

Everything from source to AST, plus every AST→AST pass, has **zero live Z3
coupling** and could move into a transpiler crate mechanically:

- **Parse**: `lexer.rs` + all 9 `parser/` files — pure `String → Program`, no
  Z3 surface ([core-lexer-parser](split-survey/core-lexer-parser.md)).
- **Pure-data core types**: `core/ast.rs`, `core/value.rs`, `core/api.rs`,
  `core/seq_helpers.rs` — `#[derive(Clone)]`, serializable, no Z3 handles.
- **AST→AST passes** (`runtime/`): `desugar.rs`, `inject.rs`, `validate.rs`,
  `introspect.rs`, `load.rs`'s loading pipeline — all run before any Z3 call
  ([runtime](split-survey/runtime.md)).
- **Metadata-producers** (`effect_loop/`): `fsm.rs::resolve_fsm` (slot
  resolution → `MainShape`) and `subscriptions.rs` (world read/write
  `AccessSets`) are pure AST static analysis whose output is exactly the
  metadata the seam must carry ([effect_loop-subscriptions](split-survey/effect_loop-subscriptions.md)).

**The AST→SMT-LIB-text boundary is expressible with front-end types alone** —
proven by `translate/smtlib.rs` and the `evident dump-smtlib` command, which
take `&SchemaDecl` (from `ast.rs`) and emit text using no `Var`/`Z3Program`/
`Context`. None of the engine-side core types are needed to *author* SMT-LIB.

#### 2. The engine below the translate core is clean and source-agnostic

The runtime's "what makes it not-just-Z3" machinery does **not** care where the
Z3 AST came from, and would survive the split largely untouched:

- **The functionizer is fully source-agnostic** ([functionize-z3eval](split-survey/functionize-z3eval.md)).
  Cranelift/symbolic/glsl/llm/satisfier all consume `&Z3Program` and registries;
  none import `translate/` or the AST. The north-star claim "functionizers
  consume Z3 AST and don't care about its source" is **verified true**.
- **`z3_eval.rs` is welded to the Z3 handle *type*, not the translate
  pipeline.** It walks `Bool<'ctx>`/`Dynamic<'ctx>` via structural inspection
  (`kind()`, `children()`, `safe_decl()`) — operations identical whether the
  handles came from the C-API translate path or from **parsing SMT-LIB text into
  the same `Context`**. Re-ingest the front-end's text via `Context::from_string`
  and the entire `simplify_assertions → extract_program_partial → compile` chain
  runs unchanged.
- **The eval/solve cluster** (`translate/eval/*`, `extract.rs`) — tactics,
  cached push/pop, model decode, UNSAT cores, decomposition — is irreducibly
  engine-side and well-bounded (~1,450 LOC) ([translate](split-survey/translate.md)).
- **The IO/FFI kernel is the clean "stays Rust forever" arm**
  ([ffi-fti-eventsources-chc](split-survey/ffi-fti-eventsources-chc.md)).
  `effect_dispatch.rs` takes a *decoded* `&Effect` (not a Z3 handle); event
  sources talk to the scheduler via `Vec<(String, Value)>` writes and
  `SchedulerEvent::Tick` wakes — entirely value-level. `chc.rs` is standalone
  (test-only, not on a live path).
- **The scheduler/state/halt machinery** (`effect_loop/scheduler.rs`,
  `state.rs`, `timing.rs`) is engine-side.

**The cross-seam wire format must be SMT-LIB *text*, not `Z3Program`.**
`Z3Program` embeds `Dynamic<'ctx>` in every computation-carrying variant
(`z3_program.rs:10-16`) — no serde, no round-trip, bound to a live `Context`. It
is an internal optimizer IR, decisively not a wire format. Text is the only
candidate, and it is sufficient: the engine re-parses it to fresh handles and the
optimizer pipeline picks up from there.

#### 3. The translate core is the wall — and two entanglements resist a clean cut

The ~8,430 LOC of **entangled** translate code is where the split is hard, and
two specific couplings are the load-bearing blockers.

**(a) The whole constraint-translation core builds live handles, not text.**
The `exprs/` cluster (~1,660 LOC) returns `Bool<'ctx>`/`Int<'ctx>`/
`Datatype<'ctx>`; `declare.rs`/`datatypes.rs` (~413 LOC) build live `Sort`/
`DatatypeSort`/consts stored in `Var<'ctx>`; `inline/*` (~1,160 LOC) materializes
consts and asserts mid-walk. `translate/smtlib.rs` emits text but only for a
**scalar subset** — it is a proof of concept, not a baseline. Making the
front-end emit SMT-LIB text for the **full language** (records→datatypes,
enums→`declare-datatype`, Seq→Array, quantifier unrolling, match→ite, claim
inlining, record lifting, string ops via raw `z3_sys`) is a **~3,200–3,500 LOC
deep rewrite of ~15 files**, not a moderate port ([translate](split-survey/translate.md)).

**(b) Translation queries the solver mid-flight.** `inline/guards.rs:17-35`
(`guard_is_satisfiable`) runs `solver.push()/assert()/check()/pop()` *inside* the
inlining loop — invoked from `inline/walk.rs:176`, `inline/calls.rs:34,122,261`,
`inline/subschema.rs:136` — to prune unsatisfiable guarded branches. A text
emitter has no solver to consult. This breaks the clean "emit text, *then*
solve" pipeline: it must either drop the optimization (always emit; correctness
preserved) or be refactored into a separate pre-solve annotation pass.

**(c) The engine re-enters translate every tick.** The scheduler
(`scheduler.rs:277`) and nested-FSM loop (`nested.rs:288`) call
`query_with_pins_and_given` per tick, which resolves through JIT / cached-solver
/ full-rebuild paths — **none accept SMT-LIB text**; all are welded to the
in-memory `SchemaDecl` + translate path ([effect_loop-subscriptions](split-survey/effect_loop-subscriptions.md)).
The cached-solver slow path (`scheduler_api.rs:54–83`) is the *closest* existing
analogue to "hand the engine a transition system once, re-assert each tick" — but
it holds a live `Solver`, not text. A split engine needs a pre-built per-FSM
**transition solver** it re-asserts prev-state + inputs into.

### The fragility a split would inherit

The `runtime/` survey found **six interlocking `'static`/thread_local invariants**
that make the current orchestration layer "fine for a CLI, one runtime per
process" but fragile for any library / multi-instance / per-test-isolation use —
exactly what the greenfield plan calls out as the legacy's flakiness:

1. `mod.rs:95` — primary `Box::leak`'d `&'static Context`, one per
   `EvidentRuntime`, never freed; **all** Z3 state hangs off it.
2. `register_enums.rs:177` — each `DatatypeSort` is `Box::leak`'d; re-declaring
   an enum name in the same context is a hard fatal error (`load.rs:107`).
3. `query.rs:485–487` — `unsafe transmute` of `Bool<'_>` → `Bool<'static>`, sound
   *only* because the context is leaked.
4. `query.rs:319–331,858–860` — leaked per-component parallel worker contexts,
   accumulating for process lifetime, behind a global setup mutex.
5. `query.rs:58–62` — `unsafe impl Send/Sync for SlowPart` over a `RefCell` + raw
   `&'static Context`, justified by a 1:1 part↔thread pairing.
6. `nested.rs:13` — `thread_local! { PERCOLATED_EFFECTS }`: per-thread, not
   per-instance; two runtimes on one thread silently share the channel.

A split that lifts `runtime/` as-is inherits all six. Eliminating them is not a
refactor of this layer — it is a **redesign around a scoped (non-`'static`)
context**, which is the same work the greenfield does from line one. This is the
single most important input to the Phase 4 decision, so it is flagged here.

### Phase 1 verdict (carried into later phases)

- The **front-end** and the **sub-translate engine** (functionizer, eval, IO
  kernel, scheduler) are clean and would split well.
- The **translate core** is the wall: a ~3.2–3.5 KLOC rewrite to emit
  full-language text, **plus** an architectural decision on the
  guard-satisfiability mid-translation query.
- The cross-seam IR is **SMT-LIB text + a metadata sidecar** (`MainShape` slots +
  `AccessSets`); `Z3Program` cannot cross.
- A split inherits **six leaked-context/thread_local fragilities**; the
  greenfield avoids them by construction.

These four findings frame the interface (Phase 2), the migration (Phase 3), and
the recommendation (Phase 4).
