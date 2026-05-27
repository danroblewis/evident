# Split survey — effect_loop / subscriptions

## Summary

9 files, 1923 LOC total. Classes:
- **Engine** (pure runtime, no front-end dependency): `scheduler.rs`, `state.rs`, `timing.rs`, `mod.rs`
- **Metadata-producer** (static analysis on the AST whose output should cross the seam as metadata): `fsm.rs`, `subscriptions.rs`
- **Entangled** (engine body that re-enters the solve path on every tick): `nested.rs`, `collect.rs`, `toposort.rs`

Six headline findings:

**(a) The tick loop re-translates from the in-memory AST every tick via the slow path.**
`scheduler.rs:277` calls `rt.query_with_pins_and_given`, which has three resolution paths
(JIT fast-path, `CachedSchema` slow-path, and `evaluate_with_extra_assertions` full-rebuild).
The `CachedSchema` slow path in `scheduler_api.rs:54–83` reuses a solver across ticks
(push/assert/check/pop), but the solver itself holds a live Z3 `Context` and was built by
`translate::` at load time. The JIT fast path (`try_functionize_z3`) also keys on the in-memory
`SchemaDecl`. Neither path accepts raw SMT-LIB text in place of the live schema object — the
engine is welded to the in-memory AST+translate path. Decoupling requires the engine to hold
a pre-built per-tick "transition solver" it can re-assert prevstate+inputs into, without
re-entering the translate pipeline.

**(b) `subscriptions.rs` `AccessSets` and `fsm.rs` `MainShape` are setup-time metadata that should
cross the seam — they are NOT per-tick engine state.**
Both are computed exactly once at startup (`mod.rs:215–220`) and never recomputed during the tick
loop. In a split architecture these are the primary payload of the metadata sidecar: the engine
needs to know (per FSM) which world fields it reads/writes, and which variables correspond to
state, state_next, effects, last_results, halt, and world. This is precisely what `MainShape`
and `AccessSets` encode today. Neither requires a live Z3 context at solve time — they are
pure structural descriptions of the schema.

**(c) `fsm.rs::resolve_fsm` is a metadata-producer, not a runtime concern.**
It operates entirely on the `SchemaDecl` AST (body item walk) and the `EvidentRuntime` schema
table. Its output, `MainShape`, is consumed by the scheduler but does not change tick-to-tick.
In the split, `resolve_fsm` belongs on the front-end side; its output is serialized into the
metadata sidecar. The only runtime residual is dynamic FSM spawning via `SpawnFsm`
(`scheduler.rs:396–437`), which calls `resolve_fsm` live on the spawned name — a genuine
entanglement requiring the engine to hold a reference to the schema table or a pre-resolved
spawn registry.

**(d) `nested.rs` re-enters the solve path on every nested-FSM step.**
`run_nested_capturing` (`nested.rs:261`) loops calling `rt.query_with_pins_and_given`
(`nested.rs:288`) for each step until `halt` fires. This is fully equivalent to the main
tick loop — same three-path solve, same live Z3 context. Nested FSM execution cannot be moved
across the seam without either: (1) treating nested FSMs as independent SMT-LIB transition
systems handed to the engine, or (2) re-entering the front-end solve path in-process.
`nested.rs` also calls `collect.rs` (toposort) for effect ordering.

**(e) `toposort.rs`/`collect.rs` are engine-side but have a self-hosted solve dependency.**
`collect.rs:181–185` calls `evident_toposort`, which re-enters the Evident solver
(`crate::portable::toposort::toposort`) to order effects. Results are memoized by
`DISPATCH_ORDER_CACHE` after the first call. This is a one-time-per-unique-shape cost
(not per-tick after the first occurrence), but the dependency on a live Evident engine instance
means `collect.rs` cannot run in a pure-engine process without some solve capability.

**(f) `state.rs` builds live `z3::ast::Datatype<'static>` objects from `Value::Enum`, requiring
a live Z3 `Context`.**
`encode_state_value` (`state.rs:37`) constructs Z3 AST nodes directly via `rt.z3_context()` and
`rt.enums_registry()`. In the split this is the pin-encoding step that precedes the per-tick
assert. The engine must either hold the Z3 context itself or receive pre-encoded opaque handles.
Since the `z3::ast::Datatype<'static>` type is parametric on a borrow of the context
(lifetime-erased to `'static` via the existing pattern), the engine must own its own Z3 context
in any split architecture.

---

## Per-file classification

