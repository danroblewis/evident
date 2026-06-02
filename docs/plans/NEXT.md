# NEXT — fresh-session handoff

Read this first if you're a Claude session starting on this branch.

## Where the project is

After iter 3.13, **all three compiler stages have been demonstrated
as Evident programs running on the kernel**:

| Stage | Pattern | Test |
|---|---|---|
| Lexer (text → tokens) | Accumulator FSM | `tests/kernel/test_consolidated_lexer.ev` |
| Parser (tokens → AST) | State-carry consumer FSM | `tests/kernel/test_parser.ev` |
| AST walker | Work-stack DFS | `tests/kernel/test_ast_walker.ev` |
| Translator (AST → text) | Walker with text accumulation | `tests/kernel/test_ast_to_text.ev` |

23 kernel tests, all green via `./test.sh`. Through all of iter 3
the only `runtime/src/` change was a ~30-line widening of
`emit.rs::discover_state_fields` in iter 3.2 to admit Datatype-typed
memberships as carry state (Var::EnumVar). Everything else
(lexer.rs, parser/, translate/) remains untouched — the CLAUDE.md
invariant held in the "language semantics is frozen" sense.

The compiler architecture is **structurally complete**. What remains
is mechanical:

- **3.14+**: extend each stage to cover the full Evident surface
- **3.17**: wire stages together end-to-end
- **3.18**: bootstrap — run the Evident compiler on its own source
- **3.19**: delete `runtime/src/`

## Required reading order

1. **`CLAUDE.md`** (~500 lines, ~5 min) — the project's invariants,
   especially the "Rust is frozen, do it in Evident" rule and the
   language + kernel specs.
2. **`docs/plans/iter-3-status.md`** (~250 lines, ~5 min) — current
   state, the four FSM patterns, what each iter demonstrated.
3. **This file** — concrete next-step proposals.

If you're touching kernel SMT-LIB shape, also read
`docs/plans/kernel-input-spec.md`.

## How to verify your environment

```bash
./test.sh                                # ~3s, 23 kernel + 175 lang + 131 conformance
./runtime/target/release/evident run tests/kernel/test_hello.ev hello
# → "hello world", exit 0
./runtime/target/release/evident run tests/kernel/test_parser.ev main
# → walks 1+2+3, exits 0
```

If `./test.sh` doesn't pass, something is wrong with the environment
(probably a missing libffi or Z3 lib). Fix before touching code.

## Concrete next-session proposals (pick one)

Each is ~1 session of focused work. They're independent — pick the
one that matches the session's energy + interest.

### A. Extend the lexer (mechanical, low-risk)

**Goal**: handle one more category of input syntax in
`stdlib/lexer.ev` so the toy lexer covers more of real Evident.

**Candidate categories**:
- Unicode operators (∈, ⇒, ⟨, ⟩, ↦, ≤, ≥, ≠) — extend `SingleCharTok`
  with multi-byte char comparisons
- Two-char operators (==, →, ::, …) — apply the `peek_next` pattern
  from `test_comment_lexer.ev`'s `is_next_dash` check
- String literals "…" — add mode 2 (in-string) to the mode-state
  pattern from comment lexing
- Float literals (`3.14`) — extend the digit accumulator to handle a
  decimal point

**Pattern**: take an existing lexer test (e.g.
`test_consolidated_lexer.ev`) as a starting template, add the new
predicate/dispatch logic, write a fixture that exercises it.

**Time estimate**: 1 hour, 100-150 LOC stdlib + 1-2 new test fixtures.

### B. Extend the parser (mechanical, medium-risk)

**Goal**: handle one more grammatical production in
`stdlib/parser.ev`.

**Candidates**:
- Parenthesized sub-expressions: parse `(1 + 2) * 3` → `EBinOp(*,
  EBinOp(+, 1, 2), 3)`. Requires mode state ("inside parens") and
  pending-expression carry.
- Identifier expressions: `Ident → EIdent` (trivial extension of the
  current toy parser).
- Comma-separated lists: `1, 2, 3` → `EList([1, 2, 3])`. Add `EList`
  and an accumulator for list items.

**Pattern**: extend `Op` and `Expr` enums in `stdlib/parser.ev`,
extend `TokenToOp` arms, mirror in the parser FSM body.

**Watch out for translator gaps** — see "Discovered gaps" below.

**Time estimate**: 1-2 hours, 50-100 LOC stdlib + 1 test fixture.

