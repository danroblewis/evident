# Display FSM → Native JIT — Autonomous Plan (R36+)

## Goal

Get Mario's `display` FSM to fully JIT-compile via Cranelift, eliminating
the 18ms slow-path solve per tick (currently the dominant cost).

## Current state (post R35)

- Mario: 35.5 fps. `display` takes ~18ms/tick on slow path (slowest FSM).
- `keyboard` JIT-compiled (well — Rust-VM compiled, 0.3ms/tick).
- `display` falls through to slow path because Cranelift codegen
  doesn't yet handle the Op kinds present in its body.

## Display body Op inventory

From simplified body (260 assertions → 42 outputs):

| Op kind                          | Frequency  | JIT support |
|----------------------------------|-----------:|-------------|
| Multi-field DT_CONSTRUCTOR (4f)  | ~20 LibCalls | NO (only 0-1 field supported) |
| Cons-chain Seq payload (`__Cell_FFIArg`) | ~80 nested | NO |
| ITE on Value output              | 3+        | NO          |
| Int ADD/SUB/MUL/UMINUS           | many      | NO          |
| Z3Step::PreBaked                 | 6+ (platforms, e_init, mario.rects, plat_x, plat_y, etc.) | NO |
| SELECT (Seq element extraction)  | many      | NO          |
| DT_ACCESSOR (record field)       | many      | NO          |
| 1-arity UNINTERPRETED (`__arr`)  | many      | NO          |
| SEQ_CONCAT (string concat)       | 0 in display | NO       |
| Comparisons (LT/LE/GT/GE)        | 2+        | NO          |
| AND/OR/NOT                       | implied   | NO          |
| IsVariant recognizer             | 0 in display | NO       |
| Guarded step                     | 0 in display | NO       |
| Seq step with non-literal elems  | many      | partial     |

## Phases

### Phase A — Diagnostic [done]

Above table.

### Phase B — Test harness (parallel via subagent)

Write `runtime/tests/sdl_window_jit_pipeline.rs` modeled on
`effects_producer_pipeline.rs`:

1. Load a minimal SDL-window schema (one FSM, one tick, opens a window,
   waits, closes it).
2. Build Z3Program via the full pipeline.
3. Compile via `cranelift_jit::compile_program`.
4. Verify `jit.call(...)` returns successfully.
5. Actually dispatch the resulting effects through the effect runner
   so a real SDL window opens for ~2 seconds.

This test gates each Cranelift codegen addition — if a phase breaks the
test, we know to roll back.

### Phase C — Cranelift codegen extensions (sequential)

Each sub-phase: implement → run `cargo test --test sdl_window_jit_pipeline`
→ run Mario → measure `display: jit=yes/no` → measure fps.

#### C1. Z3Step::PreBaked → constant Value emission

PreBaked steps hold a pre-extracted `Value` (e.g. a `Seq(Body)` of 4
record-Seqs that Z3 simplified to per-field accessor pins which then
got gap-filled via model extraction). 

Codegen approach:
- Maintain a `value_pool: Vec<Value>` in `JitProgram` (alongside `_string_pool`).
- For each PreBaked step, push the Value onto the pool, get index `i`.
- Emit `ev_clone_from_pool(out_slot, pool_ptr, i)` helper call.
- Helper does `*out_slot = (*pool_ptr.add(i)).clone()`.

This single phase unlocks ~6 outputs in display.

#### C2. ITE on Value outputs

For `(ite c t e)` where c is Bool and t/e are Values:
- Compile c → Bool register.
- Emit branch: `brif c, then_block, else_block`.
- then_block: compile t into out_slot, jump to merge.
- else_block: compile e into out_slot, jump to merge.
- merge_block: continue.

The output slot is the same in both branches — Cranelift's SSA handles
the dataflow automatically because we write to memory (not registers).

#### C3. Int arithmetic ops

Already partially supported (kind_of_dynamic returns Int). Need to emit
the actual IR for ADD/SUB/MUL/UMINUS as Cranelift `iadd/isub/imul/ineg`.

