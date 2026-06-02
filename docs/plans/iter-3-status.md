# Iteration 3 — status

**Both halves of a compiler (lexer + parser) now run as Evident
programs on the kernel.** All the architectural primitives needed
for full self-hosting are demonstrated by working programs. The
remaining work to delete `runtime/src/` is mechanical.

## What works after iter 3

### Multi-tick state evolution (3.0)
FSMs evolve primitive state across ticks via the `_<name>` /
`<name>` carry mechanism. Demonstrated by `test_counter.ev`
(counts 0..4) and `test_echo_lines.ev` (stdin read loop).

### Datatype state carry (3.2)
Kernel decodes Datatype values from the model via recursive walk
(`decode_datatype_value`) and re-emits as SMT-LIB literals for the
next tick's `_<name>` carry. Lets FSMs carry algebraic data like
`TokenList` across ticks. Demonstrated by `test_tokens_carry.ev`.

### Character classification (3.1)
`stdlib/lexer.ev` provides reusable character predicates:
- `IsDigitChar(c, out)`
- `IsAlphaChar(c, out)`
- `IsWhitespace(c, out)`
- `DigitToInt(c, n)` — lookup table inverse of single-digit `str_from_int`
- `SingleCharTok(c, t, recognized)` — produces Token + accept flag
- `MaybeKeyword(s, t)` — classifies an alpha run as Kw* or Ident

### Multi-char accumulator pattern (3.4 + 3.5)
The "collect a run into one token" pattern:
- Carry a `partial` value across ticks
- On each tick: `continuing` / `starting` / `finishing` / passive
- On `finishing` tick: emit the completed token AND optionally a
  boundary single-char token (via nested `TLCons` two-deep)
- Always advance pos; no "stand still on boundary" trick needed

Two flavours:
- Alpha runs: `partial ∈ String`, `_partial = ""` means not collecting
- Digit runs: `partial ∈ Int`, `_partial < 0` means not collecting

### Keyword recognition (3.6)
Adding 7 nullary `Kw*` variants to the Token enum + a 7-arm
`MaybeKeyword` lookup ternary. At the alpha-run boundary, the
collected string is classified via `MaybeKeyword`. `KwClaim`
vs `Ident("claim")` is naturally distinguished.

### Mode-state lexing (3.7)
A `mode ∈ Int` state field carries the current lexer context:
- 0 = normal
- 1 = inside line comment

Mode transitions on cur_char + next_char lookahead. While
`mode = 1`, all chars are silently consumed. The pattern
generalises to:
- Block comments (`/* … */`)
- String literals (`"…"` with escape handling)
- Raw strings, multi-line strings, heredocs
- Any context-sensitive lexer mode

### File I/O integration (3.3)
Tick 0: `effects = ⟨ReadFile(path)⟩` (single effect).
Tick 1: `last_results[0]` is `StringResult(contents)`; FSM matches
to extract the string into a state field; walk begins at pos=0.
Tick 2+: normal walk.

The "sentinel _input = '' detects haven't-started" pattern
generalises to any "tick 0 bootstraps, tick 1+ does real work."

### Token list serialization (3.9)
The inverse of the accumulator: consume a TokenList one element
per tick, emit each token's textual form. Uses `match _list` to
destructure the Datatype state in three places (head extraction,
tail advance, is-nil check).

This is the parser's primary pattern: each tick reads one input
token, dispatches on variant, advances state.

### Consolidated lexer (3.8)
One program that combines everything: alpha runs, digit runs,
keyword recognition, comment skipping, single-char operators,
whitespace. Processes `"claim x = 1\n"` and produces the correct
TokenList:
  `KwClaim, Ident("x"), Eq, IntLit(1)`

### Toy parser (3.11)
A parser-as-FSM consumes a TokenList element-by-element and builds
a nested `Expr` AST value via Datatype state carry. Input
`⟨IntLit(1), Plus, IntLit(2), Plus, IntLit(3)⟩` produces:
  `EBinOp(OpPlus, EBinOp(OpPlus, EInt(1), EInt(2)), EInt(3))`

Demonstrates: left-associative greedy combination, `current` and
`pending` carry state, `TokenToOp` dispatch via names-match,
`match _list` destructure pattern.

stdlib/parser.ev gives the `Op` and `Expr` enums + `TokenToOp` claim.

### AST tree walker (3.12)
A depth-first traversal of an `Expr` tree via an `ExprList`
work-stack. Each tick pops the head Expr, classifies the variant,
pushes children for compound nodes. Halts on empty stack.

Test: walks `EBinOp(+, EBinOp(+, EInt(1), EInt(2)), EInt(3))` and
correctly counts 3 EInt leaves.

### AST → text translator (3.13)
**THE COMPILER LOOP CLOSES.** Walks an `Expr` tree via a
`WorkList` of `WorkItem` (Process(Expr) | Emit(String)) — pushing
both subtrees and literal-text chunks onto the stack so structural
characters like `(`, ` `, `)` interleave correctly between subtree
visits. Output accumulates in `output_str ∈ String` across ticks.

Test produces `(+ (+ 1 2) 3)` from
`EBinOp(+, EBinOp(+, EInt(1), EInt(2)), EInt(3))` — actual SMT-LIB
prefix notation derived from the AST by an Evident program.