### C. Extend the AST → text translator (mechanical, low-risk)

**Goal**: handle one more `Expr` variant in `test_ast_to_text.ev`'s
work-stack walker.

**Candidates**:
- `EIdent(s)` → emit `s` directly. Trivial.
- `EList([…])` → emit `(list e1 e2 …)`. Requires recursing through
  a Seq or chained-Cons of Exprs.
- Real SMT-LIB form: emit `(declare-fun …)` + `(assert …)` wrappers
  around the expression. Needs accumulator structure to know what
  to wrap with.

**Pattern**: add match arms to the `proc_is_*` predicates in
`test_ast_to_text.ev`, push corresponding WorkItem chains.

**Time estimate**: 1 hour, 30-50 LOC in the FSM + 1 test fixture.

### D. End-to-end pipeline (UX-heavy, hits parser limits)

**Goal**: wire lexer + parser + translator into ONE Evident program
that reads source and emits SMT-LIB text in one run.

**Approach**: a multi-phase FSM with `phase ∈ Int` state. Phases
are 0 (read), 1 (lex), 2 (parse), 3 (translate), 4 (print + exit).

**Why this is tricky**: such a program is 200+ lines and hits the
Evident parser's tolerance for multi-line ternaries (the parser
errors out with "expected schema/claim/type/import/enum, got Eq"
when ternaries span too many lines or nest too deep). An attempt
in this session hit the limit.

**Workaround**: structure ternaries inline (force one-line per
ternary) or hoist intermediate Bool variables more aggressively.
The parser doesn't have a documented depth/length limit; it's
empirical.

**Time estimate**: 2-3 hours, 200-300 LOC + 1 test fixture.

### E. Add Datatype state encoding fidelity check (low-risk)

**Goal**: verify the kernel's `decode_datatype_value` / `Sv::smtlib`
round-trip is correct for deeply nested or unusual Datatype values.

**Why**: iter 3.2 added Datatype state carry; current tests
exercise relatively shallow trees (TokenList up to ~5 elements,
Expr trees up to ~4 nodes). A real compiler will hit deeper trees.

**Pattern**: write `tests/kernel/test_deep_datatype.ev` that builds
a deeply-nested value (e.g. 50-element TokenList, or 10-deep
EBinOp), carries it across ticks, and verifies it survived intact.

**Time estimate**: 30 min, 50 LOC.

## Discovered language gaps (worked around, NOT fixed)

These hit during iter 3 and have stdlib-side workarounds. **Per
CLAUDE.md, don't fix in runtime/**. If you find a workaround
doesn't fit your case, document the gap and find a different
workaround.

### Translator gap: `match` returning enum-typed bodies

`o = match t Plus ⇒ OpPlus | Eq ⇒ OpEq | _ ⇒ OpPlus` where `t` is a
Token and the arm bodies are `Op` enum values → "dropped constraint."

**Workaround**: hoist matches to Bool intermediates:
```evident
is_plus ∈ Bool = (t matches Plus)
is_eq   ∈ Bool = (t matches Eq)
o = (is_plus ? OpPlus : (is_eq ? OpEq : OpPlus))
```

### Translator gap: first-line claim params of enum type

`claim TokenToOp(t ∈ Token, ...)` doesn't translate `matches`
recognizers on `t` correctly when called via `TokenToOp(t ↦ Plus, ...)`.

**Workaround**: put all vars in the claim BODY, use names-match
composition at call sites:
```evident
claim TokenToOp
    t ∈ Token
    o ∈ Op
    ...
```

### Parser footgun: `= binds tighter than ∧/∨/<`

`c ∈ Bool = #a > 0` parses as `(c = #a) > 0`. ALWAYS wrap the RHS
of a Bool assignment in parens when it has comparisons or logical
operators:
```evident
c ∈ Bool = (#a > 0)
```

### Parser footgun: multi-line ternaries

Long multi-line ternary expressions trip the parser. If you see
`parse error: expected schema/claim/type/import/enum, got Eq` at
the top level, look for an unclosed multi-line ternary above.

**Workaround**: keep ternaries on one line where possible, or
hoist intermediates aggressively.

### Parser footgun: literal newlines inside string literals

`"abc\ndef"` (with an actual newline mid-string) → "unterminated
string literal". Use the `\n` escape sequence instead.

## FSM patterns reference

The four working architectural patterns from iter 3:

### Pattern 1: Accumulator (lexer)
**When**: input is a sequence; output is a sequence of values built up.

