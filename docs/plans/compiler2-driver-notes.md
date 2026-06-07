# compiler2 driver skeleton (P3a + P3b + P3c) — notes

Status: P3a LANDED — both acceptance fixtures compile + run green
(see Acceptance below). P3b LANDED — the census
arithmetic/comparison/membership class and the implies class flip
green through the driver (see P3b acceptance below). P3c LANDED —
the bounded shape-enumeration parsers are replaced by a Pratt
parser FSM (see P3c below).

`compiler2/driver.ev` is the first end-to-end compiler2 driver: an
Evident program (oracle-compiled to a ~209 KB .smt2) that the kernel
runs to compile a small `.ev` file by BUILDING Z3 ASTs and asking the
solver to serialize them — no SMT-LIB text concatenation anywhere in
the constraint path.

## Architecture

```
stdin: flat-path \n claim \n        (wave-4o protocol, same as compiler.smt2)
  ↓ tick 0-1  ReadLine ×2, ReadFile
  ↓ ZINIT     ticks 2..32 — one libcall per tick:
              config/context/solver/int·bool sorts/512-byte arena,
              Effect datatype (Exit-only) via translate2_ctor.ev's
              step claims (VariantNameSymStep/VariantRecognizerSymStep/
              VariantFieldSymStep/EnumSortSymStep/FieldSortSlot/
              VariantMkConstructorStep/VariantQueryStep),
              `effects : (Array Int Effect)` + `effects__len : Int`
              consts, cached 0/1 numerals.
  ↓ LEX       the fossil's consolidated lexer FSM (compiler/compiler.ev),
              copied verbatim, gated to start after ZINIT. One token
              per tick + whitespace/comment bulk-skip.
  ↓ REVERSE   fossil's pop loop (reverse cons list → forward).
  ↓ PARSE     top-level dispatch: KwEnum and parametrized / non-target
              claims SKIPPED one token per tick; the target bare-head
              claim enters the walk.
  ↓ WALK      per body line:
              1. a PURE one-tick classifier + bounded expression parser
                 (C2ParseExpr) turn the line into a work-item program
                 (C2Items — the line's build plan);
              2. one item micro-step per tick, ≤1 libcall per tick.
              Work items:
                C2Process(e)   expand EBinOp/ETernary onto the stack;
                               EInt → Z3_mk_int (AtomBuildZ3);
                               EIdent → symtab handle push (pure)
                C2Op(op)       combine top-2 handles: cmp/eq/impl →
                               BoolCmpBuildZ3; +,-,* → 2-slot args
                               buffer + ArithBinopBuildZ3; ∧,∨ →
                               BoolNaryBuildZ3; / → binary mk_div
                C2Ite          top-3 → TernaryBuildZ3
                C2DeclConst    mk_string_symbol → mk_const → symtab
                C2NatBound     (>= h 0) via BoolCmpBuildZ3(OpGeq)
                C2PinEq        (= decl_h rhs_h)
                C2AssertTop    Z3_solver_assert(top), pop
                C2Drop         pure pop
                C2ExitApp      CtorArgWriteStep + CtorAppStep on the
                               harvested Exit decl
                C2SelectLen    (= (select effects 0) app) + (= effects__len 1)
              Handle plumbing: a builder's result is read from
              last_results on the NEXT tick and applied before that
              tick's own step (`pend` ∈ {none, push, tmp, decl}).
  ↓ EMIT      Z3_solver_to_string → __cstr.copy → assemble:
              manifest header (text) + Result/last_results prelude
              (text, fossil shape) + is_first_tick decl (text) +
              the solver's serialization → puts → Exit(0)
```

## Key empirical findings

1. **`Z3_solver_to_string` serializes `declare-datatypes` AND
   `declare-fun` lines, then the asserts** — verified by kernel-run
   probe. So the emitted unit needs NO declaration tracking: every
   const and datatype the asserts mention is declared by Z3 itself,
   in kernel-parseable SMT-LIB. The only textual parts are the
   manifest header (the kernel's wire contract, not part of the
   model) and the Result/last_results/is_first_tick prelude (which
   the build context never mentions).
