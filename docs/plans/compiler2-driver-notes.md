# compiler2 driver skeleton (P3a + P3b + P3c + P3d + P3e) — notes

Status: P3a LANDED — both acceptance fixtures compile + run green
(see Acceptance below). P3b LANDED — the census
arithmetic/comparison/membership class and the implies class flip
green through the driver (see P3b acceptance below). P3c LANDED —
the bounded shape-enumeration parsers are replaced by a Pratt
parser FSM (see P3c below). P3d LANDED — the TokenList cons-list
lexer + REVERSE phase are replaced by the FTI token buffer
(compiler2/lex_fti.ev) and a cursor/window parse path (see P3d
below). P3e LANDED — user enum declarations + the full Effect
floor (LibCall/LibArg/__SeqOf_LibArg) + multi-element effects
literals + String memberships/`++` (see P3e below).

`compiler2/driver.ev` is the first end-to-end compiler2 driver: an
Evident program (oracle-compiled to a ~209 KB .smt2) that the kernel
runs to compile a small `.ev` file by BUILDING Z3 ASTs and asking the
solver to serialize them — no SMT-LIB text concatenation anywhere in
the constraint path.

## Architecture

```
stdin: flat-path \n claim \n        (wave-4o protocol, same as compiler.smt2)
  ↓ tick 0-1  ReadLine ×2, ReadFile
  ↓ ZINIT     one libcall per tick (P3e: zstep parks at 9 for the
              ED machine): config/context/solver/int·bool·string·real
              sorts/512-byte arena, the FULL Effect floor — LibArg,
              __SeqOf_LibArg, Effect(ReadLine/ReadFile/WriteFile/
              LibCall/Exit) — declared via translate2_ctor.ev's
              step claims through the generic ED FSM (see P3e),
              `effects : (Array Int Effect)` + `effects__len : Int`
              consts, cached 0/1/2 numerals.
  ↓ LEX       the fossil's per-char scanner FSM + compiler2/lex_fti.ev's
              LexFtiPlan (P3d): tokens land in a calloc'd FTI buffer
              (32 bytes/token, append order) via __mem.write_long;
              Ident/StringLit payloads are strdup'd char*s. Z3 lexer
              state = five Ints. One token per tick + bulk-skip.
  ↓ (REVERSE deleted in P3d — append order is source order; the
              lex_done tick flushes the pending string write + writes
              the EofTok sentinel, then phase 0 → 2 directly.)
  ↓ PARSE     top-level dispatch: parametrized / non-target claims
              and the RESERVED floor enums (Effect/Result/LibArg)
              SKIPPED one token per tick; a USER enum declaration
              enters the pmode-4 collection + ED run (P3e); the
              target bare-head claim enters the walk. All token
              access goes through an 8-token decoded window over
              the buffer (see P3d).
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
                C2PushH(h)     pure push of a known handle (P3e)
                C2App(d, n)    n CtorArgWriteSteps + CtorAppStep on a
                               harvested decl (n ≤ 3) — replaces the
                               P3a C2ExitApp (P3e)
                C2SelectEq(i)  (= (select effects i) top) — i ≤ 2
                C2LenEq(n)     (= effects__len n)
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
- ~~Effect enum floor~~ — LANDED in P3e (the ED machine declares
  LibArg + __SeqOf_LibArg + the full Effect in three sequential
  mk_datatypes runs; the driver registry resolves the cross-enum
  field sorts).
- ~~User enum declarations~~ — LANDED in P3e (one nullary user
  enum per compile; see the P3e section for the remaining gaps).
- General expressions are now COVERED (P3c Pratt FSM) — still
  missing: match/matches, Seq values, quantifiers, composition
  lines, string pins/atoms, multi-name memberships, chained bounds
  in memberships (`0 < x ∈ Int < 5`), and ≠ as a membership bound
  (would mis-assert mk_eq — C2Op(OpNeq) only arrives via the
  Process-expansion path, which lowers it correctly).
- Symbol table: 8 fixed slots.
- ~~No `_<name>` carry-over declares in the emitted unit~~ —
  FIXED in P3e (the missing declares crash the kernel's
  functionizer verify; see the P3e carry-over section). The
  `(assert (>= effects__len 0))` floor is still not emitted.
- ~~`effects` literal: exactly one Exit element~~ — widened in
  P3e (≤ 2 elements, LibCall + Exit).
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

## P3d — FTI lexer + cursor/window parse path (landed 2026-06-07)

The TokenList cons-list lexer and the REVERSE pop loop are DELETED.
The lexer writes tokens straight into a calloc'd FTI buffer
(compiler2/lex_fti.ev, proven by the P3-era spike) in append =
source order; the parse phases read the buffer through a bounded
decoded window. No unbounded TokenList is carried in Z3 state
anywhere in the driver any more.

Design (per docs/plans/fti-lexer-notes.md's integration recipe):

- **ZINIT** gained one step (zstep 32): `calloc(4096, 32)` — the
  token buffer, 4096 entries × 32 bytes. `tbase` captured at 33;
  the LEX gate moved 32 → 33. The buffer is `free`d on the Exit
  tick (strdup'd string payloads still leak — v1 stance).
- **LEX**: the fossil per-char scanner is verbatim; `SingleCharTok`
  became `LexCharTag` (same recognized set, Int tags); the cons
  push + tk_p1/tk_p2 negative-fold peeks are replaced by
  `LexFtiPlan` (fold decided from the cached last/prev tag Ints).
  Effects per tick: the 7 fixed kind shapes + the pending
  string-pointer write (strdup is always effects[0] of its tick).
  Z3 lexer state: `tbase, lx_count, lx_last, lx_prev, lx_pend` —
  five Ints.
- **REVERSE deleted**: `entering_parse` fires on the lex_done tick
  (phase 0 → 2), which also flushes a pending string write and
  writes the EofTok sentinel at entry lx_count.
- **FETCH / TOKEN WINDOW**: all parse modes read tokens through
  `wtoks`, a TokenList window of the 8 buffer entries starting at
  cursor `tcur` (decoded by the new `FtiTok` claim; entries past
  the sentinel read the calloc'd 0 and decode to EofTok, so "list
  nil" checks became `head matches EofTok`). A consumer acts only
  when the window covers its lookahead need (`w_need`: dispatch 3,
  skip 1, classifier 5, Pratt 1); otherwise a 3-tick refill burst
  runs: 16 `__mem.read_long` (8 tags + 8 payloads — within the
  manifest's max-effects = 16) → 8 slot-aligned `__cstr.copy` /
  getpid-filler effects (so slot i's string is last_results[i],
  no compaction indexing) → pure window rebuild. Consumption is
  cursor arithmetic: each action contributes its token count to
  `dcons` (claim-enter 2, skip 1, pin head 4, effects-literal head
  10, membership 3/5, Pratt shift 1, post-parse `) ⟩` 2), with the
  window advanced by the matching tail. C2PrattStep gained an
  explicit `scons` output slot (consumed-this-action) because
  composed-claim body locals are NOT referencable from host
  constraint expressions (probed: a host expression naming an
  inlined claim's body local drops the constraint).
- State-field delta (oracle manifest): TokenList-typed fields
  29 → 9, and every survivor is bounded ≤ 8 entries (window +
  tails + pr_ntoks). The unbounded carriers — `tokens`, `work`,
  `fwd`, `items`, `skipl`, `rem`, `p_toks` and their per-tick tail
  views — are gone. Total fields grew 354 → 440 (the window
  latches/views are many but all Int/Bool/bounded).

### P3d measurement (026-arithmetic-add, same flattened input, same kernel)

|                | ticks | wall    | z3 total | emitted unit |
|----------------|-------|---------|----------|--------------|
| before (P3c)   | 6,884 | ~347 s  | 340.6 s  | exit 0 ✓     |
| after (P3d)    | 6,190 | ~275 s  | 274.5 s  | identical bytes, exit 0 ✓ |

−694 ticks (the REVERSE pop loop was one tick per token; the fetch
bursts give some back) and −21 % wall. Mid-run per-tick z3 dropped
from ~100 ms to ~26 ms while token state was hot (the unbounded
TokenList pin strings are gone); the tail of the run converges to
the build-context cost, which dominates both before and after.
LEX remains the tick bulk (~5,400 of 6,190 — per-char scanning,
untouched by P3d).

### P3d acceptance (run 2026-06-07, oracle-built driver, all 22 in parallel)

Every row ran BOTH checks: smt2-contains on the emitted unit +
kernel run of the emitted unit vs expected exit. Driver compile
exit 0 on all rows.

| fixture | exit got/want | | fixture | exit got/want |
|---|---|---|---|---|
| 004-comparison-ternary | 1/1 ✓ | | 028-chained-comparison | 0/0 ✓ |
| 008-boolean-and | 1/1 ✓ | | 029-chained-comparison-unsat | 2/2 ✓ |
| 017-nat-membership | 0/0 ✓ | | 030-logic-and | 0/0 ✓ |
| 018-int-membership-negative | 0/0 ✓ | | 031-logic-or | 0/0 ✓ |
| 020-bool-membership | 0/0 ✓ | | 032-logic-not | 0/0 ✓ |
| 022-inequality | 0/0 ✓ | | 033-implies | 0/0 ✓ |
| 023-less-than | 0/0 ✓ | | 034-implies-vacuous | 0/0 ✓ |
| 024-greater-than | 0/0 ✓ | | 036-implies-block | 0/0 ✓ |
| 025-lte-gte | 0/0 ✓ | | 037-nested-implies-block | 0/0 ✓ |
| 026-arithmetic-add | 0/0 ✓ | | 053-bool-as-constraint | 0/0 ✓ |
| 027-arithmetic-unsat | 2/2 ✓ | | 054-not-bool-as-constraint | 0/0 ✓ |

Negative control re-verified: compiling the 026 source with claim
name `nonexistent_claim` emits only the manifest (empty
state-fields) + textual prelude — 1 assert (the last_results__len
floor), no `(+ ` — driver exit 0.

Parser unit fixtures re-verified post-signature-change:
pratt_fixture (scons slot added to its C2PrattStep binding) prints
the canonical AST and exits 0; lex_fti_fixture exits 0 unchanged.

### P3d descopes / notes

- The window refill always burns the copy tick even when no slot
  carries a string (simplicity; a skip-when-no-strings fast path
  would save ~1 tick per refill).
- Skip mode decodes full tokens (strings included) it only needs
  tags for; a tag-only fetch flavor would cut skip-phase refill
  cost but complicates window validity across the skip→dispatch
  handoff (the claim-name Ident must be decoded).
- Buffer capacity fixed at 4096 tokens, no realloc (flattened
  census inputs are ~700 tokens). Window reads past the sentinel
  rely on calloc zeroing — capacity must exceed final count + 8.
- The "faithfully-carried fossil quirks" list in
  docs/plans/fti-lexer-notes.md applies unchanged (digit-bearing
  idents, no string escapes, no FloatLit — the 021 descope).
  [UPDATE: digit-bearing idents + string escapes FIXED in the
  A1/A2 wave below; FloatLit remains the 021 descope.]

## P3e — user enums + the full Effect floor (landed 2026-06-07)

Two driver surfaces, one new machine.

### The ED machine (enum declaration FSM)

ONE FSM declares any enum — parsed `EnumDeclAst` data — in the
build context through translate2_ctor.ev's step claims
(VariantNameSymStep / VariantRecognizerSymStep / VariantFieldSymStep /
FieldSortSlot / SortRefsPack2 / VariantMkConstructorStep /
EnumSortSymStep / VariantQueryStep / CtorAppStep, plus the
EnumVariantsHead / VariantFieldCount / VariantFieldType peels),
then harvests each variant's ctor func_decl and (for nullary
variants) its mk_app value. Three acts:

- act 1 (declare, per variant): name sym → recog sym → field syms
  (0-3, skipped by arity) → ONE write-batch tick (≤ 8 effects:
  fnames + fsorts + packed srefs) → mk_constructor → write
  ctors[vidx].
- act 2 (finalize): sort sym → mk_constructor_list → write batch
  (sort name + clist) → mk_datatypes → read sort → capture.
- act 3 (harvest, per variant): read ctor handle → query_constructor
  → read ctor decl → capture decl (+ named floor-decl register
  latches) + mk_app for nullary → capture value (evt table / floor
  value latches).

It runs FOUR times per compile: three ZINIT runs — `LibArg`,
`__SeqOf_LibArg` (the self-referential cons, sort_refs path),
`Effect` with ALL FIVE floor variants — while zstep parks at 9,
and once per USER enum at parse time. Field-sort resolution is
FieldSortSlot for primitives/self plus a driver registry patch
(`Real` → mk_real_sort, `LibArg`/`Seq(LibArg)` → the harvested
floor sorts) — exactly the registry split translate2_ctor.ev's
header reserved for the driver. The emitted unit's serialization
now carries the same three declare-datatypes the fossil prelude
spelled textually (kernel-compatible by construction: the kernel
walks `__Cell_LibArg`/`__Empty_LibArg` by name, tick.rs
decode_libargs).

### User enum declarations (pmode 4)

`enum Color = Red | Green | Blue` at top level no longer skips: a
KwEnum head whose name is not reserved (`Effect`/`Result`/`LibArg`
— the floor enums every flattened source carries from
stdlib/kernel.ev) enters a collection mode that consumes
`Vname |`-pairs one tick each (nullary only; a `(` after a variant
name bails to the skip walk), then starts the ED machine with the
collected list. Harvested nullary values register in a 6-slot
enum-value table the symbol lookup falls through to, so `c = Red`
and `d ≠ North` build mk_eq over ctor-app handles. Enum-typed
memberships (`c ∈ Color`) declare consts of the harvested enum
SORT and land in the manifest as `c:Color` (non-primitive state
fields are manifest-legal; compiler.smt2's own manifest carries
Token-typed fields). The collection list is prepend-accumulated,
so the datatype declares variants in REVERSED source order —
harmless (declaration and harvest peel the same list; census
checks are substring checks).

### Effects literals (pmode 5) + the Effect floor at compile time

The single-Exit special case is gone. The classifier consumes the
8-token literal head `effects ∈ Seq ( Effect ) = ⟨` and enters an
ELEMENT walk:

- `Exit ( <expr> )` — the argument runs through the Pratt FSM
  (kind 3 now returns to the element walk, consuming only its `)`).
- `LibCall ( "lib" , "fn" , ⟨ ArgStr|ArgInt ( <lit-or-ident> ) ⟩ )`
  — a fixed two-bite parse (6 + 7 tokens; the 8-token window
  covers each bite). Its work-item program builds the value
  bottom-up through the generalized items: mk_string ×2, the arg
  (string literal / symtab ident / int), C2App(argstr_decl, 1),
  C2PushH(empty_val), C2App(cell_decl, 2), C2App(lc_decl, 3) —
  i.e. `(LibCall "libc" "puts" (__Cell_LibArg (ArgStr …)
  __Empty_LibArg))` as a HANDLE, no text.
- per element `C2SelectEq(i)` asserts `(= (select effects i) h)`;
  the closing `⟩` emits `C2LenEq(n)`. Elements are capped at 2
  (cached 0/1/2 numerals — the universal `⟨puts, Exit⟩` shape).

C2ExitApp/C2SelectLen are DELETED, replaced by the generic
C2PushH(h) / C2App(decl, n ≤ 3) / C2SelectEq(i) / C2LenEq(n)
items.

### Carry-over declares (new P3e finding — fossil parity restored)

The P3c stance "no `_<name>` carry-over decls — fine for
tick-0-exit fixtures" is WRONG once the kernel's functionizer can
extract the emitted unit: functionize VERIFIES against real Z3
solves of tick 0 AND tick 1, and the tick-1 solve pins
`(= _<name> <prev>)` unconditionally. With `_<name>` undeclared,
Z3's parse error escalates through the context's default error
handler and kills the kernel process (`Error: … unknown constant
_msg`, exit 1) — 005's unit hit this; earlier fixtures only
survived because their units refused extraction before the verify
step. The driver now appends one textual
`(declare-fun _<name> () <Type>)` per collected state field AFTER
the rendered solver body (an enum-typed carry needs its
declare-datatypes first; the kernel's declaration extraction is
order-preserving). This is what the fossil emits — compiler.smt2
carries 249 such lines.

### String surface (005)

- The lexer folds `+ +` into PlusPlus (tag 60) by next-char peek
  (same trick as the `--` comment), advancing 2; FtiTok decodes it;
  C2TokOp maps it to OpConcat at additive precedence.
- StringLit atoms parse to EStr; C2Process(EStr) is one
  Z3_mk_string tick; C2Op(OpConcat) reuses the 2-slot args-array
  path with Z3_mk_seq_concat → `(str.++ …)`.
- `msg ∈ String = …` declares a string-sort const, manifest
  `msg:String`.

### P3e acceptance (run 2026-06-07, oracle-built driver, parallel)

Every row ran BOTH checks: smt2-contains on the emitted unit +
kernel run of the unit vs expected exit (and expected stdout where
the fixture defines one). Driver compile exit 0 on all rows.

NEW targets:

| fixture | checks | result |
|---|---|---|
| 043-enum-declaration | `(Red)` in unit · run exit 0/0 · manifest `c:Color` | PASS |
| 044-enum-constraint | `(East)` in unit · run exit 0/0 | PASS |
| 002-string-literal-print | `"conformance"` + `LibCall` + `(Exit 0)` in unit · stdout `conformance` · exit 0/0 | PASS |
| 005-string-concat | `str.++` in unit · stdout `concat` · exit 0/0 | PASS |

Full 22-fixture regression table, re-run on the FINAL artifact
(after the carry-declare fix; smt2-contains green on every row):

| fixture | exit got/want | | fixture | exit got/want |
|---|---|---|---|---|
| 004-comparison-ternary | 1/1 ✓ | | 028-chained-comparison | 0/0 ✓ |
| 008-boolean-and | 1/1 ✓ | | 029-chained-comparison-unsat | 2/2 ✓ |
| 017-nat-membership | 0/0 ✓ | | 030-logic-and | 0/0 ✓ |
| 018-int-membership-negative | 0/0 ✓ | | 031-logic-or | 0/0 ✓ |
| 020-bool-membership | 0/0 ✓ | | 032-logic-not | 0/0 ✓ |
| 022-inequality | 0/0 ✓ | | 033-implies | 0/0 ✓ |
| 023-less-than | 0/0 ✓ | | 034-implies-vacuous | 0/0 ✓ |
| 024-greater-than | 0/0 ✓ | | 036-implies-block | 0/0 ✓ |
| 025-lte-gte | 0/0 ✓ | | 037-nested-implies-block | 0/0 ✓ |
| 026-arithmetic-add | 0/0 ✓ | | 053-bool-as-constraint | 0/0 ✓ |
| 027-arithmetic-unsat | 2/2 ✓ | | 054-not-bool-as-constraint | 0/0 ✓ |

Negative control re-verified on the final artifact: nonexistent
claim → manifest (empty state-fields) + textual prelude only,
1 assert (the last_results__len floor), no `(+ `, driver exit 0.
pratt_fixture re-verified post-change: prints the canonical AST,
exit 0. lex_fti_fixture / ctor_fixture untouched by this wave
(their imports didn't change).

Tick-budget note: ZINIT grew from ~33 to ~190 ticks (three ED
floor runs — declare + finalize + harvest over 10 variants — plus
string/real sorts and the 2 numeral); a user enum adds ~35
parse-time ticks. Census-fixture compile wall is unchanged in
character (~5-6 min serial; the per-tick functionizer-residual
cost and LEX still dominate, as in P3d).

### P3e descopes

- 006-enum-match needs `match` in the emit path (per-arm tester
  dispatch + accessor binds) — not in the driver; descoped.
- One user enum per compile; payloaded user variants bail to skip
  (the ED machine itself handles payloads — the floor enums use
  them — only the token-collection step is nullary-only).
- Effects literals: ≤ 2 elements; LibCall args: exactly one
  ArgStr/ArgInt (the puts shape). ⟨⟩ empty literals unsupported.
- User enums must precede the target claim in the flattened
  source (the walk needs the harvested sort/values; true for the
  corpus — stdlib precedes user code, enums precede claims).

## A1 + A2 — lexer correctness: digit-bearing idents + string escapes
## (gap census phase A, landed 2026-06-07)

The two census items gating the whole sample.ev corpus (A1: 2,159
digit-bearing ident occurrences; A2: 100 escape occurrences). Both
land in the HOST per-char scanner (driver.ev LEX section + the
lex_fti_fixture mirror); LexFtiPlan and the token encoding are
unchanged, so the classifier/parser saw no token-shape change.

Oracle behavior (probed against /usr/local/bin/evident-oracle AND
read from the pinned legacy lexer, c218dca^
bootstrap/runtime/src/lexer.rs):

1. **Ident class**: `is_ident_start` = ASCII alpha + `_`;
   `is_ident_continue` = ASCII alnum + `_`. So `t0`, `r_l1`,
   `cs_t2` are single Idents (runtime-confirmed: `t0 = 5,
   r_l1 = t0 + 1, cs_t2 = r_l1 + t0` samples to 5/6/11).
2. **`10x`**: the digit branch is matched before the ident branch
   and only consumes digits → **IntLit(10) + Ident("x")**, NOT a
   lex error (runtime-confirmed: `= 10x` produces the same parse
   error as `= 10 x`; `x10` is a single unbound Ident).
3. **Escapes**: exactly four — `\"` `\\` `\n` `\t` — each storing
   the TRANSLATED byte in the Str payload (runtime-confirmed:
   `#"a\tb" = 3` is sat). Any other escape is a LEX ERROR
   (`unknown escape \q`), and unterminated strings/escapes are lex
   errors. compiler/lexer.ev's EscapeChar table (lexer.ev:280)
   matches the four.

Scanner changes (mirrored in driver.ev + lex_fti_fixture.ev):

- A1: `is_alnum = is_alpha ∨ is_dig`; `str_continuing`/
  `str_finishing` test is_alnum (digits extend a collecting ident);
  `int_starting` gains `¬was_collecting_str` (a digit inside an
  ident never starts an int); `finish_int_only` gains an `is_alpha`
  arm (an int finishing at an alpha char pushes the IntLit — kind 2
  — and the ident starts the SAME tick, giving the oracle's
  `10x` → IntLit(10) + Ident("x") split). The negative fold is
  untouched: `t0 ∈ Int = -3` still folds (last=Minus, prev=Eq not
  atom-shaped) and `t0 - 3` stays binary (prev=Ident is atom).
- A2: a strlit gains an escape-pending mode: `esc_pend ∈ Bool`
  (one new Bool of Z3 state), armed by `\` (IsBackslash) when not
  already pending — that tick appends nothing — and consumed by the
  next char, which appends `EscapeChar(c)` (the translated byte) to
  partial_strlit. `strlit_closing` requires `¬_esc_pend`, so `\"`
  stays inside the literal; `\\` appends one backslash and disarms
  (the second `\` is the escaped char, not a new lead-in). Unknown
  escapes pass the char through — the driver has no lex-error
  channel (divergence from the oracle's hard error; the corpus
  only carries the four).
- The strdup-pending invariant ("two string-carrying tokens never
  finish on consecutive ticks") survives both: a kind-3/5 tick
  always resets/empties the collectors, so the next tick can only
  be kind 0/1 — pend flushes exactly as before.

Fixture: tests/kernel/compiler2/lex_fti_fixture.ev extended from
41 → 69 tokens (+ sentinel = 70 walked entries): `t0 ∈ Int = -3`
(fold interaction), `r_l1 = t0 + cs_t2`, `z = 10x` (the two-token
split), and all four escapes as StringLit payloads (`"a\tb"`,
`"c\nd"`, `"q\"r"`, `"s\\u"` — expectation strings carry the REAL
bytes via the fixture source's own escapes). Mismatch exit codes
unified to 100+k (the old 150+k/200+k bands collide past k=50).
Verified: oracle emit + kernel run exit 0; negative controls fire
exactly (k=59 tag 3→9 → exit 159; k=62 payload `c\nd`→`c\nX` →
exit 162).

Acceptance (run 2026-06-07, oracle-built driver, all parallel —
the full 26-fixture regression: the 22 P3d rows + 043, 044, 002,
005; every row BOTH checks: smt2-contains + kernel run of the
emitted unit vs expected exit/stdout):

| fixture | exit got/want | | fixture | exit got/want |
|---|---|---|---|---|
| 002-string-literal-print | 0/0 ✓ | | 028-chained-comparison | 0/0 ✓ |
| 004-comparison-ternary | 1/1 ✓ | | 029-chained-comparison-unsat | 2/2 ✓ |
| 005-string-concat | 0/0 ✓ | | 030-logic-and | 0/0 ✓ |
| 008-boolean-and | 1/1 ✓ | | 031-logic-or | 0/0 ✓ |
| 017-nat-membership | 0/0 ✓ | | 032-logic-not | 0/0 ✓ |
| 018-int-membership-negative | 0/0 ✓ | | 033-implies | 0/0 ✓ |
| 020-bool-membership | 0/0 ✓ | | 034-implies-vacuous | 0/0 ✓ |
| 022-inequality | 0/0 ✓ | | 036-implies-block | 0/0 ✓ |
| 023-less-than | 0/0 ✓ | | 037-nested-implies-block | 0/0 ✓ |
| 024-greater-than | 0/0 ✓ | | 043-enum-declaration | 0/0 ✓ |
| 025-lte-gte | 0/0 ✓ | | 044-enum-constraint | 0/0 ✓ |
| 026-arithmetic-add | 0/0 ✓ | | 053-bool-as-constraint | 0/0 ✓ |
| 027-arithmetic-unsat | 2/2 ✓ | | 054-not-bool-as-constraint | 0/0 ✓ |

Negative control re-verified on the new driver artifact:
nonexistent claim → manifest + textual prelude only, no user
asserts (1 floor assert, no `(+ `), driver exit 0. pratt_fixture
re-verified (imports driver.ev): canonical AST printed, exit 0.

End-to-end A1+A2 smoke through the FULL driver (beyond the lex
fixture): a source with `t0 ∈ Int = 5`, `r_l1 ∈ Int = t0 + 1` and
`msg ∈ String = "a\tb" ++ "\n" ++ "q\"r" ++ "s\\u"` piped through
driver.smt2 → emitted unit carries `(= t0 5)`,
`(= r_l1 (+ t0 1))`, manifest `t0:Int r_l1:Int ok:Bool msg:String`;
unit run exit 0 with stdout bytes exactly
`a TAB b NL q " r s \ u NL` — the translated escape bytes survive
lexer → mk_string → serialization → kernel puts.

## B1 + B2 — FTI symbol table at corpus scale + the String sort
## surface (gap census phase B, landed 2026-06-07)

### B1: the FTI symbol table (8 slots → 1024 entries)

The 8-slot flat symtab (st_n0..st_n7 / st_h0..st_h7, 32 Z3 state
fields + carries) is DELETED. The corpus's `claim main` binds 364
distinct idents; flat Z3-state slots at that scale would add ~730
manifest fields. The replacement is the FTI pattern split in two:

- **Handles** live in a calloc'd buffer (`st_base`, 1024 entries ×
  8 bytes, ZINIT zstep 21; freed on the Exit tick). A decl writes
  its const handle to `st_base[st_cnt]` via `__mem.write_long`.
- **Names** live in `st_names`, ONE Z3 String state field of
  fixed-width 32-byte records: `"|" ++ name padded to 31 with
  spaces`. `|` cannot occur inside a record (idents are
  alnum/underscore, padding is spaces), so every `index_of` match
  is 32-aligned, and lookup is pure arithmetic:
  `idx = index_of(st_names, key) / 32`.

DESIGN CHOICE vs the census note's "name-ptr + handle per entry,
linear scan": a pointer-per-name table makes *lookup* effectful and
O(n) — each probed entry costs a read_long + __cstr.copy tick pair,
and at corpus scale (364 entries, ~2,400 ident occurrences in
expressions) that extrapolates to millions of ticks. The fixed-width
name string keeps lookup at ONE pure index computation + ONE
`__mem.read_long` (through the existing pend=1 capture path),
independent of table size. The driver already carries the entire
source file as a Z3 String state field (`input`), so a ~12 KB names
string at corpus scale is consistent with existing state-size
practice — and it adds 3 state fields (st_base, st_cnt, st_names)
instead of multiplying manifest fields. Name records cap at 31
chars (longer would truncate-alias); the corpus max is 21
(`ms_rp_sfx_atom_is_int`).

Flow changes:

- `C2DeclConst` grew 2 → 3 steps: istep 0 mk_string_symbol
  (pend=2), istep 1 mk_const (pend=2 — was the pend=3 capture),
  istep 2 `write_long(st_base + 8*st_cnt, tmp)` with the handle
  pushed onto hstk, st_names appended, and st_cnt bumped PURELY on
  the same tick. pend=3 and the `pend_name` latch are deleted;
  `d_hstk_in` push is now pend=1 only. A decl is visible to every
  later tick's lookup (items run one per tick, so no same-tick
  lookup exists).
- `C2Process(EIdent)` splits: `true`/`false` (cached handles),
  enum-variant values (evt table), and unknown names (handle 0,
  fossil parity) stay PURE same-tick pushes; a name found in
  st_names runs `read_long(st_base + 8*idx)` with pend=1 — the
  handle pushes next tick, +1 tick per ident occurrence.
- ZINIT grew one step: zstep 21 callocs the handle buffer, st_base
  latches at 22, and the LEX gate moved `_zstep < 21` → `< 22`.

### B2: String as first-class state — verified, gate flipped

The P3e string surface (z_ssort at ZINIT, classifier sort class 3,
decl path string-sort consts, EStr → Z3_mk_string, `++` →
str.++, manifest `name:String` + `_name` carry declares) turns out
to already COVER the census B2 list; this wave's work was
verification + the acceptance gate, plus holding it through the B1
symtab rewrite:

- `s ∈ String` memberships: classifier c_sc=3 → string-sort const,
  manifest field `s:String`, carry `(declare-fun _s () String)`.
- String literal pins `s ∈ String = "lit"`: Pratt kind-1 over EStr.
- String equality / ≠: Z3_mk_eq is sort-generic (BoolCmpBuildZ3);
  ≠ lowers through the standard mk_eq + mk_not path.
- **019-string-membership: PASS** (the B2 gate) — emitted unit
  carries `(= s "hello")`, `(= ok (= s "hello"))`, manifest
  `s:String ok:Bool`, `_s` carry declare; unit run exit 0/0.
- B2 smoke beyond the gate: literal pin + `++` + `≠` + `=` in one
  claim (`a ∈ String = "lit"` · `b = a ++ "x"` ·
  `ne_ok = (b ≠ "nope")` …) compiles and runs exit 0 through the
  B1 driver.

### B1 acceptance: the symtab stress fixture

tests/kernel/compiler2/symtab_fixture.ev — a driver INPUT (not a
test_*.ev kernel fixture): one claim binding 41 distinct idents
(v00..v39 + ok) as a dependency chain `vNN ∈ Int = v(NN-1) + 1`,
with the last line resolving both the earliest and latest entries
(`ok ∈ Bool = ((v39 = 40) ∧ (v00 = 1))`). The emitted unit carries
the late pin `(= v39 (+ v38 1))` (and `(= v17 (+ v16 1))` etc. —
every pin past slot 8 of the old table); the chain pins force
v39 = 40, so the unit run exits 0 only if every lookup resolved
the RIGHT handle. Result: PASS (see acceptance below).

### B1+B2 acceptance (run 2026-06-07, oracle-built driver, all 27
### rows in parallel; every row BOTH checks: smt2-contains +
### kernel run of the emitted unit vs expected exit/stdout)

| fixture | exit got/want | | fixture | exit got/want |
|---|---|---|---|---|
| 002-string-literal-print | 0/0 ✓ | | 028-chained-comparison | 0/0 ✓ |
| 004-comparison-ternary | 1/1 ✓ | | 029-chained-comparison-unsat | 2/2 ✓ |
| 005-string-concat | 0/0 ✓ | | 030-logic-and | 0/0 ✓ |
| 008-boolean-and | 1/1 ✓ | | 031-logic-or | 0/0 ✓ |
| 017-nat-membership | 0/0 ✓ | | 032-logic-not | 0/0 ✓ |
| 018-int-membership-negative | 0/0 ✓ | | 033-implies | 0/0 ✓ |
| 019-string-membership | 0/0 ✓ NEW | | 034-implies-vacuous | 0/0 ✓ |
| 020-bool-membership | 0/0 ✓ | | 036-implies-block | 0/0 ✓ |
| 022-inequality | 0/0 ✓ | | 037-nested-implies-block | 0/0 ✓ |
| 023-less-than | 0/0 ✓ | | 043-enum-declaration | 0/0 ✓ |
| 024-greater-than | 0/0 ✓ | | 044-enum-constraint | 0/0 ✓ |
| 025-lte-gte | 0/0 ✓ | | 053-bool-as-constraint | 0/0 ✓ |
| 026-arithmetic-add | 0/0 ✓ | | 054-not-bool-as-constraint | 0/0 ✓ |
| 027-arithmetic-unsat | 2/2 ✓ | | | |

symtab_fixture (41 idents): driver exit 0; `(= v39 (+ v38 1))` in
the unit; unit run exit 0/0. Negative control re-verified:
nonexistent claim → manifest + textual prelude only, no user
asserts, driver exit 0. pratt_fixture re-verified (imports
driver.ev; C2PrattStep signature unchanged): canonical AST, exit 0.

## B3 — string builtins + call syntax (gap census phase B, landed 2026-06-07)

The corpus's function-call surface (`substr(s, i, j)`,
`str_from_int(n)`, `index_of(s, t[, off])`, `char_at(s, i)`,
`str_len(s)`, plus the bool trio `str_contains` / `starts_with` /
`ends_with`) and the `#` length prefix now compile through the
driver, dispatching to translate2_seq.ev's fixture-proven
StrOpBuildZ3 table.

### Call syntax: a PrCall floor marker in the Pratt FSM

DESIGN CHOICE — calls parse in C2PrattStep itself, NOT in the line
classifier. Calls occur at arbitrary expression depth
(`(index_of(s,"<") = 4 ∧ …)`, ternary Exit args), so a
classifier-level fixed shape would recreate the P3b shape zoo the
Pratt FSM deleted; the shunting-yard already owns the needed
machinery (floor markers PrLP/PrQuest, reduce-until-floor), so a
call costs one new floor marker + a comma rule + a close rule:

- **shift-call**: an `Ident` in OPERAND position whose next token
  is `(` pushes `PrCall(name, 0)` and consumes BOTH tokens — the
  FSM's only 2-token action; `scons` widened Bool → Int (0/1/2)
  and the pmode-3 lookahead need went 1 → 2 for the peek. The
  call's `(` bumps pd, so its `)` can never terminate the
  expression. An Ident shifted as a plain atom sets expop=false,
  so an Ident BEFORE `(` in operator position still ends the
  expression — the next-line-head termination rule is unchanged.
- **comma**: continues the expression only when cd > 0 (a new
  call-depth counter; at cd = 0 a `,` still terminates — the
  effects-element walk depends on that). Over a non-floor top it
  reduces (same loop as `)`); over the PrCall top it bumps the
  comma count and returns to operand state.
- **close**: `)` over the PrCall top pops k+1 operands (post-order:
  top = last arg) into ECall1/2/3(name, args…) — three fixed-arity
  Expr variants added in compiler/parser.ev (a 4-field payload is
  oracle-supported; probed end-to-end). No zero-arg calls exist in
  the surface, so argc = commas + 1.
- **`#`** is a prec-8 prefix op (PrHash, same slot as ¬) reducing
  to ECall1("str_len", e) — `#s` and `str_len(s)` are the same
  node from the walker's perspective. Per-sort dispatch (Seq →
  `__len` const) is moot until the driver has Seq-typed variables;
  every `#` operand today is String-sorted → str.len.

