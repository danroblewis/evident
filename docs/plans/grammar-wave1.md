# compiler.ev grammar coverage — wave 1

Status: **landed.** Extends `compiler/compiler.ev` from the single
`x ∈ Int = 5` MVP to N memberships with literal- or arithmetic-pin RHS.

## What landed

Two grammar shapes, both lexed+parsed from a real source string (nothing
hardcoded as tokens/AST):

1. **Multi-membership claim body** — `claim foo\n x ∈ Int\n y ∈ Int` →
   one `(declare-fun)` per membership.
2. **Expression pins** — `x ∈ Int = 1` (literal) and `y ∈ Int = x + 2`
   (one-level integer arithmetic) → `(assert (= x 1))`,
   `(assert (= y (+ x 2)))`.

Out of scope (later waves): enum / match / quantifier / subclaim / `..`
passthrough / Seq state-carry / generics.

## How the MVP generalised

The MVP parsed the whole token list by peeling **fixed positions** in a
single emit tick — only works for one membership of a known width. The
generalised driver is a **four-phase FSM** keyed off a carried `phase`
Int:

| phase | name    | per-tick action |
| ----- | ------- | --------------- |
| 0     | LEX     | consolidated-lexer; prepend Tokens to a reverse-order cons list (`_tokens`), as before |
| 1     | REVERSE | pop reverse-list head, push onto `fwd` → forward (source) order |
| 2     | PARSE   | drop the 2-token `<kw> Ident` head, then walk `fwd` ONE MEMBERSHIP PER TICK via `MembershipStep`, accumulating `out` (body) + `fstr` (manifest fields) |
| 3     | EMIT    | assemble manifest + body, `puts`, `Exit(0)` |

The PARSE walk is the `test_translate_arith_recursive.ev` per-tick
work-stack pattern, but over a `TokenList` instead of a `WorkList`.

## New code

- `compiler/parse_body.ev` — `TLHd` / `TLTl` (TokenList head/tail) and
  `MembershipStep`: consumes one `Ident ∈ Ident [= pin]` group (3 / 5 / 7
  tokens) off the front of a forward token list and produces its
  declare-fun, optional assert, manifest field, and the dropped-tail
  `rest`. Pin RHS is translated by composing `translate_arith.ev`
  (`ArithExprSmtlib` for the binop form, `AtomSmtlib` for a bare atom).
- `compiler/compiler.ev` — rewritten to the four-phase FSM above
  (`§3`/`§6` of the MVP, generalised).
- `tests/kernel/test_compiler_driver_multi_member.ev` (shape 1, constant
  input) and `tests/kernel/test_compiler_driver_arith.ev` (shape 2).

## Gotcha discovered: claim composition leaks body-local names

Claim composition (`Claim(slot ↦ value)`) binds the callee's **first-line
/ explicitly-bound params**, but its **unbound body-local variables leak
into the caller's namespace and unify by name.** `ArithExprSmtlib` /
`AtomSmtlib` carry internal `is_plus` / `is_arith` / `op` / `rhs_str`; a
caller that declares any of those names silently forces an unsatisfiable
equality (our literal-pin `is_plus = false` vs the callee's EInt-fallback
`is_plus = true` → UNSAT on tick 0, no error, just an empty model). Fix:
all of `MembershipStep`'s arithmetic locals are `ms_`-prefixed. Anyone
composing the `translate_*` claims must avoid their internal names — this
is the same hazard the `TokenToOp` "vars-in-body" note in `parser.ev`
gestures at, but it bites for **body-locals**, not just enum-typed params.

## Verification

- `./test.sh`: all phases pass.
- Kernel tests: 76 (was 74), 0 failed, green under default /
  `EVIDENT_FUNCTIONIZE=0` / `EVIDENT_FUNCTIONIZE_JIT=1`.
- `compiler/compiler.ev` (reading `/tmp/compiler-input.ev`) emits the
  correct `.smt2` for shape 1, shape 2, AND the original MVP single
  membership — byte-for-byte the MVP output on the MVP input.
- Functionizer line: all-residual (`0 JIT / 0 interp`) — the string-heavy
  `match`/ternary shapes don't extract, same class as the MVP driver and
  the printing-FSM refusals noted elsewhere. Runs correctly; not silently
  broken.

## No frozen files touched

No `bootstrap/`, no `kernel/`, no `stdlib/`, no Python. `lexer.ev` /
`parser.ev` core enums untouched (the wave-1 grammar needed no new AST
variant — `Token`, `Op`, `Expr`, `BodyItem` already covered it).