| File | LOC | Class | Why | Seam difficulty | Cross-seam coupling |
|---|---|---|---|---|---|
| `fsm.rs` | 210 | Metadata-producer | Operates on `SchemaDecl` AST; output (`MainShape`) is static per-program setup; consumed by scheduler but not per-tick | **Low** — output is a plain struct; serialize once | `EvidentRuntime::get_schema`, `rt.enums_registry()`, `crate::portable::subscriptions::access_sets`; no Z3 handles, no per-tick calls |
| `subscriptions.rs` | 102 | Metadata-producer | Produces `AccessSets` (world read/write sets) via Evident self-hosted walk; scheduler reads them per tick for routing but they never change | **Low** — `AccessSets` is `HashSet<String>`, trivially serializable | `crate::portable::subscriptions::access_sets` (re-enters Evident engine once per claim at load) |
| `scheduler.rs` | 513 | Engine (entangled) | Is the tick loop; but calls `rt.query_with_pins_and_given` every tick — direct weld to the in-memory AST/translate path | **High** — the per-tick solve call is the seam itself | `rt.query_with_pins_and_given` (line 277), `rt.enums_registry()` (line 41), live `z3::ast::Datatype<'static>` state pins |
| `mod.rs` | 417 | Engine | Startup: resolves FSMs, installs event sources, computes access sets, checks single-owner; then delegates to scheduler | **Med** — startup code is front-end-ish; tick delegation is clean | Calls `full_world_access` → `portable::subscriptions`; otherwise no per-tick front-end coupling |
| `state.rs` | 70 | Engine (entangled) | Encodes `Value::Enum` to live `z3::ast::Datatype<'static>`; per-tick pin preparation | **High** — directly constructs Z3 AST; requires live `z3::Context` | `rt.z3_context()` (line 49), `rt.enums_registry()` (lines 20, 40); Z3 `Dynamic`, `Ast` traits |
| `collect.rs` | 198 | Engine (entangled) | Post-solve effect collection; calls self-hosted toposort (one solve per unique shape); reads `SchemaDecl` body for ordering edges | **Med** — memoization means per-tick cost is amortized; but first call needs a live Evident engine | `rt.get_schema` (line 126), `evident_toposort` → `portable::toposort::toposort` (live engine call, line 184) |
| `toposort.rs` | 40 | Engine (entangled) | Thin wrapper around `portable::toposort`; owns `DISPATCH_ORDER_CACHE` | **Med** — memo cache is engine-internal; the underlying solve is front-end-facing | `crate::portable::toposort::toposort` (one Evident solve per unique node/edge shape) |
| `nested.rs` | 331 | Engine (entangled) | Nested FSM interpreter; loops calling `rt.query_with_pins_and_given` until halt | **High** — identical weld to translate path as the main scheduler; no cached solver path | `rt.query_with_pins_and_given` (line 288), `rt.get_schema` (lines 143, 268), `rt.enums_registry()` (line 214) |
| `timing.rs` | 42 | Engine | Pure diagnostic; formats timing rows to stderr | **Low** — no cross-seam dependency | None |

---

## Seam notes

### Per-tick solve: the primary weld

The critical coupling is in `scheduler.rs:277`:

```
let r = rt.query_with_pins_and_given(&fsm.claim_name, &pins, solve_input)
```

`query_with_pins_and_given` (`scheduler_api.rs:20–100`) takes three resolution paths:

1. **JIT fast-path** (`scheduler_api.rs:38`): `try_functionize_z3` — consults a pre-compiled
   Cranelift function keyed on the `SchemaDecl`'s structural signature. The compiled function
   encodes the full transition system but is backed by the in-memory Z3 AST (it lifts Z3 nodes
   into native code).

2. **Slow-path cache** (`scheduler_api.rs:54–83`): holds a `CachedSchema<'static>` with a
   pre-built Z3 solver. Per tick: `push()` → assert pins + givens → `check()` → `pop()`.
   The solver is reused across ticks (the push/pop pattern). This is the closest existing
   analogue to "hand the engine a transition system once, re-assert each tick" — but the
   solver is a live Z3 `Solver` object, not SMT-LIB text.

3. **Full rebuild** (`scheduler_api.rs:88–98`, `evaluate_with_extra_assertions`): re-runs
   `inline_body_items` over the `SchemaDecl` AST to build a fresh solver each tick. This is
   the slowest path, triggered on the first tick for a claim before its `CachedSchema` is built.

In the SMT-LIB split, all three paths would be replaced by a per-FSM transition-system solver
held in the engine. The engine receives, at startup, pre-built SMT-LIB text for each FSM's
transition relation (plus the enum/datatype sort declarations). Each tick: assert
`state = prev_state`, assert `world.X = prev_X` for relevant fields, call `check-sat`,
extract `state_next` / `world_next.X` / `effects`. No re-entry to the translate pipeline.

The key blocker today: the pin encoding in `state.rs:37` (`encode_state_value`) constructs
`z3::ast::Datatype<'static>` objects that are passed as Z3 AST to `solver.assert`. In
SMT-LIB this would become a `(assert (= state prev_state_smt2_repr))` string injection.
The engine must hold its own Z3 `Context` (or use a fresh one per tick via SMT-LIB `from_string`).