### Walker lowering (C2Process(ECallN) → items)

One new work item `C2StrOp(op_name, argc)`: a single tick that pops
argc handles and builds the node — `"len"` via BuildZ3MkSeqLength,
everything else via StrOpBuildZ3 (a/b/c = 3rd/2nd/top per arity).
The expansion table (the legacy string_ops.rs dispatch, oracle
re-probed):

| surface | lowering |
|---|---|
| `substr(s,i,j)` | P(s)·P(i)·P(j)·StrOp("substr",3) → str.substr |
| `char_at(s,i)` | P(s)·P(i)·StrOp("char_at",2) → str.at |
| `index_of(s,t)` | P(s)·P(t)·**C2PushH(z_zero)**·StrOp("indexof",3) — the oracle emits `(str.indexof s t 0)` |
| `index_of(s,t,off)` | P(s)·P(t)·P(off)·StrOp("indexof",3) |
| `str_len(s)` / `#e` | P(s)·StrOp("len",1) → str.len |
| `str_contains(s,t)` | P(s)·P(t)·StrOp("contains",2) |
| `starts_with(s,pre)` | P(pre)·P(s)·StrOp("prefixof",2) — args process SWAPPED so the stack matches (str.prefixof pre s) |
| `ends_with(s,suf)` | P(suf)·P(s)·StrOp("suffixof",2) |
| `str_from_int(e)` | the oracle's negative-safe composite: P(e)·P(0)·Op(≥)·P(e)·StrOp("int_to_str",1)·P("-")·P(0)·P(e)·Op(−)·StrOp("int_to_str",1)·Op(++)·C2Ite → `(ite (>= e 0) (str.from_int e) (str.++ "-" (str.from_int (- 0 e))))` |
| unknown name | C2PushH(0) — unbound-ident parity; args dropped unevaluated |

