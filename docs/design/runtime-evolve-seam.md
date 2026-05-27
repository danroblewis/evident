# runtime-evolve — the seam: where SMT-LIB+metadata enters the existing engine

**Strategy 2.** Make the EXISTING `runtime/` accept **SMT-LIB text + metadata** as an
FSM definition, bypassing the Evident lexer/parser/translate, and reuse its
battle-shaped EXECUTION ENGINE (scheduler, state threading, effect dispatch,
halt). This doc is the Phase-1 output: it names the single entry point, the
minimal new code, and what is reused vs. new.

It is collated from three parallel surveys of:
* the scheduler / tick entry (`effect_loop/`),
* the model-extraction path (`translate/extract.rs`, `translate/eval/`),
* the Z3 context/solver setup (`runtime/`, `chc.rs`, `translate/smtlib.rs`).

> Note: `docs/design/runtime-split.md`, `runtime-contract/`, and `runtime-smt/`
> (outputs of sibling sessions) had not landed on this branch when this was
> written. The plan anticipates this. The metadata format below is defined here
> and should be reconciled with `runtime-contract/FORMAT.md` if/when it lands.

---

## 1. The seam is ONE function

The multi-FSM scheduler's entire per-FSM, per-tick interaction with the
constraint engine is a **single call**:

```rust
// runtime/src/effect_loop/scheduler.rs:277
let r = rt.query_with_pins_and_given(&fsm.claim_name, &pins, solve_input)?;
//      └────────────────────────── runtime/src/runtime/scheduler_api.rs:20
```

`query_with_pins_and_given(claim_name, pins, given) -> Result<QueryResult, _>`:
* `pins: &[(&str, z3::ast::Datatype<'static>)]` — enum-Datatype state pins (only
  used for enum-typed `state`; **empty for scalar-state FSMs**).
* `given: &HashMap<String, Value>` — every scalar/enum input for the tick:
  `world.*` snapshot, `last_results` (a `Value::SeqEnum`), `_name` previous-tick
  values, `is_first_tick`, and the encoded current `state`.
* returns `QueryResult { satisfied: bool, bindings: HashMap<String, Value> }`.

**Everything above this call is source-agnostic** — it neither knows nor cares
whether the per-tick constraint came from a translated Evident AST or from raw
SMT-LIB. Concretely, these run unchanged regardless of input language:

| Engine concern | Where | Depends on the constraint *source*? |
|---|---|---|
| Tick loop, wake/subscription logic | `effect_loop/scheduler.rs:128–499` | No — drives off `MainShape` field names + `QueryResult.bindings` |
| State threading (`_name`, `state`, world) | `scheduler.rs:226–367` | No — re-injects prior `bindings` as `given` next tick |
| Effect collection | `effect_loop/collect.rs:17` (`collect_dispatchable_effects`) | No (Mode 1) — decodes `Value::SeqEnum` from `bindings` |
| Effect dispatch | `effect_dispatch.rs:599` (`dispatch_all`) | No — pure `Value`→IO |
| Halt (no-FSM-scheduled / `Effect::Exit`) | `scheduler.rs:443`, `459–498` | No |
| Event sources (FrameTimer, Stdin, Sigint) | `event_sources/`, `mod.rs` | No |

So the strategy-2 win is real: **feed the seam from SMT-LIB and the whole engine
above it is reused for free.**

## 2. The FSM shape comes for free from a synthetic `SchemaDecl`

The scheduler describes each FSM with `MainShape` (`effect_loop/fsm.rs:9`),
produced by `resolve_fsm` (`fsm.rs:38`). `resolve_fsm` gates on the **`fsm`
keyword** (never body shape — per project invariant) and then *walks only the
`Membership` body items* to resolve which slots the FSM uses (`state`/`state_next`
pair, `effects`, `last_results`, `world`/`world_next`, FTI params, event subs).

**Implication:** if we register a synthetic `SchemaDecl` whose `keyword` is
`Keyword::Fsm` and whose body is **only `Membership` items** (no `Constraint`s),
then `resolve_fsm`, `all_fsms` (`fsm.rs:154`), the `_name` time-shift scan
(`scheduler.rs:243`), and world-access inference all work with **zero new code**.
The behavior (the constraints) lives in the SMT-LIB text, not the AST. The
synthetic schema is a *shape declaration*, the SMT-LIB is the *behavior*.

This is the keystone of the design: we do not bypass `MainShape`; we feed it from
a metadata-built `SchemaDecl` and intercept only the solve.

## 3. Model extraction needs metadata, not the AST constraint body

