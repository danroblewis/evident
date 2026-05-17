# Plan: get Mario `display` FSM JIT-compilable

## Goal

Compile `examples/test_21_mario/main.ev`'s `display` FSM
into native code via the Z3-functionizer → Cranelift pipeline,
hit ≥45 fps (vs ~24 fps today).

## Where we are today

- Cranelift codegen handles `Z3Step::Seq` outputs + 0/1-arity
  payload constructors (Println, Exit, set_enum_nullary). Round 26
  snapshot test passes end-to-end on a 3-element `Println`/`Exit`
  effects-producer.
- The Z3 functionizer is gated OFF on the scheduler path because
  enabling it triggers a `build_cache` fatal-exit on Mario's
  `SDL_Window` install Seq.
- The AST functionizer gate-rejects `display` for "Forall body:
  body Call win.draw_rect".

## Mario `display`'s actual feature set

Walking through the body line-by-line surfaces 14 distinct
language features that have to round-trip through the JIT
pipeline:

| # | Feature                                            | Status today           |
|---|----------------------------------------------------|------------------------|
| 1 | `..Level` passthrough flattening                   | works (R18)            |
| 2 | `#_world.enemies = 2` length pins                  | works                  |
| 3 | `_world.plat_x` underscore-prefixed reads          | works                  |
| 4 | `win ∈ SDL_Window (title ↦ ...)` typed pins        | works as Memberships   |
| 5 | SDL_Window `install ∈ Seq(InstallStep) = ⟨...⟩`    | **build_cache exits**  |
| 6 | `frame = (is_first_tick ? 0 : _frame + 1)`         | works (R23)            |
| 7 | `world.tick = frame` (writer pattern)              | works                  |
| 8 | `sdl_delay(16, delay_eff)` positional claim call   | works (R15)            |
| 9 | `win.set_draw_color((c), sky_eff)` subschema call  | **inline only via R17**|
| 10| `∀ (p, b) ∈ coindexed(platforms, plat_effs) : ...` | works (R18)            |
| 11| `win.draw_rect(Rect(p.color, ...), b.effs)` inside ∀| **needs ∀-inner inline**|
| 12| `mario ∈ MarioSprite (pos ↦ _world.player.pos)`    | works (R19)            |
| 13| `effects = sky_effs ++ ⟨present_eff⟩ ++ ...`       | works (R18)            |
| 14| `done_print = (frame ≥ 240 ? Println("...") : ...)`| **JIT can't ternary**  |

And on the JIT side:

| # | JIT requirement                                    | Status today           |
|---|----------------------------------------------------|------------------------|
| A | Seq output codegen                                 | works (R26)            |
| B | Payload-bearing 1-arg ctor (Println, Exit)         | works (R26)            |
| C | Multi-field ctor — LibCall(Str, Str, Str, Seq)     | **missing**            |
| D | Computed Int payload — `Exit(frame - 240)`         | **missing (literal only)**|
| E | Computed Bool predicate — `frame ≥ 240`            | **missing**            |
| F | Ternary on Value outputs — `cond ? A : B`          | **missing**            |
| G | Composite-literal arg — `Rect(color, pos, size)`   | **missing**            |
| H | Field access from input Composite — `_world.player.pos.x` | **missing**     |
| I | Seq element access at literal index — `plat_effs[0]` | **missing**          |
| J | Int arithmetic — `frame = _frame + 1`              | **dropped in R26**     |

## The plan: 9 rounds, ~2-4 weeks

Each round closes ONE blocker, has a snapshot test that flips
from red to green, and ends with a measurable change in
Mario's `[fz/stats]` or `--profile-z3` output.

### Round 27 — Diagnose & fix the SDL_Window translator gap

**Blocker**: enabling `EVIDENT_FUNCTIONIZE_Z3_SCHED=1` fatal-exits
on Mario at `install = ⟨Run(LibCall(...)), Bind(...), ...⟩`. The
slow path runs the same translator on the same body and DOESN'T
fatal — so there's a difference in how my Z3 functionizer
invokes `build_cache` vs how `effect_loop::run_with_ctx` invokes
the same translation.

**Investigation steps:**
1. Add a tracing wrapper around the dropped-constraint exit in
   `inline.rs` that prints the call stack + the surrounding
   schema name. Run Mario both ways; diff the traces.
