# Rust runtime — progress log

**Read this first when resuming.** Tells you what's done, what's next, and any blockers.

## Current status

**Phase:** v0.6 — cardinality + indexing parser/AST done; runtime
still gated on Seq sort support (z3-0.12 crate gap). 25/25 tests green.

**Last action:** Parser/AST work for the sequence syntax landed:
  - `#expr` parses as `Expr::Cardinality(Box<Expr>)` (unary prefix).
  - `e[i]` parses as `Expr::Index(Box<Expr>, Box<Expr>)` (postfix at
    atom level, binds tighter than any binary op).
  - Compound type names `Seq(Int)`, `Set(Bool)`, etc. parse cleanly
    in membership decls.

The runtime can't actually use these yet — see "z3 crate gap" under
gotchas. Translation ignores both Cardinality and Index; declaring
a `Seq(T)` variable logs a warning. The AST shape is settled, so
when the runtime catches up, only translate.rs needs to change.

**Next action:** Either (a) wrap z3-sys's `Z3_mk_seq_sort` directly
to enable Seq runtime support, or (b) cached evaluator (Rust
lifetime gymnastics — see sketch below).

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
- **Membership-decl vs membership-constraint disambiguation.** Both
  `n ∈ Nat` (declaration) and `n ∈ {3, 5, 7}` (set membership) parse
  the same prefix. The body-item parser distinguishes by lookahead:
  if `IDENT IN IDENT` is followed by a line terminator (Newline, Eof,
  Indent), it's a declaration; otherwise it's an expression. Without
  this you can't write `n ∈ Nat` then later `m ∈ Bool` etc. and have
  set-membership constraints in the same body.
- **Z3 ast types are RC-cloneable.** `Int<'ctx>`, `Bool<'ctx>`,
  `String<'ctx>` from the `z3` crate impl `Clone` cheaply (they're
  internal-RC). So `#[derive(Clone)]` on the env's `Var` enum works,
  which is what makes quantifier unrolling clean (clone env, shadow
  the bound var, recurse on body).
- **Quantifier bound must be a literal range for now.** `∀ i ∈ {lo..hi}`
  unrolls only when both `lo` and `hi` are `Expr::Int`. Symbolic bounds
  (`{0..n - 1}` where n is a variable) need the Python length-propagation
  shim — Pass 1/2/3 in `evaluate.py`. Deferred.
- **z3 crate gap: no generic `Seq<T>`.** z3-0.12.1's `ast` module
  exposes `Bool, Int, Real, Float, String, BV, Array, Set, Datatype,
  Dynamic, Regexp` — but no general sequence type. (`String<'ctx>` is
  internally a char-seq via Z3's seq-of-codepoints, but that doesn't
  generalize.) Z3 itself supports `Seq(T)` via `Z3_mk_seq_sort` etc.
  — to use it from Rust we'd need to wrap the FFI ourselves. Until
  then, the parser + AST handle `Seq(T)`, `#x`, and `e[i]` cleanly,
  but `declare_var` warns and skips `Seq(...)` types and the
  translator returns None for Cardinality / Index expressions.

## Cached evaluator sketch

The Python runtime gained `evaluate_cached()` for the executor's hot
loop: translate the body once, hold a Z3 solver, per query do
`push → assert givens → check → extract → pop`. Drops per-step cost
from ~33ms to ~7ms in the parent project.

The Rust port can do the same, but Z3's lifetimes make it awkward.
`Solver<'ctx>`, `Int<'ctx>` etc. all borrow from `Context<'ctx>`. To
cache them per-schema, the Context has to live as long as the cache.

Two designs that work:

1. **Runtime owns a leaked Context.** `Box::leak(Box::new(Config::new()))`
   gives a `&'static Context`. Then the cache can be `HashMap<String,
   (Solver<'static>, HashMap<String, Var<'static>>)>`. Simple, but
   the Context never gets freed (one per process — OK for a CLI
   tool, fine for tests, ugly for long-running embeddings).
2. **Session struct.** A `Session<'ctx>` borrows a Context the caller
   provides. Cache lives inside the Session. The runtime hands out
   sessions; callers manage Context lifetime. Cleaner but the API
   leaks the Z3 lifetime to consumers.