(survey 2.) `translate/eval/mod.rs::evaluate` extracts by iterating an
`env: HashMap<String, Var<'static>>` (`mod.rs:125–186`). `env` is built by
`declare_var` (`translate/declare.rs`) from each `Membership`'s `type_name`
string — **not** from the constraint `Expr`s. Decoding needs:

| Metadata | Source today | From external metadata? |
|---|---|---|
| var name → sort string (`Int`/`Bool`/`Real`/`String`/`Seq(..)`/enum/record) | `Membership.type_name` | **Yes** — flat `name→sort` table |
| `EnumRegistry` (enum name → DatatypeSort + variants/fields) | enum decls via `register_enums` | **Yes** — for builtin `Effect`/`Result` it is *already registered* by loading `stdlib/runtime.ev`; user enums need a metadata enum-decl |
| `DatatypeRegistry` (`Seq(UserType)` field layout) | `SchemaDecl` bodies | **Yes** — record field list, or avoid records in v1 |

The constraint `Expr`s are **not** needed for extraction. Good.

Two extraction strategies (we use **A** for v1):

* **A (decode-by-metadata, no engine env):** reconstruct each model const by
  name+sort (`Int::new_const(ctx, name)`, etc.) and `model.eval` — exactly the
  `read_const` pattern in `translate/smtlib.rs:407`. Z3 interns symbols by
  name+sort, so a const reconstructed after `solver.from_string` resolves to the
  parser-created symbol. Scalars are trivial; enum/Seq-of-enum **outputs** are
  *assembled from metadata templates* (see §5) rather than decoded from Z3
  datatypes — this sidesteps the datatype-handle-reconstruction problem.
* **B (reuse engine env):** build `env` via `declare_var` from the synthetic
  schema, then reuse `run_cached`-style extraction. Cleaner for enums but
  requires the SMT-LIB symbols to *coincide* with engine-declared datatype
  sorts. Deferred — this is the entanglement boundary (§7).

## 4. Z3 setup: parse SMT-LIB into the leaked context

(survey 3.) `EvidentRuntime` holds a leaked `z3_ctx: &'static Context`
(`runtime/mod.rs:43`, created at `mod.rs:95`), exposed via `rt.z3_context()`.
The existing tools for loading external SMT-LIB text:

* **Parse:** `let s = z3::Solver::new(rt.z3_ctx); s.from_string(text);` (wraps
  `Z3_solver_from_string`). The only existing caller is the `translate/smtlib.rs`
  prototype — but it uses a *throwaway* context. We use the **leaked** one so the
  reconstructed consts + EnumRegistry sorts share a context.
* **Detect parse errors (the crate swallows them):** the `raw_ctx` layout-assert
  trick + `Z3_get_error_code`/`Z3_get_error_msg` (`translate/smtlib.rs:345–379`,
  also in `chc.rs:46`, `string_ops.rs:16`). Copy verbatim.
* **Solve + model:** `s.check()` → `SatResult`; `s.get_model()`; `model.eval(&c, true)`.
* **Raw z3-sys gotcha** (`chc.rs:76`): any raw `Z3_mk_*` decl must be
  `Z3_inc_ref`'d or Z3 GCs it. We avoid raw decls in v1 (the crate's
  `from_string` + `*::new_const` handle ref-counting), but note it for the
  datatype-interop boundary.

## 5. Minimal new code

```
runtime/src/smtlib_fsm/            NEW module
  mod.rs        SmtLibFsm, FsmMeta, registry type, the per-tick solve
  meta.rs       metadata format + parser (var sorts, fsm shape, effect template)
  load.rs       fixture → synthetic SchemaDecl(s) + enum regs + registry entries
runtime/src/runtime/mod.rs         + field: smtlib_fsms: RefCell<HashMap<String, SmtLibFsm>>
runtime/src/runtime/scheduler_api.rs  + intercept at top of query_with_pins_and_given
runtime/src/commands/effect_run_smtlib.rs  NEW CLI: evident effect-run-smtlib <fixture>
```

The intercept (the only change to a hot path), at the very top of
`query_with_pins_and_given`:

```rust
if let Some(fsm) = self.smtlib_fsms.borrow().get(claim_name) {
    return Ok(crate::smtlib_fsm::solve_tick(self, fsm, pins, given));
}
```

`solve_tick`:
1. Assemble full SMT-LIB = `fsm.smtlib` (declare-consts + the constraint asserts)
   + appended `(assert (= <var> <lit>))` for each *scalar/bool* entry of `given`
   that the metadata declares as an input const (`_name`, `is_first_tick`,
   `world.*`, last-result-derived scalars).
