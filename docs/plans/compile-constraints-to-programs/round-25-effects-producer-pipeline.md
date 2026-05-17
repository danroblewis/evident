# Round 25 — Effects-producer pipeline snapshot test

**Outcome:** SHIPPABLE. The canonical "Mario display" FSM
pattern — a function that emits a `Seq(Effect)` of ordered
side-effects — now flows cleanly through Stages 1-3 of the
compilation pipeline. Snapshot test
`runtime/tests/effects_producer_pipeline.rs` documents what
each intermediate form looks like and asserts on the shape.

Stage 4 (Cranelift JIT) is the documented gap: the JIT
correctly refuses to compile programs that produce `Seq`
outputs OR contain payload-bearing enum constructors. Closing
that gap unlocks Mario-class workloads.

## The pipeline, with a worked example

Source (Evident):

```text
claim display
    state ∈ DState
    state_next ∈ DState
    last_results ∈ Seq(Effect)
    effects ∈ Seq(Effect)
    state_next = Done
    eff_hello ∈ Effect = Println("hello")
    eff_world ∈ Effect = Println("world")
    eff_exit  ∈ Effect = Exit(0)
    effects = ⟨eff_hello, eff_world, eff_exit⟩
```

**Stage 2** — Z3 simplified assertions after `simplify` +
`propagate-values`:

```text
(>= last_results__len 0)
(>= effects__len 0)
(= state_next Done)
(= eff_hello (Println "hello"))
(= eff_world (Println "world"))
(= eff_exit (Exit 0))
(= (select effects 0) (Println "hello"))
(= (select effects 1) (Println "world"))
(= (select effects 2) (Exit 0))
```

**Stage 3** — `Z3Program` from `extract_program`:

```text
step 0: Scalar  state_next = Done
step 1: Seq     effects = ⟨3 elems⟩
  [0] = (Println "hello")
  [1] = (Println "world")
  [2] = (Exit 0)
step 2: Scalar  eff_world = (Println "world")
step 3: Scalar  eff_hello = (Println "hello")
step 4: Scalar  eff_exit  = (Exit 0)
```

**Stage 4** (target, NOT YET) — Cranelift native function:

```text
fn display(/* no inputs in this minimal case */)
       -> { state_next: Value, effects: Value, ... }
{
    let state_next = Value::Enum {
        enum_name: "DState",
        variant:   "Done",
        fields:    vec![],
    };
    let eff_hello = Value::Enum {
        enum_name: "Effect",
        variant:   "Println",
        fields:    vec![Value::Str("hello".into())],
    };
    let eff_world = Value::Enum {
        enum_name: "Effect",
        variant:   "Println",
        fields:    vec![Value::Str("world".into())],
    };
    let eff_exit = Value::Enum {
        enum_name: "Effect",
        variant:   "Exit",
        fields:    vec![Value::Int(0)],
    };
    let effects = Value::SeqEnum(vec![eff_hello.clone(), eff_world.clone(), eff_exit.clone()]);
    return ...;
}
```

For Mario's actual display body, the same shape: ~20 Effect
elements each built from constants and a handful of input
loads (`world.player.pos.x`, `_world.tick`, etc.). All
function-shaped.

## What this round shipped

### `runtime/tests/effects_producer_pipeline.rs`

Three tests, one per pipeline stage:

1. `stage_2_simplified_z3_assertions_match_per_element_pins` —
   asserts on the simplified Z3 ASTs (printed to test output).
2. `stage_3_extract_program_builds_seq_step` — asserts on the
   `Z3Program` structure: exactly one `Seq` step for `effects`,
   plus scalar steps for the intermediates.
3. `stage_4_jit_compiles_effects_producer` — currently
   documents the GAP via `assert!(jit.is_none())`. When the
   JIT is extended, flip the assertion and assert on the
   `Value::SeqEnum` output content.

### Z3-AST extract length inference

