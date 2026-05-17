# Round 7 — Gate diagnostics + relax record-field types

**Outcome:** PASS. Added per-rejection diagnostics that tell us
WHICH gate rule each refusal hit. Relaxed `is_simple_record_rec`
to allow `Seq(T)` / `Set(T)` field types. Filtered `Var::EnumCtor`
out of var_names alongside `Var::EnumValue`. Mario's FSMs now fail
the gate for four DISTINCT, named reasons — making Round 8+ work
specifically attackable.

## Diagnostics

`functionize::gate_diagnostics(schema, ...) -> Option<String>` —
parallel to `is_pure_assignment_body_xl` but returns `Some("why")`
instead of `false`. Mario's rejection trace under
`EVIDENT_FUNCTIONIZE_TRACE=1`:

```
[fz] display:    rejected by gate (Membership win∈SDL_Window)
[fz] game:       rejected by gate (Forall)
[fz] keyboard:   rejected by gate (Membership win∈SDL_Window)
[fz] level_gen:  rejected by gate (non-Eq Binary op Implies)
```

Four reasons, four distinct future Rounds.

## Relaxations applied

### Allow `Seq(T)` / `Set(T)` field types in records

`is_simple_record_rec` previously rejected any field whose type
wasn't primitive or a simple record. World has
`enemies ∈ Seq(Mover)` and `plat_x ∈ Seq(Int)`. Now allowed when
T is itself a simple type or an enum (deferred to runtime soundness).
This makes `world ∈ World` pass the gate, which was the blocker
for all four of Mario's FSMs.

### Filter `Var::EnumCtor` like `Var::EnumValue`

`populate_enum_variants` adds both `Var::EnumValue` (for nullary
variants like `Init`, `Done`) and `Var::EnumCtor` (for variants
with payloads like `Ok(Int)`, `Cons(Int, List)`). Round 2 filtered
only EnumValue; this round also filters EnumCtor. Without this,
the SDL_Window classification was polluted with all FFIArg /
Effect / Result variant names showing as "non-functional 1-var
components" — noise that hid the real classification.

## Updated Mario rejection breakdown

After the relaxations:

| FSM | Gate verdict | Reason |
|---|---|---|
| `display` | rejected | `win ∈ SDL_Window` (FFI bridge — won't compile) |
| `game` | rejected | `Forall` (∀ over coindexed iteration) |
| `keyboard` | rejected | `win ∈ SDL_Window` (same FFI bridge) |
| `level_gen` | rejected | `Implies` (`level_idx = 0 ⇒ Level1`) |

SDL_Window is intentional — those FSMs do FFI window manipulation,
which Z3 has to handle. Game's ∀ and level_gen's ⇒ are both real
expansion targets.

## Round 8 candidates

The four FSMs reject for different reasons. The two we can
reasonably attack:

1. **`∀ x ∈ {lo..hi}` unrolling** (game FSM, biggest unlock):
   - Gate: accept `Forall(var, Range(lo, hi), body)` when bounds
     are literal Int.
   - Extract: unroll body N times with the loop var substituted.
   - Eval: same chain logic but with N×size more steps.
2. **`a ⇒ b` (Implies) as conditional assignment** (level_gen):
   - Gate: accept top-level `BinOp::Implies(cond, body)` when
     body is itself an Eq.
   - Extract: each `cond ⇒ var = expr` becomes a substitution
     `var = (cond ? expr : <leave free>)` — but "leave free" is
     fragile.
   - Better: collect implication branches as a Ternary chain.

Of the two, ∀-unrolling is more general (the pattern shows up
across the codebase, not just Mario). Round 8 picks ∀.
