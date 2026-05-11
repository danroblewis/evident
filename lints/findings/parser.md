# Findings: runtime/src/parser.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None.

- AP-001 (no library-specific in language-core): parser.rs is in
  scope. Scanned the file for `Sdl[A-Z]`, `SDL_`, `\bGl[A-Z]`,
  `Glsl`, `Audio[A-Z]`, `\.dylib`, `\.framework/`,
  `/opt/homebrew/lib/`, `/usr/lib/lib` — zero hits. The single
  `Z3` occurrence at line 101 is in a doc comment explaining
  the downstream meaning of variant names, which is exempt under
  AP-001's "tokens in line / doc comments don't count" exception.
  Clean.
- AP-002, AP-003, AP-006, AP-007, AP-008: examples-only scope; not
  applicable.
- AP-004: conformance-only scope; not applicable.
- AP-005: applies to `runtime/tests/**.rs`. The in-file
  `#[cfg(test)] mod tests` carries no `#[ignore]` annotations
  across its 26 test functions. Clean.

## Per-file-invariant check

The parser's invariants from `lints/runtime-invariants.md` are:

  1. `Vec<Token>` → `Program` via hand-rolled recursive descent.
  2. Must NOT do semantic checking — names not bound, types not
     matching, claims not found.
  3. Must NOT reach into Z3, call any C library, or directly emit
     translated output.
  4. Should not hold mutable global state.
  5. Should not depend on `pretty.rs`.
  6. Errors should describe syntactic problems with `(line, col)`.
  7. Dependencies: `crate::ast::*` + `crate::lexer::Token` only.

Status:

- (1) Holds. `pub fn parse_program` returns `Program`; the body is
  43 `parse_*` recursive-descent functions.
- (2) Holds. No name-resolution, no type-checking, no claim/schema
  lookup. The hardcoded `Seq | Set | Bag | Map` strings at lines
  337 and 583 are *parser disambiguation* (decide whether
  `Foo(Bar)` is a compound type-name or positional pins), not
  semantic validation — they don't reject anything, they only
  format the type string differently. `Expr::Call` is emitted for
  every `name(args)` shape and the translator sorts out which
  names are builtins (line 1242–1245 comment confirms).
- (3) Holds. Zero `z3::`, `Z3_`, `libffi`, `libloading`,
  `extern "C"`, `Solver`, `Cif`, `dlopen` in source.
- (4) Holds. Zero `static mut`, `lazy_static!`, `OnceCell`,
  `Mutex`, `RefCell`, etc. All state lives on `Parser { toks,
  pos }`.
- (5) Holds. Zero `pretty` references; only `crate::ast::*` and
  `crate::lexer::Token` are imported.
- (6) **Violated** — see "Candidate new rules" below. Of 30
  `ParseError(format!(...))` sites, 0 mention `(line, col)`.
  The structural reason is that `lexer::Token` is a bare
  `enum Token` with no position field, so the parser has nothing
  to attach. The lexer DOES track `line`/`col` internally (used
  in `LexError`) but discards them when emitting tokens.
- (7) Holds. Two crate imports at the top: `crate::ast::*` and
  `crate::lexer::Token`. Nothing else.

## Candidate new rules

### Suggested AP-009: parser-errors-include-source-position

**Pattern observed at parser.rs throughout (30 sites).** Examples
from the file:

> `return Err(ParseError(format!("expected {:?}, got {:?}", expected, self.peek())))` (line 40)
> `return Err(ParseError(format!("expected schema/claim/type/import/enum, got {:?}", other)));` (line 90)
> `return Err(ParseError(format!("expected enum name, got {:?}", other)));` (line 111)
> `return Err(ParseError("expected newline + indented arms after `match scrutinee`".into()));` (line 1156)

**Why it might be bad.** The per-file invariant for parser.rs is
explicit: "Errors should describe syntactic problems with
`(line, col)`." Every `ParseError` produced by parser.rs today
carries only a Rust-`Debug` rendering of the offending token, with
no source coordinates. A user error from a 200-line `.ev` file
gets diagnostics like `expected ',' or '∈' after param name, got
Ident("\u{2208}")` with no indication of WHERE in the file the
problem is. Unlike the lexer (which always emits `lex error at
line X, col Y: ...`), parser errors are single-point regressions
in user-facing diagnostics.