### Translator fidelity: operators + idents (3.14)
The iter 3.13 translator always emitted `(+ ` for any binop and had
no `EIdent` arm, so a parsed `x = 1` would mis-translate. The walker
now reads the actual `Op` from the `EBinOp` (`OpEq → "(= "`, else
`"(+ "`) and emits `EIdent(s)` leaves directly. Test
`test_translate_eq_ident.ev` produces `(= x 1)` from
`EBinOp(OpEq, EIdent("x"), EInt(1))`. No new architecture — added
`proc_is_ident` / `proc_op` match arms to the existing work-stack FSM.

### Translator gaps discovered (worked around, not fixed)

Two cases the runtime translator drops:
- First-line claim params of enum type fail `matches` recognizers.
  Workaround: put all vars in the body, use names-match.
- `match scrutinee` returning enum-typed bodies fails. Workaround:
  hoist match into Bool intermediates + ternary-on-Bool.

Both have stdlib workarounds. Per CLAUDE.md invariant, runtime
unchanged.

## Architectural primitives demonstrated

| Pattern | Where shown |
|---|---|
| Primitive state carry (`_x` / `x`) | test_counter |
| Datatype state carry | test_tokens_carry |
| Read external input via ReadFile | test_file_io, test_file_lexer |
| Walk a string char-by-char | test_lexer_walker |
| Accumulate a run into one token | test_multichar_ident, test_multichar_int |
| Lookup-table classification | DigitToInt, MaybeKeyword in stdlib |
| Two-char lookahead | test_comment_lexer (`--` detection) |
| Mode-state context lexing | test_comment_lexer |
| Consume a Datatype list across ticks | test_serializer |
| Pattern match on Datatype state | test_serializer |
| Multi-effect tick (emit 2 tokens at once) | test_multichar_ident finishing-with-op |

All using the kernel's effect dispatch + libffi + Datatype state
carry. Zero Rust changes in iter 3.

## What's left

See `docs/plans/completion-roadmap.md` for the authoritative plan
from this state to "runtime/ deleted." Phases A-F, sub-steps per
phase, acceptance criteria, LOC estimates.

This doc is **history**; the roadmap is **plan**. Don't put forward-
looking content here.

## Code state

```
runtime/src/        ~10,400 LOC (emit.rs got +30 in iter 3.2 for Datatype state-field
                                  type widening; language semantics unchanged)
kernel/src/         ~880 LOC    (+70 from iter 3.2 Datatype carry)
stdlib/             ~480 LOC    (~160 added in iter 3.1-3.13)
  ├── combinatorics.ev
  ├── kernel.ev
  ├── lexer.ev      ~120 LOC    (Token/TokenList + predicates + MaybeKeyword + DigitToInt)
  ├── parser.ev     ~50 LOC     (Op + Expr + ExprList + WorkItem + WorkList + TokenToOp)
  └── toposort.ev
tests/kernel/       23 programs (all green)
```

Per CLAUDE.md, the Rust runtime LOC should be **trending toward
zero**. Iter 3 was foundation work; the roadmap's Phase A starts
the actual reduction by replacing `lexer.rs` with `stdlib/lexer.ev`.

## Test surface

| File | Asserts |
|---|---|
| `test_hello.ev` | Basic libc puts + exit 0 |
| `test_exit_42.ev` | Custom exit code propagates |
| `test_multiple_prints.ev` | Seq walked in order |
| `test_concat_composition.ev` | `++` joins per-concern effect Seqs |
| `test_libcall_puts.ev` | libffi LibCall to libc `puts` |
| `test_libcall_putchar.ev` | LibCall with `ArgInt` |
| `test_multi_tick.ev` | State + last_results carry across ticks |
| `test_file_io.ev` | 3-tick ReadFile → WriteFile → puts |
| `test_counter.ev` | Multi-tick state evolution + `str_from_int` |
| `test_echo_lines.ev` | Stdin echo loop until EOF |
| `test_lexer_walker.ev` | Per-char classification (iter 3.1) |
| `test_tokens_carry.ev` | TokenList Datatype carry (iter 3.2) |
| `test_real_lexer.ev` | Walker + classify + accumulate (iter 3.1+3.2) |
| `test_file_lexer.ev` | File-driven lexer (iter 3.3) |
| `test_multichar_ident.ev` | Alpha runs → one Ident (iter 3.4) |
| `test_multichar_int.ev` | Digit runs → one IntLit (iter 3.5) |
| `test_keyword_lexer.ev` | Keyword recognition (iter 3.6) |
| `test_comment_lexer.ev` | Comment mode (iter 3.7) |
| `test_consolidated_lexer.ev` | All lexer features combined (iter 3.8) |
| `test_serializer.ev` | TokenList consumer (iter 3.9) |
| `test_parser.ev` | TokenList → Expr via FSM (iter 3.11) |
| `test_ast_walker.ev` | Depth-first Expr traversal + leaf count (iter 3.12) |
| `test_ast_to_text.ev` | Expr → SMT-LIB prefix text (iter 3.13) |

All 23 tests run via `./test.sh --kernel` in ~1s.