2. **The kernel matches Effect variants BY NAME at dispatch time**
   (kernel/src/tick.rs), so a build-context Effect enum carrying only
   the variants the program actually constructs (here: Exit) yields a
   runnable unit. A hand-assembled unit with Exit-only Effect ran
   exit-1 under the kernel before the driver was built.
3. **Oracle gap:** an enum-constructor literal in a slot binding
   (`BoolCmpBuildZ3(op ↦ OpGeq, …)`) breaks the callee's `matches`
   translation ("dropped constraint"). Workaround: pin a body var
   (`d_op_geq ∈ Op = OpGeq`) and bind that. Probed + minimized.
4. The plain (non-rc) Z3 context means NO per-handle inc_ref ceremony
   is needed — ASTs live as long as the context (ctor_fixture.ev
   already proved this; the driver follows it).

## Scope (what the skeleton compiles)

- scalar memberships `x ∈ Int|Nat|Bool`, optionally `= <expr>` pin or
  a single `<,>,≤,≥` bound; Nat ⇒ Int sort + `(>= x 0)`.
- bare assert lines `name = <expr>`.
- `effects ∈ Seq(Effect) = ⟨Exit(<expr>)⟩` — single-element literal.
- `<expr>` (C2ParseExpr, one tick, pure): atom · `atom op atom` ·
  `atom ? atom : atom` · `( atom op atom )` ·
  `( atom op atom ∧|∨ atom op atom )` · either paren form `? atom : atom`.
  Build depth behind a handle is unbounded (the work stack recurses);
  it is the PARSER that is bounded.

## P3b widening (landed 2026-06-07)

New surface on top of the P3a skeleton:

- **Standalone constraint lines** via the new one-tick bounded
  parser `C2ParseCons` (+ `C2OpClass`): `x < 5`, `x ≠ 0`,
  chained `0 < x < 5` (lowered to a conjunction), implies lines
  `cmp ⇒ cmp`, `name ⇒ name op atom`, `¬name ⇒ name op atom`,
  and the nested right-assoc `cmp ⇒ ( cmp ) ⇒ cmp`. The classifier
  gained a fourth line kind (`c_is_cons`); claim-end detection now
  admits lines starting with IntLit/¬.
- **OpNeq lowering**: `≠` Process-expands to
  `Process(l) · Process(r) · C2Op(OpEq) · C2Not` — the documented
  two-step (mk_eq then mk_not). New `C2Not` work item (pops 1,
  pushes the mk_not handle); `ENot` expands through it too.
- **true/false literals**: lexer keywords KwTrue/KwFalse become
  EIdent("true"/"false") atoms; the symtab lookup resolves them to
  Z3_mk_true / Z3_mk_false handles cached at ZINIT (zsteps 30/31;
  the LEX gate moved 30 → 32).
- **Negative int literals**: token-level sign fold in the lexer
  cons — a Minus directly under a finishing IntLit with no
  atom-shaped token before it (IntLit/Ident/RParen/StringLit)
  becomes IntLit(-n). `10 - 13` stays a binary sub; `= -3`,
  `∧ -3`, `(-3` fold.
- **C2ParseExpr nested-group shape**
  `( ( a op a ) op a ∧|∨ a op a )` — covers 018's pin.

## P3b acceptance (run 2026-06-07, oracle-built driver, all 18 in parallel)

Every row below ran BOTH checks (smt2-contains on the emitted unit +
kernel run of the unit against expected/exit). All census FAILs
before; all PASS through the driver now:

017 ✓ (exit 0) · 018 ✓ ((- 10 13), (- 3) numerals; exit 0) ·
020 ✓ ((= flag true) via mk_true; exit 0) · 022 ✓ ((not (= x 0));
exit 0) · 023 ✓ · 024 ✓ · 025 ✓ · 027 ✓ (UNSAT → exit 2) ·
028 ✓ ((and (< 0 x) (< x 5))) · 029 ✓ (UNSAT → exit 2) ·
033 ✓ ((=> (> x 3) (< x 10))) · 034 ✓ · 036 ✓ · 037 ✓
((=> (> x 3) (=> (< x 10) (= y 99))) — right-assoc) · 053 ✓
((=> flag (= x 42))) · 054 ✓ ((=> (not flag) (= x 99))).

Regression gates re-verified in the same batch: 026 ✓ (exit 0,
`(+ ` present) and 008 ✓ (exit 1, `(and` present).

Census semantics note: an implies BLOCK (036) is compiled greedily —
`x > 3 ⇒ │ y = 10 │ x < 100` becomes `(x>3 ⇒ y=10) ∧ (x<100)`, not
`x>3 ⇒ (y=10 ∧ x<100)` — the lexer drops newline/indent structure.
Equivalent for 036's pinned model; NOT equivalent in general.
Real block scoping needs indentation-aware lexing (descoped below).

## P3c — Pratt parser FSM (landed 2026-06-07)

The shape zoo (C2ParseExpr's 8 shapes + C2ParseCons's 7 line forms +
C2OpClass) is DELETED — ~440 lines of bounded enumeration replaced by
a precedence-climbing shunting-yard FSM. driver.smt2 shrank ~209 KB →
~148 KB.

Architecture:

- `C2PrattStep` (compiler2/driver.ev) — a pure step claim: state
  in/out is (toks, operand stack ExprList, operator stack PrOps,
  expect-operand flag, paren depth, `?`-pending depth). Each
  invocation performs exactly ONE action: shift (atom / `(` / `¬` /
  binary op / `?`), reduce one operator onto the operand stack,
  close a paren group, swap PrQuest→PrTern on `:`, or report done.
  The driver runs it once per tick under the new `pmode = 3`;
  tests/kernel/compiler2/pratt_fixture.ev drives the same claim
  standalone.
- The line classifier now only picks the line KIND: plain/bounded
  memberships still build items directly in one tick; everything
  carrying an expression (membership pins, the effects literal's
  Exit argument, and ALL standalone constraint / bare-equality
  lines — formerly C2ParseCons's seven shapes plus the bare-line
  path) enters the Pratt FSM with `(pk_kind, pk_name, pk_sc,
  pk_nat)` latched. On done, the parsed Expr becomes the line's
  work-item program (the walker is unchanged — it already recursed
  to arbitrary depth; only the PARSER was bounded).
- An expression ends at the first token that cannot extend it
  (the next body line's head, the effects literal's closing
  `) ⟩`, EOF). Lexed newlines don't exist, so implies-BLOCK
  greediness is unchanged from P3b (see census semantics note).

Precedence ladder — probed empirically against the frozen oracle
(`scripts/build-oracle.sh` binary), NOT taken from CLAUDE.md:

    ⇒ 1 (right-assoc) · ?: 2 (right) · ∨ 3 · ∧ 4 ·
    < > ≤ ≥ = ≠ all at 5 (the chain level) · + - 6 · * / 7 · ¬ 8

Oracle findings that contradict/extend the documented table:

1. **CLAUDE.md's footgun "⇒ binds tighter than ∧" is empirically
   false for the legacy parser**: `a ∧ b ⇒ c` → `(=> (and a b) c)`
   and `a ⇒ b ∧ c` → `(=> a (and b c))`. ⇒ is the LOOSEST binary
   operator (and right-assoc: `a ⇒ b ⇒ c` → `(=> a (=> b c))`).
   The driver follows the oracle.
