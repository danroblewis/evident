# runtime-contract — a portable behavior oracle for the Evident FSM engine

This directory captures the current Evident multi-FSM runtime's hard-won
execution semantics as **implementation-agnostic fixtures**, behind a pluggable
engine trait. It preserves the *behavior* while the *code* is free to be
rewritten — it is the oracle that `new-runtime`, or any split/replacement
engine, must pass.

The unit of capture is **one tick**:

```
(transition relation as SMT-LIB) + metadata + prev-state + inputs
        →  golden next-state model (SMT-LIB) + golden effects (text)
```

Two engines run every fixture (`runtime/tests/behavior_contract.rs`):
- **`CurrentRuntimeEngine`** runs each fixture through the *real* `EvidentRuntime`
  tick primitive. Passing proves the captured golden is the current runtime's
  actual behavior.
- **`SmtLibEngine`** solves the portable `*.smt2` with Z3 alone (no Evident
  pipeline). Passing proves the SMT-LIB capture is faithful and engine-neutral.

Both pass **15/15** today.

## Layout

```
runtime-contract/
  README.md            ← you are here
  FORMAT.md            ← the fixture file format (the spec)
  survey/              ← Phase-1 behavior survey + INDEX (the catalog)
    INDEX.md           ← the 7 behaviors → fixtures map + contract TODOs
    01-tick.md … 07-last-results.md
  fixtures/<name>/     ← 15 captured fixtures (see below)
    meta.json            roles + typed pins (given) + typed golden (expect)
    problem.smt2         the transition RELATION (sorts + constraints)
    prev.smt2            pins the FSM's previous state
    inputs.smt2          pins external inputs (world.X, last_results, …)
    expected_model.smt2  golden next-state assignment (SMT-LIB witness)
    expected_effects.txt golden dispatched effects, one per line
    source.ev            the Evident FSM (provenance; replayed by the runtime engine)

runtime/tests/behavior_contract.rs   ← the harness (trait + 2 engines + runner)
```

The harness lives under `runtime/tests/` so it runs as part of `./test.sh`
(Phase 2, `cargo test`). It reads fixtures from `../runtime-contract/fixtures/`.

## The 15 fixtures (6 clusters across 7 behaviors)

| Cluster | Fixtures | Behavior |
|---|---|---|
| A | `tick_hello_init`, `tick_counter_start`, `tick_exit_42` | tick basics + `Exit` / halt encoding |
| B | `effects_int_to_str`, `effects_chain_four`, `effects_empty_absorbing` | effect emission + ordering (mode-1) |
| C | `prev_first_tick_zero`, `prev_increment`, `prev_record_fields` | state threading (`_var` time-shift) |
| D | `feedback_format_tick`, `feedback_parse_read` | `last_results` / effect feedback |
| E | `world_writer_producer`, `world_reader_consumer` | multi-FSM via shared world (single-tick slices) |
| F | `unsat_bad_transition`, `unsat_hello_done_to_init` | negative / impossible transitions |

Each fixture's golden was **derived from the current engine** (its `source.ev`
probe `sat_`/`unsat_` claim passes `evident test`) and **cross-checked against
Z3** (Method A admissibility + Method B uniqueness, or UNSAT for negatives).
The full format — `meta.json` schema, the tagged-JSON `Value` encoding, the
SMT-LIB conventions (enum→`declare-datatypes`, `match`→nested `ite`,
`Seq(Effect)`→`seq.++`) — is in [`FORMAT.md`](FORMAT.md).

## Running the suite

```sh
# just the contract suite
cd runtime && cargo test --release --test behavior_contract -- --nocapture

# or the whole repo (the contract suite runs inside Phase 2)
./test.sh
```

Three tests: `fixtures_discovered` (≥6 found), `current_runtime_engine_matches_all_goldens`
(the gate), `smtlib_capture_is_faithful` (the portability proof).

## How to add a fixture