2. Most likely hypothesis: the slow path passes given values
   that pin `title`/`width`/`height` to literals, allowing the
   `LibCall(..., ⟨ArgStr(title), ArgInt(width), ArgInt(height), ...⟩)`
   args to constant-fold to literals before reaching the
   dropped-constraint check.
3. My function-izer ALSO passes the given. But it might process
   declarations in a different order, leaving the args symbolic
   at translation time.

**Test**: a new `tests/sdl_window_translation.rs` that loads a
SchemaDecl with an `install`-shaped body item, builds the cache
once with explicit given pinning of the params (title/width/
height), once without, and asserts the first succeeds.

**Fix**: align the function-izer's call site with the slow-path
order. Specifically, ensure `apply_pinned_ints` + `apply_seq_lengths`
run BEFORE `inline_body_items` touches the install Seq.

**Acceptance**: `--z3-functionizer-sched` no longer fatal-exits
Mario. Stats show `[fz/stats] z3-fz·` for at least one Mario FSM.

---

### Round 28 — Fully resolve `win.draw_rect(...)` inside ∀ bodies

**Blocker**: `inline_positional_calls` (Round 15) handles
top-level Constraint Calls but doesn't recurse into ∀ bodies.
After ∀-over-Seq unrolling (R18), the unrolled bodies contain
`win.draw_rect(...)` which is still a Call — and is rejected by
the gate.

**Investigation**: dump `extract_program`'s output for `display`
under `--profile-functionizer` after R27 lands. We expect to see
the gate reject at `Forall body: body Call win.draw_rect`.

**Test**: `tests/forall_inner_subschema_call.rs`. A fixture FSM
with `∀ (p, b) ∈ coindexed(seqA, seqB) : recv.method(p, b.field)`
and assert it extracts cleanly to a `Z3Step::Seq` with the
per-element method calls inlined.

**Fix**:
1. After `expand_foralls` (in `try_extract_one_chain`), re-run
   `inline_positional_calls` on the result. The unrolled bodies
   are top-level Constraints now; the existing inliner handles
   them.
2. Alternatively: extend `inline_positional_calls_rec` to
   recurse into `Expr::Forall` bodies before unrolling.

**Acceptance**: Mario's `display` extracts a `Z3Program` with N
`Z3Step::Scalar`/`Seq` steps and ZERO un-inlined Call
constraints. `[fz/stats]` shows `display z3-fz✓ jit✗`.

---

### Round 29 — JIT codegen: multi-field enum constructors

**Blocker**: `LibCall(Str, Str, Str, Seq(FFIArg))` is 4 fields.
Round 26's codegen handles 0/1-arity only. After R27/R28,
`display`'s extracted Z3Program will have ~20 `LibCall(...)`
constructions — none of which compile.

**New helpers in `value_builders.rs`**:
```rust
ev_set_enum_2args(out, e, v, a0_kind, a0_data, a1_kind, a1_data)
ev_set_enum_3args(...)
ev_set_enum_4args(...)
```

Or — cleaner — a single variadic helper that takes a `*const ValueRaw` array:
```rust
ev_set_enum(out, e_ptr, e_len, v_ptr, v_len, args_ptr, args_count)
```

Where each `ValueRaw` is a small tagged union the JIT fills:
```rust
#[repr(C)]
pub struct ValueRaw {
    tag: u32,  // 0=Int, 1=Str, 2=Bool, 3=Enum*
    i64_payload: i64,
    str_ptr: *const u8,
    str_len: usize,
}
```

The JIT allocates `args_count` ValueRaw slots on the stack
(via `create_sized_stack_slot`), fills each via stores, and
passes the pointer.

**Test**: hand-built `Z3Program` with `LibCall("a", "b", "c",
empty_seq)` and assert the JIT output matches the expected
`Value::Enum`.

**Acceptance**: snapshot test passes. Mario `display`'s
extracted program — when run through the JIT — compiles
without falling back.

---

### Round 30 — JIT codegen: ternary on Value outputs

**Blocker**: `done_print = (frame ≥ 240 ? Println("mario done")
: NoEffect)` — the Value-typed conditional. Round 26 doesn't
emit conditional construction.

**New IR**: branch on the predicate, two basic blocks for the
then/else arms each emitting their respective `ev_set_*` call
into the output slot, merge block.