`replace(s,a,b)` is NOT lowered (StrOpBuildZ3 has no
Z3_mk_seq_replace row yet — zero corpus occurrences). `str_to_int`
has no legacy surface spelling; the StrOpBuildZ3 row stays unused.
`e` in the str_from_int composite is processed THREE times — pure
AST rebuild, Z3 hash-conses; the emitted text matches the oracle's
let-shared form semantically (substring checks see the inline
form).

### B3 acceptance (run 2026-06-07, oracle-built driver, all 38 rows
### parallel; every row BOTH checks: smt2-contains + kernel run of
### the emitted unit vs expected exit/stdout)

NEW — the string-class census fixtures, ALL ELEVEN green (the gate
asked for ≥ 4):

| fixture | emitted shape spot-check | exit got/want |
|---|---|---|
| 003-int-multiply-to-string | `(= msg (ite (>= (* 6 7) 0) (str.from_int (* 6 7)) (str.++ "-" (str.from_int (- 0 (* 6 7))))))` — the oracle composite, byte-for-byte modulo let-sharing; stdout `42` | 0/0 ✓ |
| 011-string-length | `(= n (str.len "hello"))` — `#` on a string LITERAL | 5/5 ✓ |
| 012-substring | `(= pre (str.substr "Edge<Rect>" 0 4))`; stdout `Edge` | 0/0 ✓ |
| 014-index-of | `(str.indexof "Edge<Rect>" "<" 0)` — 2-arg form, implicit 0 | 4/4 ✓ |
| 050-string-concat | 3-operand `++` standalone bare-eq line | 0/0 ✓ |
| 067-str-len-function | `(= n (str.len s))` AND `(= m (str.len s))` — `#s` and `str_len(s)` are the same node; manifest `s:String n:Int m:Int ok:Bool` | 0/0 ✓ |
| 068-substr-slice-var | `(str.substr s 0 4)` over a String VAR + eq pin | 0/0 ✓ |
| 069-substr-is-exact-unsat | substr + contradictory pin → UNSAT | 2/2 ✓ |
| 071-index-of-present-and-absent | three indexof calls in one ∧-chain incl. `(- 1)` (lex fold) | 0/0 ✓ |
| 072-index-of-with-offset | `(str.indexof s "." 2)` — explicit 3-arg form | 0/0 ✓ |
| 073-char-at | `(= ok (= (str.at s 1) "b"))` — call inside a paren eq | 0/0 ✓ |

