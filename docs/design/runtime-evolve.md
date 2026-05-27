# runtime-evolve — a working SMT-LIB-driven mode of the existing runtime

**Strategy 2** of the runtime split: instead of a from-scratch SMT-LIB runtime
(`runtime-smt/`, strategy 1), make the **existing `runtime/`** accept
**SMT-LIB text + metadata** as an FSM definition — bypassing the Evident
lexer/parser/translate — and reuse its battle-shaped execution engine
(scheduler, state threading, effect dispatch, halt, event sources).

This doc is the finalized record: the entry point, what was reused vs. new,
the v1 subset, and the honest limits. The seam analysis that motivated the
design is in [`runtime-evolve-seam.md`](runtime-evolve-seam.md).

## TL;DR

`evident effect-run-smtlib <fixture.json>` runs an FSM whose **per-tick
constraint is raw SMT-LIB** through the **same** `effect_loop::run` as
`evident effect-run`. The Evident-source path is completely untouched and
`./test.sh` stays green; the SMT-LIB path is additive and opt-in (an empty
registry by default — the only hot-path change is one early `if` in
`query_with_pins_and_given`).

Worked, oracle-matched demos (each byte-identical to its `evident effect-run`
equivalent):

| Fixture | Exercises | Halt |
|---|---|---|
| `countdown` | scalar state threading, templated effects | `Effect::Exit(0)` |
| `decr_halt` | the other halt path | no-FSM-scheduled |
| `clock_watcher` | **2 FSMs** coordinated via the existing world plumbing | reader `Exit` |
| `pure_counter` | a **real** transpiled `examples/test_20_pure_counter.ev` — `last_results` read-back + SMT-LIB `str.++` | `Exit(0)` |

Fixtures + the oracle `.ev` files live in `runtime/tests/fixtures/smtlib/`;
the comparison harness is `runtime/tests/smtlib_fsm.rs`.

## How it works — the seam

The multi-FSM scheduler's entire per-tick interaction with the constraint
engine is **one call**: `query_with_pins_and_given(claim, pins, given) ->
QueryResult` (`runtime/src/runtime/scheduler_api.rs`). Everything above it —
the tick loop, wake/subscription logic, state threading, effect
collection/dispatch, halt detection, event sources — is **source-agnostic**:
it drives off the FSM's `MainShape` field names and the returned
`bindings`, never the constraint's origin.

So the design feeds that seam from SMT-LIB and reuses the rest:

1. **Metadata → a synthetic `fsm` schema.** A fixture's metadata (var/sort
   table, FSM-shape slots, effect template) builds a `Keyword::Fsm`
   `SchemaDecl` whose body is **Memberships only** (no constraints). The
   scheduler's `resolve_fsm` / `MainShape` / `all_fsms` / `_var` time-shift
   scan all work off that shape with **zero new code** — FSM identity is the
   `fsm` keyword, exactly as the project invariant requires (no shape
   detection). World read/write sets are inferred by the existing
   `portable::subscriptions::access_sets` walk; the synthetic schema carries
   dotted-Identifier marker constraints (`world.X`, `world_next.X`) that the
   walk classifies.

2. **The behavior is the SMT-LIB.** `query_with_pins_and_given` is intercepted
   for registered SMT-LIB FSMs and routed to `smtlib_fsm::solve_tick`, which:
   parses the constraint text into the runtime's **leaked Z3 context**
   (`Solver::from_string`, with the `raw_ctx` error-code check since the crate
   swallows parse errors); asserts the tick's scalar `given`/pins + any
   `last_results` input bindings as `_eq` constraints; `check()`s; and
   **assembles** the `bindings` the scheduler consumes — scalar outputs read
   back by name+sort, and an `effects` `Value::SeqEnum` built from the metadata
   effect template (guards keyed on model Bools, args from literals or model
   scalars). That `Value::Enum{Effect}` shape is exactly what the existing
   `collect_dispatchable_effects` + `decode_effect` + `dispatch_all` already
   consume.

The state of the FSM threads through the engine's existing machinery: previous
`count` is re-injected as the `_count` given, `is_first_tick` is engine-provided,
world writes flow `world_next.X` → snapshot → reader `world.X`, and both halt
paths (`Effect::Exit` and no-FSM-scheduled) are the engine's.

## Reused vs. new — the ledger

| Concern | Reused as-is | New for SMT-LIB |
|---|---|---|
| Tick loop, wake/subscription logic | ✅ `effect_loop/scheduler.rs` | — |
| `MainShape` / `resolve_fsm` / `all_fsms` | ✅ `effect_loop/fsm.rs` | synthetic `SchemaDecl` it walks |
| World access-set inference | ✅ `portable::subscriptions::access_sets` | dotted-Identifier markers it classifies |
| State threading (`_name`, world) | ✅ `effect_loop/scheduler.rs` | — |
| Effect collect (Mode 1) + dispatch | ✅ `collect.rs`, `effect_dispatch.rs` | effect `Value`s assembled from template |
| Builtin `Effect`/`Result` enums | ✅ via `stdlib/runtime.ev` | — |
| Leaked Z3 context | ✅ `rt.z3_ctx` | parse SMT-LIB into it |
| Per-tick solve | — | `smtlib_fsm::solve_tick` |
| Model → `Value` | partial (scalar reconstruct by name+sort) | effect/state assembly + `last_results` binding |
| Metadata + fixture loader + CLI | — | `smtlib_fsm/` + `commands/effect_run_smtlib.rs` |