Recommend (1) for the experimental port — it's the simplest path to
demonstrating the perf story. If we ever care about clean shutdown,
switch to (2).

## Next slices

Done in this session:

- [x] String literals + `=`/`≠` on strings.
- [x] `given` parameter on `query` (pre-bind values via solver assertion).
- [x] Sub-schema field expansion (`task ∈ Task` → `task.id`,
      `task.duration`, …) — recursive, handles nested user types.
- [x] Set literal expressions `{1, 2, 3}` and ∈ over them.
- [x] Range literals `{lo..hi}` (only valid as a quantifier bound).
- [x] Quantifier translation `∀ i ∈ {lo..hi} : body` — unrolled when
      both bounds are literal Ints.
- [x] `..ClaimName` passthrough composition (names-match).
- [x] Claim composition with mappings (`Foo(x mapsto y, lit mapsto 5)`).
      Bare-identifier and literal mapping values + sub-schema mapping
      (`state mapsto state.player` re-keys every matching field).
- [x] `subclaim` declaration. Body has the same shape as a top-level
      decl; runtime lifts subclaims into the global schemas table so
      they're reachable by ClaimCall / passthrough from anywhere.
- [x] Cardinality `#x` and indexing `e[i]` syntax + AST. Runtime
      translation deferred behind the z3 crate gap.
- [x] `Seq(T)` / `Set(T)` parse as compound type names in membership
      decls. Runtime declaration deferred (logs warning).

In rough order of leverage:

- [ ] **Seq sort runtime support.** z3-0.12 doesn't expose a generic
      `Seq<T>`. Path: write thin wrappers around `z3_sys::Z3_mk_seq_sort`,
      `Z3_mk_seq_length`, `Z3_mk_seq_nth`. Unsafe but small. Once done,
      Cardinality and Index translation come for free.
- [ ] Composite Datatypes for Seq(T)/Set(T) where T is a user type.
- [ ] Cached evaluator (push/pop). See sketch below — non-trivial in
      Rust because the cached solver borrows the Context by lifetime.
- [ ] Symbolic ∀ bounds via length propagation (see Python's
      `evaluate.py` "Pass 1/2/3").
- [ ] `assert name = value` top-level ground facts. Mostly subsumed
      by the `given` parameter, but useful for one-shot REPL-style
      use.

## Test mapping

All in `tests/basic.rs`. 16/16 passing.

| Rust test                            | Mirrors                                |
|--------------------------------------|----------------------------------------|
| `z3_hello_world`                     | (toolchain check)                      |
| `simple_nat_satisfied_with_n_gt_5`   | Python `test_load_source_basic_schema` |
| `impossible_is_unsat`                | Python `test_load_source_unsat`        |
| `two_vars_relation`                  | (multi-var smoke)                      |
| `bool_implies`                       | (Bool + ⇒)                             |
| `string_literal_eq`                  | (String =)                             |
| `string_neq_excludes_literal`        | (String ≠)                             |
| `given_binds_int`                    | Python `query(name, given=…)`          |
| `given_violation_unsat`              | (given that contradicts schema)        |
| `given_sub_schema_field`             | (given on dotted field name)           |
| `sub_schema_field_expansion`         | Python `task ∈ Task` expansion         |
| `nested_sub_schema`                  | (recursive expansion)                  |
| `set_literal_membership`             | (`x ∈ {a, b, c}`)                      |
| `set_literal_strings`                | (string set membership)                |
| `forall_range_unroll`                | (`∀ i ∈ {0..3}` unroll)                |
| `exists_range_unroll`                | (`∃ i ∈ {0..5}` unroll)                |
| `passthrough_names_match`            | `..claim` with shared name             |
| `passthrough_introduces_var`         | `..claim` adds a new var to scope      |
| `passthrough_conflict_unsat`         | passthrough vs parent constraint conflict |
| `claim_call_with_mapping`            | `Claim(slot mapsto var)`               |
| `claim_call_mixed_mappings`          | mappings with literals and idents      |
| `claim_call_unmapped_internal`       | unmapped internal slot → fresh const   |
| `claim_call_sub_schema_mapping`      | `state mapsto state.player` re-keys fields |
| `subclaim_register_and_call`         | subclaim defined inside parent body    |
| `subclaim_visible_to_sibling`        | subclaim accessible from sibling decl  |
