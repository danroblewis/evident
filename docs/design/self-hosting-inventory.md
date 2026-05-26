# Self-Hosting Inventory — Tier Ladder and Port Order

> **Audience.** Future agents picking what to port next, and the
> reviewer of any individual port PR who wants to know whether the
> shape is reachable from where the runtime is today.
>
> **Companion docs.** [`docs/self-hosting.md`](../self-hosting.md)
> explains the swap-interface pattern (call site seam +
> `portable/` module convention).
> [`docs/perf/log-unroll-feasibility.md`](../perf/log-unroll-feasibility.md)
> is the measurement that pins what Tier 3 will and won't buy.
> This doc is the *map*: every `runtime/src/**/*.rs` file gets one
> tier, the next ten ports are named in order, and the question
> "do we wait for CC's FSM-with-loops?" gets a direct answer.

## § Tier ladder

Every file gets exactly one tier. The tiers answer **how reachable
this file is for porting from Rust to an Evident pass**, not how
algorithmically complex its work is.

| Tier | Name | What it means | Blocked on |
|---|---|---|---|
| **0** | **Kernel** | Must stay in Rust. The platform programs run *against* — parser, lexer, Z3 FFI binding, libffi marshaling, Cranelift codegen, AST/Value/Z3Program data definitions, OS event-source bridges. The thing Evident *is*, not a program written in it. | Never portable. |
| **1** | **Pure pass** | A stateless `Fn(&AST) -> AST'` (or `&AST -> bool`, `&AST -> Stats`). One match dispatch, no recursion through user-defined claims, no Z3 calls. The shape `pretty.rs` leaf arms, `validate.rs`, and `subscriptions.rs` land in. | Nothing — portable **today**. |
| **2** | **Tree recursion** | A pure pass that *recurses through the AST*. Today's Evident encodes tree recursion as mutual subclaim dispatch — the structure works, but it's blocked on `translate/inline/recursion.rs`'s "recursive claims don't constrain outputs" gap (see [`docs/self-hosting.md#1-recursive-claims-dont-constrain-their-outputs`](../self-hosting.md#1-recursive-claims-dont-constrain-their-outputs)). | **Recursive-claim output-binding fix** (a translate/inline gap, not a CC feature). |
| **3** | **Bounded loop / fixpoint** | A pass with a static upper bound (`∀ i ∈ {0..N-1}`) or an FSM that halts within N ticks. Becomes portable when CC's `halts_within` lands AND the body is affine per Z's log-unroll report (otherwise log-unroll degenerates to naive). | **CC** (`halts_within`) + an **affine-step detector**. |
| **4** | **Unbounded / external** | Depends on something outside the constraint world: real IO, wall-clock time, network, threads, OS signals, an LLM call, a subprocess. Lives in Rust forever unless a functionizer-that-calls-out (LLM, future FFI bridge) is built. | Either stays Rust or becomes an LLM-functionized pass. |

A file can **split**: one part Tier 1, another Tier 4. Where that
happens it's flagged explicitly.

### Functionizer mapping

| If the pass is … | Functionizer |
|---|---|
| Tier 1 / Tier 2, output is uniquely determined by input | **Cranelift JIT** (the existing path) |
| Tier 3, affine state update | **Cranelift + log-unroll** (post-CC) |
| Tier 3, branching state update (Mario-shaped) | **Cranelift per-tick** (no log-unroll win) |
| Tier 1 / Tier 2 but output is partially constrained (under-determined) | **SatisfierFunctionizer** (session-W) |
| Tier 1 / Tier 2 but the relation is *opaque* (no algebraic shape) | **LLM functionizer** (session-BB) |
| Tier 4 | **Cannot be functionized** — stays Rust, or routes through a future custom strategy |

## § Inventory table

LOC = wall lines of the file at the time this doc was written
(2026-05-25; total runtime/src/ = 35,071 LOC across 111 files).

Files are grouped by directory. The **"What"** column is the
one-line `//!` summary the file's module doc gives; the **"FZ"**
column names the functionizer the Evident port would lean on (or
`n/a` for Tier 0).

### `core/` — data definitions (Tier 0)

The vocabulary of the codebase. Type definitions, not orchestration.
All Tier 0: programs are *written using* these types, not in
terms of them.

| File | LOC | Tier | What | FZ |
|---|---|---|---|---|
| `core/ast.rs` | 539 | 0 | AST node enums — `Expr`, `BodyItem`, `SchemaDecl`, … | n/a |
| `core/value.rs` | 80 | 0 | `Value` (runtime values returned by queries) | n/a |
| `core/z3_types.rs` | 350 | 0 | Z3-typed bindings — `EnumRegistry`, `DatatypeRegistry`, `Var`, … | n/a |
| `core/z3_program.rs` | 266 | 0 | `Z3Program` IR consumed by functionizers | n/a |
| `core/api.rs` | 29 | 0 | `QueryResult` / `RuntimeError` — public-facing | n/a |
| `core/functionizer.rs` | 51 | 0 | `Functionizer` + `CompiledFunction` traits | n/a |
| `core/seq_helpers.rs` | 22 | **1** | Pure string utilities (`parse_seq_type`, helper names) | Cranelift |
| `core/mod.rs` | 17 | 0 | Module decls + re-exports | n/a |

### `parser/`, `lexer.rs` — front-end (Tier 0)

The thing that produces ASTs. Cannot be self-hosted in Evident
unless Evident can parse itself — circular by construction. Tier 0
forever.