`apply_seq_lengths` (build_cache's pre-pass) pins the literal
length BEFORE body translation, so the SeqLit assignment
emits `(= 3 3)` which simplifies to `true` and disappears.
That leaves the per-element pins (select effects 0..2)
floating without a length anchor. `extract_program` couldn't
build a Seq step without knowing the length.

Round 25 makes `extract_program` infer length from
consecutive `(select arr 0..K)` pins when no explicit
`(= arr__len N)` survived: largest `K` such that all of
`0..=K` are present yields length `K+1`.

This unblocks Stage 3 for ALL effects-producer FSMs without
needing the user to write `#effects = N` explicitly.

## The Stage 4 gap

The JIT (`runtime/src/cranelift_jit.rs`) currently refuses
two things this pattern needs:

1. **Seq output construction.** The JIT signature is
   `extern "C" fn(*const i64, *mut i64)` — fixed-shape slot
   arrays of primitive values. A `Value::SeqEnum` of 3
   payload-bearing Effects can't fit. Need a richer FFI
   boundary or a Rust-side construction helper.

2. **Payload-bearing constructor calls.** `Println("hello")`
   has a String payload (not i64-fittable). `Exit(0)` is OK
   (Int payload). The constructor for `Println` allocates a
   Rust String.

The cleanest near-term approach: keep the JIT focused on
COMPUTING the dynamic inputs (any non-constant args to the
constructors) and let a Rust-side helper construct the
`Value::Enum` values. For this test fixture (all constants),
the JIT could emit ZERO computation and a single "produce
this fixed Vec of literals" instruction.

### What "predict the Cranelift output" looks like in practice

For the test fixture's effects:

```text
; pseudo-Cranelift IR
fn display(out: *mut OutputSlots) {
    ; effects[0] = Println("hello") — purely constant
    call_rust build_value_enum_str(out + offset_eff_hello,
                                    /*variant=*/"Println",
                                    /*str=*/"hello")
    call_rust build_value_enum_str(out + offset_eff_world, "Println", "world")
    call_rust build_value_enum_int(out + offset_eff_exit, "Exit", 0)
    ; effects = SeqEnum([eff_hello, eff_world, eff_exit])
    call_rust build_seq_enum_3(out + offset_effects,
                                out + offset_eff_hello,
                                out + offset_eff_world,
                                out + offset_eff_exit)
    ; state_next = Done
    call_rust build_value_enum_nullary(out + offset_state_next,
                                        /*variant=*/"Done")
    return
}
```

For Mario display, the same shape but ~20 build_value_enum_*
calls per tick. With each callback at ~20-50 ns, that's
~1µs per tick of effect construction — vs the current
~13ms of Z3 solving. The 10000× speedup target is reachable.

## Round 26 plan

1. **Seq output codegen.** Add `OutputKind::SeqEnum` to the
   JIT. Each Seq output gets a `*mut Vec<Value>` (or similar
   stable layout) in the output slot. The JIT emits
   per-element construction calls into the Vec.

2. **Payload-bearing constructor codegen.** Add Rust-side
   helper functions:
   - `build_value_enum_str(out: *mut Value, variant: &str, payload: &str)`
   - `build_value_enum_int(out: *mut Value, variant: &str, payload: i64)`
   - etc.
   The JIT emits `call_indirect` to these helpers, passing
   the variant name as a static reference and the payload as
   computed values.

3. **String literal handling.** Either intern strings into a
   side table (JIT passes index) or pass `&'static str` from
   a string-pool the runtime owns. The latter is simpler.

4. **Flip the Stage 4 assertion.** Once the JIT compiles
   this test fixture, the assertion goes from `is_none()` to
   `is_some()` and includes the expected output content.

5. **Bench Mario display under the new JIT.** Expected: per-tick
   solve drops from 13.9ms → ~10-50µs (the Z3 work disappears;
   what's left is constructor allocation + LibCall dispatch).
   If that's right, Mario hits its 60 fps target.

## Test coverage as of this round

- All 444 cargo + 119 conformance tests pass.
- 3/3 effects-producer pipeline tests pass (with stage 4
  asserting the documented gap).
- `EVIDENT_FUNCTIONIZE_STATS=1` continues to show 0
  function-ization on Mario — unchanged, but now we
  understand exactly what's blocking it.
