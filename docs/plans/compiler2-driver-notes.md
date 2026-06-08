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

## C3 + C4 — ctor applications + matches recognizers (gap census,
## landed 2026-06-07)

The user-datatype expression surface: payloaded variant
CONSTRUCTION (`B(7)`, `B(3 + 4)`, `Cons(1, Nil)` — ≈440 corpus
occurrences) and `matches` RECOGNIZERS (`e matches B(_)`,
`e matches A` — 323 occurrences) compile through the driver, plus
the n-arm match-pin that flips 006.

### Payloaded user enum collection (the P3e descope lifted)

pmode 4 no longer bails on `(` after a variant name: a payload
variant `Vname ( Ty [, Ty] ) [|]` is consumed in ONE tick
(4/5/6/7 tokens), building `EVDeclP(name, EVFCons(EVFType(ty), …))`
for the ED machine. Field types are restricted to
Int/Bool/String/Real/self (the declared enum's own name — the ED
machine's FieldSortSlot self-recursion path); an unknown type or a
THIRD field bails to skip (a 3-field variant head is 9 tokens —
past the 8-token window). The pmode-4 lookahead need is dynamic:
7 when ww_t1 is `(`, else 2 (refills refetch from tcur, so the
decision token is the one the consumer acts on).

### The uev registry (ctor decls + testers per user variant)

The ED machine's USER run now harvests, per variant (indexed by
ed_vidx, ≤ 6 slots — uev_n/uev_d/uev_t):

- the ctor func_decl (act-3 step-2 capture — same read the floor
  runs use for named registers, now table-stored), and
- the TESTER func_decl: a nullary variant's step-3 tick emits the
  2-effect batch `⟨mk_app, rdtest⟩` (value lands lr[0] — the evt
  capture is unchanged — tester lr[1]); a payload variant's step-3
  tick emits the single rdtest (tester lr[0]). qout+8 is the tester
  slot VariantQueryStep wrote at step 1.

