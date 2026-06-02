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

## What's left for full self-hosting

### Iter 3.10+ — complete the lexer
- Unicode operators (∈, ⇒, ⟨, ⟩, ↦, ≤, ≥, ≠) — same shape as
  SingleCharTok but with multi-byte chars
- Two-char operators (==, →, ::, …)
- String literals "…" with escape handling — mode-state pattern
  from 3.7 + content collection + `\n` `\"` `\\` recognition
- Float literals
- Indentation tracking (for body-item nesting)

Each chunk: 50-100 LOC of stdlib/lexer.ev. No architectural moves.

### Iter 3.11+ — extend the parser (toy → complete)
Iter 3.11 proved the parser-as-FSM pattern. To grow toward a full
Evident parser:
- More Token variants → more arms in TokenToOp + dispatch logic
- More AST variants (Membership, ClaimCall, SchemaDecl, EnumDecl)
  → grow `Expr` / new `BodyItem`, `SchemaDecl`, `Program` enums
- Operator precedence → multiple `pending` slots + reduce logic
- Recursive descent for nested expressions → mode-state machine
  (like the comment-skipping pattern from iter 3.7)
- Multi-token productions (`claim Name body`) → multi-mode FSM

Each is incremental work on stdlib/parser.ev + the FSM. No more
architectural moves.

### Iter 3.12+ — AST → SMT-LIB translator
- FSM that walks the parsed AST and produces SMT-LIB text via
  WriteFile or stdout
- For each AST node, emit the corresponding SMT-LIB form
- The biggest piece in raw LOC; probably 500+ LOC of Evident

### Iter 3.13 — bootstrap
- Self-host the lexer in Evident running on the kernel
- Replace `runtime/src/lexer.rs` (360 LOC) with the Evident lexer
- Verify byte-for-byte equivalent output
- Same for parser, then translator
- Each stage reduces `runtime/src/` LOC

### Iter 3.14 — delete the Rust compiler
- Once the self-hosted compiler can compile itself,
  `runtime/` becomes bootstrap-only
- Move it to `bootstrap/`
- Final state: `kernel/` + `bootstrap/runtime/` + Evident `.ev`
  files. The runtime IS the kernel; the compiler is Evident.

## Code state

```
runtime/src/        ~10,500 LOC (UNCHANGED in iter 3 — invariant held)
kernel/src/         ~820 LOC    (+70 from iter 3.2 Datatype carry)
stdlib/             ~470 LOC    (~150 added in iter 3.1-3.11)
  ├── combinatorics.ev
  ├── kernel.ev
  ├── lexer.ev      ~120 LOC    (Token/TokenList + predicates + MaybeKeyword + DigitToInt)
  ├── parser.ev     ~30 LOC     (Op + Expr + TokenToOp)
  └── toposort.ev
tests/kernel/       21 programs (all green)
```

Per CLAUDE.md, the Rust runtime LOC should be **trending toward
zero**. Iter 3 didn't shrink it (foundation work), but iter 3.13+
starts the reduction.

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

All 21 tests run via `./test.sh --kernel` in ~1s.