2. **`=` is a chain-comparison member**: `flag = x < 5` →
   `(and (= flag x) (< x 5))`, `0 < x = 5` → `(and (< 0 x) (= x 5))`.
   A run of chain ops lowers to a conjunction of consecutive pairs,
   duplicating the shared middle operand (`ok = x + 1 < y` →
   `(and (= ok (+ x 1)) (< (+ x 1) y))` — verified oracle behavior).
   ≠ chains too.
3. **Ternary sits between ⇒ and ∨**: `a ⇒ b ? c : d` →
   `(=> a (ite b c d))`, `a ∧ b ? c : d` → `(ite (and a b) c d)`,
   `a ? b : c ∧ d` → `(ite a b (and c d))`; the else branch
   right-nests (`a ? b : c ? d : e`).

Known divergences from the fossil/oracle (semantics-preserving):

- 3+-element chains emit nested 2-ary conjunctions
  (`(and (and a b) c)`) where the oracle emits flat `(and a b c)`;
  ∧/∨ likewise build 2-ary nested nodes (BoolNaryBuildZ3 with 2
  args per reduce step).
- Compound membership PINS parse the full RHS as one expression:
  `ok ∈ Bool = ¬a ∧ b` pins `ok = (¬a ∧ b)`, whereas the oracle's
  bare-line chain rule would read `ok = ¬a ∧ b` (as a constraint
  line) as `(and (= ok ¬a) b)`. Fixture surface always
  parenthesizes compound pins, so nothing observable changes.