1. **Pick a single-tick behavior** worth pinning (see `survey/INDEX.md` for the
   catalog and what's already covered).
2. **Write `fixtures/<name>/source.ev`** — a standalone runnable file:
   `import "stdlib/runtime.ev"` + the enum(s)/type(s) + the `fsm` declaration
   (copy the closest `examples/test_*.ev`) + one probe `sat_`/`unsat_` claim that
   pins the inputs and asserts the golden.
3. **Derive + confirm the golden via the current engine**:
   `evident test runtime-contract/fixtures/<name>/source.ev` — the probe claim
   must pass. (This is what makes the golden *current behavior*, not invented.)
4. **Write `meta.json`** per [`FORMAT.md` §2–3](FORMAT.md): `fsm_claim`, the role
   vars, the typed `given` pins, and the typed `expect` block
   (`model` + `effects`, or `unsat` + `forbidden`). **Pin every input that drives
   a checked output** — the determinism rule; otherwise Z3 picks freely and the
   golden isn't unique.
5. **Write the SMT-LIB** (`problem/prev/inputs/expected_model.smt2`) per
   [`FORMAT.md` §4](FORMAT.md) and `expected_effects.txt` per §6. Record
   `how_built` (`transpiled` for a scalar nucleus, else `handwritten`).
6. **Validate with Z3** (Method A/B/UNSAT, [`FORMAT.md` §5](FORMAT.md)):
   ```sh
   cat problem.smt2 prev.smt2 inputs.smt2 expected_model.smt2 > /tmp/a.smt2
   echo "(check-sat)" >> /tmp/a.smt2 && z3 /tmp/a.smt2   # sat
   ```
7. Drop the directory in `fixtures/` — it is auto-discovered. Run the suite.

## How a NEW engine plugs in

A replacement runtime proves it preserves behavior by implementing one trait
and passing the suite:

```rust
trait FsmEngine {
    fn name(&self) -> &str;
    fn tick(&self, fx: &Fixture) -> Outcome;   // Sat{model, effects} | Unsat | Unsupported
}
```

`tick` receives a `Fixture` (its `meta` + the `*.smt2` paths) and returns what
the engine computed for that tick. The runner (`run_engine` + `diff`) loads
every fixture, calls `tick`, and diffs against the golden:
- positive fixtures: each `expect.model[k]` must match, and (if the engine
  surfaces effects) the dispatched `effects` must match in order;
- negative fixtures: either the engine reports `Unsat`, **or** it forces an
  output that differs from each `expect.forbidden[k]`.

A new engine that ingests SMT-LIB implements `tick` like `SmtLibEngine` (solve
`problem ⧺ prev ⧺ inputs`, extract the model). A new engine that ingests Evident
implements it like `CurrentRuntimeEngine` (run `source.ev`). Either way, **green
across all fixtures = behavior preserved.**

## Faithfulness, stated honestly

- Every golden is **doubly witnessed**: the current Evident runtime produces it
  (`CurrentRuntimeEngine`), and Z3 confirms the SMT-LIB capture produces the same
  (`SmtLibEngine`, Method A admissible + Method B unique). A capture that drifted
  from runtime behavior would fail one of the two.
- The fixtures pin the **full input frame**, so each transition has exactly one
  golden model (the determinism rule). Where an input was left free and surfaced
  nondeterminism, the fixture was tightened, not the assertion loosened (see
  `prev_record_fields`).

## Contract TODOs — design fresh, don't fake

These are behaviors the survey identified that the current capture does **not**
pin as single-tick fixtures. They are recorded here so a new engine designs them
deliberately rather than inheriting an accidental encoding. None are faked into a
fixture.

1. **Cross-FSM wake propagation / same-tick writer→reader visibility.** Cluster E
   captures a *writer's* tick and a *reader's* tick in isolation. The scheduler
   property "writer changes `world.X` → reader is woken next tick" and
   "writer-first ordering lets a reader see the just-written value in the same
   tick" are **multi-tick** properties of `run_with_ctx`, not a single
   `query_with_pins_and_given`. A future multi-tick fixture format (a sequence of
   ticks with a shared world) should capture these.

2. **Effect dispatch ordering — mode 2 (toposort) + nondeterministic tie-break.**
   All effect fixtures are **mode 1** (the FSM declares an `effects` slot →
   dispatch is the literal `Seq` order, deterministic). Mode 2 (no slot; effects
   scraped from all bindings and toposorted by `Seq(Effect)` edges) has a
   **random tie-break** (`EVIDENT_DISPATCH_SEED`); capturing it needs a pinned
   seed and a fixture format that records the edge graph.

3. **Negative transitions under the functionizer fast-path.** Pinning an *output*
   as a `given` does **not** trigger UNSAT in the current runtime — the
   functionizer fast-path treats given values as ground truth (it echoes them and
   reports SAT). So `CurrentRuntimeEngine` witnesses a negative as
   "the forced output differs from the forbidden one" (`expect.forbidden`), while
   `SmtLibEngine` witnesses the genuine UNSAT. A new engine should decide
   explicitly whether an over-pinned output is a contradiction (UNSAT) or ground
   truth (echo) — the contract documents both witnesses rather than picking one.

4. **Async event sources** (FrameTimer/`tick_count`, Stdin/`stdin_line`,
   Sigint/`signal_received`). These inject world writes from wall-clock / external
   I/O via plugin-as-writer. They are outside a deterministic transition fixture
   (the value depends on real time / external input). A new engine implements the
   `EventSource` trait; the contract for *how a user FSM reads the injected world
   field* is already covered by Cluster E's reader slice.

5. **`run(F, init)` percolated child effects.** A nested embedded FSM driven by
   `run`/`halts_within` inside a parent tick percolates child effects up. This is
   a nested-FSM-unroll concern, separate from the flat per-tick relation captured
   here; it deserves its own fixture cluster once the multi-tick format exists.

6. **Transpiler gap (`translate/smtlib.rs`).** The existing Evident→SMT-LIB
   transpiler covers only a scalar QF subset (no enums / `match` / `Seq`), so 12
   of 15 `problem.smt2` files are `handwritten`. Extending the transpiler to emit
   `declare-datatypes`, lower `Expr::Match`→nested `ite`, and `SeqLit`→`seq.++`
   would let most fixtures be regenerated as `transpiled` — a transpiler TODO, not
   a contract blocker. The scalar base (`runtime/tests/smtlib_roundtrip.rs`) stays
   untouched.

See `survey/INDEX.md` for the per-behavior detail behind each TODO.