Full 27-row regression (the B1+B2 acceptance list), re-run on the
SAME artifact — every row both checks green:

| fixture | exit got/want | | fixture | exit got/want |
|---|---|---|---|---|
| 002-string-literal-print | 0/0 ✓ | | 028-chained-comparison | 0/0 ✓ |
| 004-comparison-ternary | 1/1 ✓ | | 029-chained-comparison-unsat | 2/2 ✓ |
| 005-string-concat | 0/0 ✓ | | 030-logic-and | 0/0 ✓ |
| 008-boolean-and | 1/1 ✓ | | 031-logic-or | 0/0 ✓ |
| 017-nat-membership | 0/0 ✓ | | 032-logic-not | 0/0 ✓ |
| 018-int-membership-negative | 0/0 ✓ | | 033-implies | 0/0 ✓ |
| 019-string-membership | 0/0 ✓ | | 034-implies-vacuous | 0/0 ✓ |
| 020-bool-membership | 0/0 ✓ | | 036-implies-block | 0/0 ✓ |
| 022-inequality | 0/0 ✓ | | 037-nested-implies-block | 0/0 ✓ |
| 023-less-than | 0/0 ✓ | | 043-enum-declaration | 0/0 ✓ |
| 024-greater-than | 0/0 ✓ | | 044-enum-constraint | 0/0 ✓ |
| 025-lte-gte | 0/0 ✓ | | 053-bool-as-constraint | 0/0 ✓ |
| 026-arithmetic-add | 0/0 ✓ | | 054-not-bool-as-constraint | 0/0 ✓ |
| 027-arithmetic-unsat | 2/2 ✓ | | | |

