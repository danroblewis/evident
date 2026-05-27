# Effect emission + ordering

## Contract (1 paragraph)

Each tick of the multi-FSM scheduler collects all `Effect` values produced by a solved tick, orders them, and dispatches them in sequence via `effect_dispatch::dispatch_one`. Two collection modes exist, selected at load time by whether the FSM body declares a named `effects` slot (the `primary_var` passed into `collect_dispatchable_effects`). Mode 1 (primary slot) is the common case: the runtime decodes the `Seq(Effect)` bound to that name and dispatches its elements left-to-right, matching the literal order the FSM body declared. Mode 2 (no slot) scrapes all `Value::Enum { enum_name == "Effect" }` and `Value::SeqEnum` bindings from the model, synthesizes node/edge names, and runs the self-hosted Evident `ToposortRanks` claim to linearize them. A random tie-break (seeded from `EVIDENT_DISPATCH_SEED` or system time) resolves unconstrained pairs. Cyclic edge sets fall back to input order with a stderr warning. The dispatch is synchronous within a tick: all effects run before the scheduler checks halt or advances to tick N+1. `Effect::Exit(n)` is deferred — it sets `exit_requested` on the `DispatchContext`; all co-scheduled effects in the tick still execute, and the process exits at end-of-tick with `n as i32`.

## Mode 1 (primary `effects` Seq slot)

`collect_dispatchable_effects` checks `primary_var` first (line 23–30, `collect.rs`). When set and `bindings` contains a `Value::SeqEnum` at that key, the function immediately returns the decoded sequence — no toposort, no shuffle, no cache lookup. This is the early return path taken by every `fsm` demo that writes `effects = ⟨...⟩`.

Decoding: each element is a `Value::Enum { enum_name: "Effect", variant, fields }`. `ast_decoder::decode_effect` pattern-matches `variant` to the Rust `Effect` enum variant (see `translate/decode_ast.rs:569–694`). The literal Seq order is the dispatch order — the first element fires first, the last fires last. There is no deduplication; the same effect value can appear multiple times and fires once per occurrence.

## Mode 2 (no slot): node/edge toposort

When `primary_var` is `None` (or the named binding is absent), the function walks all bindings and builds a dispatch graph. Nodes are synthetic string names: bare `Effect`-enum bindings become their binding name; `SeqEnum` members become `"name[i]"` (with auto-edges connecting `name[0]→name[1]→...` within the same Seq). `SeqComposite` fields that are `Seq(Effect)` generate `"outer[i].field[j]"` names with intra-bundle auto-edges.

Declared ordering edges come from `SeqLit` body constraints: any body item of the form `name = ⟨...⟩` contributes to `seq_chains::extract_seq_effect_chains`, which returns pairwise `(a, b)` edges encoding `a` must precede `b`. Bindings whose name appears in a SeqLit RHS are treated as ordering declarations only — they contribute edges but no new dispatch nodes (preventing duplicate dispatches).

Tie-break: before calling `evident_toposort`, `nodes` is shuffled with a `rand::rngs::StdRng` seeded from `EVIDENT_DISPATCH_SEED` env var (u64) or from `SystemTime::now()`. The toposort result is deterministic given a fixed shuffle, so a fixed seed reproduces the same ordering.

Cycle recovery: if `evident_toposort` returns `None` (UNSAT — cyclic edges), `cycle_recovery` warns to stderr and returns the (shuffled) input slice, giving some ordering rather than halting.

Memoization: after the first solve, `DISPATCH_ORDER_CACHE` (a `Mutex<Option<HashMap<DispatchKey, Vec<String>>>>`) stores the linearization keyed on `(sorted nodes, sorted edges)`. Shape-stable programs pay one toposort solve on tick 0; all later ticks hit the cache. Cache lookup happens before the toposort call (lines 162–177, `collect.rs`).

## Effect value shape

At the Value layer, each `Effect` is a `Value::Enum { enum_name: "Effect", variant: String, fields: Vec<Value> }` decoded by `translate/decode_ast.rs::decode_effect`. The `Seq(Effect)` slot in the model is a `Value::SeqEnum(Vec<Value>)`. Variant shapes relevant for fixtures:

| Evident syntax | `variant` | `fields` |
|---|---|---|
| `NoEffect` | `"NoEffect"` | `[]` |
| `Print("s")` | `"Print"` | `[Value::Str("s")]` |
| `Println("s")` | `"Println"` | `[Value::Str("s")]` |
| `Exit(n)` | `"Exit"` | `[Value::Int(n)]` |
| `IntToStr(n)` | `"IntToStr"` | `[Value::Int(n)]` |
| `ParseInt("s")` | `"ParseInt"` | `[Value::Str("s")]` |
| `ReadLine` | `"ReadLine"` | `[]` |
| `Time` | `"Time"` | `[]` |
| `ShellRun("cmd")` | `"ShellRun"` | `[Value::Str("cmd")]` |

A Z3 sentinel string (pattern `!...!`, auto-assigned by Z3 to unconstrained String vars) is silently filtered at the dispatch boundary for `Print`/`Println` — it is not an error; the effect simply produces no output.

## Determinism note

Mode 1 is fully deterministic: the dispatch sequence equals the literal `⟨...⟩` order in the Evident source, which is the Z3 model's `Seq(Effect)` order. No randomness is involved.

Mode 2 is non-deterministic by default: the pre-toposort shuffle uses wall-clock nanoseconds as a seed. Two runs of the same program may dispatch unconstrained effects in different orders. To make mode-2 tests reproducible, set `EVIDENT_DISPATCH_SEED=<u64>` in the environment before running. For behavior-contract fixtures, prefer mode-1 programs (FSMs with an explicit `effects ∈ Seq(Effect)` slot) — mode-2 is intended for legacy or migration use and carries an inherent ordering ambiguity.

## Fixture candidates

1. **`test_02_counter.ev`, state `Count(3)` — `sat_count_emits_int_to_str`.**
   Mode-1 effect emission. The claim pins `state = Count(3)` and asserts `effects = ⟨IntToStr(3)⟩`. This is the minimal single-effect fixture: one `IntToStr` at a concrete integer value. The corresponding `claim sat_count_emits_int_to_str` is already in the file and passes. Fixture value: `Value::SeqEnum([Value::Enum { enum_name: "Effect", variant: "IntToStr", fields: [Value::Int(3)] }])`.

2. **`test_08_exit_code.ev`, state `Init` — `sat_init_exits_42`.**
   Mode-1 two-effect ordered emission. The claim pins `state = Init` and asserts `effects = ⟨Println("exiting with code 42"), Exit(42)⟩`. Covers multi-effect Seq ordering (Println before Exit) and the deferred-Exit contract (Exit fires after all co-scheduled effects). Fixture value: `Value::SeqEnum([Println("exiting with code 42"), Exit(42)])` in that order.

3. **`test_03_seq_chain.ev`, state `Init` — `sat_init_emits_chain_then_exit`.**
   Mode-1 four-effect chain in one tick. The claim asserts `effects = ⟨Println("first"), Println("second"), Println("third"), Exit(0)⟩` with `state = Init`. This is the canonical "batch of effects in literal order" fixture — three Printlns followed by Exit in a single solve. Verifies that multi-element SeqEnum ordering is preserved end-to-end through decode and dispatch.
