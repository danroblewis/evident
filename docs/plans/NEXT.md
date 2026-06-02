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

## Where we are in the roadmap

See `docs/plans/completion-roadmap.md` for the authoritative phase
plan. As of this handoff:

- **Phase A (lexer parity)**: ✅ 7/7 done. Token enum mirrors the
  Rust lexer; `MaybeKeyword` + `SingleCharTok` + `MaybeTwoCharOp` +
  string-literal / float / indent helpers all live in
  `stdlib/lexer.ev`. The oracle (`scripts/lexer-oracle.py`) is in
  place; its expected-pass corpus should be re-run + expanded.
- **Phase B (parser parity)**: 0/11 sub-steps. **Start here.**
- **Phase C (translator)**: 0/14 sub-steps. Blocked on B.
- **Phases D-F**: not started.

## Concrete next-session proposals (pick one)

Each is ~1 session of focused work. The first three map directly to
roadmap Phase A sub-steps; the others are useful side-quests.

### Phase B sub-step (current focus)

Pick a sub-step from roadmap Phase B. The Phase A lexer now emits
the full token vocabulary, so the parser can be written against it.

| Sub-step | What | Difficulty |
|---|---|---|
| B1 | Multi-binop precedence | medium (shift-reduce or precedence-climb FSM) |
| B2 | Parenthesized subexpressions | low (mode-state for paren depth) |
| B3 | Type parsing (TypeName, Seq(T), generics Edge&lt;T&gt;) | low-medium |
| B4 | Membership body items (`x ∈ Type`, chained membership) | medium |
| B5 | Schema declarations (`claim Name body…`) | medium (multi-token productions) |
| B6 | Enum declarations | low (similar to B5) |
| B7 | Pull-up `..ClaimName`, names-match composition | low |
| B8 | Quantifiers `∀ x ∈ S : body` | medium |
| B9 | Match expressions + patterns | medium-high |
| B10 | All 7 composition mechanisms surfaced as parse productions | medium (coverage check) |
| B11 | Import statements | low |

**Pattern**: extend `Expr` / new `BodyItem` / `SchemaDecl` / `Program`
enums in `stdlib/parser.ev`. The parser FSM uses mode-states for
recursive descent (like comment-skipping in iter 3.7) and a `pending`
slot for precedence/shift-reduce (like the toy parser in iter 3.11).

**Time estimate**: 1-2 hours per sub-step. Total Phase B: 10-20 sessions.

### Phase A oracle expected-pass update (high-value cleanup)

The oracle's expected-pass corpus was set when Phase A had 0/7
sub-steps done. Now that all 7 are complete, re-run it and update
which inputs match — some "expected-failure" cases (subclaim, true/
false, in, mapsto, etc.) should now PASS.

**Time estimate**: 30 minutes. Pure script work, no Evident code.

### End-to-end pipeline experiment (UX-heavy, hits parser limits)

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

### Datatype state encoding fidelity check (low-risk)

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

Branch: `rust-runtime-shrink`. All prior work has been pushed to origin.

**After each commit, push to origin:**
```bash
git push origin rust-runtime-shrink
```

This is part of the handoff contract — leaving local-only commits
strands them if the working tree is wiped or another session opens
elsewhere. The agent that tested this handoff in iter 3.14 committed
locally but didn't push; a manual push was needed to complete the
chain.

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