- A malformed line (token that can't start an operand) completes
  with ENoExpr; the driver drops the line and skips one token to
  keep the walk progressing (the shape zoo claim-ended instead).

## P3c acceptance (run 2026-06-07, oracle-built driver, all parallel)

Every row ran BOTH checks (smt2-contains on the emitted unit +
kernel run of the emitted unit against expected/exit).

Regressions — all six P3a/P3b gates stay green through the Pratt
path:

| fixture | exit got/want | emitted shape spot-check |
|---|---|---|
| 026-arithmetic-add | 0/0 ✓ | `(= y (+ x 2))` |
| 008-boolean-and | 1/1 ✓ | `(Exit (ite (and (> 3 0) (< 3 10)) 1 0))` |
| 018-int-membership-negative | 0/0 ✓ | `(= ok (and (= (- 10 13) (- 3)) (< (- 3) 0)))` |
| 028-chained-comparison | 0/0 ✓ | `(and (< 0 x) (< x 5))` |
| 037-nested-implies-block | 0/0 ✓ | `(=> (> x 3) (=> (< x 10) (= y 99)))` (right-assoc) |
| 053-bool-as-constraint | 0/0 ✓ | `(=> flag (= x 42))` |

Newly green — census fixtures the shape zoo could NOT parse:

| fixture | exit got/want | emitted shape |
|---|---|---|
| 030-logic-and | 0/0 ✓ | standalone `(and (> x 0) (< x 10))` |
| 031-logic-or | 0/0 ✓ | bare-eq disjunction `(or (= x 1) (= x 2))` |
| 032-logic-not | 0/0 ✓ | `(= b false)` + pin `(= ok (not b))` |
| 004-comparison-ternary | 1/1 ✓ | unparenthesized `(ite (< 3 5) 1 0)` |

036-implies-block (P3b green) re-verified through the new path: 0/0 ✓.

Negative control re-verified: compiling 026 with claim name
`nonexistent_claim` emits only manifest + textual prelude (zero
user asserts, no `(+ `), driver exit 0.

Parser unit fixture: tests/kernel/compiler2/pratt_fixture.ev —
drives C2PrattStep over `a + b * c < d ∧ ¬ ( e ⇒ f )` one action
per tick and string-compares the rendered AST against the canonical
`(and (< (+ a (* b c)) d) (not (=> e f)))`. Oracle-compiled,
kernel-run: prints exactly the canonical string, exit 0.

## Descopes (P3c+)

- **021-real-membership**: needs FloatLit collection in the driver's
  lexer FSM (currently `3.14` lexes as IntLit·Dot·IntLit and kills
  the walk), Z3_mk_real_sort at ZINIT, mk_numeral for the literal,
  and a Real branch in the decl/manifest paths. Cleanly separable;
  nothing else blocks on it.
- Indentation-aware blocks (see semantics note above).
- Effect enum floor: LibCall's `Seq(LibArg)` payload needs the
  multi-datatype sort registry (LibArg + __SeqOf_LibArg + Effect in
  one or three mk_datatypes batches) — translate2_ctor.ev documents
  the mechanics; the driver only declares Exit.
- User enum declarations are skipped, not translated.
- General expressions are now COVERED (P3c Pratt FSM) — still
  missing: match/matches, Seq values, quantifiers, composition
  lines, string pins/atoms, multi-name memberships, chained bounds
  in memberships (`0 < x ∈ Int < 5`), and ≠ as a membership bound
  (would mis-assert mk_eq — C2Op(OpNeq) only arrives via the
  Process-expansion path, which lowers it correctly).
- Symbol table: 8 fixed slots.
- No `_<name>` carry-over declares in the emitted unit and no
  `(assert (>= effects__len 0))` floor — fine for tick-0-exit
  fixtures; multi-tick user programs need them.
- `effects` literal: exactly one Exit element.
- max-effects emitted as the fixed 16 (bootstrap's cap), not derived.

## How to build + run

```
evident-oracle emit compiler2/driver.ev driver_main -o driver.smt2   # repo root
TD=$(mktemp -d)
scripts/flatten-evident.sh tests/conformance/features/026-arithmetic-add/source.ev > $TD/flat.ev
printf '%s\nmain\n' "$TD/flat.ev" | kernel/target/release/kernel driver.smt2 > $TD/out.smt2
kernel/target/release/kernel $TD/out.smt2   # expect exit 0
```

## Acceptance (run 2026-06-07, kernel @ this commit, oracle-built driver.smt2)

Both fixtures were CENSUS FAILURES under the fossil — these are
compiler2's first wins.

- **026-arithmetic-add: PASS.** ~6,857 ticks / ~4.6 min wall.
  Emitted unit (947 bytes) contains `(+ ` (census check) with the
  full compound pin intact — `(assert (= y (+ x 2)))` — plus the Nat
  bounds, the bool pin `(= ok (= y 5))`, and the effects select/len
  asserts. Kernel run of the emitted unit: exit 0 (expected 0).
- **008-boolean-and: PASS.** ~6,777 ticks / ~4.6 min wall.
  Emitted unit contains `(and`, `(> 3 0)`, `(< 3 10)` (census
  checks) inside `(Exit (ite (and (> 3 0) (< 3 10)) 1 0))`. Kernel
  run: exit 1 (expected 1).
- **Negative control: PASS.** Compiling the 026 source with claim
  name `nonexistent_claim` emits ONLY the manifest + textual prelude —
  no user asserts, no `(+ ` — with the driver itself exiting 0 (every
  claim skipped, empty solver serialization).

Tick budget: ~30 ZINIT + ~5,400 LEX + ~2×#tokens REVERSE/SKIP +
~40-60 WALK + 3 EMIT. Per-tick ~41 ms (functionizer-residual FSM
shapes — the known P2 finding; perf-only).

- tests/kernel/compiler2/solver_emit_fixture.ev — the emit-backbone
  fixture (finding 1 + 2 pinned executable): oracle-compiled,
  kernel-run exit 0.

## Next steps

- Full Effect floor (sort registry), user enums via the ctor
  steps, the rest of the membership surface. (P3c's Pratt FSM
  closed the expression-parser gap.)
- Tick-rate: measure; if the walk dominates, batch pure ticks
  (classify + expansion) into the libcall ticks.
- Census: run the full conformance suite under a driver-backed
  compile path once coverage is wide enough to be interesting.