2. `Solver::new(ctx); from_string; check error code; check()`.
3. UNSAT → `QueryResult { satisfied:false, .. }` (scheduler logs + halts).
4. SAT → extract each metadata var (scalar) into `bindings`; **assemble**
   `state_next`/`effects` Values from the metadata effect-template keyed on model
   booleans/ints (§5 below). Return `QueryResult`.

**Metadata format (v1, JSON):**
```jsonc
{
  "fsm": "counter",
  "vars":   { "count": "Int", "_count": "Int", "is_first_tick": "Bool",
              "guard_more": "Bool", "guard_done": "Bool" },
  "outputs": ["count"],          // scalars to expose in bindings
  "effects_var": "effects",      // the Seq(Effect) slot name (Mode-1 collect)
  "last_results_var": "last_results",
  "effects": [                   // ordered; each entry optionally guarded by a model Bool
    { "guard": "guard_more", "effect": { "Println": { "arg": "starting" } } },
    { "guard": "guard_done", "effect": { "Exit":    { "code": 0 } } }
  ]
}
```
Effect arg sources: a string/int **literal**, or `{ "var": "name" }` to pull the
model value of a scalar const. This produces exactly the `Value::Enum {
enum_name:"Effect", variant, fields }` shape that `decode_effect`
(`translate/decode_ast.rs:569`) + `dispatch_all` already consume.

**Fixture on disk:** a directory or a single file pairing `*.smt2` + `*.json`.
The CLI loads `stdlib/runtime.ev` first (registers builtin `Effect`/`Result`
enums into `EnumRegistry`), then the fixture, then calls the **same**
`effect_loop::run(&rt, opts)` as `effect-run`.

## 6. Reused vs. new — the ledger

| Concern | Reused as-is | New |
|---|---|---|
| Tick loop / wake / subscriptions | ✅ `effect_loop/scheduler.rs` | — |
| `MainShape` / `resolve_fsm` / `all_fsms` | ✅ `effect_loop/fsm.rs` | synthetic `SchemaDecl` it walks |
| State threading (`_name`, world) | ✅ `scheduler.rs` | — |
| Effect collect (Mode 1) + dispatch | ✅ `collect.rs`, `effect_dispatch.rs` | effect Values *assembled* from template |
| Builtin `Effect`/`Result` enums | ✅ via `stdlib/runtime.ev` | — |
| Z3 leaked context | ✅ `rt.z3_ctx` | parse SMT-LIB into it |
| Per-tick solve | — | `solve_tick` (SMT-LIB parse+check+extract) |
| Model→Value | partial (scalar reconstruct = `smtlib.rs:407` pattern) | effect/state assembly from metadata |
| Metadata + fixture loader + CLI | — | `smtlib_fsm/`, `effect_run_smtlib.rs` |

## 7. Entanglement boundary (honest note)

The clean reuse holds for **scalar-state FSMs with metadata-templated effects** —
which covers pure counters, world-coordinated multi-FSM programs (world fields
are scalar), and the countdown/format demos. The boundary is **enum-typed
`state` driven *by SMT-LIB datatypes***: the engine encodes enum state as a Z3
`Datatype` pin (`effect_loop/state.rs::encode_state_value`) bound to the
runtime's *registered* `DatatypeSort`. For an SMT-LIB-authored enum to interoperate,
its `(declare-datatypes …)` must resolve to *that* sort, not a parser-created
duplicate of the same name — which needs raw-z3-sys sort-handle reconciliation
(strategy B, §3). v1 represents "state" as a scalar (`Int`/`Bool`/`String`) plus
the `_name` time-shift, which is exactly how `examples/test_20_pure_counter.ev`
works and is fully expressible in SMT-LIB. Enum-state-via-SMT-LIB-datatypes is
documented here as the next increment, not attempted in v1.

## 8. Phase map (what this seam enables)

* **Phase 2** — loader + intercept + `solve_tick`; single tick of a scalar FSM
  fixture matches the Evident path's `bindings`.
* **Phase 3** — multi-tick via the existing scheduler (countdown with effects +
  halt) matches `evident effect-run` on the Evident equivalent.
* **Phase 4** — ≥2 SMT-LIB FSMs coordinated through the existing world plumbing;
  one real transpiled `examples/test_*.ev`.
* **Phase 5** — contract fixtures (if present) + `./test.sh` green; this doc +
  `runtime-evolve.md` finalize reused-vs-new.
