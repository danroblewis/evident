# sample.ev compile crash — diagnosis (Z3_mk_ite null operand)

Status: **root-caused 2026-06-08.** Found via `EVIDENT_EFFECT_TRACE`
(kernel commit `043ca68`). This is the blocker for task #24
(self-hosted compile of `sample.ev`).

## Symptom

Compiling `sample.ev` with the self-hosted driver (`kernel + c2_driver.smt2`)
segfaults at tick 142,698. gdb backtrace:

```
#2  ast_manager::mk_app(int, int, expr*, expr*, expr*)   ← Z3 derefs a null expr*
#3  Z3_mk_ite ()
#4/#5 libffi.so.8
#6  kernel::libcall::call          ← driver-dispatched LibCall
#7  kernel::tick::run_inner
```

The fatal effect (last `[eff]` before the fault):

```
[eff 227550] libz3::Z3_mk_ite([ctx, cond=…503888, then=Int(0), else=Int(0)])
```

`then` and `else` are **null `Z3_ast` handles (0)**. Z3 dereferences null
in `mk_app` → SIGSEGV.

## Mechanism (handle-stack underflow at C2Ite)

A ternary is compiled as a work-item program (`compiler2/driver.ev`,
line ~5382): `Process(cond), Process(then), Process(else), C2Ite`. The
`C2Ite` step calls `TernaryBuildZ3` (`compiler2/translate2_bool.ev:145`)
with `cond_h↦d_h_3rd, then_h↦d_h_2nd, else_h↦d_h_top` — i.e. it expects
the three operand handles to be the top three of the handle stack.

At the crash tick the handle stack was `d_h_top=0, d_h_2nd=0,
d_h_3rd=…503888`. So only the **cond** handle was present; the
**then/else** branch sub-expressions never pushed a handle.
`TernaryBuildZ3` is unconditional (no operand guard) so it passed the two
zeros straight to `Z3_mk_ite`.

## Source trigger

`sample.ev`'s char-recogniser (flattened lines ~460–479): a ~18-level
right-associative chained ternary mapping a char to a `Tok`/`Op` enum:

```evident
t = ( c = "↦" ? OpMapsto
    : c = "≤" ? OpLe
    : …
    : c = "∀" ? OpForall
    : c = "∃" ? OpExists
    : ErrTok(c) )
```

The crash fires at the deepest levels (`∀`/`∃`), where the innermost
ternary is `c = "∃" ? OpExists : ErrTok(c)` — **branches are enum
constructor expressions** (`OpExists` nullary, `ErrTok(c)` with a String
payload), not Z3-buildable arithmetic/bool expressions.

## What reproduces and what doesn't

Run through the self-hosted driver (`printf '/path.ev\nmain\n' |
EVIDENT_EFFECT_TRACE=1 kernel c2_driver.smt2`):

- **Repro A** — `result ∈ Int = (c = 1 ? 10 : c = 2 ? 20 : 30)` (chained
  ternary, Int cond, Int branches): **compiles, valid mk_ite operands.**
- **Repro B** — same with all-nullary enum branches (`OpA|OpB|OpC`):
  **compiles, valid operands.**
- **Repro C** — deep chain, String conds with multibyte literals, result
  enum with a **payload variant** `TErr(String)`, final else `TErr(c)`:
  **gets stuck (exit 1) at tick ~638 while building the `TErr(String)`
  datatype** (`Z3_query_constructor`) — never reaches the ternary.

So the trigger is an **interaction**, not a single shape: a payload-enum
datatype + a deep ternary chain whose branches are enum-constructor
expressions. The simple shapes each work; the combination is what
underflows the handle stack. (Repro C also surfaced a *second*,
adjacent problem in payload-enum datatype construction.) This
entanglement across subsystems is exactly why a one-line repro was
elusive — and why it motivated the `driver_main` decomposition
(`docs/plans/driver-subsystem-map.md`).

Repro files: `tests/seam/known-failing/repro_chain_{int,enum}.ev`
(pass) and `repro_deep.ev` (stuck) — kept as the regression targets.

## Root cause (hypothesis, high confidence)

In the `C2Ite` path, a ternary **branch** that is an enum-constructor
expression (`OpExists`, `ErrTok(c)`) does not get a `Process` step that
pushes a Z3 AST handle — so when `C2Ite` pops, the branch slots are 0.
Simple value branches (Int literal, nullary enum in a payload-free enum)
do push handles, which is why repros A/B pass.

## Fix recommendation

1. **Defensive guard (safety net, do first).** `TernaryBuildZ3` /
   the `C2Ite` step must refuse a 0 operand: emit a diagnostic + `Exit`
   instead of passing null to `Z3_mk_ite`. A compiler must not segfault
   on an unsupported construct — it must error nameably. Cheap, local,
   and turns every future occurrence into a clean signal.

2. **Real fix (in the decomposition).** Ensure enum-constructor branch
   expressions lower to a handle inside the ternary path. This belongs
   in the extracted, unit-testable ternary helper the `driver_main`
   decomposition is producing — with `repro_deep.ev` (reduced to the
   minimal crashing shape) as its unit test. Also chase the payload-enum
   datatype-build stick that Repro C exposed.