| File | LOC | Tier | What | FZ |
|---|---|---|---|---|
| `lexer.rs` | 416 | 0 | Unicode operators + word-keywords → tokens | n/a |
| `parser/mod.rs` | 98 | 0 | Token-stream utilities + parser entry | n/a |
| `parser/types.rs` | 170 | 0 | Type-name parsing (generics, pins) | n/a |
| `parser/patterns.rs` | 95 | 0 | Match-arm pattern parsing | n/a |
| `parser/schema.rs` | 209 | 0 | Schema/claim/type/subclaim body parsing | n/a |
| `parser/atoms.rs` | 140 | 0 | Literals, calls, tuples | n/a |
| `parser/exprs.rs` | 395 | 0 | Expression precedence climbing | n/a |
| `parser/body_item.rs` | 361 | 0 | Body-item dispatch + membership desugaring | n/a |
| `parser/program.rs` | 212 | 0 | File → schemas + enums + imports | n/a |
| `parser/tests.rs` | 280 | 0 | Parser unit tests | n/a |

### `translate/` — Evident AST → Z3 (mostly Tier 0)

This whole directory is the bridge between Evident programs and
Z3. Most files call Z3 functions directly — they're kernel by
definition. A handful of files are pure AST→AST preprocessors
that *could* port (Tier 1/2); those are called out.

