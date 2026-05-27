# Split survey — runtime (EvidentRuntime orchestration)

## Summary

- 16 files; 3902 total LOC.
- By class: **front-end** 7 files (desugar, inject, validate, register_enums, load,
  introspect, lenient); **straddle** 4 files (query, sample, scheduler_api, reflection);
  **engine** 1 file (nested — owns `PERCOLATED_EFFECTS` thread_local);
  **support** 4 files (mod, autotune, stats, analysis — bookkeeping and tooling that
  serve both sides).

### Headline findings

**(a) Clean front-end passes (AST→AST before emit)**

`desugar.rs`, `inject.rs`, `validate.rs`, `register_enums.rs`, and the loading pipeline
in `load.rs` are all pure AST transforms executed unconditionally at load time, before
any Z3 translate call happens. They do not touch the Z3 context. `introspect.rs` is the
same — it mutates AST and flushes solver caches but never calls translate. `lenient.rs`
is a trivial env-var RAII guard used only by the functionizer path inside `query.rs`.

`register_enums.rs` is the one near-exception: it constructs `'static` Z3
`DatatypeSort` objects (via `Box::leak`) and stores them in the `EnumRegistry`. These
are pure type declarations with no solver state, but they ARE bound to the leaked
`'static` Z3 context. They are effectively compile-time artifacts used as type metadata
by the translate layer; re-declaring the same enum name in the same Z3 context is fatal.
This is the weakest "clean" pass — in a split world it would need to either remain
load-time-only, or the DatatypeSort metadata would move into the engine.

**(b) How welded query/sample are to translate+solve (the straddle)**

`query.rs` (1151 LOC) is the most deeply entangled file. The translate→solve→decode
sequence is as follows (see Seam notes for line citations). The weld operates on three
levels simultaneously:

1. **In-memory Z3 ASTs** — `build_cache` returns a `CachedSchema<'static>` containing a
   live Z3 `Solver<'static>` with all body assertions already asserted into it. All
   subsequent per-tick calls `push()/assert(given)/check()/pop()` against this cached
   solver. The Z3 objects live in, and are owned by, the leaked `'static` `Context`.
2. **`'static` lifetime transmute** — `query.rs:485–487` applies an
   `unsafe std::mem::transmute` to extend the lifetime of `Bool<'_>` slices to
   `Bool<'static>`, which is only sound because the context is leaked. Any split that
   makes the context non-static breaks this.
3. **Per-component decomposition and JIT** — the slow-part builders
   (`build_sequential_slow`, `build_parallel_slow`) create NEW leaked `'static`
   contexts for parallel components and call `translate_var` to reproduce Z3 AST nodes
   in those private contexts. This code is in `query.rs:820–884`.

`sample.rs` is a thin wrapper: it calls `build_cache` (translate) then
`sample_cached_inner` (solve). The weld is identical in structure but simpler (no JIT).

`scheduler_api.rs` reuses the JIT path from `query.rs` via `try_functionize_z3`, then
falls back to `evaluate_with_extra_assertions` — same translate+solve sequence, exposed
as the per-tick entry point the effect loop calls.

`reflection.rs` encodes the AST as a Z3 Datatype (translate), then calls
`evaluate_with_extra_assertion` (solve). It is straddled purely because the AST
encoding produces live Z3 objects that must match the already-loaded `'static` context.

**(c) Held live-Z3 state / thread_local / leaked-Context fragility (KEY finding)**

The current runtime has multiple interlocking `'static`-lifetime invariants that are
fragile across multiple engine instances:

1. **`mod.rs:95` — leaked primary context**: `EvidentRuntime::new()` does
   `Box::leak(Box::new(Context::new(&cfg)))` to produce a `'static Context`. This
   means every Z3 AST node anywhere in the runtime (inside `CachedSchema`, `SlowPart`,
   `EnumRegistry`, `DatatypeRegistry`, compiled function caches) is implicitly tied to
   this single leaked context. There is no `Drop` and no cleanup; the context leaks for
   the process lifetime. Creating a second `EvidentRuntime` creates a second leaked
   context, and the two are entirely separate — Z3 ASTs from one cannot be used in the
   other (Z3 would panic or produce wrong results).

2. **`register_enums.rs:177` — leaked DatatypeSorts**: `register_enums` does
   `Box::leak(Box::new(dt))` per enum sort, and the resulting `&'static DatatypeSort`
   is stored in the `EnumRegistry`. Once an enum is registered in a context, it can
   never be re-registered in the same context under the same name (`load.rs:107` comment:
   "leaked DatatypeSorts live forever in Z3; re-declaring same name will fail"). This is
   a hard, invisible state constraint: a second load of the same source file that
   includes enum declarations will fail if it reuses the same context.