Negative control re-verified on the new artifact: nonexistent
claim → manifest (empty state-fields) + textual prelude only,
exactly 1 assert (the last_results__len floor), no `(+ `, driver
exit 0. pratt_fixture re-verified post-signature-change (cd/ncd
threaded, scons Bool → Int): canonical AST printed, exit 0.
lex_fti_fixture re-verified: exit 0 (its imports gained the Expr
variants via parser.ev; no behavior change).

### B3 descopes / notes

- `#` on a Seq operand (`__len` const) is unreachable until the
  driver has Seq-typed variables; every `#` today is String-sorted
  → str.len. The per-sort split documented in translate2_seq.ev's
  CardBuildZ3 is the ready-made hook.
- `replace(s,a,b)` (str.replace) is not lowered — StrOpBuildZ3 has
  no Z3_mk_seq_replace row; zero corpus occurrences.
- A capitalized Ident followed by `(` in operand position now
  parses as a CALL (e.g. ctor applications `IVec2(0, 0)`) and —
  being absent from the lowering table — pushes handle 0. That is
  the same silent-drop behavior unknown idents already had (fossil
  parity); C3 (ctor registry dispatch) replaces the fallthrough.
- The Pratt operand/operator stacks may now hold call argument
  frames; depth stays bounded by expression nesting (corpus max
  well under the 8-token window pressure — the stacks are Z3-state
  enum lists, not window-bound).