| File | LOC | Tier | What | FZ |
|---|---|---|---|---|
| `translate.rs` | 81 | 0 | Module façade for the AST → Z3 pipeline | n/a |
| `translate/preprocess.rs` | 486 | **1**/0 | Pre-translation passes: pin literal-int vars, propagate Seq lengths, fold bounds. Mostly **Tier 1** AST→AST; the parts that read solver state stay Tier 0. **Split — port the AST→AST half.** | Cranelift |
| `translate/datatypes.rs` | 185 | 0 | Build/cache Z3 datatypes for user types in Seq(UserT) | n/a |
| `translate/declare.rs` | 354 | 0 | Declare typed Z3 consts for memberships | n/a |
| `translate/extract.rs` | 571 | 0 | Extract satisfying model from Z3 solver | n/a |
| `translate/encode_ast.rs` | 1,034 | 0 | Encode Rust AST (Program) as Z3 datatype matching `stdlib/ast.ev` | n/a |
| `translate/decode_ast.rs` | 959 | 0 | Z3 model → Rust AST. Inverse of encode_ast | n/a |
| `translate/exprs/mod.rs` | 130 | 0 | Expression-translator dispatcher | n/a |
| `translate/exprs/bool.rs` | 497 | 0 | Bool-sort expression translation | n/a |
| `translate/exprs/scalar.rs` | 307 | 0 | Int/Real/String translation | n/a |
| `translate/exprs/quant.rs` | 299 | 0 | Quantifier unrolling (calls Z3) | n/a |
| `translate/exprs/record_lift.rs` | 396 | **2**/0 | Componentwise comparison/arithmetic lift. AST→AST; the lift logic is portable. **Split.** | Cranelift |
| `translate/exprs/match_expr.rs` | 161 | **2**/0 | Match → nested ITE folding. AST→AST. **Split — lift portion portable.** | Cranelift |
| `translate/exprs/range.rs` | 45 | 0 | Range literals (calls Z3 directly) | n/a |
| `translate/exprs/enums.rs` | 341 | 0 | Enum recognition + constructor calls into Z3 | n/a |
| `translate/exprs/mapping.rs` | 304 | 0 | Mapping-literal translation | n/a |
| `translate/exprs/seq_eq.rs` | 599 | 0 | Seq equality + indexing into Z3 | n/a |
| `translate/exprs/seq_field.rs` | 222 | 0 | Seq field resolution helper | n/a |
| `translate/eval/mod.rs` | 253 | 0 | Solver orchestrator (declare + assert entry points) | n/a |
| `translate/eval/solver.rs` | 191 | 0 | Default tactic chain, numeric helpers, Z3 priming | n/a |
| `translate/eval/cached.rs` | 374 | 0 | Cached-solver strategy across ticks | n/a |
| `translate/eval/decompose.rs` | 342 | 0 | Per-component classification (calls Z3) | n/a |
| `translate/eval/decode.rs` | 329 | 0 | Z3 model → `Value` decoding | n/a |
| `translate/eval/core.rs` | 130 | 0 | UNSAT-core extraction | n/a |
| `translate/eval/extra.rs` | 333 | 0 | Multi-assertion variants | n/a |
| `translate/inline/mod.rs` | 35 | 0 | Module re-exports | n/a |
| `translate/inline/walk.rs` | 288 | 0 | Recursive constraint-translation walker — body-item dispatch + assert-into-solver | n/a |
| `translate/inline/calls.rs` | 387 | 0 | Top-level claim invocation inlining | n/a |
| `translate/inline/recursion.rs` | 98 | 0 | Inlining depth counter (cap 64), per-call fresh consts | n/a |
| `translate/inline/dispatch.rs` | 247 | **1**/0 | Call-name resolution + static `∀`-unroll analysis. Resolution part is AST→AST. **Split.** | Cranelift |
| `translate/inline/rewrite.rs` | 185 | **1**, but ⛔ **per-solve-hot** | Pure AST rewrites: prefix-injection, bound-var substitution. Pure in isolation, but reached only via `inline_body_items` ← `translate/eval/*` (every solve). **Do not port** — circular + per-solve-hot (see port-queue #10). | Cranelift |
| `translate/inline/guards.rs` | 81 | 0 | `track_assert`, guard sat-check (calls Z3) | n/a |
| `translate/inline/membership.rs` | 226 | 0 | Membership body-item arm — fires type pins, declares Z3 consts | n/a |
| `translate/inline/subschema.rs` | 233 | 0 | Subclaim-of-type inlining + ∀-unrolled subschema | n/a |

### `runtime/` — top-level API (mixed)

This is where the swap-interface seam lands. Files that drive
Z3 are Tier 0; pure AST passes are Tier 1/2 (the obvious port
targets); files that touch wall-clock / env / IO are Tier 4.

| File | LOC | Tier | What | FZ |
|---|---|---|---|---|
| `runtime/mod.rs` | 317 | 0 | `EvidentRuntime` struct + top-level API surface | n/a |
| `runtime/load.rs` | 209 | 0 | Source loading orchestrator (calls fs + parser + Z3) | n/a |
| `runtime/query.rs` | 1,616 | 0 | Top-level `query` — drives Z3 solve. **Biggest single file.** | n/a |
| `runtime/sample.rs` | 48 | 0 | Sample N models via Z3 push/pop loop | n/a |
| `runtime/register_enums.rs` | 479 | 0 | Build Z3 Datatypes for enums | n/a |
| `runtime/scheduler_api.rs` | 120 | 0 | Per-tick query entry for multi-FSM scheduler | n/a |
| `runtime/analysis.rs` | 62 | 0 | Decomposition + component classification (calls Z3) | n/a |
| `runtime/reflection.rs` | 303 | 0 | Encode user-program as Z3 datatype for self-hosted passes — IS the seam | n/a |
| `runtime/introspect.rs` | 134 | **1** | Mutate loaded claims (replace body items). Pure AST mutation. | Cranelift |
| `runtime/validate.rs` | 88 | **1** | `enforce_external_only` + `register_subclaims`. Pure AST walks. **Ported by DD.** | Cranelift |
| `runtime/subscriptions.rs` (top-level) | 313 | **1** | `world_access_sets` + `body_references_identifier`. Pure AST walks. **Ported by EE.** | Cranelift |
| `runtime/desugar.rs` | 273 | **1** | Seq concat flatten + unified-world syntax + system boundary. Pure AST. | Cranelift |
| `runtime/inject.rs` | 588 | **1** | Smart-inject `state_next`/`last_results`/`effects`, `_var` time-shift, claim-arg types, LHS-eq types. Pure AST. **Biggest pure-pass target.** | Cranelift |
| `runtime/generics.rs` | 256 | **1** | `<T>` monomorphization: string parsing + textual substitution + clone | Cranelift |
| `runtime/stats.rs` | 134 | **1** | `FunctionizeStats` aggregator (struct + accumulators) | Cranelift |
| `runtime/profile.rs` | 324 | **4** | Bottleneck profiler — times each Z3 solve. Wall-clock dependency. | n/a |
| `runtime/autotune.rs` | 126 | **4** | Pricing FSM that times arith-solver candidates (30-frame window). Wall-clock. | n/a |
| `runtime/lenient.rs` | 31 | **4** | `EVIDENT_LENIENT` env-var RAII guard. Reads env, mutates global. | n/a |
| `runtime/scheduler_api.rs` (dup) | — | — | (same row above) | — |

> The split is striking: of `runtime/`'s ~5,500 LOC, only the
> ~1,720 LOC across `validate` + `subscriptions` + `desugar` +
> `inject` + `generics` + `stats` + `introspect` is Tier 1. The
> remaining ~3,780 LOC drives Z3 directly (Tier 0) or measures
> wall time (Tier 4).

### `effect_loop/` — multi-FSM scheduler (mostly Tier 4, parts Tier 1)

The scheduler IS the runtime FSM. It calls Z3 every tick and
dispatches IO. Most of it stays Rust. But the helpers — toposort,
shape resolution, state encoding — are pure passes.

| File | LOC | Tier | What | FZ |
|---|---|---|---|---|
| `effect_loop/mod.rs` | 439 | **4** | Public `run_with_ctx` entry. Drives Z3 + IO each tick. | n/a |
| `effect_loop/scheduler.rs` | 697 | **4** | Subscription-driven scheduler loop. The runtime's main loop. | n/a |
| `effect_loop/collect.rs` | 359 | **2**/0 | Effect collection from a Z3 model. AST recursion through effect Seq. **Split.** | Cranelift |
| `effect_loop/fsm.rs` | 299 | **1** | `MainShape` resolution from `SchemaDecl`. Pure pattern match. | Cranelift |
| `effect_loop/state.rs` | 97 | **1** | Encode/decode FSM state values (Value ↔ Z3 datatype). Pure. | Cranelift |
| `effect_loop/seq_chains.rs` | 95 | **1** | Extract a Seq-effect chain. Pure structural walk. | Cranelift |
| `effect_loop/toposort.rs` | 76 | — | **Evident-only (session PORT-toposort).** The Rust Kahn's algorithm + the `EVIDENT_TOPOSORT_IMPL` env gate are DELETED; ordering routes through `portable/toposort.rs` (the `ToposortRanks` integer-rank claim in `stdlib/toposort.ev`). What's left here is the per-tick shape cache, node→Effect marshaling, and cycle recovery. | n/a |
| `effect_loop/timing.rs` | 47 | **4** | Per-tick timing summaries (IO + clock) | n/a |

### `commands/` — CLI subcommands (Tier 4)

Every command is `argv → fs.read → rt.load → rt.query → println`.
All Tier 4 by IO. The pure inner work usually lives in `runtime/`
or another module already — the command file is just glue.

| File | LOC | Tier | What | FZ |
|---|---|---|---|---|
| `commands.rs` | 14 | 0 | Module index | n/a |
| `commands/common.rs` | 200 | 4 | Shared CLI helpers — load, error printing, env | n/a |
| `commands/check.rs` | 68 | 4 | `evident check` — parse-and-warn | n/a |
| `commands/query.rs` | 83 | 4 | `evident query` (explain-UNSAT mode) | n/a |
| `commands/test.rs` | 606 | 4 | `evident test` — sat_*/unsat_* discovery + runner | n/a |
| `commands/sample.rs` | 102 | 4 | `evident sample` — N-models | n/a |
| `commands/profile.rs` | 357 | 4 | `evident profile` — bottleneck print | n/a |
| `commands/effect_run.rs` | 326 | 4 | `evident effect-run` — drives the multi-FSM loop | n/a |
| `commands/infer_types.rs` | 298 | 4 | `evident infer-types` — orchestrates `stdlib/passes/literal_types.ev`. **Already self-hosted under the hood.** | n/a (uses LLM/symbolic indirectly through the pass) |
| `commands/desugar.rs` | 174 | 4 | `evident desugar` — orchestrates `stdlib/passes/desugar_passthrough.ev`. **Already self-hosted under the hood.** | n/a |
| `commands/lint.rs` | 85 | 4 | `evident lint` — orchestrates `stdlib/passes/lint_*.ev` | n/a |

### `functionize/` — JIT strategies (Tier 0)

Each functionizer is *itself* the implementation of the language
platform. They cannot self-host — porting them would be circular
(an Evident pass that compiles Evident to Cranelift would *use*
the very thing it's compiling).

| File | LOC | Tier | What | FZ |
|---|---|---|---|---|
| `functionize/mod.rs` | 32 | 0 | Trait + factory | n/a |
| `functionize/cranelift.rs` | 1,565 | 0 | Z3Program → native x86 via Cranelift | n/a |
| `functionize/symbolic.rs` | 1,059 | 0 | Symbolic regression (black-box sampling + GP search) | n/a |
| `functionize/llm.rs` | 757 | 0 | LLM code-gen strategy (calls Anthropic API) | n/a |
| `functionize/glsl.rs` | 732 | 0 | GLSL fragment-shader codegen | n/a |
| `functionize/satisfier.rs` | 310 | 0 | Partial-variable satisfier — Z3 sampling over under-determined outputs | n/a |

### `event_sources/` — async I/O bridges (Tier 4)

Every event source is a thread + channel writing to a reserved
world field. They're FFI shims by another name — Tier 4 forever.

| File | LOC | Tier | What | FZ |
|---|---|---|---|---|
| `event_sources/mod.rs` | 285 | 4 | `EventSource` trait + queue | n/a |
| `event_sources/frame_timer.rs` | 136 | 4 | Periodic-tick thread | n/a |
| `event_sources/sigint.rs` | 149 | 4 | Signal handler bridge | n/a |
| `event_sources/stdin.rs` | 159 | 4 | stdin-reader thread | n/a |
| `event_sources/file_line_reader.rs` | 176 | 4 | File-line reader thread | n/a |
| `event_sources/file_watcher.rs` | 130 | 4 | inotify/FSEvents bridge | n/a |
| `event_sources/wall_clock.rs` | 123 | 4 | Wall-clock timer | n/a |
| `event_sources/declarative_install.rs` | 152 | 4 | Declarative install dispatcher | n/a |
| `event_sources/reflection.rs` | 126 | 4 | Reflection event source | n/a |

### Single-file modules

| File | LOC | Tier | What | FZ |
|---|---|---|---|---|
| `lib.rs` | 31 | 0 | Crate root — module decls + re-exports | n/a |
| `main.rs` | 43 | 4 | CLI dispatch to subcommands | n/a |
| `pretty.rs` | 30 | **1** | Thin re-export over `portable::pretty::RustPretty` — **the canonical example port** | Cranelift |
| `effect_dispatch.rs` | 1,084 | **4** | Effect → IO (Println, LibCall, ParseInt, …). The IO surface. | n/a |
| `decompose.rs` | 321 | 0 | Re-separate composed Z3 models into connected components (union-find over `z3::ast`) | n/a |
| `subscriptions.rs` | (listed above) | — | — | — |
| `value_builders.rs` | 431 | 0 | Cranelift → Rust callbacks for Value construction (libffi marshaling) | n/a |
| `ffi.rs` | 649 | 0 | libffi + libloading shim for dynamic C calls | n/a |
| `fti.rs` | 134 | 0 | FTI type-name → install fn registry (calls into event_sources/) | n/a |
| `z3_eval.rs` | 1,472 | 0 | Z3 program extraction + simplification | n/a |
| `z3_profile.rs` | 203 | 4 | Z3 profiling + stats collection (wall-clock) | n/a |
| `portable/mod.rs` | 71 | 0 | `Portable` trait + module index for the swap interface | n/a |
| `portable/pretty.rs` | 371 | **1**/**2** | `PrettyImpl` trait + `RustPretty` + `EvidentPretty`. **Tier 1 for the Rust impl + leaf shapes; Tier 2 for the Evident impl on recursive shapes (currently blocked).** | Cranelift |

## § Summary counts

| Tier | File count | LOC | Comment |
|---|---|---|---|
| 0 — Kernel | ~75 | ~22,000 | Parser, lexer, translate, functionize, core/, Z3 evaluators, FFI/FTI shims |
| 1 — Pure pass | 16 | ~2,950 | The portable surface available *today* without any new language features |
| 2 — Tree recursion | 5 (split) | ~1,400 | Blocked on `translate/inline/recursion.rs` recursive-claim fix |
| 3 — Bounded loop / fixpoint | 0 today, ~2 candidates | ~1,000 | All real Tier 3 candidates are *inside* Tier 4 files (the scheduler loop body, profile loop body). Pure Tier 3 outside an IO loop barely exists in this codebase. |
| 4 — Unbounded / external | ~15 | ~6,700 | Commands + event sources + scheduler + effect_dispatch + autotune + profile |

The total is approximate because some files split. The big takeaway:
**~2,950 LOC of pure Tier 1 work is reachable today; another ~1,400
LOC is gated on the recursive-claim fix, not on CC.**

## § Bootstrap chain analysis

### The kernel — what must stay Rust

A small, identifiable kernel grounds the rest. Three layers:

```
                            Z3 (C library)
                                │
                          rust-z3 binding (out-of-tree)
                                │
                                ▼
  ┌─────────────────────────────────────────────────────┐
  │  Tier-0 KERNEL  (~22K LOC)                          │
  │                                                     │
  │  parser + lexer       (~1,800 LOC)                  │
  │  translate/           (~7,500 LOC)  ← AST → Z3      │
  │  functionize/         (~4,500 LOC)  ← Z3 → native   │
  │  ffi/value_builders/  (~1,200 LOC)  ← libffi shim   │
  │  core/                (~1,400 LOC)  ← data types    │
  │  effect_dispatch.rs   (~1,100 LOC)  ← IO surface    │
  │  z3_eval / decompose  (~1,800 LOC)  ← Z3 helpers    │
  │  event_sources/       (~1,400 LOC)  ← OS bridges    │
  └─────────────────────────────────────────────────────┘
                                ▲
                                │ depends on
                                │
  ┌─────────────────────────────────────────────────────┐
  │  Tier-1/2 PORTABLE  (~4,400 LOC)                    │
  │                                                     │
  │  pretty (ported)                                    │
  │  validate (DD porting)                              │
  │  subscriptions (EE porting)                         │
  │  desugar, inject, generics, introspect, stats,      │
  │  effect_loop helpers (fsm, state, toposort,         │
  │    seq_chains), preprocess (partial)                │
  └─────────────────────────────────────────────────────┘
                                ▲
                                │
  ┌─────────────────────────────────────────────────────┐
  │  Tier-4 OUTER SHELL  (~6,700 LOC)                   │
  │                                                     │
  │  commands/*, effect_loop/{mod,scheduler}, profile,  │
  │  autotune, main.rs                                  │
  │                                                     │
  │  Stays Rust because it does IO.                     │
  └─────────────────────────────────────────────────────┘
```

The arrow is "depends on": Tier 1/2 passes call into Tier 0
(`encode_ast`, `query`, model decoding) but Tier 0 does not call
back into Tier 1/2 *except* through the swap-interface seam. That
seam is the whole point — it makes the dependency one-directional.

### Are there cycles?

**Yes, one structural cycle exists but is broken by the seam.**

A self-hosted pass (e.g. `stdlib/passes/pretty.ev`) is *loaded by*
`EvidentRuntime` and *runs through* `EvidentRuntime::query`. But
`EvidentRuntime::query` is implemented in `runtime/query.rs`,
which is Tier 0. So the chain is:

```
user code
  → rt.pretty(item)            (caller-visible)
  → EvidentPretty.expr(item)   (Tier 1 — portable surface)
  → self.rt.query("Pretty", …)  (Tier 0 — kernel)
  → translate + Z3 solve       (Tier 0)
  → JIT-cached Cranelift call  (Tier 0)
```

The kernel never reaches back up into Tier 1 *except* via the
swap-interface seam. The cycle is broken by selection:
`portable/pretty.rs::default_impl()` picks Rust *or* Evident at
construction time. If the Evident impl is chosen, the kernel still
runs every step underneath — there's no recursive call from the
JIT compiler back to a self-hosted pass that compiles JIT code.

**The dangerous cycle that does NOT exist (and must not):** a
self-hosted pass that the kernel needs in order to *load itself.*
If `validate.ev` had to be loaded by the Evident runtime in order
for `validate.ev` itself to load, we'd be stuck. The Rust impl is
preserved as the default precisely so this can't happen — the
runtime can always come up without any pass file.

### The longest dependency chain

From `evident effect-run mario.ev` down to `Z3.check()`:

```
main.rs                                    Tier 4
  → commands::effect_run::cmd_effect_run    Tier 4
  → EvidentRuntime::run_with_ctx            Tier 0 (effect_loop/mod.rs)
  → run_scheduler                           Tier 4 (effect_loop/scheduler.rs)
  → rt.scheduler_query_*                    Tier 0 (scheduler_api.rs)
  → translate::evaluate_*                   Tier 0 (translate/eval/*)
  → inline_body_items                       Tier 0 (translate/inline/*)
  → expr translators                        Tier 0 (translate/exprs/*)
  → assert into z3::Solver                  Tier 0 (rust-z3)
  → Z3.check()                              kernel
```

Nine layers. Most are tightly coupled (mod.rs's, dispatchers). No
single function dominates — the most isolated layer is the inner
expr translator, which is also the most likely to admit individual
ports as Tier 2 fixes land.

## § Prioritized port order

After DD (`validate.rs`) and EE (`subscriptions.rs`) land, here
are the next ten in order. The ordering optimizes for:

1. **Surfaces the swap-interface seam to more callers.** Every port
   that goes through `portable/` exercises the marshaling path and
   makes the next port easier.
2. **Big LOC payoff per unit of complexity.** Prefer pure passes
   over splits.
3. **Test surface already exists.** Pick passes whose Rust
   behaviour has good test coverage so cross-validation is easy.
4. **Avoid Tier 2 until the recursive-claim fix lands.** Don't
   schedule a port whose pass needs `match` arms with recursive
   `Pretty(child)` calls until that gap closes.

### Port queue

| # | File | LOC | Tier | Why this next |
|---|---|---|---|---|
| 1 | `runtime/stats.rs` | 134 | 1 | Pure aggregator struct — accumulators only. **Smallest possible second port.** Sets a precedent for "stat-collection passes" (next: `z3_profile.rs` stats). |
| 2 | `effect_loop/toposort.rs` | 140 | ✅ **DONE** | Cut over to Evident-only (session PORT-toposort): Rust Kahn's algorithm + `EVIDENT_TOPOSORT_IMPL` gate DELETED, ordering routes through `portable/toposort.rs` (`ToposortRanks` integer-rank claim). Setup-only (per-tick cache), Mario tick-0 ~51ms. |
| 3 | ~~`effect_loop/fsm.rs` (`MainShape` resolution)~~ — **DROPPED, do not port** | 299 | — | **NOT a candidate.** This is keyword-gated SLOT RESOLUTION, not detection: `resolve_fsm` returns `None` unless `keyword == Keyword::Fsm` (session TT killed shape-detection — the `fsm` keyword is the SOLE FSM signal). The body walk only resolves *which slots* an already-`fsm` schema uses. The old "is this an FSM, and which slots?" phrasing here was wrong and misleading — self-hosting this would (a) re-introduce the rejected "classify a schema as an FSM by shape" framing the keyword gate exists to forbid, and (b) add per-schedule cost on the hot scheduler path for work that is already cheap and correct. Leave in Rust. |
| 4 | `effect_loop/state.rs` | 97 | 1 | Pure Value ↔ Z3-datatype encoding — the field-by-field walk is mechanical and would mirror what `portable/pretty.rs` already does for AST encode. |
| 5 | `effect_loop/seq_chains.rs` | 95 | 1 | Pure structural walk — extracts a Seq-effect chain. Small. |
| 6 | `runtime/introspect.rs` | 134 | 1 | Pure schema mutation expressed as functional rewrite (`replace_body_item_in_claim` etc.). Doubles as a stress-test of "AST → AST" passes returning the whole program. |
| 7 | `runtime/generics.rs` | 256 | 1 | `<T>` monomorphization: string parsing + textual substitution + clone. The string-parsing half is non-trivial in Evident; ports as a stress-test for `stdlib`'s string ops. |
| 8 | `runtime/desugar.rs` | 273 | 1 | Seq concat flatten + unified-world syntax. **The Seq-concat flatten especially: it's a fan-out rewrite that exercises Seq operations.** |
| 9 | `runtime/inject.rs` (in three pieces) | 588 | 1 | Biggest pure-pass surface. Land one rule at a time: `inject_fsm_params` (~200 LOC), then `inject_prev_tick_decls` (~150), then `inject_claim_arg_types` / `inject_lhs_eq_types` (~200). |
| 10 | ~~`translate/inline/rewrite.rs`~~ — **DISQUALIFIED, do not port** | 185 | — | **NOT a candidate (session safe-port).** It is pure AST→AST in isolation (hence the Tier-1 label), but its only callers are `translate/inline/membership.rs` + `subschema.rs`, reached *exclusively* through `inline_body_items` ← `translate/eval/{core,cached,decompose,extra,mod}.rs` — i.e. the **per-solve / eval-core path** that runs on every `query` and every scheduler tick. Self-hosting it is the same trap that disqualified `preprocess`'s collect functions: (a) **circular** — a rewrite pass would itself need to translate→inline→rewrite to run; (b) **per-solve-hot** — sessions YY/ZZ measured the self-hosted walk ~10⁴× slower *per invocation*, tolerable once at load but lethal on the per-solve path. Tier 1 measures "is it a pure pass," not "is it on the hot path"; rewrite.rs is both, and the second kills it. Leave in Rust. |

### After #10 — what's gated on what

| Wave | Files | Gate |
|---|---|---|
| Wave 1 (now) | The 10 above | Nothing — Tier 1, portable today |
| Wave 2 | `pretty` full surface; `translate/inline/dispatch.rs` resolution; `translate/exprs/match_expr.rs` lift portion; `translate/preprocess.rs` AST half | **Recursive-claim output-binding fix** in `translate/inline/recursion.rs` |
| Wave 3 | `runtime/autotune.rs` (FSM body, sans timing) | **CC's `halts_within`** (FSM has a fixed 30-tick window) + a way to *expose* the wall-clock measurement to the Evident side without IO |
| Never | Parser, lexer, translate kernel, functionize, ffi/value_builders, event_sources, commands | Tier 0 / Tier 4 forever |

## § Direct answer — do we wait for CC's FSM-with-loops?

**No. Don't wait.**

Three reasons, in order of weight:

### 1. The Tier 1 surface alone is ~2,950 LOC of reachable work

After DD/EE close, ten more ports are ready *with no new language
or runtime feature.* That's months of focused self-hosting work
before CC's `halts_within` becomes the bottleneck.

### 2. The actual bottleneck above Tier 1 is NOT CC

The next gap that bites is **"recursive claims don't constrain
their outputs"** in `translate/inline/recursion.rs` — see
[`docs/self-hosting.md#1-recursive-claims-dont-constrain-their-outputs`](../self-hosting.md#1-recursive-claims-dont-constrain-their-outputs).
This is what blocks `pretty`'s recursive shapes today. It is a
*translator* fix, completely orthogonal to CC's halts-within work.
If the team waits for CC and then immediately tries to port `pretty`
fully, it'll be blocked on a *different* fix and the wait will have
bought nothing.

### 3. Tier 3 is structurally thin in this codebase

The classic Tier 3 use case — "a pure pass that loops a bounded
number of times" — barely exists outside the IO loop. The looping
files in this codebase are:

- **`effect_loop/scheduler.rs`** — loops over ticks but does IO
  every tick → Tier 4.
- **`runtime/autotune.rs`** — loops over candidate solvers but
  needs wall-clock measurements → Tier 4.
- **`runtime/profile.rs`** — same → Tier 4.
- **`runtime/sample.rs`** — push/pop loop but drives Z3 directly →
  Tier 0.
- **Translator inlining recursion** (depth ≤ 64) — drives Z3 →
  Tier 0.

Per Z's measurement
([`docs/perf/log-unroll-feasibility.md`](../perf/log-unroll-feasibility.md)),
log-unroll only helps for **affine** bodies, and the affine
candidates in this repo are tiny (frame counters, RNG seeds — none
of which are pure passes that we'd want to port). The Mario `game`
loop has ratio ≈ 1.98×: log-unroll wins nothing.

**So the runway when CC lands is: a few hundred LOC of
log-unrollable counter logic, after the recursive-claim fix lands,
after the 2,950 LOC of Tier 1 work above lands.** That's the right
order to pursue these in.

### What to ask for instead

If the team wants to *prepare* for CC, the highest-value
preparation is **the recursive-claim output-binding fix** in
`translate/inline/recursion.rs`. That unlocks Tier 2 (~1,400 LOC)
and is on the critical path between "Tier 1 is exhausted" and
"Tier 3 becomes interesting." See
[`docs/plans/03-language-prereqs/01-recursive-claims.md`](../plans/03-language-prereqs/01-recursive-claims.md)
(which already exists with unchecked acceptance criteria).

## § Mode-2 candidates — function → constraint

Everything above is **mode-1** self-hosting: replace a Rust *algorithm*
with an Evident *walk* that computes the same answer (pretty, validate,
subscriptions, toposort, …). The SMT-LIB north star
([`docs/design/smtlib-as-compile-target.md`](smtlib-as-compile-target.md))
names a second mode: a Rust function whose job is really to *decide a
property* is more honestly written as the **constraint it checks**, not
as the procedure that checks it. The win is legibility — the invariant
becomes the spec — not LOC.

Not every mode-2 candidate should *route through a Z3 solve*, though.
The test is: does a solve buy anything over stating the constraint
plainly in Rust? When the property is a cheap set/relation check on a
handful of items, a solve is a strictly slower way to compute the same
thing, and a bare SAT/UNSAT answer often can't even produce the witness
the caller needs (e.g. an error message naming the offending pair). For
those, the right move is to **write the Rust so it reads as the
constraint** and record it here as a mode-2 candidate, rather than force
a solve.

| Site | The constraint it really is | Routed through Z3? | Why |
|---|---|---|---|
| `effect_loop/mod.rs::check_single_owner` (multi-writer disjoint check) | The relation world-field → writer is a (partial) **function**: every field has at most one writer. A disjointness / all-different property. | **No — kept Rust (session safe-port).** | Load-time-but-on-every-`effect-run`; a solve here is a slower set-intersection, and SAT/UNSAT can't name the conflicting writer pair the error reports. The Rust is now a field→owner map — the constraint *is* the code. |

`check_single_owner` is the worked reference for "express the invariant
declaratively, keep it in Rust": the function-doc states the property
first and the map second; the prior O(writers²) pairwise-overlap scan is
gone. If a future seam makes a witnessed all-different solve cheap and
faithful (model extraction naming the violating pair), this is the first
candidate to actually route through it.

## § Risks

### Perf

**Risk.** Every self-hosted pass adds an `EvidentRuntime::query`
call. The first call JIT-compiles; steady state is microseconds
(see [`docs/self-hosting.md#cost`](../self-hosting.md#cost)). Still,
a pass invoked once per claim during loading adds up.

**Mitigation.**
- Hold `EvidentPretty`-style impls across calls (already the
  documented pattern; reaffirm in any new port's module doc).
- Benchmark before each port commit using a representative file
  (Mario, the SDL demos). Reject if total load time regresses
  > 10%.
- For passes that are called from inside the per-tick scheduler
  (rare — most are load-time only), measure per-frame impact and
  refuse to land if it shows.

### Correctness

**Risk.** The Evident impl silently produces wrong output. The
"recursive claims don't constrain outputs" gap surfaces this as
a *known* failure mode: bounded inlining caps at depth 64 and any
deeper recursion returns garbage. A naïve port that doesn't yet
hit recursion can drift into recursion later as the Evident pass
grows.

**Mitigation.**
- Every port must ship a `runtime/tests/<name>_equivalence.rs`
  cross-validation test (Rust impl vs Evident impl) on a
  representative corpus.
- Add a "faithful subset" comment in each pass file naming exactly
  which shapes the Evident impl handles. Sentinels (`<unsupported-…>`)
  for the rest, pinned by the equivalence test.
- The Rust impl stays the default until the equivalence test has
  100% identity coverage on the production corpus.

### Debug experience

**Risk.** A bug in a self-hosted pass is harder to debug than the
Rust equivalent: the trace goes through Z3 + the JIT + an enum-
encoded AST, with no Rust-level breakpoints in the pass logic. A
diff between Rust and Evident impls becomes the only diagnostic.

**Mitigation.**
- Keep the swap-interface seam `EVIDENT_<PASS>_IMPL=rust|evident`
  in *every* port. A user hitting a bug can switch to the Rust impl
  and confirm the issue is pass-specific.
- Add a `EVIDENT_TRACE_SWAP=1` env var that logs every swap-call
  with input/output for diff-against-Rust comparison. (One-time
  infrastructure investment; pays off every port forward.)
- Reject any port PR whose Evident pass cannot be *read* — if the
  pass is so dense that a reviewer can't trace what it does in the
  failing case, the gain in LOC isn't worth the loss in
  debuggability.

### Bootstrap fragility

**Risk.** A future port lands a pass that the runtime itself needs
to *load* — creating an implicit circular dependency where the
runtime can no longer come up without the pass file present.

**Mitigation.**
- Hard rule: **every Tier 1 / Tier 2 port keeps the Rust impl as
  the *constructible default*.** `EvidentRuntime::new()` must
  succeed with no Evident pass files on disk. The seam selects
  Evident only when explicitly requested.
- Code-review check: any PR that modifies `runtime/load.rs` to
  require a pass file is a bootstrap-fragility regression and
  should be rejected.

### Sequencing across parallel sessions

**Risk.** Sessions DD + EE + future ports race on the same areas
(both touch `core/ast.rs` re-encoding, or both add a stdlib pass).
Merge conflicts. Or worse, two ports independently both depend on
some private helper in `translate/encode_ast.rs` and copy it
locally, drifting over time.

**Mitigation.**
- One port per session for now (DD = validate, EE = subscriptions).
  No two-port sessions until the seam is mature.
- After the next ~3 ports land, harvest the duplicate `encode_*` /
  `decode_*` helpers into a published module (`translate/marshal.rs`
  or similar). This is the "future cleanup" already mentioned in
  [`docs/self-hosting.md#marshaling`](../self-hosting.md#marshaling).
- Track the duplication in a stub TODO comment in each port's
  module doc, so a future cleanup PR can find them all by grep.

## § Appendix — files by tier (compact list)

For quick reference and grep.

**Tier 0 (~75 files, ~22K LOC):** `core/{ast,value,z3_types,z3_program,api,functionizer,mod}.rs`, `core/seq_helpers.rs` (T1 helper inside T0 dir), `lexer.rs`, `parser/{mod,types,patterns,schema,atoms,exprs,body_item,program,tests}.rs`, `translate.rs`, `translate/{preprocess (partial), datatypes, declare, extract, encode_ast, decode_ast}.rs`, `translate/exprs/{mod, bool, scalar, quant, record_lift (partial), match_expr (partial), range, enums, mapping, seq_eq, seq_field}.rs`, `translate/eval/{mod, solver, cached, decompose, decode, core, extra}.rs`, `translate/inline/{mod, walk, calls, recursion, dispatch (partial), guards, membership, subschema}.rs`, `runtime/{mod, load, query, sample, register_enums, scheduler_api, analysis, reflection}.rs`, `functionize/{mod, cranelift, symbolic, llm, glsl, satisfier}.rs`, `commands.rs`, `lib.rs`, `decompose.rs`, `value_builders.rs`, `ffi.rs`, `fti.rs`, `z3_eval.rs`, `portable/mod.rs`.

**Tier 1 (16 files, ~2,950 LOC):** `pretty.rs`, `portable/pretty.rs`, `core/seq_helpers.rs`, `runtime/validate.rs`, `runtime/subscriptions.rs` (top-level, also `subscriptions.rs`), `runtime/desugar.rs`, `runtime/inject.rs`, `runtime/generics.rs`, `runtime/stats.rs`, `runtime/introspect.rs`, `effect_loop/fsm.rs`, `effect_loop/state.rs`, `effect_loop/seq_chains.rs`, `effect_loop/toposort.rs`, `translate/inline/rewrite.rs`, `translate/preprocess.rs` (partial — AST half).

**Tier 2 (5 split files, ~1,400 LOC blocked):** `translate/exprs/record_lift.rs`, `translate/exprs/match_expr.rs`, `translate/inline/dispatch.rs` (resolution half), `effect_loop/collect.rs` (Tier 2 walk through Tier 0 dispatch), `portable/pretty.rs` (Evident impl for recursive shapes).

**Tier 3 (0 today, ~2 future candidates):** None pure today. *Possibly* `runtime/autotune.rs`'s state machine body (if wall-clock can be projected out) — but that's a Tier 3 contained inside a Tier 4 file.

**Tier 4 (~15 files, ~6,700 LOC):** `main.rs`, `commands/*.rs`, `event_sources/*.rs`, `effect_dispatch.rs`, `effect_loop/{mod,scheduler,timing}.rs`, `runtime/{profile,autotune,lenient}.rs`, `z3_profile.rs`.