3. **`query.rs:485–487` — `unsafe transmute` on solver assertions**: solver assertions
   extracted from a `CachedSchema` are transmuted to `'static` so they can be stored in
   the `ClaimPlan` cache and reused across ticks. The safety justification is that the
   context is leaked. Any change to context lifetime invalidates this.

4. **`query.rs:326–331` / `query.rs:858–860` — leaked parallel worker contexts**: for
   parallel slow-solve components, `new_leaked_context()` and the inline `Box::leak` in
   `build_parallel_slow` create additional leaked contexts per component, serialized via
   `z3_setup_lock`. These are permanently leaked and accumulate for the process lifetime.
   There is no tracking of how many contexts exist.

5. **`nested.rs:13–16` — `thread_local! { PERCOLATED_EFFECTS }`**: effects from nested
   `run(F, init)` are placed in a thread-local `RefCell<Vec<Effect>>`. The scheduler
   calls `take_percolated_effects()` to drain it after each tick. In a multi-threaded or
   multi-instance scenario, this thread-local is per-thread, not per-runtime-instance;
   two runtimes on the same thread would share the same percolated-effects channel
   silently.

6. **`query.rs:61–62` — `unsafe impl Send/Sync for SlowPart`**: `SlowPart` holds a
   `RefCell` (not `Sync`) and raw `&'static Context`. The manual `unsafe impl Sync` is
   justified by the 1:1 part-to-thread pairing guarantee, but it is load-bearing: any
   accidental aliasing of a `SlowPart` across threads would be UB.

**Bottom line on fragility inheritance**: a split that takes the current `runtime/`
layer as-is would inherit ALL of these invariants. The greenfield plan's goal of
"avoiding the leaked-Context fragility" is concrete and achievable only by redesigning
the engine layer around a non-leaked, scoped context — which is a new design, not a
refactored split of the current code.

## Per-file classification

| File | LOC | Class | Why | Seam difficulty | Cross-seam coupling (esp. held Z3 state) |
|---|---|---|---|---|---|
| `mod.rs` | 170 | **entangled** | Defines `EvidentRuntime` struct: holds `&'static Context`, all Z3 caches, and exposes `z3_context()` to engine callers | high | Struct definition is the root of ALL Z3 lifetime coupling; must be split at the type level |
| `load.rs` | 149 | **front-end** | Parses source, resolves imports, runs all AST→AST passes before any Z3 call; flushes caches on reload | low | Calls `register_enums` (which touches the context), then flushes solver caches — no direct translate |
| `query.rs` | 1151 | **straddle** | Translate (build_cache), simplify, decompose, JIT/slow-solve per component; all against `&'static Context` | high | `&'static Context`, leaked solver caches, parallel leaked contexts, `unsafe transmute`; see Seam notes |
| `sample.rs` | 30 | **straddle** | Calls `build_cache` (translate) + `sample_cached_inner` (solve) | med | Same context coupling as query but simpler (no JIT) |
| `scheduler_api.rs` | 100 | **straddle** | Per-tick scheduler entry: delegates to JIT path then `evaluate_with_extra_assertions` | med | Calls `try_functionize_z3`; also directly accesses `self.z3_ctx` and `self.enums` to encode Datatype pins |
| `desugar.rs` | 432 | **front-end** | `unify_world_syntax`, `unify_state_syntax`, `desugar_seq_concat` — pure AST→AST rewrites, no Z3 | low | None; pure AST manipulation |
| `inject.rs` | 287 | **front-end** | `inject_claim_arg_types`, `inject_lhs_eq_types` — pure AST→AST inference, no Z3 | low | None; reads `EnumRegistry.by_variant` (read-only borrow, not Z3 state) |
| `validate.rs` | 21 | **front-end** | `enforce_external_only` (delegates to Evident pass), `register_subclaims` — both pure AST operations | low | None |
| `register_enums.rs` | 420 | **front-end\*** | Builds Z3 `DatatypeSort`s at load time, but these are type metadata not solver state | med | Calls `Box::leak(Box::new(dt))` (register_enums.rs:177); leaked sorts bind to the `'static` context; re-declaration fatal |
| `reflection.rs` | 212 | **straddle** | Encodes user AST as Z3 Datatype (translate), then calls `evaluate_with_*` (solve) | med | Calls `encode_program` → live `z3::ast::Datatype<'static>`; calls `evaluate_with_extra_assertion` (full translate+solve) |
| `analysis.rs` | 54 | **straddle** | `analyze_decomposition`, `classify_components`, `query_with_core` — all call translate+solve functions | med | Thin wrappers over `crate::translate::*`; no independent Z3 state but always requires live context |
| `introspect.rs` | 105 | **front-end** | Mutates AST (prepend/replace body items), mirrors into `program.schemas`, flushes solver cache | low | Calls `self.cache.borrow_mut().clear()` — touches RefCell cache but no Z3 translate |
| `nested.rs` | 539 | **engine** | Drives `run(F, init)` nested FSMs to completion; owns `PERCOLATED_EFFECTS` thread_local | high | `thread_local! { PERCOLATED_EFFECTS }` (nested.rs:13); also calls `query_with_pins_and_given` per tick = re-entrant engine call |
| `autotune.rs` | 97 | **support** | Z3 `smt.arith.solver` auto-tuner; pure timing statistics, no Z3 objects | low | No Z3 coupling; pure timing/config state |
| `stats.rs` | 111 | **support** | JIT statistics structs; no Z3 coupling | low | None |
| `lenient.rs` | 24 | **support** | RAII guard for `EVIDENT_LENIENT` env var; no Z3 coupling | low | None |