## D3 + C2 — last_results select + Result floor; set-literal
## memberships (gap census D3 + C2, landed 2026-06-07)

### D3: the Result floor is the FOURTH ED run

The ED machine now declares `Result` (all six kernel variants, in
prelude order: NoResult, IntResult, StringResult, RealResult,
EofResult, ErrorResult) in the build context after the Effect run
(ed_src 3; zstep keeps holding at 9). Two additions to the machine:

- **Arena fix**: Result has SIX variants; the old `ctors[5]` region
  (+64..104) collided with sort_names at +104 — the sixth ctor
  write clobbered it and Z3_mk_constructor_list segfaulted. The
  arena is now ctors[8] at +64..128, names/souts/clists at
  +128/136/144, qout at +152 (walk regions at +200 unchanged).
  This was THE bug of the wave — everything compiled until the
  first 6-variant enum existed.
- **Tester + accessor harvest**: during the Result run, a payload
  variant's act-3 steps 3/4 read the query out-block's tester
  (qout+8) and accessor (qout+16) slots instead of fillers. The
  step-3 read captures on the `_ed_step = 3` tick (variant name
  still current); the step-4 read lands AFTER the variant walk
  advanced, so `res_acc_pend` (0/1/2) carries which register the
  value belongs to. Harvested: z_irtest/z_iracc (IntResult),
  z_srtest/z_sracc (StringResult) — the only ctors the corpus
  matches over last_results.

