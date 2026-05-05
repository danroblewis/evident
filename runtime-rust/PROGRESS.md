# Rust runtime — progress log

**Read this first when resuming.** Tells you what's done, what's next, and any blockers.

## Current status

**Phase:** v0.9 — CLI binary added. 43/43 tests green (5 unit + 34 lib + 4 CLI).

**Last action:** Added `src/main.rs` with `query` and `parse`
subcommands. `cargo run -- query file.ev SchemaName --given key=value`
loads, parses, evaluates, and prints `KEY=VALUE` lines (or `UNSAT`).
4 new tests spawn the compiled binary and check stdout / exit codes.

**Phase:** v0.8 — Seq runtime support landed. 34/34 tests green.

**Last action:** `Seq(Int)` / `Seq(Bool)` / `Seq(String)` now actually
declare and translate at runtime. We didn't end up using Z3's native
Seq sort — `z3-sys` 0.8 has `Z3_mk_seq_sort` and `Z3_mk_seq_length`
but not `Z3_mk_seq_nth` (only `Z3_mk_seq_at` which returns a length-1
sub-sequence with no way to extract the element). Pivoted to modeling
each Seq as an `Array(Int → T)` plus a separate length variable, which
is well-supported by the safe `z3` crate.

  - `#s` translates to the length variable (Int).
  - `s[i]` translates to `Array.select(i)` cast to the element sort.
  - Model extraction reads length first, then `arr.select(i)` for
    `i ∈ 0..length`. Indices past length are unconstrained in Z3
    (Arrays are total functions); we just don't read them.

5 new tests cover Int/Bool/String elements, Seq with ∀, and length
arithmetic.

**Next action:** Composite element types (`Seq(UserType)`) — would
need declaring a Z3 Datatype for the element first, similar to the
Python `_declare_element_sort`. After that, symbolic ∀ bounds via
length propagation, and CLI / file-loading.

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
- **z3 crate gap: no generic `Seq<T>` ast type.** z3-0.12.1's `ast`
  module exposes `Bool, Int, Real, Float, String, BV, Array, Set,
  Datatype, Dynamic, Regexp` but no generic sequence. `z3-sys` 0.8
  also doesn't expose `Z3_mk_seq_nth` (the only seq element-access
  primitive), only `Z3_mk_seq_at` which returns a length-1 sub-seq.
  We work around this by encoding `Seq(T)` as `Array(Int → T)` + a
  separate length variable. Slightly less expressive (Arrays are
  total functions, so the model has values at all indices, not just
  0..len) but correct for our use case — we just don't read past
  `len` during model extraction.

## Cached evaluator (implemented)

Done — went with the leaked-Context design (option 1 in the original
sketch). The runtime owns a `&'static Context` from `Box::leak` and a
`RefCell<HashMap<String, CachedSchema<'static>>>`. First call to
`query_cached` for a given schema runs `build_cache` (declarations +
constraint translation into a fresh solver); subsequent calls run
`run_cached` which does push → assert givens → check → extract → pop.

Notes for anyone touching this:

- `load_source` clears the cache. Loading a new schema can reference
  existing ones via ClaimCall / passthrough, so old cache entries
  may now be stale. Simplest to flush.
- Cache key is the schema name. If you ever support reloading a
  schema by the same name with a different body, cache invalidation
  has to handle that.
- The leaked Context is one per `EvidentRuntime`. Tests create many
  runtimes; each leaks one Context. Fine for a test process; in a
  long-running embedding switch to a `Session<'ctx>` design where
  the caller controls Context lifetime.

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
- [x] **Cached evaluator.** Leaked `'static Context`,
      `RefCell<HashMap<String, CachedSchema>>` cache, push/pop per
      query. ~33× speedup on a multi-passthrough schema.
- [x] **Seq sort runtime support** for primitive element types
      (Int / Bool / String). Modeled as Array(Int → T) + length;
      cardinality + indexing translate cleanly.
- [x] **CLI** — `evident-runtime query <file> <name> [--given …]`
      and `evident-runtime parse <file>`. Spawns-binary integration
      tests verify stdout + exit codes.

In rough order of leverage:

- [ ] Composite element types (`Seq(UserType)`) — would need a Z3
      Datatype per user type, mirroring the Python
      `_declare_element_sort`. The Array+Length encoding still works,
      just with a Datatype range sort.
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
| `cached_query_matches_uncached`      | cached path produces identical results |
| `cached_query_per_call_givens`       | per-query givens, cache reused         |
| `cached_query_unsat`                 | UNSAT case across cache hits           |
| `cached_query_perf_smoke`            | cached < uncached over many iterations |
| `seq_int_basic`                      | `Seq(Int)` declared, `#`, `[]` work    |
| `seq_bool_basic`                     | `Seq(Bool)` round-trips                |
| `seq_string_basic`                   | `Seq(String)` round-trips              |
| `seq_with_quantifier`                | `∀ i ∈ {0..N} : s[i] > 0`              |
| `seq_cardinality_in_arithmetic`      | `#s + 1 = 5` pins length               |

**`tests/cli.rs` (4)** — spawns the compiled binary:

| `cli_query_sat_prints_bindings`      | KEY=VALUE lines on stdout              |
| `cli_query_unsat_exits_1`            | UNSAT path: stdout "UNSAT", exit 1     |
| `cli_query_with_given`               | `--given key=value` flag               |
| `cli_parse_lists_schema_names`       | `parse` subcommand lists names         |
