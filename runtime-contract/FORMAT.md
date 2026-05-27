# Fixture format — the behavior contract

A **fixture** captures one tick of the Evident multi-FSM runtime as an
implementation-agnostic record:

```
(transition SMT-LIB + metadata + prev-state + inputs)  →  (golden next-state model + effects)
```

Each fixture is a directory `runtime-contract/fixtures/<name>/` containing six
files. Three are the portable, engine-neutral capture (`*.smt2`); one is the
machine-readable metadata that names the FSM structure and carries the golden
in typed form (`meta.json`); one is the golden model as SMT-LIB
(`expected_model.smt2`); one is the golden dispatched effects as text
(`expected_effects.txt`). A seventh convenience file, `source.ev`, carries the
Evident FSM the behavior was derived from (provenance + lets the current-runtime
adapter replay the real pipeline).

```
fixtures/<name>/
  meta.json            # roles + typed pins + typed golden (the machine contract)
  problem.smt2         # the transition RELATION (sorts + constraints), no concrete values
  prev.smt2            # pins the FSM's previous state (state enum, _var time-shift)
  inputs.smt2          # pins external inputs (world.X, last_results, is_first_tick)
  expected_model.smt2  # golden next-state assignment as SMT-LIB witness
  expected_effects.txt # golden dispatched effects, one per line
  source.ev            # the Evident FSM (provenance; replayed by CurrentRuntimeEngine)
```