```text
  v_cond = emit_predicate(frame >= 240)
  brif v_cond, blk_then, blk_else
blk_then:
  call ev_set_enum_str(out, "Effect", "Println", "mario done")
  jump blk_merge
blk_else:
  call ev_set_enum_nullary(out, "Effect", "NoEffect")
  jump blk_merge
blk_merge:
  ...
```

**Test**: `Z3Step::Scalar { var: "x", expr: Ite(cond, A, B) }`
where A and B are constructors. Verify both branches.

**Acceptance**: Mario's done_print + done_exit compile.

---

### Round 31 — JIT codegen: Int arithmetic for computed payloads

**Blocker**: `frame = (is_first_tick ? 0 : _frame + 1)` — frame
is an Int output. After R30 the ternary is handled, but the
`_frame + 1` arithmetic isn't emitted yet (R26 stripped this).

**New IR**: emit native `iadd`/`isub`/`imul` for Int
computations. The result lands in an i64 register, then
`ev_set_int(out, computed_i64)` writes it.

**Test**: hand-built `Z3Program` with `frame = a + b * 2` and
verify the JIT produces `Value::Int(...)` with the right value
across input combinations.

**Acceptance**: Mario's `frame`, `world.tick`, and other Int
intermediates compile. The `frame ≥ 240` predicate (used in
R30's ternary) also lands here.

---

### Round 32 — JIT codegen: composite-literal args

**Blocker**: `Rect(p.color, p.aabb.pos, p.aabb.size)` — building
a Rect value from input field references. After R28 unrolls the
∀, each iteration becomes `win.draw_rect(Rect(platforms[i].color,
...), plat_effs[i].effs)`. We need to construct the Rect.

**Two sub-cases:**
1. **Rect is a user `type`** with primitive fields → it's a
   `Value::Composite { fields: {color: ..., pos: ..., size: ...} }`
   or each field is a separate i64-typed output slot. Mario
   uses the latter (record-as-vectors).
2. **The Rect appears as an arg to a subschema method** (already
   inlined to `LibCall(...)` after R28). The LibCall's Seq(FFIArg)
   payload contains the Rect's leaves: `⟨ArgInt(color.r),
   ArgInt(color.g), ..., ArgInt(pos.x), ArgInt(pos.y), ...⟩`.

So the input to the JIT, after extract, is already flat — we
just need to read the right input slots into the LibCall's arg
ValueRaw fields.

**New JIT support**: `Expr::Field` and `Expr::Index` resolution
during emit_write_value. For `platforms[i].color.r`, emit a
load from the appropriate input slot at the right offset.

The platforms Seq's fields get pre-extracted by `extract_program`
when ∀-unrolling. After R28 we should see flat `input.<seq>_<i>.<field>`
references that the JIT loads from the inputs array.

**Test**: ∀-unrolled body that reads `seq[i].field` and uses it
as an arg to a constructor.

**Acceptance**: Mario's `plat_effs[i].effs[j]` per-element draw
calls compile.

---

### Round 33 — Seq-element binding to existing outputs

**Blocker**: `phase_chain ∈ Seq(Effect) = ⟨clear_eff,
plat_effs[0].effs[0], plat_effs[0].effs[1], ...⟩` — the Seq
elements reference EARLIER outputs (clear_eff, plat_effs).
Round 26 only handles literal elements.

**Need**: when building `effects = seq_concat`, the elements can
be `Expr::Identifier(name)` referring to a previously-built
output slot. Emit a clone-from-slot rather than a fresh ctor
build.

**New helper**:
```rust
ev_clone_from_slot(out: *mut Value, src: *const Value)
```

In the JIT: `emit_write_value` on an Identifier expression
generates `ev_clone_from_slot(out_slot, env[name].ptr)`.

For `plat_effs[0].effs[0]` (Field-of-Index of an earlier Seq
output), we'd need to walk into the Value::SeqEnum and grab
the i-th element's i-th field. This is doable in a helper:
```rust
ev_clone_seq_element_field(out, seq: *const Value, seq_idx: usize, field_idx: usize)
```

**Acceptance**: Mario's phase_chain compiles. The full effects
Seq (concat of sky_effs + present_eff + phase_chain + done +
delay) compiles.

---

### Round 34 — Enable on scheduler + soak test

After R27-R33, every individual piece of Mario `display`
compiles. Round 34 is the integration:

1. Flip the default: `EVIDENT_FUNCTIONIZE_Z3` and
   `EVIDENT_FUNCTIONIZE_Z3_SCHED` to ON unless explicitly off.
2. Run the full conformance test suite. Anything that breaks is
   a regression in one of R27-R33; fix in place.
3. Run Mario at `--profile-all`. Confirm display now shows
   `z3-fz✓ jit✓` and the per-tick solve time drops by ≥90%.
4. Verify pixel-for-pixel output matches the slow-path run
   (write a test that diffs the SDL_RenderPresent calls).

**Acceptance**: Mario runs at ≥45 fps. `--profile-z3` shows
display's check() calls drop from 241 to 0 (the slow path never
runs for display).

---

### Round 35 — Apply to the other 3 Mario FSMs

`keyboard`, `game`, `level_gen` use mostly the same primitives.
After R34, they should be in scope.

Likely needs:
- `level_gen`'s `Implies` body items (`state.step = 0 ⇒
  InitGameState`) — convert to guarded equality in extract.
- `game`'s ∀-over-Seq with `_var` arithmetic in the body.

These are smaller deltas individually but together produce the
full Mario speedup.

**Acceptance**: Mario hits its plan target of ≤5ms/tick on at
least one FSM body. Overall fps ≥45.

## Order rationale

R27 first: nothing else runs if build_cache exits. R28 next:
the gate has to accept display before any JIT work matters.
R29-R33 are pure JIT codegen extensions and can be done in any
order; the order above matches Mario's body top-down (frame
arithmetic before draw calls before final concat). R34 is the
integration test. R35 is the rollout to other FSMs.

## What this plan DOESN'T fix

- **Multi-output FSMs that genuinely need Z3 search.** If a body
  has `∀ ... : ∃ ...` or relations the function-izer can't
  decompose, we still fall back to Z3. Mario isn't this; most
  programs aren't.
- **Effects whose payload involves another FSM's output.**
  Round 33 handles cross-step references WITHIN one solve. Cross-
  FSM (e.g. `display` reading `game`'s output via world.X) is
  already handled by the scheduler's `given` flow.
- **Dynamic Seq lengths.** Mario pins `#plat_effs = 4` etc.
  explicitly. A body with `effects ∈ Seq(Effect); #effects = n`
  where n comes from input is harder — the JIT'd function
  needs variable-size Vec construction at runtime. Not in scope
  for Mario.

## Risk: order-of-translation differences

R27 is the riskiest because it's diagnosis. If it turns out
that the slow path ALSO has a translator gap but silently skips
the constraint (vs my function-izer's fatal-exit), the fix
becomes "make the function-izer skip the same way" which is
less satisfying but quick. If the slow path actually succeeds
where my function-izer fails, the fix is in the function-izer's
build_cache invocation order, which is mechanical.

## Risk: the JIT-cache invalidation

Mario reruns the same FSMs 241 times with different given
values (world state evolves per tick). After R34, the JIT-
compiled function should be CALLED 241 times, not re-built.
The current cache is keyed on `(name, given_keys)` which is
stable per FSM. Compile cost lands on tick 0 only.

Risk: if the given_keys vary across ticks (e.g. tick 0 has no
`_var` reads because is_first_tick), the cache misses and we
re-compile. R34 should also verify the cache key stays stable
across the run.

## How to validate progress per round

Each round MUST land with:
1. A new test fixture in `runtime/tests/` that exercises the
   specific feature being added.
2. A short doc in `docs/plans/compile-constraints-to-programs/`
   noting what changed and any Mario-specific observations.
3. `./test.sh` green.
4. The previous round's tests still passing.

Run `--profile-all` on Mario after each round and snapshot the
output into the round doc, so we can see the progression.

## Estimated timeline

- R27: 1-3 days (diagnosis dominates; fix likely small).
- R28: 1 day (recurse a pass that already exists).
- R29: 2-3 days (multi-arg ABI design).
- R30: 1-2 days (Cranelift branches are well-trodden).
- R31: 1 day (straightforward IR).
- R32: 2-3 days (field-of-index resolution).
- R33: 1-2 days.
- R34: 1-2 days integration + bench.
- R35: 1-3 days per remaining FSM.

Total: ~2-4 weeks of focused work depending on what surprises
R27 surfaces.