For Scalar Int outputs, emit:
- `(+ a b)` → load operands, `iadd`, store via `ev_set_int`.
- Mixed literal + variable: handle constants via `iconst`.

#### C4. Multi-field DT_CONSTRUCTOR with arbitrary payload types

Currently:
- 0 fields: `ev_set_enum_nullary`.
- 1 field (Int/String): `ev_set_enum_int` / `ev_set_enum_str`.
- 2+ fields: NONE.

Need:
- `ev_set_enum_multifield(out, enum_name, variant, args_ptr, args_len)`
  helper that takes pre-built `Value` args.
- For each field, allocate a stack slot, recursively `emit_write_value`
  into it, then collect pointers + call the helper.

This handles LibCall(String, String, String, Seq(FFIArg)).

#### C5. Cons-chain Seq payload values

Z3 represents Seq(T) inside an enum payload as `__SeqOf_T` Cons chain:
- `__Empty_T()` → SeqEnum([]).
- `__Cell_T(head, tail)` → SeqEnum([head, ...tail_elements]).

Codegen for `__Cell_T(head, tail)`:
- Build `tail` value recursively into stack slot.
- Build `head` value into another stack slot.
- Emit a helper that prepends: `ev_seq_prepend_clone(out_slot, head_slot, tail_slot)`.

Or simpler: detect the entire chain at compile time, collect element
exprs, emit a `ev_seq_new + ev_seq_push_clone` loop just like Z3Step::Seq.

This is the cleanest approach — handle Cons chains as a "virtual Seq"
at compile time.

#### C6. SELECT + 1-arity UNINTERPRETED + DT_ACCESSOR

For `(select arr idx_lit)` where arr is in env:
- If arr is a Seq output of the same program, read the cached Seq.
- If arr is a given Seq value, read the env-mapped slot.

For `(field__arr X)` 1-arity UNINTERPRETED:
- Compile X → enum value in slot.
- Emit helper `ev_extract_seq_field(out, in_slot, field_name)` that
  reads X.field (a SeqEnum) and stores in out.

For `(field X)` DT_ACCESSOR:
- Compile X → enum value in slot.
- Emit helper `ev_extract_enum_field(out, in_slot, field_name)`.

#### C7. SEQ_CONCAT for strings

`(str.++ a b ...)`:
- Compile each operand → String slot.
- Emit helper `ev_str_concat(out, a_slot, b_slot, ...)` or chain N
  via a varargs-style helper.

#### C8. Comparisons + Boolean ops + IsVariant recognizer

Standard Cranelift IR:
- LT/LE/GT/GE: `icmp slt/sle/sgt/sge`.
- AND/OR/NOT: `band/bor/bxor` (with proper i64 ↔ b1 conversion).
- IsVariant: helper `ev_test_variant(in_slot, variant_str) -> i64`.

#### C9. Guarded steps (if needed)

For `Z3Step::Guarded { var, branches: [(guard, body), ...] }`:
- Compile each guard → Bool.
- Cascade `brif` branches, falling through to subsequent guards.
- Each branch's body writes to out_slot.

Not present in display, but useful for game/level_gen.

### Phase D — Integration & verify

- Run full test suite: `./test.sh` — all 451 cargo + 119 conformance pass.
- Run Mario, capture `EVIDENT_FUNCTIONIZE_STATS=1` output.
- Verify `display: jit=yes`.
- Measure fps before/after.

### Phase E — Mario .ev simplification (fallback)

If a specific Op kind proves too complex to JIT-compile, refactor Mario's
display body to avoid it. Allowed surgical changes:
- Replace `..Level` passthrough with explicit field copies (avoids
  PreBaked gap-fill).
- Inline subclaim bodies that complicate Z3 simplification.
- Re-shape `mario.rects` from MarioSprite subschema to a plain
  Seq literal in display.

## Success criteria

1. `runtime/tests/sdl_window_jit_pipeline.rs` passes (window opens via JIT).
2. `EVIDENT_FUNCTIONIZE_STATS=1` shows `display: jit=yes`.
3. Mario fps ≥ 45 (up from 35.5).
4. Full test suite passes (`./test.sh`).