### `fsm.rs` slot-resolution → metadata sidecar

`resolve_fsm` (`fsm.rs:38`) walks a `SchemaDecl`'s body items and produces `MainShape`, which
maps exactly onto the metadata fields the split interface must carry:

| `MainShape` field | Metadata concept | Engine use |
|---|---|---|
| `state_var` / `state_next_var` | `state` / `state_next` variable names | Pin `state` each tick; read `state_next` from model |
| `state_type` | State enum type name | Seed initial state; encode state for Z3 pin |
| `effects_var` | `effects` variable name | Read `Seq(Effect)` from model for dispatch |
| `last_results_var` | `last_results` variable name | Inject `Seq(Result)` from previous tick's dispatch |
| `world_var` / `world_next_var` | world read / world write | Whether FSM reads or writes world |
| `world_type` | World record type name | Know which world fields exist |
| `event_subscriptions` | Async wake subscriptions | Scheduler routing |
| `fti_params` | Typed resource bridges | Plugin install |

All fields are derivable from the `SchemaDecl` AST at parse time, with no Z3 dependency.
In a split architecture, `resolve_fsm` runs on the front-end, and its output is serialized into
the per-FSM metadata record in the sidecar. The engine never sees the raw `SchemaDecl`.

### `subscriptions.rs` `AccessSets` → metadata sidecar

`AccessSets` (`subscriptions.rs:9`) holds `reads: HashSet<String>` and `writes: HashSet<String>`,
each containing world-field names. This is computed once in `mod.rs:215–220` via
`fsm::full_world_access`, which calls `portable::subscriptions::access_sets` (the Evident
self-hosted walk). The scheduler reads `access_sets[j].reads` and `access_sets[j].writes` on
every tick to:
- Route world-write notifications to subscribing FSMs (`scheduler.rs:188, 344`)
- Scope writer snapshots to write-set fields only (`scheduler.rs:329`)
- Check single-owner invariant at startup (`mod.rs:226–240`)

Since `AccessSets` is populated at startup and read-only during the tick loop, it belongs in
the metadata sidecar alongside `MainShape`. In the engine it is engine-internal state — held
in `access_sets: Vec<AccessSets>` alongside `fsm_rt` — but its origin is purely static analysis
of the AST.

The `body_references_identifier` utility (`subscriptions.rs:19`) is a one-off load-time scan
used to check stdin-resource ownership conflicts (`mod.rs:117`). It also belongs in the
front-end pass, not in the engine.

### `nested.rs`: nested FSM execution — full solve re-entry

`run_nested_capturing` (`nested.rs:261–331`) is a miniature tick loop:
```
for step in 0..max_steps {
    given.insert(input.clone(), current.clone());
    let r = rt.query_with_pins_and_given(fsm_name, &pins, &given)  // line 288
    ...
    current = next;
}
```
This is identical in structure to the main scheduler tick. It uses the same
`query_with_pins_and_given` entry point with the same three resolution paths.
`nested.rs` calls `collect_dispatchable_effects` (`nested.rs:312`) which can invoke
the self-hosted toposort solver for effect ordering.

In the split, nested FSMs would each get their own pre-built transition-system solver
in the engine, addressed by name. The engine's `run_nested` would loop: assert prev state,
solve, extract halt + state_next, advance — same as today but against the SMT-LIB solver
rather than the translate pipeline.

### `toposort.rs`/`collect.rs`: setup-once solver, not per-tick

`collect.rs:169–196` calls `evident_toposort` only for the Mode-2 (no `effects` var) path.
In Mode-1 (the common case where `effects ∈ Seq(Effect)` is declared), `collect.rs:24–30`
reads the SeqEnum directly from the model and returns immediately — no toposort needed.
When toposort is needed, `DISPATCH_ORDER_CACHE` memoizes the result: the Evident solver is
invoked once per unique `(sorted nodes, sorted edges)` shape. In a split architecture,
this one-time solve can be run at startup (front-end side, during static analysis) or
on first encounter (engine side, with a resident mini-engine for toposort only). The cache
key is stable (depends only on binding names, not values), so the output belongs in the
precomputed metadata for programs using Mode-2 effects.

### SpawnFsm: the one genuine per-tick schema-table lookup

`scheduler.rs:398–404` calls `resolve_fsm(rt, &claim_name)` live on a spawned FSM name —
the only per-tick invocation of a front-end metadata-producer. This cannot be precomputed
because spawn targets are determined at runtime (spawned by `Effect::SpawnFsm` values from
the model). In the split, the engine must either: (a) hold a reference to the pre-resolved
`MainShape` registry (all possible spawn targets serialized into the sidecar at startup), or
(b) receive spawn shapes via an IPC call back to the front-end. Option (a) is strongly
preferred — programs that spawn FSMs have a closed set of spawn targets resolvable at
compile time.