The structural blocker is in `lexer.rs`: `pub enum Token` carries
no position field, so the parser has nothing to attach. Lexer
tracks `line`/`col` internally for `LexError` but throws them
away when constructing each `Token` push. This is also why the
fix isn't a one-line PR — it needs the lexer changed first.

**Suggested fix.** Change `lexer::Token` from a bare enum to a
`struct Token { kind: TokenKind, line: usize, col: usize }` (or
attach a `Span`). Update the lexer's emit sites to fill those.
Then in parser.rs, switch from `format!("…, got {:?}", peek)` to
`format!("at line {}, col {}: …, got {:?}", peek.line, peek.col,
peek.kind)` — or carry a structured `ParseError { line, col,
message }` matching `LexError`'s shape. The lexer-side change is
mechanical (every `tokens.push(Token::X)` becomes
`tokens.push(Token { kind: X, line, col })` with the loop's
existing `line` / `col` locals).

**Detection idea.** Grep for the absence of `line` / `col` in
`ParseError` constructions in `runtime/src/parser.rs`. Concretely:
fail if any `ParseError(` call site in parser.rs lacks both
`.line` and `.col` (or the words `line` / `col` in the format
string). Could also be enforced as "every `ParseError` in
parser.rs must carry a `line` and `col` field" once the
struct-shape change lands.

**Verdict.** Clears the bar. The pattern is concrete (30
identical-shape sites), the fix is constructive and mechanical
(lexer change + sed across parser), and it WILL recur — every
new `parse_*` function adds another error site. Worth a rule.
However: do NOT add to `lints/checks.sh` until the lexer-side
prerequisite is filed and accepted, because every parser-side
fix depends on a Token shape that doesn't exist yet. Mark this
as `Status: proposed` in the rule file pending the lexer change.

### Review-only observation: hardcoded compound-head set

**Pattern observed at parser.rs:337 and parser.rs:583.**

> `if matches!(head.as_str(), "Seq" | "Set" | "Bag" | "Map") && matches!(self.peek(), Token::LParen)` (337)
> `let is_known_compound_head = matches!(head, "Seq" | "Set" | "Bag" | "Map");` (583)

Two sites enumerate the same string set to disambiguate "compound
type name" (`Seq(Int)` is a type) from "positional pins"
(`IVec2(0, 0)` is a record literal). A new compound type kind
(e.g. `Vec(Int)` or `Tree(Int)`) would need both sites updated and
nothing in the type system enforces the pair stays in sync.

**Why it might be bad (but doesn't clear the bar).** This is real
duplication and it's parser-internal — not strictly a layering
violation. It's borderline "library knowledge in the parser"
(Seq/Set/Bag/Map are stdlib container kinds), but the parser
needs SOME way to distinguish the two `Foo(arg)` shapes
syntactically and there isn't a great purely-grammatical
alternative (`Foo(arg)` with `arg` being a type vs `Foo(arg)`
with `arg` being a value isn't decidable at parse time without
this kind of carve-out).

**Verdict: review-only.** The duplication is two sites and the
list of compound heads is genuinely closed (no more are planned).
A `const COMPOUND_HEADS: &[&str] = &["Seq", "Set", "Bag", "Map"];`
constant at the top of parser.rs would dedupe but doesn't rise
to a rulebook entry.

### Review-only observation: inline duplicate of `peek_compare_op`

**Pattern observed at parser.rs:1010-1018.**

> ```rust
> let op = match self.peek() {
>     Token::Eq  => Some(BinOp::Eq),
>     Token::Neq => Some(BinOp::Neq),
>     ...
> };
> ```

The same six-arm match is also `peek_compare_op` at line 1330 and
is called via `peek_compare_op(self.peek())` at lines 472, 511,
1031, 1034. The `parse_compare` site (1010) is the only one that
inlines.

**Verdict: review-only.** A minor refactor; trivially fixed by
changing line 1010 to `let op = peek_compare_op(self.peek());`.
Not rule-worthy.

## Clean

The file is clean against all 8 active rules. Three concerns
against its per-file invariants:

  1. Source-position-free errors — the only meaningful one. Worth
     promoting to a rule once the lexer-side `Token` shape is
     extended; held back from the rulebook today because the
     lexer change is a prerequisite that doesn't exist yet.
  2. Hardcoded compound-head set duplicated in two sites —
     review-only.
  3. Inline duplicate of `peek_compare_op` — review-only.