### D3: last_results / is_first_tick as build-context consts

ZINIT grew zsteps 22-31: mk_array_sort(Int → Result) → the
`last_results` const → the `last_results__len` Int const → the
`is_first_tick` Bool const → the len floor `(>= last_results__len 0)`
built and ASSERTED in the build context → the symtab pre-seed
(slots 0/1 = last_results / is_first_tick; st_names initialized
with the two fixed-width records, st_cnt starts at 2). The LEX
gate moved `_zstep < 22` → `< 32`.

Prelude consequences (the `_saw_result` dance, sample.ev:894-898):

- the len floor now ALWAYS serializes out of the solver, so the
  textual `last_results__len` declare + floor-assert lines are gone
  unconditionally (the negative control still shows exactly 1
  assert — it just lives in the rendered body now);
- `saw_lr` latches when a constraint resolves the ident
  `last_results`; the textual Result datatype + last_results
  declare lines drop exactly then (the serialization carries them);
- `saw_ift` does the same for the `is_first_tick` declare.
- Z3 empirically does NOT serialize declared-but-unmentioned consts
  or datatypes, so a unit that never touches last_results keeps the
  textual prelude with no duplicate-sort error.

### D3: Pratt postfix indexing + C2SelH

`e[i]` parses in the Pratt FSM as a PrIdx floor marker: a `[` in
operator position (operand just completed) pushes PrIdx and bumps
pd (so its `]` can never terminate the expression); `]` over the
PrIdx top pops base + index into ECall2("__index", base, idx); a
`]` over anything else reduces first (same loop as `)`). The
walker lowers "__index" to P(base)·P(idx)·C2SelH — a new one-tick
work item: mk_select(2nd, top), pops 2, pushes 1. The base ident
resolves through the symtab like any name (the last_results seed
is what makes `last_results[0]` work; any future array-sorted
const comes free).

### D3: the restricted match-pin (pmode 6)

`name ∈ Type = match <scrut>` + exactly two arms
(`Ctor ( bind ) ⇒ <atom>` then `_ ⇒ <atom>`), scrut = plain ident
or `last_results [ <int> ]`, Ctor ∈ {IntResult, StringResult}.
Three window bites (scrutinee · arm 1 · arm 2; lookahead 6), then
ONE items program lowering to the fossil's documented shape:

    (ite ((_ is Ctor) scrut) (Ctor__f0 scrut) else-atom)

