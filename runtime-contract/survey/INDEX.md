# Behavior survey — INDEX

This index collates the seven area surveys (`01-tick` … `07-last-results`) into
**the catalog of behaviors to capture as portable fixtures**. It is the Phase-1
gate of `docs/plans/behavior-contract.md`.

## The atomic primitive

Every behavior below reduces to ONE runtime call — the tick:

```
EvidentRuntime::query_with_pins_and_given(claim_name, pins, given)
    -> QueryResult { satisfied, bindings: HashMap<String, Value> }
```
(`runtime/src/runtime/scheduler_api.rs:20`). The multi-FSM scheduler
(`runtime/src/effect_loop/scheduler.rs:277`) calls exactly this once per FSM per
tick, then collects effects from the model via
`collect_dispatchable_effects` (`runtime/src/effect_loop/collect.rs:17`).

A **fixture** therefore captures one tick:
`(prev-state pin + inputs as `given`) → (next-state model bindings + dispatched effects)`.
The transition relation is serialized as SMT-LIB; the golden model as SMT-LIB;
the golden effects as text. See `../FORMAT.md` (Phase 2) for the file layout.

## The seven behaviors (and how each is captured)

| # | Behavior | Capturable as single-tick fixture? | Source survey |
|---|----------|-------------------------------------|---------------|
| 1 | **Tick** — prev-state+inputs → next-state+effects | ✅ yes — it *is* the fixture unit | `01-tick.md` |
| 2 | **State threading** — enum `state`/`state_next` pair; `_var` time-shift; `is_first_tick` | ✅ yes — pin `state` / `_x` / `is_first_tick` as inputs | `02-state-threading.md` |
| 3 | **Effect emission + ordering** — decode `effects` Seq, dispatch in order (mode 1); toposort (mode 2) | ✅ mode-1 (literal Seq order); ⚠️ mode-2 toposort tie-break needs a fixed `EVIDENT_DISPATCH_SEED` → contract TODO | `03-effects.md` |
| 4 | **Halt** — `Effect::Exit(code)` (graceful, end-of-tick); implicit halt (no FSM scheduled); UNSAT | ✅ Exit is encoded *in the effect list* → capturable; implicit-halt is a steady `Done→Done` tick emitting `⟨⟩`; cross-tick "no wake" is a scheduler property → contract TODO | `04-halt.md` |
| 5 | **Multi-FSM coordination** — shared `World`, writer merges `world_next.X`, readers wake on delta, single-owner | ⚠️ partial — one writer's tick (→ `world_next.X`) and one reader's tick (`world.X` given → output) ARE single-tick fixtures; cross-FSM *wake propagation* and same-tick visibility are multi-step → contract TODO | `05-multi-fsm.md` |
| 6 | **`given`-pinned inputs** — pin `var = value`, solve the rest | ✅ this is the input convention every fixture uses | `06-given.md` |
| 7 | **last_results / effect feedback** — tick K effects → `EffectResult`s → pinned as tick K+1's `last_results` | ✅ pin `last_results` as an input directly (no live dispatch needed) | `07-last-results.md` |

## The `given` input-value convention (from `06-given.md`)

A fixture pins inputs by their `Value` type:
- `Int/Bool/Real/Str` → scalar equality
- `Enum{..}` → encoded to a Z3 Datatype, asserted equal (state, last_results elems)
- `Seq*` → length pin + per-index element equality
- record/`Composite` fields → flattened to `name.field` scalar pins (e.g. `_pos.x`)

**Determinism rule (critical for fixtures):** unpinned vars get Z3's arbitrary
choice. A fixture must either pin every input that drives a checked output, or
assert only on outputs uniquely forced by the pins. All fixtures below pin the
full input frame, so each has exactly one golden model.

## Effect → Result map (from `07-last-results.md`, for feedback fixtures)

`IntToStr→StringResult` · `ParseInt→IntResult|ErrorResult` · `Time/MonotonicTime→IntResult`
· `Println/Print/Exit/NoEffect→NoResult` · `ShellRun→StringResult|ErrorResult`.

## Fixtures to capture (Phase 3 work-list)

Clustered for parallel capture. Each row: name → (FSM source, pinned inputs) →
(golden next-state, golden effects). "From" cites the example + the existing
`sat_/unsat_` claim that already asserts the golden on the current runtime
(provenance — the golden is *current behavior*, not invented).