*`register_enums.rs` is called "front-end\*" because it executes at load time and produces only type metadata, but it does create `'static`-bound Z3 objects (leaked `DatatypeSort`s).

## Seam notes

### The fragility audit: `thread_local`, cached Context, leaked handles

**`mod.rs:42–43, 95`** — The leaked primary context:
```rust
pub(super) z3_ctx: &'static Context,
// ...
let ctx: &'static Context = Box::leak(Box::new(Context::new(&cfg)));
```
This is the root cause of all `'static` lifetime propagation through the codebase. Every
`CachedSchema<'static>`, `ClaimPlan`, `SlowPart`, `EnumRegistry` entry, and
`DatatypeRegistry` entry lives relative to this leaked allocation. The context is never
freed. A second `EvidentRuntime` instance in the same process gets a second leaked
context; their Z3 ASTs cannot cross contexts.

**`mod.rs:46–55`** — Per-schema solver caches (`cache`, `fn_cache`, `slow_path_cache`):
All three caches hold `CachedSchema<'static>` or `ClaimPlan` values that contain live
`Solver<'static>` objects. The solvers are not cleared between ticks — they are reused
via `push()/pop()`. These are owned by the leaked context. Flushing them on reload
(`load.rs:100–105`) is a necessary correctness measure, not a cleanup — the Z3 context
itself remains.

**`register_enums.rs:177`**:
```rust
let leaked: &'static DatatypeSort<'static> = Box::leak(Box::new(dt));
```
Enum sorts are leaked separately from the main context. They are permanent. The
`load.rs:107` comment explicitly calls this out: "Note: leaked DatatypeSorts live
forever in Z3; re-declaring same name will fail." This means the only safe usage model
is: load all source files once at startup, never reload an enum declaration. This is
what the CLI does; a library that tries to reload is not supported.

**`query.rs:483–488`** — The `unsafe transmute`:
```rust
// Z3 ASTs are reference-counted by the `'static` Context; transmute lifetime is sound.
let assertions_local = cached.solver.get_assertions();
let assertions: Vec<z3::ast::Bool<'static>> = unsafe {
    std::mem::transmute::<Vec<z3::ast::Bool<'_>>, Vec<z3::ast::Bool<'static>>>(
        assertions_local)
};
```
`get_assertions()` returns `Bool<'_>` tied to the solver's borrow. The transmute to
`'static` is justified by the leaked context: the underlying Z3 AST objects will never
be freed. But it creates a hard coupling: any design that uses a scoped (non-`'static`)
context would break this, requiring a full redesign of the simplification/decomposition
pipeline.

**`query.rs:319–331`** — `z3_setup_lock` and `new_leaked_context`:
```rust
fn z3_setup_lock() -> std::sync::MutexGuard<'static, ()> { ... }
fn new_leaked_context() -> &'static Context {
    let _guard = z3_setup_lock();
    Box::leak(Box::new(Context::new(&cfg)))
}
```
For parallel slow components, each component gets its own leaked context. There is a
global mutex (`SETUP_LOCK: Mutex<()>`) serializing context creation, because Z3 context
creation is historically racy. These leaked worker contexts also accumulate permanently.
The comment at `query.rs:58–62` explains the unsafety contract:
```
// SAFETY: Parallel parts each own a private Z3 context touched by exactly one thread
// (1:1 part↔thread pairing).
unsafe impl Send for SlowPart {}
unsafe impl Sync for SlowPart {}
```

