# Adding `if/then/else` to Evident — Implementation Plan

Resumable plan for adding `if cond then a else b` as an
expression-level construct in Evident. Designed so a fresh context
can pick this up cold; battery-low note from the user prompted the
write-down.

## Why this exists

User has long wanted if/then/else. Independent of any pending
self-hosted-desugar work, this is a generally useful construct
that sharpens the language for users writing claims with
conditional values. Examples it enables:

```evident
-- Today: requires multi-line dispatch via implications
result ∈ Int
state = Win  ⇒ result = 100
state = Loss ⇒ result = 0

-- With if/then/else:
result = if state = Win then 100 else 0
```

```evident
-- Componentwise on records
clamped.x = if pos.x < 0 then 0 else pos.x
```

```evident
-- Inside larger expressions
total = base + if has_bonus then 50 else 0
```

## Design choices to make BEFORE writing code

Pick one option per row before implementing. Defaults are
recommended; deviations should have a reason in the commit message.

| Choice | Options | Recommended |
|---|---|---|
| Surface keyword | `if`/`then`/`else` vs. `?:` ternary | **`if`/`then`/`else`** (matches user's mental model; `?:` clashes with set comprehensions and feels Cish) |
| Multi-branch | `if/elif/else` vs. nested `if/then/else` only | **Nested only** (parser doesn't need new keyword; `elif` is sugar for `else if`) |
| `else` required? | Yes vs. no (open `if`) | **Required for value position** (no value means no expression). For Bool body context, optional `else` defaults to `true`. |
| Block form | Indented body branches like `⇒` | **Yes** — matches `⇒` and quantifier blocks. `if cond then\n    A\n    B\nelse\n    C` |
| Whitespace sensitivity | Block form requires deeper indent for branch | **Yes** — same rule as `⇒` indented form |
| Type unification | Must both branches be the same type? | **Yes** — Z3 ITE requires it. Mismatch is a translator error. |
| Nesting limit | Hardcoded vs. unbounded | **Unbounded** — recursive parser handles arbitrary depth |

## AST shape

Add to `runtime/src/ast.rs`:

```rust
pub enum Expr {
    // ... existing variants ...
    /// `if cond then then_branch else else_branch` —
    /// expression-level conditional. Z3 maps to Bool::ite for
    /// every value type that supports ITE (Int, Real, Bool, Str,
    /// Datatype, …). Both branches must translate to the same
    /// Z3 sort.
    IfThenElse(Box<Expr>, Box<Expr>, Box<Expr>),
}
```

## Lexer (`runtime/src/lexer.rs`)

Add three new keywords:

```rust
pub enum Token {
    // ... existing ...
    If,
    Then,
    Else,
}

// In keyword_or_ident:
"if"   => Token::If,
"then" => Token::Then,
"else" => Token::Else,
```

No new operators or symbols. Single-line implementation.

## Parser (`runtime/src/parser.rs`)

Add an `if` arm to `parse_atom` (or wherever the highest-precedence
expression slot is). Two forms:

**Inline form:** `if cond then a else b` — single line, all on one
expression:

```rust
fn parse_if(&mut self) -> Result<Expr> {
    self.bump(); // if
    let cond = self.parse_expr()?;
    self.eat(&Token::Then)?;
    let then_branch = self.parse_expr()?;
    self.eat(&Token::Else)?;
    let else_branch = self.parse_expr()?;
    Ok(Expr::IfThenElse(
        Box::new(cond),
        Box::new(then_branch),
        Box::new(else_branch),
    ))
}
```

**Block form:** indented branches (mirror `⇒` block syntax):

```evident
result = if state = Win then
    100
else
    0
```

Detection: after `then`, check for Newline + Indent. If present,
parse a stack of expressions at deeper indent and AND-combine them
(though for if-then-else "ANDing" doesn't make sense for value
branches — the block form is more useful for Bool branches; for
value branches with multiple statements, use sequencing or
parens). **Recommendation: skip block form initially** — inline
form covers 95% of use cases. Add block form if user asks.

## Translator (`runtime/src/translate/exprs.rs`)

The `IfThenElse` variant needs handling in **every** `translate_*`
path because the branches can be any type:

```rust
fn translate_int<'ctx>(...) -> Option<Int<'ctx>> {
    // ... existing arms ...
    Expr::IfThenElse(c, a, b) => {
        let cond = translate_bool(c, ctx, env, schemas)?;
        let then_v = translate_int(a, ctx, env)?;
        let else_v = translate_int(b, ctx, env)?;
        Some(cond.ite(&then_v, &else_v))
    }
}
// Same shape for translate_bool, translate_real, translate_str.
```

**Z3's `Ast::ite` method**: takes a Bool condition and two values
of the same sort. Returns a value of that sort. The z3 crate
exposes this on the `Bool` type as `bool.ite(then_ast, else_ast)`.

**For Datatype/enum branches** (likely needed if Stage 12+ desugar
work uses if/then/else over enum-typed values): `resolve_enum_ast`
gets a new arm:

```rust
Expr::IfThenElse(c, a, b) => {
    let cond = translate_bool(c, ctx, env, schemas)?;
    let then_v = resolve_enum_ast(a, ctx, env, schemas)?;
    let else_v = resolve_enum_ast(b, ctx, env, schemas)?;
    Some(cond.ite(&then_v, &else_v))
}
```

Z3's `ite` works for any sort.

## stdlib/ast.ev

Add the new `Expr` variant:

```evident
enum Expr =
    -- ... existing 17 variants ...
    EIfThenElse(Expr, Expr, Expr)
```

Encoder + decoder updates (`encode_ast.rs` + `decode_ast.rs`):
add the corresponding arm. Both are mechanical.

## What this DOES enable

- Value-level conditionals in user code (the headline win).
- Compact dispatch tables in `type` definitions.
- Some shorter expressions in passes (replacing `(c ⇒ a) ∧ (¬c ⇒ b)`
  with `if c then a else b` where the result is a Bool/value).

## What this does NOT enable (honest scope)

- **It does not enable the desugar migration.** Bare-identifier-as-
  passthrough still needs to express:
  ```
  output_body[i] = if (∃ name : input_body[i] = BIConstraint(EIdentifier(name))
                              ∧ name ∈ claim_names)
                   then BIPassthrough(name)
                   else input_body[i]
  ```
  Even with if/then/else, the `then` branch can't reference the `name`
  bound by the `∃` in the condition — Evident's `∃` doesn't export
  bindings. The migration needs **either** pattern matching with
  guards **or** per-element rewriting via runtime decomposition
  (separate plan; see `self-hosting-status.md`'s migration section).

- It does not add `let` bindings.

- It does not add lambdas or higher-order list operations.

These are separate language features. If/then/else is independently
useful but doesn't unlock the migration on its own.

## Implementation order (resumable)

Each numbered step is a single commit. Each is self-contained;
running tests after each should pass.

### Step 1: Lexer
- Add `Token::{If, Then, Else}` variants in `lexer.rs`.
- Add the three keyword strings to `keyword_or_ident()`.
- Add lexer unit tests for tokenizing `if cond then a else b`.

### Step 2: AST
- Add `Expr::IfThenElse(Box<Expr>, Box<Expr>, Box<Expr>)` to `ast.rs`.

### Step 3: Parser (inline form)
- Add `parse_if` in `parser.rs`.
- Hook into `parse_atom` (or wherever the initial expression slot
  recognizes new keywords).
- Add 4-5 parser unit tests covering: simple inline, nested
  if-in-then-branch, nested if-in-else-branch, missing `then`
  errors, missing `else` errors.

### Step 4: Translator (Int + Bool + String + Real)
- Add `Expr::IfThenElse` arm to `translate_int`, `translate_bool`,
  `translate_str`, `translate_real`.
- Each uses `cond.ite(&then_v, &else_v)`.
- Add integration tests in `runtime/tests/`: queries with
  `result = if cond then a else b` for each scalar type.

### Step 5: Translator (enum/Datatype)
- Add `Expr::IfThenElse` arm to `resolve_enum_ast`.
- Test: `today = if is_weekend then Sat else Mon`.

### Step 6: Encoder + decoder for `EIfThenElse`
- `stdlib/ast.ev` gains `EIfThenElse(Expr, Expr, Expr)`.
- `encode_ast.rs:encode_expr` adds the arm.
- `decode_ast.rs:decode_expr` adds the arm.
- A round-trip test in `tests/roundtrip_ast.rs`.

### Step 7: Type-mismatch error message
- When the two branches translate to different sorts, the Option
  chain returns None and the constraint drops. Surface a better
  error: detect the mismatch in the translator and produce a
  named error rather than silent drop.
- Test: `if cond then 5 else "hi"` should produce a clear error.

### Step 8: Documentation
- Update `CLAUDE.md` — add `if/then/else` to the operator
  precedence table, the syntax decision-guide table, and the body
  conventions section.
- Update `docs/rust-runtime-capabilities.md`'s expression-syntax
  table.

## Tests to write (across steps)

| Test | File | What it covers |
|---|---|---|
| `lex_if_then_else_keywords` | `parser/tests/...` or inline lexer test | Tokens emitted correctly |
| `parse_if_then_else_inline` | `parser.rs::tests` | Basic `if a then b else c` |
| `parse_if_then_else_nested_then` | same | `if a then if b then x else y else z` |
| `parse_if_then_else_nested_else` | same | `if a then x else if b then y else z` (the `elif` pattern) |
| `parse_if_then_else_missing_then_errors` | same | Clear error |
| `parse_if_then_else_missing_else_errors` | same | Clear error |
| Conformance: `tests/lang_tests/test_if_then_else.ev` | new file | sat/unsat tests for value branches of each type |
| Round-trip: `roundtrip_if_then_else` | `tests/roundtrip_ast.rs` | Encode + decode |

## Things to watch

1. **Operator precedence.** Where does `if/then/else` fit? It's
   essentially a prefix-y construct — should be at low precedence
   (above quantifiers) so `if a then b + 1 else b - 1` parses as
   intended. Likely needs to live near the top of `parse_expr`,
   maybe just below quantifiers.

2. **Conflict with existing identifier `if`?** Check that no test
   programs use `if` as a variable name. If they do, those
   programs will break — easy to find with a `grep ' if ' programs/`.

3. **Z3 ITE on Datatype values.** Should work via the same
   `Bool::ite` path, but verify with a test (Step 5). Z3 does
   support ITE over algebraic datatypes natively.

4. **Empty/unit branches.** `if cond then else` with empty
   branches — should be a parse error, not a silent skip. Test it.

5. **Boolean fold.** `if cond then a else b` where both are Bool
   could fold to `(cond ⇒ a) ∧ (¬cond ⇒ b)` — Z3 handles this
   natively via ITE, no special-casing needed. Don't pre-optimize.

6. **Performance.** ITE in Z3 is well-supported; no perf concern
   for typical use. The if-then-else over deep Datatype values
   (relevant for desugar passes if/when) might be slower than
   straight equality, worth measuring but not pre-fixing.

## Resume points

If picking this up cold:

1. Read this plan top-to-bottom.
2. Read `docs/design/self-hosting-status.md` for context on what
   self-hosting state we're in (so the "what this does not enable"
   section makes sense).
3. Start at **Step 1**. Each step is independently committable.
4. Running tests between steps catches regressions early.
5. **No deep design decisions remain** — the choices in the
   "Design choices" table are pre-made (defaults). Only deviate if
   there's a concrete reason that emerges during implementation.

## Estimated scope

- Lexer: 5 lines
- AST: 5 lines
- Parser: ~30 lines + ~50 lines of tests
- Translator: ~40 lines (4-5 paths × ~8 lines each)
- Encoder/decoder: ~10 lines each + 1 test
- Conformance .ev tests: ~80 lines
- Docs: ~30 lines

Total: ~250 lines added, ~5 commits, probably 1 focused session
to ship if no surprises.

## Status (as of plan write, commit `0335473`)

- ✅ Decoder shipped (round-trip tested)
- ✅ Self-hosting state documented (`self-hosting-status.md`)
- ⏳ if/then/else: this plan is the resumable starting point. No
  code yet.
- ⏳ Migration of `bare-identifier-as-passthrough`: blocked on
  either if/then/else (partial unblock) or pattern-matching with
  guards (full unblock) or per-element rewriting infrastructure
  (alternative path).