### Cluster A — tick basics + Exit/halt encoding
1. **`tick_hello_init`** — `test_01_hello` / `sat_init_advances_to_done` + `sat_init_emits_greeting_and_exit`.
   `state=Init` → `state_next=Done`, effects `⟨Println("hello from evident"), Exit(0)⟩`. `halt=true, exit_code=0`.
2. **`tick_counter_start`** — `test_02_counter` / `sat_start_seeds_count_five`.
   `state=Start` → `state_next=Count(5)`, effects `⟨Println("starting count")⟩`. (Payload enum out; nullary-first-variant seed rule.)
3. **`tick_exit_42`** — `test_08_exit_code` / `sat_init_exits_42`.
   `state=Init` → `state_next=Done`, effects `⟨Println("exiting with code 42"), Exit(42)⟩`. `halt=true, exit_code=42`.

### Cluster B — effect emission + ordering (mode-1)
4. **`effects_int_to_str`** — `test_02_counter` / `sat_count_emits_int_to_str`.
   `state=Count(3)` → effects `⟨IntToStr(3)⟩`.
5. **`effects_chain_four`** — `test_03_seq_chain` / `sat_init_emits_chain_then_exit`.
   `state=Init` → effects `⟨Println("first"), Println("second"), Println("third"), Exit(0)⟩`. (Ordered batch in one tick.)
6. **`effects_empty_absorbing`** — `test_01_hello` (Done arm) / `unsat_done_returns_to_init` (provenance).
   `state=Done` → `state_next=Done`, effects `⟨⟩`. (Basis of implicit halt.)

### Cluster C — state threading (`_var` time-shift)
7. **`prev_first_tick_zero`** — `test_19_prev_tick` / `sat_first_tick_count_is_zero`.
   `is_first_tick=true, state=Counting` → `count=0`.
8. **`prev_increment`** — `test_19_prev_tick` / `sat_subsequent_tick_increments`.
   `is_first_tick=false, _count=7, state=Counting` → `count=8`.
9. **`prev_record_fields`** — `test_22_prev_record` / `sat_independent_field_pins`.
   `is_first_tick=false, _pos.x=7, _pos.y=11` → `pos.x=8, pos.y=13`. (Per-field record prev-tick pin.)

### Cluster D — last_results / feedback
10. **`feedback_format_tick`** — `test_02_counter` (Format arm).
    `state=Format(5), last_results=⟨StringResult("5")⟩` → `state_next=Count(4)`, effects `⟨Println("tick 5")⟩`.
    (Golden derived by running the current engine — no existing sat_ claim; Phase-3 agent adds a probe claim to confirm.)
11. **`feedback_parse_read`** — `test_04_parse_int` (Read arm).
    `state=Read, last_results=⟨IntResult(42), ErrorResult("…")⟩` → effects
    `⟨Println("good: parsed an Int"), Println("bad: ERROR was correct"), Exit(0)⟩`, `state_next=Done`. (Multi-position `last_results[0]`/`[1]`.)

### Cluster E — multi-FSM via world (single-tick slices)
12. **`world_writer_producer`** — `test_09_two_fsms` / `sat_producer_writes_n`.
    `state=PTick(3)` → `world_next.n=3`.
13. **`world_reader_consumer`** — `test_09_two_fsms` / `sat_consumer_emits_int_to_str_when_n_positive`.
    `world.n=7, state=CWait` → effects `⟨IntToStr(7)⟩`.

### Cluster F — negative / UNSAT (given over-constrains)
14. **`unsat_bad_transition`** — `test_02_counter` / `unsat_count_increments`.
    `state=Format(3)` with `state_next` pinned to `Count(4)` → **UNSAT** (the only legal next is `Count(2)`).

## Counts
14 fixtures across 6 clusters (gate requires ≥6). Cluster A/B/C/D/F are fully
single-tick-faithful; Cluster E captures the writer & reader tick slices and
defers cross-FSM wake propagation as a documented **contract TODO** (see each
fixture's `meta.json` and `../README.md`).

## Contract TODOs surfaced by the survey (not faked — designed fresh later)
- **mode-2 effect toposort tie-break** — nondeterministic without a pinned seed.
- **cross-FSM wake propagation / same-tick writer→reader visibility** — a
  multi-step scheduler property, not a single tick.
- **async event sources** (FrameTimer, Stdin, Sigint) — wall-clock / external;
  outside a deterministic transition fixture.
- **`run(F, init)` percolated child effects** — nested-FSM unroll, separate concern.