**`nested.rs:13–16`** — Thread-local percolated effects:
```rust
thread_local! {
    static PERCOLATED_EFFECTS: RefCell<Vec<Effect>> = const { RefCell::new(Vec::new()) };
}
```
This is the engine-side output of nested `run(F, init)`. The scheduler calls
`take_percolated_effects()` (exported from `mod.rs:27`) after each tick. The thread-local
is per-thread, not per-runtime-instance. If two `EvidentRuntime` instances ran on the
same thread (e.g. in tests), their nested run effects would share the same drain channel.
Current usage (CLI: one runtime per process, one scheduler thread) makes this safe.
Greenfield design should scope this to a context or a per-call-site channel instead.

**Summary**: There are NO external `thread_local` patterns outside `nested.rs`. The main
fragility vectors are (1) the leaked primary context and everything bound to it, (2) the
leaked per-enum `DatatypeSort`, and (3) the leaked per-parallel-component worker
contexts. All three are "safe for a CLI tool" by design (the original `mod.rs:42`
comment: "one per process is fine for a CLI tool") and all three would need to be
redesigned for library/multi-instance use.

### `query.rs` straddle: the translate→solve→decode sequence

The main entry point `EvidentRuntime::query` (`query.rs:933`) shows the structure
plainly:

```
query(name, given)
  └─ resolve_runs(base, given)           // engine: drives nested FSMs via Z3 (nested.rs)
  └─ try_functionize_z3(name, schema, given)   // straddle: builds CachedSchema + JIT
       └─ build_cache(schema, ...)       // TRANSLATE: AST → solver with assertions
       └─ simplify_assertions(ctx, ...)  // Z3 simplify tactic (still in translate domain)
       └─ decompose_simplified(...)      // pure Rust decomposition on Z3 Bool nodes
       └─ compile_one_component(...)     // JIT: extract Z3Program → Cranelift artifact
       └─ build_sequential_slow(...)     // SOLVE: build solver per uncompiled component
       └─ execute_plan(plan, given)      // SOLVE: run JIT + slow-solve, merge bindings
  └─ crate::translate::evaluate(...)    // TRANSLATE+SOLVE (fallback): full rebuild per tick
```

The bisection point for an SMT-LIB split would need to be between `build_cache`
(translate) and `execute_plan` (solve). Concretely:

- **Front-end emit path**: call `build_cache` → run Z3 simplify → emit the simplified
  assertions as SMT-LIB text, plus variable metadata (env). This is what the prototype
  in `commands/dump_smtlib.rs` does for the non-FSM subset.
- **Engine solve path**: accept SMT-LIB text + metadata → construct a `Solver`, assert
  the SMT-LIB, pin givens, `check()`, decode model.

The weld difficulty is **high** for three reasons:
1. The decomposition/JIT pipeline operates on live Z3 `Bool<'static>` objects returned
   from `simplify_assertions`. An SMT-LIB round-trip would lose the in-memory AST
   structure needed for `collect_touched_names` and `extract_program_partial`.
2. The slow-part solvers (`SlowPart.cached`) hold pre-asserted assertions + env built
   against the `'static` context. Replacing them with SMT-LIB parse would require
   reconstructing the env from metadata.
3. The `unsafe transmute` at `query.rs:485–487` is only sound because the context is
   `'static`; a scoped context requires a different lifetime strategy throughout.

The cleanest migration path would be to treat the JIT/decomposition layer as engine-side
optimization that operates on the in-memory Z3 IR (not SMT-LIB text), and scope the
SMT-LIB seam to the translate→emit step only — i.e., SMT-LIB as an IR for the
front-end→engine handoff, with the engine optionally re-parsing it into a solver. This
is the approach the `docs/design/smtlib-as-compile-target.md` plan describes.

The `query_cached` path (`query.rs:959–1008`) adds a second variation: it maintains a
`CachedSchema` per schema name in `self.cache`, rebuilding only on structural-signature
changes. This cache is the fast path for the multi-FSM scheduler per-tick cost. In a
split design, this cache would move to the engine side (it is purely solve-time state),
but its invalidation logic (the `autotune` integration at `query.rs:1002–1006`) would
need to be preserved.