New code, in full:
- `runtime/src/smtlib_fsm/{mod,meta,tests}.rs` — types, JSON metadata parser,
  per-tick solve, 8 unit tests.
- `runtime/src/runtime/smtlib_reg.rs` — `register_smtlib_{fsm,world,program}`
  (inject synthetic schema + registry entry).
- `EvidentRuntime.smtlib_fsms` registry field + the one-line intercept.
- `commands/effect_run_smtlib.rs` + `effect-run-smtlib` subcommand.
- `runtime/tests/fixtures/smtlib/` + `runtime/tests/smtlib_fsm.rs`.

## The metadata format (v1)

One JSON program: an optional shared `world` record + an `fsms` array. Each FSM
carries `meta` + `smtlib` (inline) or `smtlib_file` (loader-resolved).

```jsonc
{
  "world": { "type": "World", "fields": [{ "name": "tick", "sort": "Int" }] },
  "fsms": [{
    "meta": {
      "fsm": "clock",
      "vars": [{ "name": "n", "sort": "Int" }, { "name": "_n", "sort": "Int" }, …],
      "outputs": ["n", "world_next.tick"],   // scalars exposed in bindings
      "effects_var": "effects",
      "last_results_var": "last_results",     // optional
      "inputs": [                              // optional: last_results -> const
        { "var": "fmt_str", "sort": "Str", "index": 0,
          "variant": "StringResult", "default": { "lit_str": "?" } }
      ],
      "effects": [                             // ordered; each optionally guarded
        { "guard": "g_le3", "variant": "IntToStr", "args": [{ "var": "count" }] },
        { "guard": "g_gt3", "variant": "Exit",     "args": [{ "lit_int": 0 }] }
      ],
      "world_next_var": "world_next", "world_type": "World"  // writer; world_var for readers
    },
    "smtlib": "(declare-const n Int) … (assert (= n (ite is_first_tick 1 (+ _n 1)))) …"
  }]
}
```

Sorts: `Int`/`Nat`/`Pos` → Int, `Bool`, `Real`, `Str`/`String`. Effect args:
`{lit_str|lit_int|lit_bool}` or `{var}` (a scalar model value). Var names may be
dotted (`world.tick`) — `.` is a valid SMT-LIB simple-symbol char. (No
`runtime-contract/FORMAT.md` had landed on this branch to reconcile against; if
it lands, align field names here.)

## Limits (honest notes)

The clean engine reuse holds for **scalar-state FSMs** (`Int`/`Bool`/`Real`/
`String` threaded via the `_name` time-shift) with metadata-templated effects.
That covers pure counters, world-coordinated multi-FSM programs (world fields
are scalar), and the format-an-int `last_results` pattern — i.e. the demos
above and a real corpus example.

**The entanglement boundary is enum-typed `state` driven by SMT-LIB
`(declare-datatypes …)`.** The engine encodes enum state as a Z3 `Datatype` pin
bound to the runtime's *registered* `DatatypeSort`
(`effect_loop/state.rs::encode_state_value`). For an SMT-LIB-authored enum to
interoperate, its datatype declaration must resolve to *that* sort, not a
parser-created duplicate of the same name — which needs raw-z3-sys sort-handle
reconciliation. v1 represents "state" as a scalar plus the `_name` time-shift
(exactly how `examples/test_20_pure_counter.ev` works), which is fully
expressible in SMT-LIB. Enum-state-via-SMT-LIB-datatypes is the documented next
increment, not attempted here.

Other v1 limits: effects are assembled from a metadata template (the SMT-LIB
solves scalar/Bool guards + string/int payloads; it does not emit the `Effect`
datatype itself); `last_results` is read via explicit input bindings (one
payload field per binding); records/Seq values are out of subset. The per-tick
solve re-parses the (small) constraint text each tick — correct, not yet
perf-tuned (a parse-once cache is the obvious follow-up, mirroring the
Evident path's `slow_path_cache`).

## Validation

- `./test.sh` green throughout (Evident path untouched; the SMT-LIB path is
  additive). The `cargo test --release` phase includes
  `runtime/tests/smtlib_fsm.rs` (7 integration tests) and the
  `smtlib_fsm` module unit tests (8).
- Each fixture is asserted byte-identical to its Evident oracle via the binary
  — the Evident-source path is the ground truth, and the SMT-LIB path matches
  it while sharing the engine. This *is* the strategy-2 contract: same engine,
  two front doors, same observable behavior.
- `runtime-contract/` (the shared oracle from the sibling split sessions) had
  not landed on this branch; when it does, point `smtlib_fsm.rs` at its
  fixtures — the comparison harness (`assert_paths_match`) is already the right
  shape for it.