User-ctor ACCESSORS are not harvested (no corpus gate needs them;
the Result floor's z_iracc/z_sracc cover the match-pin binds).

### C3: ctor-app dispatch in the walker

ECall1/2/3 lowering gained a registry row before the unknown-name
fallthrough: a call name found in uev lowers to
`P(arg0)…P(argk-1) · C2App(uev_decl, k)` — mk_app over the
harvested decl. Compound args come free (handles), so the legacy
dropped-`Exit(3+4)` class is structurally impossible — the
ctor_app fixture pins exactly that shape. Nested ctor args
(`Cons(1, Nil)`: a nullary variant ATOM inside a call) resolve
through the existing evt symtab fallthrough.

### C4: `matches` in the Pratt FSM

A new postfix action: in operator position, `matches Ctor`,
`matches Ctor(_)`, `matches Ctor(_, _)` is consumed in ONE action
(2/5/7 tokens — C2PrattStep's token destructure deepened to 7,
scons already Int) and replaces the top operand with
`EMatches(ctor, top)` (a new fixed-payload Expr variant in
compiler/parser.ev — corpus binds are ALL wildcards, so only the
ctor name is carried). It binds tightest (applies to the completed
top operand; corpus scrutinees are atoms/paren groups). A
malformed pattern is NOT in ps_cont, so the expression ends before
it (classifier parity). The pmode-3 lookahead need is dynamic:
7 when ww_t0 is KwMatches, else 2.

The walker lowers EMatches to `P(e) · C2App(tester, 1)` — the
tester app IS mk_app (translate2_match.ev's MatchesBuildZ3
semantics through the existing C2App item; serialized as
`((_ is Ctor) e)`). Tester lookup: uev + the IntResult/StringResult
floor; a miss pushes handle 0 (unbound parity).

### C4 gate: the n-arm match-pin (pmode 6 generalized — 006 flips)

The fixed 2-arm `Ctor(b) ⇒ atom · _ ⇒ atom` shape is replaced by
an arm-collection loop: nullary arms (`Red ⇒ "stop"`, 3 tokens),
payload arms (6 tokens), and the wildcard arm (3 tokens, ends the
match) accumulate through a PENDING slot — a new arm promotes the
pending one into a tested slot (≤ 2); the match ends at the
wildcard (default = its atom) or at the first non-arm head
(default = the pending arm's atom, its test dropped — the legacy
right-to-left fold rule, translate2_match.ev). Lowering is the
nested ite-over-tester items program built bottom-up (def · ite ·
ite), testers via the same uev + floor lookup. 006's 3-arm match
= 2 tested arms + default:

    (= word (ite ((_ is Red) signal) "stop"
               (ite ((_ is Yellow) signal) "slow" "go")))

The old 2-arm Result shape maps to 1 tested arm + wildcard default
(lastresults_fixture re-verified byte-identical shapes). >2 tested
arms bails the line (silent-drop parity).

### C3+C4 acceptance (run 2026-06-07, oracle-built driver)

NEW targets (every row: driver compile exit 0 + smt2 shape check +
kernel run of the emitted unit):

| target | emitted shape spot-check | result |
|---|---|---|
| ctor_app_fixture (NEW unit fixture) | `(= x (B (+ 3 4)))` — the compound-arg ctor app INTACT (the legacy renderer dropped exactly this); `(= ok ((_ is B) x))` + `(= nok ((_ is A) x))` tester apps; `(declare-datatypes ((E 0)) (((B (B__f0 Int)) (A))))` serialized; manifest `x:E ok:Bool nok:Bool`; `_x () E` carry | run exit 0/0 ✓ |
| 006-enum-match (FLIPPED) | `(= word (ite ((_ is Red) signal) "stop" (ite ((_ is Yellow) signal) "slow" "go")))` — 2 tested arms + dropped-test default, the legacy fold; census contains `(Light 0)` `(Red)` `(Yellow)` `(Green)` all present | stdout `go`, exit 0/0 ✓ |
| lastresults_fixture (pmode-6 rewrite regression) | `(= r (select last_results 0))` + `(= v (ite ((_ is IntResult) r) (IntResult__f0 r) 7))` — byte-identical to the D3 shapes | run exit 0/0 ✓ |

Negative control re-verified on the new artifact: nonexistent
claim → manifest (empty state-fields) + textual prelude, exactly
1 assert, no `(+ `, driver exit 0. pratt_fixture re-verified
post-change (token destructure deepened to 7, `matches` action,
EMatches variant): canonical AST printed, exit 0. lex_fti_fixture
re-verified: exit 0.

Full 41-fixture census regression (the D3+C2 list incl.
041/042/062), re-run on the FINAL artifact — every row ALL checks
(driver compile exit 0 + every expected/smt2-contains line + kernel
run exit + stdout where defined):

| fixture | exit | | fixture | exit |
|---|---|---|---|---|
| 002-string-literal-print | 0/0 ✓ | | 030-logic-and | 0/0 ✓ |
| 003-int-multiply-to-string | 0/0 ✓ | | 031-logic-or | 0/0 ✓ |
| 004-comparison-ternary | 1/1 ✓ | | 032-logic-not | 0/0 ✓ |
| 005-string-concat | 0/0 ✓ | | 033-implies | 0/0 ✓ |
| 008-boolean-and | 1/1 ✓ | | 034-implies-vacuous | 0/0 ✓ |
| 011-string-length | 5/5 ✓ | | 036-implies-block | 0/0 ✓ |
| 012-substring | 0/0 ✓ | | 037-nested-implies-block | 0/0 ✓ |
| 014-index-of | 4/4 ✓ | | 041-set-literal-membership | 0/0 ✓ |
| 017-nat-membership | 0/0 ✓ | | 042-set-not-member | 0/0 ✓ |
| 018-int-membership-negative | 0/0 ✓ | | 043-enum-declaration | 0/0 ✓ |
| 019-string-membership | 0/0 ✓ | | 044-enum-constraint | 0/0 ✓ |
| 020-bool-membership | 0/0 ✓ | | 050-string-concat | 0/0 ✓ |
| 022-inequality | 0/0 ✓ | | 053-bool-as-constraint | 0/0 ✓ |
| 023-less-than | 0/0 ✓ | | 054-not-bool-as-constraint | 0/0 ✓ |
| 024-greater-than | 0/0 ✓ | | 062-empty-set-membership-unsat | 2/2 ✓ |
| 025-lte-gte | 0/0 ✓ | | 067-str-len-function | 0/0 ✓ |
| 026-arithmetic-add | 0/0 ✓ | | 068-substr-slice-var | 0/0 ✓ |
| 027-arithmetic-unsat | 2/2 ✓ | | 069-substr-is-exact-unsat | 2/2 ✓ |
| 028-chained-comparison | 0/0 ✓ | | 071-index-of-present-and-absent | 0/0 ✓ |
| 029-chained-comparison-unsat | 2/2 ✓ | | 072-index-of-with-offset | 0/0 ✓ |
| | | | 073-char-at | 0/0 ✓ |

symtab_fixture re-verified (41-ident chain through the new walker
rows): compile exit 0, `(= v39 (+ v38 1))` in the unit, run exit
0/0 ✓.

Tick-budget note: a user enum's harvest gained nothing per nullary
variant (the rdtest rides the mk_app tick as a second effect) and
nothing per payload variant (the rdtest replaces the step-3
filler). Census-fixture compile wall ~10-12 min parallel-5 on this
box — same character as D3 (LEX + functionizer-residual per-tick
cost dominate; the driver's state grew by the uev/mp tables).

### C3+C4 descopes

- 052-seq-literal stays descoped: `s ∈ Seq(Int)` + `⟨10, 20, 30⟩`
  needs Seq-sorted consts + seq-literal lowering (census phase D
  Seq values), not ctor machinery.
- 3-field payload variants (collection window cap); user-ctor
  accessors (match-pin binds over user payloads — body falls back
  to unbound handle 0); match arm bodies are atoms (full-expr
  bodies are C5); one user enum per compile (unchanged).
- `matches` patterns with named binds don't exist in the corpus
  and are accepted as wildcards (the names at pattern positions
  are not inspected).

## C5 + D2 — match payload binds + full-expr arm bodies; conditional
## effects + Seq literals (gap census, landed 2026-06-07)

The corpus's two L-tier walker items.

### C5: pmode 6 arm bodies are full Pratt expressions

The arm-with-atom-body shapes are gone. The pmode-6 walk now
detects only arm HEADS — `Ctor ( bind ) ⇒` (5 tokens), `Ctor ⇒`
(2), `_ ⇒` (2) — latches (ctor, bind, is-wildcard) and re-enters
the Pratt FSM per arm body under a new pk_kind 4. The body
expression ends at the first token that cannot extend it (the next
arm's head Ident, the next body line's head, EOF — lexed newlines
don't exist, same boundary rule as every other expression context).

DESIGN CHOICE — Pratt re-entry per arm body, not a queued body
token range: the FSM already owns expression termination, the
8-token window, calls, `matches`, indexing and ternaries; a
separate body-range parser would recreate exactly the bounded-shape
class P3c deleted. The handoffs are free — the head tick IS the
Pratt-enter tick (head tokens consumed as cons0), and the body-done
tick IS the promotion tick.

Arm accounting moves to body-COMPLETION time (mq_*): a finished
non-wildcard arm becomes the PENDING arm, promoting the previous
pending one into a tested slot (≤ 2, the C4 cap unchanged); a
wildcard body's completion fires the match with def = its parsed
expression; a non-arm head with a pending arm fires with the
pending arm as the dropped-test default (the legacy fold rule).

### C5: scoped payload-bind substitution

Two new PURE work items bracket each bind-carrying arm body in the
items program: `C2BindScope(bind, acc)` arms a 3-register walker
scope (bsc_on/bsc_n/bsc_acc) and `C2BindEnd` clears it. Item
expansion is depth-first (children prepend ahead of the tail), so
everything the body's `C2Process` expands to runs strictly between
the two markers — the bind name shadows ONLY within its arm body,
at any nesting depth. An in-scope `EIdent(bind)` leaf bypasses the
symtab and expands to the accessor app over the scrutinee —
P(scrut)·C2App(acc, 1), translate2_match.ev's MatchPayloadBuildZ3
semantics = the oracle's `(Ctor__f0 scrut)` — including the
`last_results[i]` scrutinee flavor (re-select per occurrence).

Accessor decls: the ED machine's USER run now harvests each payload
variant's FIELD-0 accessor into the uev registry (uev_a0..a5)
alongside ctor + tester — the act-3 step-4 tick (previously a
filler for user runs) emits the qout+16 read, with `uev_acc_pend`
carrying the variant index across the advance tick (the same pend
dance as the Result floor's z_iracc/z_sracc). Lookup is uev + the
IntResult/StringResult floor.

Oracle findings (probed, pinned by the fixture):

- `B(v) ⇒ v * 2 · A ⇒ 0` →
  `(= y (ite ((_ is B) x) (* (B__f0 x) 2) 0))`.
- A dropped-test DEFAULT arm's bind still reads through the
  accessor: `A ⇒ 0 · B(v) ⇒ v + 1` →
  `(ite ((_ is A) x) 0 (+ (B__f0 x) 1))` (Z3 accessors are total).
  The driver wraps the default in its bind scope accordingly.

### D2: conditional effects — the oracle's guarded select/len shape

Probed empirically (the census's discover-empirically clause):
`effects = (c ? ⟨a, b⟩ : ⟨d⟩)` does NOT emit per-slot ites. The
oracle emits ONE assert, a right-nested guard tree with per-branch
conjunctions:

    (and (=> c (and (= effects__len 2) (= (select effects 0) a)
                    (= (select effects 1) b)))
         (=> (not c) (and (= effects__len 1)
                          (= (select effects 0) d))))

nesting in else position for deeper chains; `⟨⟩` lowers to
`(and (= effects__len 0))`.

Driver lowering (pmode 5 became a CHAIN walk; el_st substates):

- The classifier's 8-token effects head now branches on token 7:
  `⟨` keeps the P3e literal path byte-for-byte; `(` enters the
  chain (el_st 1).
- Branch GUARDS are full Pratt expressions under pk_kind 6 with a
  new `qstop` input to C2PrattStep: a top-level `?` (pd = qd = 0)
  in operator position TERMINATES the expression (the `?` belongs
  to the chain and is consumed on completion). Covers the corpus's
  `is_first_tick` / `(¬_got_path)` / arbitrary Bool guards.
- In a chain, element eqs become HANDLES: `C2SelEqH(i)` (select +
  mk_eq, pushed) replaces `C2SelectEq(i)` (assert); the branch
  close folds k eqs + `C2LenEqH(n)` into a conjunction handle via
  C2Op(OpConj) — Z3's serialization flattens the 2-ary nesting back
  to a flat `(and …)` (conjunct ORDER differs: ours len last,
  oracle len first — cosmetic, the accepted P3c divergence class).
- The chain end (`)` at chain depth 1) folds the (g_k, B_k) pairs
  bottom-up into the nested guard tree, one 8-item run per level
  (C2ChainLvl: Dup3 · Not · Swap · Impl · Rot3 · Impl · Swap ·
  Conj, over three new PURE stack items C2Dup3/C2Swap/C2Rot3), and
  asserts the root. ≤ 4 guards / 5 branches (corpus max: 4);
  then-branches must be literals, chains nest in else position
  only (the corpus shape).
- Literals: 0-4 elements (cached 0..4 numerals — z_three/z_four
  are new zsteps 32-33; the LEX gate moved 32 → 35), `⟨⟩` empty
  branch literals, and LibCall's empty-args `⟨ ⟩ )` bite (the
  corpus getpid shape, el_b2e).

### D2: user Seq(Int) values (051/052/057/065)

- `s ∈ Seq(Int)` declares s : (Array Int Int) (z_iarr, zstep 34) +
  `s__len : Int` + the `(>= s__len 0)` floor — entirely through
  existing items (two C2DeclConsts + NatBound/AssertTop/Drop ×2);
  manifest `s:Seq(Int)`, TWO carry declares (`_s () (Array Int
  Int)` + `_s__len () Int` — the oracle's lines). Both names land
  in the symtab, so `s[i]` (D3's PrIdx/C2SelH) and `s__len`
  resolve like any const.
- `s = ⟨10, 20, 30⟩` enters pmode 8: one full-Pratt element per
  `,` (pk_kind 7), each asserting `(= (select s i) e_i)` via
  P(s)·P(EInt i)·SelH·P(e)·PinEq·Assert — zero new work items; the
  closing `⟩` asserts `(= s__len n)` (mk_int numerals — element
  count NOT capped at 4). One registered seq var per compile.
- `#s` dispatches per sort: the registered seq var's `#`/str_len
  lowers to its s__len const; everything else stays str.len —
  translate2_seq.ev's CardBuildZ3 split, realized in d_c1_items.
- DIVERGENCE (deliberate): the oracle RECORDS a per-var length and
  substitutes it, emitting degenerate `(= 3 3)` / `(= 3 5)` shapes
  and never constraining s__len; the driver pins s__len for real.
  Sat-equivalent on every census fixture (065's contradiction is
  `(= s__len 3) ∧ (= s__len 5)` — still UNSAT → exit 2), and
  strictly more faithful state.

### C5+D2 acceptance (run 2026-06-07, oracle-built driver)

NEW unit fixtures (driver INPUTS; both: compile exit 0 + emitted
shape + kernel run):

| fixture | emitted shape spot-check | result |
|---|---|---|
| tests/kernel/compiler2/match_bind_fixture.ev (C5) | `(= y (ite ((_ is B) x) (* (B__f0 x) 2) 0))` — the oracle shape byte-for-byte: payload bind through the harvested USER accessor, full-expr body, dropped-test default; `(= x (B 21))`; serialized `(declare-datatypes ((E 0)) (((B (B__f0 Int)) (A))))` | unit `Exit(y - 42)` → exit 0/0 ✓ |
| tests/kernel/compiler2/cond_effects_fixture.ev (D2) | ONE assert: `(and (=> c (and (= (select effects 0) (LibCall "libc" "puts" (__Cell_LibArg (ArgStr "yes") __Empty_LibArg))) (= (select effects 1) (Exit 0)) (= effects__len 2))) (=> (not c) (and (= (select effects 0) (Exit 1)) (= effects__len 1))))` — the oracle's guard tree, flat conjunctions | unit takes the c = true path: stdout `yes`, exit 0/0 ✓ |

Census flips — all four D2 gates (driver compile exit 0 +
smt2-contains + kernel run, both checks per row):

| fixture | gate check | exit got/want |
|---|---|---|
| 051-seq-type-index | `select` in unit; `#s = 3` + indexed pins + ok conjunction | 0/0 ✓ |
| 052-seq-literal | `select` in unit; literal select-eqs + `(= s__len 3)` | 0/0 ✓ |
| 057-seq-cardinality | `ite` in unit; 4-element literal, `(= s__len 4)` consistent with `#s = 4` | 0/0 ✓ |
| 065-seq-length-contradiction | `Exit` in unit; `(= s__len 3) ∧ (= s__len 5)` → UNSAT | 2/2 ✓ |

Full 42-fixture census regression (the C3+C4 list), re-run on the
FINAL artifact — every row ALL checks green (driver compile exit 0,
every smt2-contains line, kernel run exit, stdout where defined):
002 003 004 005 006 008 011 012 014 017 018 019 020 022 023 024
025 026 027 028 029 030 031 032 033 034 036 037 041 042 043 044
050 053 054 062 067 068 069 071 072 073 — 42/42 PASS (046 rows
total with the four flips above; zero failures).

Driver-input fixtures re-verified on the final artifact:
ctor_app_fixture (ctor app + testers) 0/0 ✓ · lastresults_fixture
(the pmode-6 rewrite's regression — the D3 ite/accessor shapes
unchanged) 0/0 ✓ · symtab_fixture (41-ident chain) 0/0 ✓.

Negative control re-verified: nonexistent claim → manifest +
textual prelude, exactly 1 assert, no `(+ `, driver exit 0.
pratt_fixture re-verified post-signature-change (qstop input,
bound false): canonical AST printed, exit 0. lex_fti_fixture
re-verified: exit 0. The other compiler2 unit fixtures
(match/ctor/bool/record/seq/solver_emit) re-run: all exit 0.

### C5+D2 descopes

- Match-pins remain MEMBERSHIP pins (`y ∈ Type = match x`); a bare
  `y = match x` line has no sort for the decl (zero corpus
  occurrences — the corpus's 185 `= match` pins are all membership
  form).
- 2-field payload binds (`Ctor(a, b) ⇒`): only the field-0
  accessor is harvested; corpus match binds are all 1-field.
- Guarded-writer effects lines (`cond ⇒ effects = ⟨…⟩`): zero
  corpus occurrences — the corpus's only conditional effects shape
  is the nested ternary chain (sample.ev:1061). Descoped honestly.
- Then-branch nesting (`((a ? X : Y) ? … : …)`): chains nest in
  else position only (the corpus shape).
- ≥ 5 guards / ≥ 5-element effects literals (corpus max: 4 / 2).
- Seq element type Int only (the census surface); one registered
  seq var per compile; combined membership+literal pins
  (`s ∈ Seq(Int) = ⟨…⟩` on one line) unsupported — the census
  sources declare and pin on separate lines.

## E1 + F1 — state carries + manifest floor; first-line param
## lists + multi-name groups (gap census, landed 2026-06-07)

The last two items before F2 (composition).

### E1: `_name` carries are EXPLICIT memberships (oracle-probed)

The corpus idiom is NOT bare `_count` references — every one of
the 46 carries is paired with an explicit `_count ∈ Int`
membership line. Probed: the oracle REJECTS an unbound `_count`
mention ("dropped constraint"), so declare-on-first-use was the
wrong wiring; the explicit membership IS the declaration. Driver
rules for a `_`-prefixed membership (c_is_carry):

- declares a build-context const + symtab entry like any
  membership (so `(+ _count 1)` builds normally), with the type
  the source gives it (corpus carry types: Int/Bool/String +
  TokenList — the one-user-enum class covers the latter);
- is EXCLUDED from manifest state-fields and gets NO `__name`
  textual carry declare of its own (c_field_add gates on
  ¬c_is_carry) — the kernel pairs `_count` with state field
  `count` by name at carry time.

THE DUPLICATE-DECLARE DANCE: the base field's membership appends
a textual `(declare-fun _count () Int)` to cdstr (the kernel
asserts into it from tick 1 even when the program never reads
it — the functionizer's verify solves tick 1). But once a user
constraint MENTIONS `_count`, the solver serialization carries
that declare itself and the textual line would be a duplicate-
declare Z3 parse error. So the first d_lk_read of a `_`-name
splices the line out of cdstr (d_carry_strip — prefix search
`(declare-fun _count () ` cut to its newline; the " () " tail
keeps `_count` from prefix-matching `_count2`; later mentions
find nothing and no-op). The unreferenced-carry case (`_done`
declared, never read) keeps its textual line — exactly the
oracle's layout, probed: referenced carries serialize early,
unreferenced ones stay in the trailing textual block.

### E1: manifest floor

- `(assert (>= effects__len 0))` now lives in the BUILD context
  (zsteps 35-36, the ze_lrge/ze_lrassert pattern; the LEX gate
  moved 35 → 37). The oracle emits this floor unconditionally for
  every unit. Consequence: every emitted unit gains the assert +
  the serialized effects__len declare; the NEGATIVE CONTROL's
  assert count moves 1 → 2.
- max-effects: probed — the oracle hardcodes 16. Driver parity
  (already 16); no derivation rule exists to follow.
- state-fields ORDER: probed — the oracle sorts alphabetically
  (`ok:Bool x:Int y:Int z:Int` for 049's x,y,z,ok declaration
  order). The driver keeps DECLARATION order. Divergence accepted:
  kernel/src/manifest.rs parses the list into a name-keyed field
  set; nothing in tick.rs is order-sensitive.
- is_first_tick: verified end-to-end — the D3 build-context const
  + saw_ift textual suppression carry the corpus pin idiom
  `count = (is_first_tick ? 5 : _count + 1)` byte-for-byte into
  `(ite is_first_tick 5 (+ _count 1))`.

### F1: the pmode-9 GROUP walk (param lists + multi-name)

One FSM, two flavors:

- PARAM (`claim main(a, b ∈ Int, ok ∈ Bool)`): a parametrized
  TARGET claim enters pmode 9 from dispatch (d_enter_claimp,
  3-token head `claim main (`); groups are separated by `,`, the
  list closes at `)`. Non-target parametrized claims still skip —
  F2's composition pass will index them instead.
- BODY (`x, y, z ∈ Nat` — 049): an `Ident , Ident` line head in
  the claim walk enters pmode 9 (unambiguous: expression grammar
  has no top-level comma).

Collect substate: one `Ident ,` per tick pushes the name onto a
TokenList. The `Ident ∈ Type` tick declares the LAST name through
the classifier's own c_mem_items/c_field/c_cdecl (the window
positions line up with a plain membership) and latches the group
type (sc/nat/tyname). Drain substate: one pending name per tick
declares through pg_drain_items with the latched type. Every name
lands as: build-context const + symtab entry + manifest field +
textual carry declare — exactly a membership.

ORACLE DIVERGENCE (deliberate, the Seq-length divergence class):
the oracle SUBSTITUTES pinned param values away (`a = 3` emits
`(= 3 3)`), never declares `a`, and its own emitted unit DIES
under the kernel ("state var `a` not in model", exit 3 — probed
on `claim main(a, b ∈ Int, ok ∈ Bool)`). The driver declares
params for real; its unit runs exit 0. Strictly more faithful.

### E1+F1 acceptance (run 2026-06-07, oracle-built driver)

NEW unit fixtures (driver INPUTS; both: compile exit 0 + emitted
shape + kernel run):

| fixture | emitted shape spot-check | result |
|---|---|---|
| tests/kernel/compiler2/carry_fixture.ev (E1) | manifest `state-fields = count:Int done:Bool` (carries excluded); exactly ONE `(declare-fun _count () Int)` (serialized — the textual line spliced at first mention); trailing textual `(declare-fun _done () Bool)` (unreferenced carry kept); `(assert (>= effects__len 0))` floor; `(= count (ite is_first_tick 5 (+ _count 1)))` — the oracle's pin shape byte-for-byte | unit runs TWO ticks (tick 0 getpid filler, tick 1 reads the kernel's `_count = 5` carry → `Exit(count - 6)`) → exit 0/0 ✓ |
| tests/kernel/compiler2/params_fixture.ev (F1) | `claim main(a, b ∈ Int, ok ∈ Bool)` — manifest `state-fields = b:Int a:Int ok:Bool` (group-drain order; kernel is order-insensitive), REAL declares `a`/`b`/`ok`, carries `_b`/`_a`/`_ok` textual; `(= ok (= (+ a b) 7))` | unit exit 0/0 ✓ — where the ORACLE's own emit of this claim dies under the kernel (probed: exit 3, "state var `a` not in model") |

Census flips (driver compile exit 0 + smt2-contains + kernel run,
both checks per row):

| fixture | gate check | exit got/want |
|---|---|---|
| 049-multi-name (F1) | `ite` in unit; `x, y, z ∈ Nat` → three REAL declares + NatBounds + real pins (the oracle substitutes `(= 1 1)` degenerates — driver strictly more faithful) | 0/0 ✓ |
| 058-given-pins-value | `ite` in unit; `x + y = 10` constraint + `x = 3` pin solve y = 7 | 0/0 ✓ |

(058 needed no new machinery — the standalone-constraint Pratt
path already covered it; it had simply never been run/recorded.
Verified both checks and added to the regression list.)

Negative control re-verified: nonexistent claim → manifest (empty
state-fields) + textual prelude, exactly TWO asserts now (the
last_results__len floor + the NEW effects__len floor — the
expected count moved 1 → 2 with E1), no `(+ `, driver exit 0.

Oracle-path unit fixtures re-verified on the edited tree:
pratt_fixture (canonical AST printed, exit 0) · lex_fti_fixture
0 ✓ · match/ctor/bool/record/seq/solver_emit 0 ✓ (six rows).

Full 46-fixture census regression (the C5+D2 list incl. the four
seq flips), re-run on the FINAL artifact in the continuation
session (the original validation parked on a functionizer
regression, since fixed on main — driver compiles back at ~14 s
per fixture) — every row BOTH checks green (driver compile exit 0,
every smt2-contains line, kernel run exit + stdout where defined):
002 003 004 005 006 008 011 012 014 017 018 019 020 022 023 024
025 026 027 028 029 030 031 032 033 034 036 037 041 042 043 044
050 051 052 053 054 057 062 065 067 068 069 071 072 073 — 46/46
PASS; 48/48 with the 049 + 058 flips above (049 re-probed on the
final artifact: three real declares each with a NatBound, real
`(= x 1)`/`(= y 2)`/`(= z 3)` pins, unit exit 0).

Driver-input fixtures re-verified on the final artifact:
carry_fixture 0/0 ✓ (shape re-checked: exactly ONE
`(declare-fun _count () Int)`, `state-fields = count:Int done:Bool`
— carries excluded, `_done` textual carry kept, effects__len floor
present, `(ite is_first_tick 5 (+ _count 1))` byte-for-byte) ·
params_fixture 0/0 ✓ (`state-fields = b:Int a:Int ok:Bool`, real
a/b/ok declares, `_ok` textual carry, `(= ok (= (+ a b) 7))`) ·
ctor_app_fixture 0/0 ✓ · lastresults_fixture 0/0 ✓ ·
symtab_fixture 0/0 ✓ · match_bind_fixture 0/0 ✓ ·
cond_effects_fixture stdout `yes`, 0/0 ✓.

### E1+F1 descopes

- Bare `_name` mentions without the explicit carry membership:
  oracle parity is an error class (dropped constraint); the driver
  resolves them as unknown idents (handle 0).
- A `_name` mention BEFORE the base field's membership line would
  splice nothing and leave a duplicate declare; corpus style is
  memberships-then-pins (all 46 pairs).
- Multi-name groups: scalar types only (Int/Nat/Bool/String/
  Result/the user enum); no bounds/pins on group lines
  (`a, b ∈ Int < 5` — zero corpus occurrences); Seq(...)-typed
  params descoped (`Seq(LibArg)` appears only in non-target
  claims).
- Param-list group size is unbounded (one name per tick), but
  param TYPES live at window positions t2 — multi-token types
  don't fit the 4-token group tail.

## F2 — claim-call composition: slot binding + body inlining
## (gap census, landed 2026-06-07)

The keystone. Three composition surfaces compile through the
driver: `Helper(slot ↦ value, …)` slot calls, bare `Helper`
lines, and `..Helper` passthroughs — all by INLINING the callee's
body tokens through the normal classifier/walker, exactly as the
census analysis predicted: the FTI buffer is random-access, so
"splice the callee body" is a cursor push/pop, not token surgery.

### The claim index (skip-pass byproduct)

The dispatcher's skip pass now records every skipped
`claim Name …` top: the name appends to `ci_names` (the B1
fixed-width-32 record string; lookup = pure index_of/32) and the
body-start cursor (`_tcur + 2` — the token after `claim Name`,
which is `(` for parametrized claims) writes to a new 256×8
calloc'd buffer (zstep 37; ci_base latches at 38; the LEX gate
moved 38 → 39 positions, i.e. `_zstep < 38`). One write_long per
claim, riding the skip-entry tick (replaces its filler). One-pass
consequence: a callee must PRECEDE its call site textually —
true throughout the corpus (stdlib → helpers → main).

### The call walk (pmode 10) and slot values as handles

A classify-tick head whose Ident resolves in ci_names becomes a
composition line (the classify tick also emits the cursor
read_long; capture next tick):

- `Name ( slot ↦ …` (ww_t1 LParen + ww_t3 OpMapsto) → pmode 10:
  per slot, the `slot ↦` head (2 tokens) latches the slot name
  and re-enters the Pratt FSM under the new pk_kind 8 — slot
  values are FULL expressions (the 282a5b3 CallArgsStep
  expr-slot capability, now trivial: the value's items program
  runs and leaves its HANDLE on hstk). The value ends at the
  call's `,` (cd = 0) or `)` (pd = 0) — both already Pratt
  termination points. `,` consumes and loops; `)` FIRES.
- bare `Name` (1 token) and `..Name` (Dot Dot Ident, 3 tokens)
  fire directly on the capture tick.

The fire tick pops the k slot handles off hstk (post-order: top =
last slot; the d_h peel deepened to 4) into a `C2Binds` table
(CBCons(name, handle, rest)), pushes the suspended frame
`CFCons(return-cursor, saved-pfx, saved-binds, rest)` onto
`il_frames`, overrides tcur/wend to the callee body cursor
(wtoks → TLNil forces a refetch — the cursor/window machinery
handled the jump exactly as the census predicted), and arms the
param-skip: a `(` at the body start is the callee's first-line
param list, walked over one token per tick to its matching `)`
(il_pd tracks nesting for `Seq(LibArg)` param types) — slot
params bind at the call site, so the list carries nothing.

### α-renaming and scoped resolution

- A slot call installs prefix `"__cN_"` (il_cnt, one per call
  site — the corpus's own `__callN` convention). Bare/`..`
  splices INHERIT the current (pfx, binds) — names-match is
  exactly "compile as if written in the caller".
- Memberships inside a frame declare `pfx ++ name`; manifest
  fields + textual carry declares use the prefixed name.
- A membership whose name is BOUND (a slot) or ALREADY DECLARED
  under the scoped name does NOT redeclare — its items head
  becomes `C2Process(EIdent(…))` (resolve the existing handle)
  and any Nat/bound re-asserts over it; c_field_add skips. This
  is what keeps `..GreetsHi` / bare `IsPositive` (whose bodies
  re-declare the caller's `text`/`n`) from emitting duplicate
  declares and duplicate `_name` carry lines (Z3 mk_const is
  idempotent; the TEXTUAL duplicate would be a kernel-side parse
  error). Same dup head on membership-PIN lines (pk_nodecl).
- Ident resolution order in the walker's process path:
  match-arm bind (bsc, innermost) → slot binds (pure handle
  push) → `pfx ++ name` in st_names → plain st_names →
  true/false/enum values → unknown (handle 0).
- The callee body ends at the next top-level keyword / EOF /
  non-line head: at depth 0 that ends the walk (unchanged); at
  depth > 0 it POPS — cursor/pfx/binds restore from the frame.

Oracle divergence (the accepted class): the oracle SUBSTITUTES
pinned values through composition (`(> 5 0)`, `(= 5 5)` in 047);
the driver declares and pins for real (`(> x 0)`, `(= x 5)`) —
strictly more faithful, sat-equivalent on every gate.

### Caps (probed, stated honestly)

- Slots ≤ 4 per call. Corpus single-line maximum is 6
  (histogram over the flattened corpus: 246×2, 40×3, 25×4, 3×5,
  8×6) and ONE 8-slot site spans two lines (MembershipStep,
  sample.ev:447) — the mission's "corpus max 4" is wrong;
  widening is mechanical (deeper d_h peel + cs slots) and
  deferred until the corpus compile needs it.
- Depth ≤ 8 (corpus reaches ≥ 5); a composition head at depth 8
  falls through to the Pratt/line-end path.
- Claim index: 256 entries; names ≤ 31 chars (the fixed-width
  record cap; prefix + name ≤ 31 — corpus ident max 21 + "__cN_"
  fits to N ≤ 9999).

### F2 acceptance (run 2026-06-07, oracle-built driver)

NEW unit fixture tests/kernel/compiler2/compose_fixture.ev — an
EXPRESSION slot value through TWO nested frames:
`main: Wrap(v ↦ 2 + 3, res ↦ r)` → `Wrap: Add2(x ↦ v + 10,
out ↦ mid)` → `Add2: tmp ∈ Int · tmp = x + 1 · out = tmp + 1`.
Emitted unit carries `(= __c2_tmp (+ 2 3 10 1))` (the slot-value
handles composed through both frames; Z3 flattens the 2-ary
nesting), `(= __c1_mid (+ __c2_tmp 1))`, `(= r __c1_mid)`,
manifest `r:Int __c1_mid:Int __c2_tmp:Int ok:Bool` + the matching
carry declares. Driver exit 0; unit run exit 0/0 ✓.

Census flips — ELEVEN composition-class fixtures green (gate
asked ≥ 6; every row driver compile exit 0 + smt2-contains +
kernel run of the unit):

| fixture | form | exit got/want |
|---|---|---|
| 094-bare-unconditional-sat | bare `IsPositive` | 0/0 ✓ |
| 095-bare-unconditional-unsat | bare, `n = 0` contradiction | 2/2 ✓ |
| 102-mapped-renames-sat | `GreetsHi(text ↦ greeting)` — unit carries `(= greeting "hi")` twice, oracle-identical | 0/0 ✓ |
| 103-mapped-renames-unsat | slot call + contradiction | 2/2 ✓ |
| 107-mapped-unconditional-sat | slot call | 0/0 ✓ |
| 108-mapped-unconditional-unsat | slot call | 2/2 ✓ |
| 109-passthrough-uncond-sat | `..GreetsHi` | 0/0 ✓ |
| 110-passthrough-uncond-unsat | `..GreetsHi` + contradiction | 2/2 ✓ |
| 047-passthrough-sat | `..Base` declares x IN the caller; `(= y (+ x 1))`, ite Exit | 0/0 ✓ |
| 048-passthrough-inherits-constraints-unsat | `..Base` constraint inherited | 2/2 ✓ |
| 115-passthrough-multiple-unsat-high | TWO `..` splices, `n = 10` vs `n < 10` | 2/2 ✓ |

Full 48-fixture census regression (the E1/F1 list), re-run on the
F2 artifact in parallel — every row BOTH checks green (driver
compile exit 0, every smt2-contains line, kernel run exit +
stdout where defined):
002 003 004 005 006 008 011 012 014 017 018 019 020 022 023 024
025 026 027 028 029 030 031 032 033 034 036 037 041 042 043 044
049 050 051 052 053 054 057 058 062 065 067 068 069 071 072 073
— 48/48 PASS.

Driver-input fixtures re-verified on the F2 artifact (all driver
exit 0 + unit run green): carry_fixture 0 ✓ · params_fixture 0 ✓ ·
ctor_app_fixture 0 ✓ · lastresults_fixture 0 ✓ · symtab_fixture
0 ✓ · match_bind_fixture 0 ✓ · cond_effects_fixture stdout `yes`
0 ✓. Negative control re-verified: nonexistent claim → manifest
(empty state-fields) + textual prelude, exactly 2 asserts, no
`(+ `, driver exit 0 (the skip pass now also INDEXES every claim
— write_long effects only, no observable change). Oracle-path
fixtures re-verified: pratt_fixture (canonical AST, exit 0 —
C2PrattStep signature UNCHANGED by F2; kind 8 is driver-side
latching only) · lex_fti_fixture 0 ✓ · match/ctor/bool/record/
seq/solver_emit 0 ✓ (six rows).

### F2 descopes

- 045/046 (subschema expansion/constraint): need RECORD types —
  `type Point` + `p ∈ Point` + `p.x` field access. Not claim-call
  composition; descoped honestly (the census's record-lift
  machinery is its own item).
- Callees AFTER the call site textually (one-pass index; zero
  corpus occurrences).
- Positional binding `(a, b) ∈ Claim`; conditional inline
  `cond ⇒ ClaimName`; composition calls in EXPRESSION position
  (`x = Helper(…)` parses as an unknown call → handle 0) — zero
  corpus occurrences of all three at statement level.
- Carry (`_x ∈ Type`) memberships INSIDE inline frames: the
  prefixed-name pairing (`__cN__x` vs `___cN_x`) doesn't line up
  with the kernel's `_field` convention; corpus carries are all
  in `main` (callers pass them via slots — `rem ↦ _rem` works:
  the VALUE resolves in the caller's scope before binding).
- Slot count > 4 per call (see Caps above — corpus needs 6/8 at
  two callees; widen before the sample.ev compile).
- A 5th+ slot, malformed slot heads, or a malformed value bail
  the line (consume 1, classifier resumes — silent-drop parity).

## G1 — burndown wave: records, conditional inline, quantifiers,
## positional binding, string stragglers (landed 2026-06-08)

Worked from the .goalpost conformance artifact's failing-50 list.
Canonical harness before: 88/138. After: see G1 acceptance below.
Seven classes, in landing order:

### replace() → str.replace (013 · 070 · 080)

StrOpBuildZ3 (compiler2/translate2_seq.ev) gained the
`"replace"` → Z3_mk_seq_replace row; the walker's d_c3_items
dispatches `replace(s, a, b)` to it. Three fixtures flip.

### Infix contains — `"lit" ∈ s` (075 · 076)

A new classifier line kind (c_strin_line — StringLit head + `∈` +
Ident): a fixed item program for `(str.contains s "lit")` + assert
(oracle-probed operand order: container = the VAR, needle = the
literal). The line previously fell into d_mem_line and declared an
EMPTY-named const.

### Type splice — record flattening (045 · 046 · 119 · 120 · 124 ·
### 133 · 134 · 135)

The skip pass now indexes `type Name` tops alongside claims (same
ci_names/ci_base index). A membership whose TYPE resolves in the
index (and is not a scalar/enum/Seq) becomes a composition jump
with prefix `"name."` — fields flatten to dotted consts
(`p ∈ Point` declares `p.x`/`p.y`; oracle parity: the legacy
compiler emits `(declare-fun f.p () Int)`). Two flavors:
`f ∈ Foo (p ↦ 5)` runs the EXISTING pmode-10 slot walk (the type's
first-line params bind at the use site — the driver substitutes
where the oracle declares + pins; sat-equivalent, the F2
divergence class), and bare `s ∈ Sprite` jumps direct. cw_ty /
ts_pfx latch the type prefix; il_cnt is NOT burned (no α-rename).

- Inside a type frame (prefix ends with ".") memberships are NOT
  manifest state fields and append no textual carry — the oracle's
  exact layout (h.local declared, absent from state-fields).
- DOT-FOLD in C2PrattStep: in operator position, `. Ident` over an
  EIdent top operand folds the flattened dotted name (`p.x`,
  `s.pos.x` chains, `s.rects[0]` then indexes). A 2-token action;
  no signature change.
- Seq fields inside type bodies: c_seqmem_items / sq_name now use
  the SCOPED name (s.rects + s.rects__len); the `#` dispatch
  (d_sl_seq) resolves through the frame prefix. 133-135 flip on
  exactly this.

### Conditional inline — `cond ⇒ ClaimName` (096 · 097 · 098 ·
### 116 · 117 · 118)

A standalone line whose parsed root is `(=> guard EIdent(Name))`
with Name in the claim index (gc_hit, at Pratt completion) splices
Name's body with every asserted constraint wrapped in
`(=> guard …)` — the oracle's shape, probed: the membership floor
`(>= n 0)` re-asserts under the guard, declares stay unguarded.
Flow: the guard expression builds to a HANDLE (its items run under
pmode 10, cw_st 3 → 4 — the cursor read rides the detection tick),
then the jump fires with il_guard armed; the matching frame pop
disarms it (il_gd). C2AssertTop becomes a 2-step item under an
active guard (mk_implies → assert tmp). ONE active guard; nested
guarded splices descoped (zero corpus occurrences).

THE WAVE'S BUG: the Pratt call-head shift (`Ident` + `(` in
operand position) swallowed the NEXT LINE's paren — lexed newlines
do not exist, so `flag ⇒ Pos` followed by `(¬x) ⇒ …` parsed
`Pos((¬x))` as a call, fed handle 0 to mk_implies, and Z3 killed
the compile ("ast is not an expression"). FIX: C2PrattStep gained
a `calls` input — a fixed-width-32 whitelist of legal call names
(the 9 string builtins + harvested user ctor names, built
driver-side as d_cb_names); an Ident outside it shifts as a plain
atom and the `(` ends the expression (statement boundary
restored). pratt_fixture binds calls ↦ "".

### Bounded quantifiers (038 · 039 · 040 · 136)

Statement-level `∀|∃ v ∈ {lo..hi} : body` and `∀ v ∈ seq : body`.
The head is classifier-detected (range: the 8-token bite
`∀ v ∈ { lo . . hi` + a pmode-11 `} :` close; seq: 5/7-token heads
incl. the dotted form `s.nums`); the body parses ONCE through the
Pratt FSM (pk_kind 9); the fl loop then re-walks the parsed Expr
once per element with the bound name expanding to the element
value — range: the numeral (one mk_int per element); seq:
`(select s i)` — and/or-folds 2-ary (Z3's serialization flattens;
the accepted P3c divergence class), asserting at the end.
∀-over-seq takes its bound from sv_len, recorded when a
`#seq = k` pin completes (svr_hit at Pratt completion). The fl
loop suppresses the classifier (d_classify gained ¬_fl_on) and is
tok_ready-gated (refills must not collide with item effects).

### Positional binding (081-089 — tuple-in, method calls, arg
### inference)

Four head shapes at the classifier: statement calls
`add(2, 3, mid)` (Ident-in-index + `(`, no `↦`), method calls
`x.add(3, result)` / `box.value.add(50, r)` (4/6-token heads, the
receiver Process-pushed FIRST = first positional), tuple-in
`(3, 4, result) ∈ add` (LParen + atom + Comma head), and
method-tuple-in `(4, r) ∈ x.add` (receiver resolved at the close,
landing on TOP of the stack — the pj_recv zip layout). Elements
are full Pratt expressions (pk_kind 10) building to HANDLES on
hstk; the close resolves the callee in the claim index, reads its
cursor, and jumps a fresh "__cN_" frame (pmode 12, pt_st
0 → 2 → 3 → fire).

Param-name harvest: the EXISTING param-skip walk (il_ps) now
collects the callee's first-line param names (pn0..pn5 — an Ident
following the opening `(` or a depth-1 `,`); ips_done zips them
with the held handles into il_binds (pz_act). ARG INFERENCE: a
lone UNDECLARED ident element (`mid`) declares an Int const +
manifest field + textual carry before binding — the oracle's
mid:Int shape, probed. Caps: ≤ 4 positional args (corpus max 3,
incl. receiver), ≤ 6 collected param names.

### Slot width 4 → 6 (no fixture; the F2 next-step item)

d_h peel deepened to 6, cs_n4/cs_n5, b_h4/b_h5, il_binds_new rows
for k = 5/6, cw_slot cap < 6, binds peel ilb_n4/n5 (lookup +
c_bnd). The 8-slot MembershipStep site still needs 6 → 8 before
the corpus compile.

### G1 acceptance (canonical harness, 2026-06-08)

`.goalpost/bin/run-conformance.sh` run from the worktree (GP_ROOT
resolves via BASH_SOURCE, so the harness measures THIS tree's
driver; stage1 oracle-built at run start):

- BEFORE: 88/138 passed, 50 failed (the burndown artifact).
- checkpoint (all classes except positional binding): 111/138.
- AFTER: 120/138 passed, 18 failed, 0 timed out (wall 340 s,
  stage1 oracle-built). +32 flips, ZERO regressions — the 18
  failures are exactly the descope list below (021 · 090-093 ·
  121-123 · 125-132 · 137-138).

Re-verified on the final driver: the full driver-input unit
fixture suite (carry · params · compose · symtab · ctor_app ·
lastresults · match_bind all exit 0; cond_effects stdout `yes`
exit 0), the oracle-path fixtures (pratt — calls slot bound — ·
lex_fti · bool · ctor · match · record · seq · solver_emit all
exit 0), and the negative control (nonexistent claim → manifest +
textual prelude, exactly 2 asserts, no `(+ `, driver exit 0).

### G1 descopes (probed, stated honestly)

- 021-real-membership: unchanged (FloatLit lexing + Real decls).
- 090-093 tuple→record coercion: binding a tuple to a record
  param needs the type's FIELD NAMES at the call site
  (`v.x ↦ 3, v.y ↦ 4`); the type index doesn't carry field lists.
- 121/122 record arithmetic (`pos + IVec2(10, 20)`): the
  componentwise broadcast lift needs per-field re-walks of record
  exprs; 122's bare `s ∈ Sprite` also leaves the unbound record
  param unflattened (params bind at use sites only).
- 123: a quantifier in PIN position (inside a type body) — the fl
  machinery is statement-level only.
- 125-132 + 137/138 composite seq/set elements: need real record
  DATATYPES (mk_Pair ctors + plain-named field accessors) — the
  ED machine's field syms are __fN-shaped and its registry is
  single-enum; its own wave.

## G2a — record DATATYPES: real Z3 tuple sorts, composite Seq/Set,
## tuple→record coercion, Real (landed 2026-06-08)

Closes the 18-fixture burndown G1 left as descopes (021 · 090-093 ·
121-123 · 125-132 · 137-138). Where G1 flattened a record into dotted
scalar consts (a frame splice — `p ∈ Point` → `p.x`/`p.y`), G2a adds
a SECOND lowering: a record can become a real Z3 datatype sort with
PLAIN field-name accessors, so a `Seq(Point)`/`Set(Point)` element is
a single ctor app and `t.duration` is `(duration (select s i))`.

The two lowerings coexist by design. The rule (document it, because
it is the load-bearing invariant):

> A record TYPE that is used as a Seq/Set element type, or appears in
> a record-pin broadcast (`offset ∈ IVec2 = …`), gets a datatype sort
> in the registry. A record VARIABLE used scalar-at-a-time
> (`s.pos.x = 5`) stays G1 frame-flattened. The registry's RtIdxOf
> returns −1 for any type whose sort handle is still 0, so a lookup
> that misses falls straight back to G1 behavior — the two paths
> never fight over the same name.

The draft (converged across the two parked worktrees
agent-a9bd3fb992c9f9c5f / agent-ac92a028fb826f9da, byte-identical)
carried this. The pieces:

### The record registry + the RD machine (pmode 13)

The SKIP pass (the existing claim-body skip) now also collects each
`type Name` top's fields — names + types — into one of 3 registry
slots (rt_n*/rt_f*/rt_t*/rt_nf*; corpus max is 2 record types, +1
headroom). When the skip stops at the next top-level keyword the RD
machine declares the record as a Z3 datatype in ONE
`Z3_mk_tuple_sort` call (field accessors carry the PLAIN field names),
then harvests the ctor decl, the per-field accessors, and the
`(Array Int T)` + `(Array T Bool)` sorts for composite seq/set use.
A type whose fields don't all resolve (generics, >6 fields,
self-reference) abandons its slot: sort stays 0 ⇒ every lookup misses
⇒ pure-G1 frame behavior. Fields live in the fixed-width 32-byte
`|name<pad31>` record pattern (same as st_names); handles are plain
Int state. Helper claims RtRecName / RtIdxOf / RtSortOf / RtFieldAcc.

### Record literals + decls as ctor apps (C2RecVal / C2RecDecl)

Two new C2Item variants. C2RecVal expands a record IDENT to a ctor
app over its flattened dotted consts (`a` → `(mk_Item a.id a.kind)`),
recursing through nested record fields. C2RecDecl declares the dotted
consts of a record instance from the registry — needed because a bare
type jump (`a ∈ Item (id ↦ 1, …)`) has no body memberships to declare
the params. Used by composite set literals, `∀`-over-set unrolls, and
the type-slot call path (which now also DECLAREs+PINs each param as a
real dotted const, sat-equivalent to G1's substitution).

### Composite Seq/Set memberships + set-literal walk (pmode 14)

`xs ∈ Seq(T)` (T a registry type) declares an `(Array Int T)` const +
`xs__len`; `xs ∈ Set(T)` declares ONE `(Array T Bool)` const. A
set literal `s = {a, b}` folds `mk_set_add` over `mk_empty_set`, each
element a C2RecVal ctor app, recording the element names for
`∀`-over-set and `#s` cardinality. `a ∈ items` asserts
`(select items (mk_T …))`.

THE WAVE'S BUG (the wedge that killed both prior agents at the
100k-tick limit): the pmode-14 set-literal element walk spliced its
C2Items into `witems`, but `d_processing` — the predicate that lets
the item-execution machinery actually CONSUME those items — did not
list pmode 14. So the items sat un-executed, no token advanced, and
the driver spun the filler branch to the tick cap (351 s wall →
rc 3). FIX: one disjunct, `∨ (in_parse ∧ (_pmode = 14))`, in
d_processing. Bisected with minimal sources (`items = {a}` alone
reproduced; slot-calls + bare `Set(Item)` decl did not). After the
fix the set class compiles in ~19 s/fixture like every other.

### __field accessor reads, ∀-over-composite, rb broadcast

`groups[0].items` (postfix `.field` over a NON-ident operand) folds
to `ECall2("__field", base, field)` in the Pratt FSM (ps_dotf2), then
the walker lowers it to `mk_app` of the accessor resolved by FIELD
NAME across the registry (RtFieldAcc; first hit wins, corpus is
unambiguous). `∀ t ∈ tasks : t.duration ≥ 0` over a composite seq
expands the dotted bound name to `(duration (select tasks i))`
(d_vb_dot). The rb loop re-walks a record-pinned body once per field
(`offset_pos ∈ IVec2 = pos + IVec2(10,20)` → per-field
declare+pin), reading dotted consts through the symtab and selecting
a registry-ctor call's field-index argument.

### Tuple→record coercion at the positional-binding zip (090-093)

A `( atom , atom )` tuple element parses as one 5-token Pratt bite
(pt_tup_b) pushing two handles. At the bind zip, a record-typed param
binds the two handles to its first two FIELD NAMES
(`v.x ↦ h0, v.y ↦ h1`); a scalar param binds the first handle only
(092's only-when-record contract). Param TYPES are harvested by the
existing param-skip walk (pty*) to drive the decision.

### Real / FloatLit (021)

`3.14` lexes by re-entering int collection after the `.` (lx_fr
armed); the fraction-finish writes ONE FloatLit token packed as
`scaled·8 + digits`. C2AtomE lowers it to `ECall2("__real", …)`, the
walker to one `Z3_mk_numeral("314/100", Real)`. Negative floats
descoped (zero corpus occurrences).

### G2a acceptance

Probed per-fixture through the canonical harness path (oracle-built
stage1, kernel-run, smt2-contains + exit checks; mktemp throughout).
Flips, all PASS: 021 · 045 · 081 · 090 · 091 · 092 · 093 · 096 ·
110 · 119 · 121 · 122 · 125 · 126 · 127 · 128 · 129 · 130 · 131 ·
132 · 133 · 137 · 138. Regression anchors (001 · 119) hold; the
oracle-path unit fixtures match main's pre-G2a baseline (pratt green;
the record/bool/ctor/match/seq/lex/solver/carry/params/compose
exit-1s are PRE-EXISTING on main, re-verified — NOT G2a
regressions). The full canonical run lands the burndown.

### G2a remaining descope

- 123-subschema-shadowing-quantifier: a quantifier in PIN position
  (inside a type body) — the `fl` quantifier machinery is
  statement-level only, unchanged from G1. Fails fast (compile
  error), not a wedge.

## Next steps

- Widen slot caps 4 → 8 (MembershipStep) before attempting the
  corpus compile; multi-line call sites come free (no lexed
  newlines).
- The rest of the membership surface (chained bounds in
  memberships, Real/021).
- Match as a general EXPRESSION (non-pin positions); bare
  `y = match` pins.
- Tick-rate: measure; if the walk dominates, batch pure ticks
  (classify + expansion) into the libcall ticks.
- Census: run the full conformance suite under a driver-backed
  compile path once coverage is wide enough to be interesting.