State:
- `pos ∈ Int` — input position
- `partial ∈ String` or `partial_int ∈ Int` — accumulating run
- `tokens ∈ TokenList` — built-up output

Per-tick: classify cur_char, decide
`continuing / starting / finishing / pass`, update partial,
prepend to tokens on finishing. Halt at end-of-input + no pending
partial.

**Examples**: `test_multichar_ident.ev`, `test_multichar_int.ev`,
`test_consolidated_lexer.ev`.

### Pattern 2: Consumer (parser)
**When**: input is a Datatype-typed sequence; output is an
accumulating Datatype.

State:
- `list ∈ TokenList` (or whatever input enum) — remaining input
- `current ∈ Expr` (or whatever output enum) — building output
- helper carry vars (e.g. `pending ∈ Op`)

Per-tick: `match _list TLCons(h, t)` to extract head + advance to
tail. Classify head, update output. Halt when remaining list is
TLNil.

**Examples**: `test_serializer.ev`, `test_parser.ev`.

### Pattern 3: DFS via work-stack (walker)
**When**: walking a recursive Datatype tree depth-first.

State:
- `stack ∈ ExprList` (or WorkList) — pending items
- output state (counter, accumulated string, …)

Per-tick: pop head, classify, push children if compound. Halt on
empty stack. Order: push right then left for preorder DFS.

**Examples**: `test_ast_walker.ev`, `test_ast_to_text.ev`.

### Pattern 4: Mode-state (context-sensitive)
**When**: input semantics depends on context (inside string, inside
comment, etc.).

State:
- `mode ∈ Int` (or richer enum) — current context
- everything else needed per mode

Per-tick: classify cur_char, decide mode transition, dispatch
based on mode + char.

**Examples**: `test_comment_lexer.ev`, `test_consolidated_lexer.ev`.

## File-by-file quick reference

```
CLAUDE.md                            — invariants (READ FIRST)
docs/plans/NEXT.md                   — this file
docs/plans/iter-3-status.md          — current state (READ SECOND)
docs/plans/iter-2-status.md          — kernel + runtime evolution
docs/plans/kernel-input-spec.md      — SMT-LIB shape contract
docs/plans/kernel-iteration-1.md     — original kernel plan
docs/rust-runtime-justification.md   — runtime baseline audit

runtime/src/                         — Rust compiler (~10,400 LOC, language-frozen;
                                       emit.rs got +30 LOC in iter 3.2 for Datatype carry)
kernel/src/                          — trampoline + libffi (~880 LOC; +70 in iter 3.2
                                       for Datatype state decode/encode)
stdlib/
  kernel.ev                          — Effect/Result/LibArg + Build* sugar
  lexer.ev                           — Token + TokenList + char predicates + DigitToInt + MaybeKeyword (~104 LOC)
  parser.ev                          — Op + Expr + ExprList + WorkItem/List + TokenToOp (~57 LOC)
  combinatorics.ev                   — Distinct / Sorted
  toposort.ev                        — Toposort claim

tests/lang_tests/                    — `claim sat_*` / `unsat_*` test files
tests/kernel/                        — Evident programs run via kernel
tests/conformance/                   — Python CLI black-box tests
scripts/run-lang-tests.py            — lang test driver
scripts/run-kernel-tests.py          — kernel test driver
scripts/dump-codebase.sh             — emit codebase as one markdown blob
scripts/strip-comments.py            — Rust comment stripper for the dump
test.sh                              — runs all 5 phases (~3s)
```

## Git state

Branch: `rust-runtime-shrink` (pushed to origin).
Last commit at handoff: `e027bd8 docs: iter-3-status — all three compiler stages demonstrated`
All work in this session has been committed and pushed.

## Communication style for the next session

Per CLAUDE.md "Tone and style": be concise, give short updates at
key moments, end with a 1-2 sentence summary. Don't ask
clarifying questions when the project conventions answer them
(grep first, then ask).

## When in doubt

- If a feature seems missing → check stdlib first, then propose
  adding it there (not to runtime/)
- If a test fails → run `./test.sh --kernel` to isolate, check
  the test fixture's expected output matches reality (Z3 escape
  conventions, indexing semantics, etc.)
- If the parser errors at top level → look for unclosed multi-line
  ternaries; fall back to inline form
- If a constraint drops → check "Discovered language gaps" above
  for a known workaround

Good luck. The architecture is proven. The runway is mechanical.
