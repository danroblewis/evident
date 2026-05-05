# Rust runtime — progress log

**Read this first when resuming.** Tells you what's done, what's next, and any blockers.

## Current status

**Phase:** v0.1 subset working end-to-end. 5/5 tests green.

**Last action:** All M0-M6 milestones land in one go. The pipeline
(lexer → parser → translate → Z3) handles the SimpleNat shape, plus
multi-var arithmetic, UNSAT detection, and Bool implies.

**Next action:** Pick the next slice of Python features to port. See
"Next slices" below — recommend starting with String literals + `=` on
strings, since that's needed for `assert ground` patterns and the
adventure-game style schemas use them everywhere.

## Milestones

- [x] **M0**: Cargo project compiles, `z3` crate dependency builds, a
  trivial `Solver::new + check()` test passes. Validates toolchain.
- [x] **M1**: AST types defined for the v0.1 subset (SchemaDecl,
  Membership, Expr, BinOp).
- [x] **M2**: Lexer handles ASCII tokens + the Unicode operators
  (`∈`, `∧`, `∨`, `¬`, `⇒`, `≤`, `≥`, `≠`). `--` comments.
  Indentation tracked via `Indent(n)` tokens after `Newline`.
- [x] **M3**: Parser parses `schema/claim/type Name` with indented body
  containing `x ∈ Type` decls and arbitrary expression constraints.
  Standard precedence climbing (implies → or → and → compare → +/- →
  */ → unary → atom).
- [x] **M4**: Translate `n ∈ Nat` to `Int.new_const + n >= 0`. `n ∈ Bool`
  to `Bool.new_const`. Comparisons, arithmetic, boolean combinators.
- [x] **M5**: Runtime API: `EvidentRuntime::new() → load_source(s) →
  query("Name") → QueryResult { satisfied, bindings }`.
- [x] **M6**: First Python-equivalent test passes:
  `SimpleNat { n ∈ Nat ; n > 5 }` returns satisfied with `n > 5`.

## Next slices

- [ ] More numeric ops (=, ≠, <, ≤, ≥, +, -, *, /, mod-via-trick).
- [ ] Bool ops (∧, ∨, ¬, ⇒) and the `⇒`-binds-tighter-than-∧ trap.
- [ ] Multiple variables in one schema.
- [ ] Set literals + ∈ on them.
- [ ] String literals.
- [ ] Type composition (sub-schema field expansion).
- [ ] Quantifiers ∀ / ∃ over literal ranges.

## Known gotchas (record as we hit them)

- **Z3 headers location.** The `z3-sys` crate needs `z3.h` and a libz3
  to link against. We don't have homebrew z3 installed; instead we
  point at the copy bundled with Python's `z3-solver` package (used
  by the parent runtime). See `.cargo/config.toml`. If you move
  Anaconda or upgrade, those paths will break.
- **Bool equality vs Int equality.** `translate_bool` has to try Bool
  operands first and fall back to Int for `Eq`/`Neq`. Otherwise
  `p = true` (Bool) gets routed through `translate_int` and silently
  drops. Same trap exists in the Python translator for indexed Bool
  fields (the "= true / = false" workaround in CLAUDE.md).
- **Initial Indent(0) emission.** Don't emit `Indent(0)` in the
  lexer prologue — the `at_line_start = true` initial state will
  cause the first non-blank stretch to emit one naturally. Otherwise
  you get a duplicate Indent and the parser's first-token check fails.
- **Lexer `at_line_start` bookkeeping is fragile.** When skipping
  blank lines or comment-only lines inside the at_line_start branch,
  remember to keep `at_line_start = true` (don't fall through to the
  general loop).

## Next slices

In rough order of leverage:

- [ ] String literals + `=`/`≠` on strings.
- [ ] More numeric ops we already lex but haven't tested: `mod` via
      `x - (x/n)*n`, integer division precision matches Python.
- [ ] Set literal expressions `{1, 2, 3}` and ∈ over them.
- [ ] Range literals `{0..N}` for use in ∀.
- [ ] Quantifier translation `∀ i ∈ {0..N} : body` — unroll when N is
      a literal int.
- [ ] Sub-schema field expansion (`task ∈ Task` → `task.id`,
      `task.duration`, …) so multi-field types work.
- [ ] Multiple assertions: `assert n = 5` style ground facts.
- [ ] Sequence and Set sorts as Z3 sorts (Seq, Array(T, Bool)).
- [ ] Composite Datatypes for Seq(T)/Set(T) where T is a type.
- [ ] Cardinality `#x` constraints.
- [ ] `..ClaimName` passthrough composition.
- [ ] Claim composition with mappings (`mapsto`).
- [ ] `subclaim` declaration + invocation with fresh internals.
- [ ] Cached evaluator (push/pop) — port the Python optimization.

## Test mapping

| Rust test                                    | Mirrors                                |
|----------------------------------------------|----------------------------------------|
| `tests/basic.rs::z3_hello_world`             | (toolchain check)                      |
| `tests/basic.rs::simple_nat_satisfied_with_n_gt_5` | Python `test_load_source_basic_schema` |
| `tests/basic.rs::impossible_is_unsat`        | Python `test_load_source_unsat`        |
| `tests/basic.rs::two_vars_relation`          | (multi-var smoke)                      |
| `tests/basic.rs::bool_implies`               | (Bool var + ⇒ smoke)                   |
