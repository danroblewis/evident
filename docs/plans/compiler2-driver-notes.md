# compiler2 driver skeleton (P3a + P3b + P3c + P3d) — notes

Status: P3a LANDED — both acceptance fixtures compile + run green
(see Acceptance below). P3b LANDED — the census
arithmetic/comparison/membership class and the implies class flip
green through the driver (see P3b acceptance below). P3c LANDED —
the bounded shape-enumeration parsers are replaced by a Pratt
parser FSM (see P3c below). P3d LANDED — the TokenList cons-list
lexer + REVERSE phase are replaced by the FTI token buffer
(compiler2/lex_fti.ev) and a cursor/window parse path (see P3d
below).

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
  ↓ LEX       the fossil's per-char scanner FSM + compiler2/lex_fti.ev's
              LexFtiPlan (P3d): tokens land in a calloc'd FTI buffer
              (32 bytes/token, append order) via __mem.write_long;
              Ident/StringLit payloads are strdup'd char*s. Z3 lexer
              state = five Ints. One token per tick + bulk-skip.
  ↓ (REVERSE deleted in P3d — append order is source order; the
              lex_done tick flushes the pending string write + writes
              the EofTok sentinel, then phase 0 → 2 directly.)
  ↓ PARSE     top-level dispatch: KwEnum and parametrized / non-target
              claims SKIPPED one token per tick; the target bare-head
              claim enters the walk. All token access goes through an
              8-token decoded window over the buffer (see P3d).
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

## Next steps

- Full Effect floor (sort registry), user enums via the ctor
  steps, the rest of the membership surface. (P3c's Pratt FSM
  closed the expression-parser gap.)
- Tick-rate: measure; if the walk dominates, batch pure ticks
  (classify + expansion) into the libcall ticks.
- Census: run the full conformance suite under a driver-backed
  compile path once coverage is wide enough to be interesting.