The decision to use a **JSON sidecar** for metadata (over naming-convention
hints inside the SMT-LIB, or SMT-LIB `(set-info)` annotations): JSON is
machine-readable by any parser, independently versionable (`format_version`),
type-safe for `Value` round-trips, and keeps the `.smt2` files purely
relational. `(set-info)` is string-only (you'd encode JSON in a string anyway)
and is stripped by many SMT pre-processors; naming conventions are untyped and
break silently on rename.

---

## 1. The atomic unit being captured

Every fixture is one call to the runtime's tick primitive
(`runtime/src/runtime/scheduler_api.rs:20`):

```
query_with_pins_and_given(fsm_claim, pins, given) -> { satisfied, bindings }
```

followed, for the effects, by `collect_dispatchable_effects`
(`runtime/src/effect_loop/collect.rs:17`). The fixture pins the full input
frame (`given`) so the model is unique (see the **determinism rule** below),
then records the golden `bindings` (as `expected_model.smt2` + `meta.expect.model`)
and the golden ordered effects (as `expected_effects.txt` + `meta.expect.effects`).

**Determinism rule.** Z3 assigns any unconstrained variable an arbitrary value.
A fixture MUST either pin every input that drives a checked output, or assert
only on outputs uniquely forced by the pins. Fixtures here pin the full input
frame, so each has exactly one golden model. Phase-3 self-consistency runs the
SMT-LIB uniqueness check (§5, Method B) on every checked output to prove this.

---

## 2. `meta.json` schema (format_version 1)

```jsonc
{
  // ── identification ───────────────────────────────────────────────
  "format_version": 1,                          // bump on incompatible change
  "name": "tick_counter_start",                 // == directory name
  "fsm_claim": "counter",                        // the `fsm` name passed to query_with_pins_and_given
  "source_ev": "fixtures/tick_counter_start/source.ev", // repo-relative path to the FSM
  "from_example": "test_02_counter",             // example the behavior came from (or null)
  "from_claim": "sat_start_seeds_count_five",    // existing sat_/unsat_ claim asserting the same golden (null if derived)
  "how_built": "handwritten",                    // "transpiled" | "handwritten" — how problem.smt2 was produced
  "notes": "",

  // ── FSM structure roles ──────────────────────────────────────────
  "state_var":        "state",                   // prev-tick state input  (null if FSM has none)
  "state_next_var":   "state_next",              // next-tick state output (null if none)
  "effects_var":      "effects",                 // effect-list output     (null if none)
  "last_results_var": "last_results",            // effect-result input    (null if unused)
  "world_fields": [],                            // bare field names under world.* (multi-FSM)

  // ── inputs / pins ────────────────────────────────────────────────
  // var-name → tagged Value (§3). Every key is asserted equal before solving.
  // Mirrors prev.smt2 + inputs.smt2; this is the form the current-runtime
  // adapter consumes (it builds `given` directly from here).
  "given": {
    "state":        {"enum": "CountState", "variant": "Start", "fields": []},
    "last_results": {"seq_enum": "Result", "elems": []}
  },

  // ── expectation ──────────────────────────────────────────────────
  "expect": {
    "unsat": false,                              // true → solve must be UNSAT (model/effects ignored)
    "model": {                                   // subset of bindings to check (typed)
      "state_next": {"enum": "CountState", "variant": "Count", "fields": [{"int": 5}]}
    },
    "effects": [                                 // ordered golden effects (typed); mirrors expected_effects.txt
      {"enum": "Effect", "variant": "Println", "fields": [{"str": "starting count"}]}
    ],
    "halt": false,                               // true iff any effect this tick is Exit(_)
    "exit_code": null                            // Int payload of Exit, else null
  },

  // ── solver hint ──────────────────────────────────────────────────
  "effects_in_smt": false  // true → the effects Seq is encoded in problem.smt2 (pure-SMT-checkable)
}
```

`source_ev` is repo-relative; for fixtures the convenient location is the
fixture's own `source.ev`, but it MAY point at an `examples/test_*.ev` directly.

`state_var`/`state_next_var`/`effects_var`/`last_results_var` are `null` when the
FSM has no such slot (e.g. test_22's `walker` has no enum state — only `_var`
record threading; its `state_var` is `null`).

---

## 3. Tagged-JSON `Value` encoding

Every value in `given` and `expect` is a single-key tagged object. Mirrors
`runtime/src/core/value.rs::Value` exactly.

| `Value` variant | JSON | Example |
|---|---|---|
| `Int(i64)` | `{"int": N}` | `{"int": 5}` |
| `Bool(bool)` | `{"bool": B}` | `{"bool": true}` |
| `Real(f64)` | `{"real": F}` | `{"real": 3.14}` |
| `Str(String)` | `{"str": "…"}` | `{"str": "hi"}` |
| `SeqInt` | `{"seq_int": [N,…]}` | `{"seq_int": [1,2]}` |
| `SeqBool` | `{"seq_bool": [B,…]}` | `{"seq_bool": [true]}` |
| `SeqStr` | `{"seq_str": ["…",…]}` | `{"seq_str": ["a"]}` |
| `SeqEnum` | `{"seq_enum": "Name", "elems": [Value,…]}` | `{"seq_enum": "Result", "elems": []}` |
| `Enum{enum_name,variant,fields}` | `{"enum": "Name", "variant": "Ctor", "fields": [Value,…]}` | `{"enum": "CountState", "variant": "Count", "fields": [{"int": 5}]}` |
| `Composite` | `{"composite": {"field": Value, …}}` | `{"composite": {"x": {"int": 3}, "y": {"int": 4}}}` |
| `SeqComposite` | `{"seq_composite": [{"field": Value}, …]}` | `{"seq_composite": [{"x": {"int": 1}}]}` |
| `SetInt`/`SetBool`/`SetStr` | `{"set_int"/"set_bool"/"set_str": […]}` | `{"set_int": [1,5]}` |

Rules:
- `"fields"` and `"elems"` are ALWAYS present arrays (empty for nullary / empty seq) — never omitted.
- `"seq_enum"` names the element enum once; each `elems` entry is a full tagged Value.
- `"composite"` keys are bare field names (`"x"`), not dotted paths.

**`last_results` element variants** (the `Result` enum from `stdlib/runtime.ev`,
which the dispatcher's `EffectResult` maps onto):

| `EffectResult` | `Result` variant name | fields |
|---|---|---|
| `NoResult` | `NoResult` | `[]` |
| `Int(n)` | `IntResult` | `[{"int": n}]` |
| `Str(s)` | `StringResult` | `[{"str": s}]` |
| `Bool(b)` | `BoolResult` | `[{"bool": b}]` |
| `Real(f)` | `RealResult` | `[{"real": f}]` |
| `Handle(h)` | `HandleResult` | `[{"int": h}]` |
| `Error(s)` | `ErrorResult` | `[{"str": s}]` |

---

## 4. The SMT-LIB files

A consumer that wants to solve the transition concatenates
`problem.smt2 ⧺ prev.smt2 ⧺ inputs.smt2`, then appends `(check-sat)` /
`(get-model)`. **None of the files contain `check-sat`/`get-model`** — the
consumer adds them (so it can also append witness/uniqueness assertions). All
syntax below is verified against Z3 4.16.0.

### 4.1 `problem.smt2` — the transition relation

- **Sort/datatype declarations.** Each enum → one `declare-datatypes` entry;
  multiple enums batch into one call (matches Z3 `create_datatypes`). Scalars →
  `declare-const`. Records → flattened per-field scalar consts (`_pos.x`, `_pos.y`).
- **Transition constraints** — `(assert …)` lines for `state_next`, derived
  locals (`count`, `n_str`…), and — when `effects_in_smt:true` — `effects`.
  `match` lowers to nested `ite` with `is-VARIANT` recognizers and `VARIANT_i`
  selectors.
- **Infra consts** — `is_first_tick`, `_count`, `_state`, … declared here so
  `prev`/`inputs` can pin them.
- **No concrete values, no `check-sat`.**

Enum → ADT (selectors named `VARIANT_INDEX`, 0-based):

```smt2
(declare-datatypes
  ((CountState 0) (Effect 0) (Result 0))
  (((Start) (Count (Count_0 Int)) (Format (Format_0 Int)) (Done))
   ((NoEffect) (Print (Print_0 String)) (Println (Println_0 String))
    (Exit (Exit_0 Int)) (IntToStr (IntToStr_0 Int)) (ParseInt (ParseInt_0 String))
    (ReadLine) (Time))
   ((NoResult) (IntResult (IntResult_0 Int)) (StringResult (StringResult_0 String))
    (ErrorResult (ErrorResult_0 String)))))
```

`match state` → nested `ite` (test_02 `state_next`):

```smt2
(assert (= state_next
  (ite (is-Start  state) (Count 5)
  (ite (is-Count  state) (Format (Count_0 state))
  (ite (is-Format state) (ite (<= (Format_0 state) 1) Done (Count (- (Format_0 state) 1)))
                          Done)))))
```

`Seq(Effect)` (only when `effects_in_smt:true`) uses Z3's built-in sequence sort:
`⟨a,b⟩` → `(seq.++ (seq.unit a) (seq.unit b))`; `⟨⟩` → `(as seq.empty (Seq Effect))`.
Both `is-VARIANT` (Z3 extension) and the standard `((_ is VARIANT) x)` parse;
fixtures use `is-VARIANT` for brevity.

### 4.2 `prev.smt2` — previous-state pins

`(assert (= state Init))`, `(assert (= _count 7))`, `(assert (= is_first_tick false))`,
`(assert (= _pos.x 7))`, … Every const named must be declared in `problem.smt2`.

### 4.3 `inputs.smt2` — external input pins

`world.X` fields, `last_results` elements (when representable in SMT), free
inputs. Empty (comment-only) for self-contained FSMs.

### 4.4 `expected_model.smt2` — golden witness

One `(assert (= NAME VALUE))` per uniquely-forced output. It documents what the
current runtime produced; it does not itself solve. The header comments cite the
`evident test` command + claim it was captured from. For `unsat`
fixtures it is a comment-only file (no model exists).

---

## 5. Checking protocol (pure-SMT engine)

A purely SMT-LIB engine validates a fixture three ways (all verified on z3 4.16):

- **Method A — forward (model is admissible):**
  `problem ⧺ prev ⧺ inputs ⧺ expected_model` + `(check-sat)` ⇒ **sat**.
  UNSAT here = the relation excludes the golden (regression).
- **Method B — uniqueness (per checked output):** for each `(= NAME VALUE)` in
  the witness, `problem ⧺ prev ⧺ inputs` + `(assert (not (= NAME VALUE)))` +
  `(check-sat)` ⇒ **unsat**. SAT here = output under-constrained (the
  determinism rule is violated).
- **UNSAT fixtures (Cluster F):** `problem ⧺ prev ⧺ inputs` + `(check-sat)`
  ⇒ **unsat**.

These are what Phase-3 self-consistency and the Phase-4 `SmtLibEngine` run.

---

## 6. `expected_effects.txt` — canonical effect text

One effect per line, in the order the runtime dispatches them
(`collect_dispatchable_effects`). For the mode-1 fixtures here (the FSM declares
an `effects` slot), dispatch order = the source `Seq(Effect)` order, so it is
deterministic.

```
effect-line ::= Variant            -- nullary: NoEffect, ReadLine, Time
              | Variant( payload )
payload     ::= string-lit | int-lit | string-lit ',' ' ' int-lit
string-lit  ::= '"' (char | '\"' | '\\' | '\n' | '\t')* '"'   -- backslash escaping (NOT SMT doubled-quote)
```

Variant names match `runtime/src/core/ast.rs::Effect`. **Empty effect list = empty
file (zero bytes)** — no `NoEffect` line. Examples:

```
Println("exiting with code 42")
Exit(42)
```
```
IntToStr(3)
```

---

## 7. `transpiled` vs `handwritten`

`how_built` records how `problem.smt2` was produced:

- **`transpiled`** — the FSM body's checked nucleus is in the scalar QF subset
  that `runtime/src/translate/smtlib.rs` emits (Int/Nat/Pos/Bool/Real/String,
  `+ - * /`, comparisons, `∧ ∨ ¬ ⇒`, set/range `∈`, ternary→`ite`). In practice
  these still need the state-enum `declare-datatypes` hand-appended (the
  transpiler doesn't emit ADTs yet), so `transpiled` means "the scalar block was
  or could be auto-generated; enum lines were added by hand." Candidates:
  `prev_first_tick_zero`, `prev_increment`, `prev_record_fields`.
- **`handwritten`** — anything with enum-valued state in/out, `match`, or
  `Seq(Effect)` (most fixtures). Authored by hand per §4.

Extending `translate/smtlib.rs` with enum/`match`/`SeqLit` support (emit
`declare-datatypes`, lower `Expr::Match`→nested `ite`, `SeqLit`→`seq.++`) would
move most fixtures to `transpiled` — a **transpiler TODO**, not a blocker. The
scalar base tested by `runtime/tests/smtlib_roundtrip.rs` stays untouched.

---

## 8. How the two engines consume a fixture

- **`CurrentRuntimeEngine`** (proves the golden IS current behavior): loads
  `source.ev` into `EvidentRuntime`, builds `given` from `meta.given`, calls
  `query_with_pins_and_given(fsm_claim, …)`, collects effects, diffs `bindings`
  vs `meta.expect.model` and effects vs `expected_effects.txt`. Phase-4 gate.
- **`SmtLibEngine`** (proves the SMT-LIB capture is faithful): runs Method A/B/UNSAT
  (§5) on `problem ⧺ prev ⧺ inputs` vs `expected_model.smt2`. Validates the
  portable artifact. Bonus engine; the basis on which a brand-new runtime plugs in.

Both passing means the SMT-LIB problem is a faithful, implementation-agnostic
capture of the current runtime's behavior — the whole point of the contract.