An arm-1 body equal to its bind name reads the payload through the
harvested accessor; any other atom body passes through. This
covers the corpus's exact 2 occurrences (sample.ev:108's
`match last_results[0]` / StringResult(s) ⇒ s / _ ⇒ "") and the
common `match r` over a Result-pinned var. `Result` is sort class
4 in the classifier (z_ressort consts, manifest `r:Result`, carry
`(declare-fun _r () Result)` — datatype-typed state is
kernel-legal, same as compiler.smt2's Token fields).

### C2: set-literal memberships (pmode 7)

`x ∈ { a, b, … }` / `x ∉ { … }` (`{`/`}`/`∉` newly lexed, tags
71/72/73; `[`/`]` are 69/70) walk one int element per tick,
or-folding an Expr. The fold builds nested 2-ary ors but Z3's
serialization flattens them — the emitted text is the oracle's
flat `(or (= x 2) (= x 4) (= x 6))` byte-for-byte. Asserted at
the closing brace, wrapped in C2Not for `∉`. The
empty set folds to `false` (062: `x ∈ {}` → `(assert false)` →
UNSAT). Set lines declare nothing: no manifest field, no carry —
they constrain an existing var (c_field_add excludes them).

### C2: enum-typed membership status (verify + complete)

P3e's machinery verified intact under the new floor: 043/044 green
on the final artifact (enum-sort consts via ue_sort, manifest
`c:Color`, carry declares, variant-atom ≠/= through the Pratt
walker's evt table). Multiple user enums per compile and payloaded
user variants stay descoped (unchanged from P3e).


### D3+C2 acceptance (run 2026-06-07, oracle-built driver, parallel;
### every row BOTH checks: smt2-contains + kernel run of the emitted
### unit vs expected exit/stdout)

NEW targets (driver compile exit 0 on every row):

| fixture | emitted shape spot-check | exit got/want |
|---|---|---|
| lastresults_fixture (D3) | `(= r (select last_results 0))` + `(= v (ite ((_ is IntResult) r) (IntResult__f0 r) 7))` — the oracle shapes byte-for-byte; manifest `r:Result v:Int w:Int ok:Bool`; serialized Result datatype replaces the textual prelude (saw_lr) | 0/0 ✓ |
| 041-set-literal-membership | `(or (= x 2) (= x 4) (= x 6))` — flat, oracle-identical; `ite` (census check) | 0/0 ✓ |
| 042-set-not-member | `(not (or (= x 1) (= x 2) (= x 3)))` | 0/0 ✓ |
| 062-empty-set-membership-unsat | `(assert false)` — `x ∈ {}` | 2/2 ✓ |

Full 38-fixture regression (the B3 acceptance list), re-run on the
FINAL artifact — every row compile exit 0, smt2-contains OK, run
exit matched, stdout matched where defined:

| fixture | exit | | fixture | exit |
|---|---|---|---|---|
| 002-string-literal-print | 0/0 ✓ | | 029-chained-comparison-unsat | 2/2 ✓ |
| 003-int-multiply-to-string | 0/0 ✓ | | 030-logic-and | 0/0 ✓ |
| 004-comparison-ternary | 1/1 ✓ | | 031-logic-or | 0/0 ✓ |
| 005-string-concat | 0/0 ✓ | | 032-logic-not | 0/0 ✓ |
| 008-boolean-and | 1/1 ✓ | | 033-implies | 0/0 ✓ |
| 011-string-length | 5/5 ✓ | | 034-implies-vacuous | 0/0 ✓ |
| 012-substring | 0/0 ✓ | | 036-implies-block | 0/0 ✓ |
| 014-index-of | 4/4 ✓ | | 037-nested-implies-block | 0/0 ✓ |
| 017-nat-membership | 0/0 ✓ | | 043-enum-declaration | 0/0 ✓ |
| 018-int-membership-negative | 0/0 ✓ | | 044-enum-constraint | 0/0 ✓ |
| 019-string-membership | 0/0 ✓ | | 050-string-concat | 0/0 ✓ |
| 020-bool-membership | 0/0 ✓ | | 053-bool-as-constraint | 0/0 ✓ |
| 022-inequality | 0/0 ✓ | | 054-not-bool-as-constraint | 0/0 ✓ |
| 023-less-than | 0/0 ✓ | | 067-str-len-function | 0/0 ✓ |
| 024-greater-than | 0/0 ✓ | | 068-substr-slice-var | 0/0 ✓ |
| 025-lte-gte | 0/0 ✓ | | 069-substr-is-exact-unsat | 2/2 ✓ |
| 026-arithmetic-add | 0/0 ✓ | | 071-index-of-present-and-absent | 0/0 ✓ |
| 027-arithmetic-unsat | 2/2 ✓ | | 072-index-of-with-offset | 0/0 ✓ |
| 028-chained-comparison | 0/0 ✓ | | 073-char-at | 0/0 ✓ |

Negative control re-verified on the final artifact: nonexistent
claim → manifest (empty state-fields) + textual prelude (Result
datatype + last_results + is_first_tick) + exactly 1 assert (the
last_results__len floor, now serialized from the build context),
no `(+ `, driver exit 0. pratt_fixture re-verified post-change
(PrIdx variant + the `[`/`]` action rules): canonical AST printed,
exit 0. lex_fti_fixture re-verified: exit 0 (LexCharTag gained the
5 new chars; no behavior change for its inputs).

Tick-budget note: ZINIT grew by ~80 ticks (the Result ED run over
6 variants incl. the tester/accessor harvest reads) + 10 zsteps
(last_results/is_first_tick consts + floor assert + symtab seed).
Census-fixture compile wall ~8-9 min parallel-13 on this box —
same character as P3e (LEX + functionizer-residual per-tick cost
dominate).

### D3+C2 descopes

- 006-enum-match stays descoped: its match has THREE nullary-ctor
  arms (`Red ⇒ "stop"`) over a USER enum — needs user-enum tester
  harvest + n-arm match (C4/C5). The pmode-6 shape is fixed at
  2 arms with a payload ctor ∈ {IntResult, StringResult}.
- match-pin arm bodies are ATOMS (int/string/ident/bool literal —
  the corpus's arm bodies are `s` / `""` / `n` / `0`); full-expr
  arm bodies are C5.
- Set-literal elements are int literals only (the census fixtures'
  surface; ident elements would need the same one-tick fold with a
  symtab Process — trivial when a corpus use appears).
- The lastresults fixture is a driver INPUT, not a fossil-driven
  test_*.ev: the FOSSIL compiler.smt2 drops `last_results[0]` and
  the match (it emits only `(= r v)`) — a fossil gap the frozen
  oracle does not share. compiler2 is now AHEAD of the fossil on
  this surface.

## Next steps

- The rest of the membership surface (multi-name, chained bounds
  in memberships, Real/021).
- match/matches in the emit path (unblocks 006).
- Tick-rate: measure; if the walk dominates, batch pure ticks
  (classify + expansion) into the libcall ticks.
- Census: run the full conformance suite under a driver-backed
  compile path once coverage is wide enough to be interesting.
