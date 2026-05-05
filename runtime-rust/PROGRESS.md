# Rust runtime — progress log

**Read this first when resuming.** Tells you what's done, what's next, and any blockers.

## Current status

**Phase:** v1.5 — composite element types (`Seq(UserType)`).
67/67 tests green (11 lib + 41 lib-style + 10 CLI + 5 sample).

**Last action:** Closed the missing piece in the v1.4 merge: `#pts`
(cardinality) on a `Var::DatatypeSeqVar` now returns the length
variable. Translate's `Cardinality(Identifier(name))` arm previously
only consulted `var.as_seq()`, which matches `SeqVar` but not
`DatatypeSeqVar`; the new arm tries `as_datatype_seq()` as a fallback.
Without this, `#pts = 3` silently dropped and the model came back
with `len = 0`, so `extract_seq_composite` produced an empty Vec
even though field-equality constraints were satisfiable. The two
`seq_composite_*` tests in `tests/basic.rs` are the regression cover.

**Phase:** v1.4 — `execute` subcommand + headless plugin framework.
65/65 tests green (11 lib + 39 lib-style + 10 CLI + 5 sample).

**Last action:** Implemented `evident execute <file.ev>` as a sync,
blocking-stdin step loop. New `runtime-rust/src/executor.rs`:

  - `Plugin` trait with `handles_types() / initialize() / before_step()
    / after_step()`. `before_step → None` and `after_step → false`
    both signal halt.
  - `StdinPlugin<R: Read>` reads one char per step, contributes
    `var.char / var.eof / var.fd / …` as `given` values. Emits one
    final EOF step then halts.
  - `StdoutPlugin<W: Write>` constrains structural fields per step
    (`fd=1, open=true, …`) and writes `var.out` to the writer after
    each successful solve.
  - `run_headless(rt, input, output)` and `run_with_plugins(rt, &mut plugins)`.
  - State pair detection: `foo / foo_next` of the same non-IO type;
    initial state defaults via `default_for_type` (Nat→0, Bool→false,
    String→"", Seq(_)→empty).
  - Embedded I/O stdlib (flat type definitions for `Stdin`, `Stdout`,
    `Stderr`, `CharInput`, `CharOutput`) auto-loaded by `cmd_execute`.
    Flat (no `..` passthrough) because the Rust runtime's `declare_var`
    doesn't yet recurse into passthroughs during sub-schema expansion.
  - Z3 string outputs are unescaped at the StdoutPlugin boundary so
    `\u{a}` becomes a real newline byte; otherwise the executor would
    emit literal `\u{a}` text to stdout.

**v1 limitations vs. Python `evident.py execute`:**

  - **Headless only.** No SDL, no TCP, no batch-mode plugins. Programs
    that declare `∈ SDLOutput` etc. won't activate any plugin and will
    error with "no I/O plugin matches".
  - **Embedded io stdlib is flat.** The Python `stdlib/io.ev` builds
    Stdin via `..CharInput → ..Readable → ..Descriptor` passthrough
    chains; the Rust runtime's `declare_var` doesn't recurse through
    those, so we shipped a flattened version inline. Field set is
    intentionally limited to what the executor populates (no
    `connection`, `position`, `path`, etc. — those wouldn't have
    accurate values from the plugin anyway).
  - **No `import`.** Programs that begin with `import "stdlib/io.ev"`
    will fail to parse; the embedded stdlib provides Stdin/Stdout
    directly, so most one-file programs don't need `import`.
  - **No string concat / int_to_str.** Schemas using `++` or
    `int_to_str` won't translate. Useful programs are still possible
    (echo, simple state machines, char-level filters) but the full
    `programs/ev-nl.ev` etc. require the Python runtime.
  - **Single-character stdin only.** Multi-byte UTF-8 will read one
    byte per step and produce mangled `var.char` for non-ASCII text.
    Same limitation as the Python streaming plugin.
  - **No `batch` / `repl` subcommands.** Both still print the parked
    "use evident.py" message. `execute` is the only added subcommand.

**Phase:** v1.3 — real `sample` via blocking clauses.
58/58 tests green in this slice (11 lib + 39 lib-style + 9 CLI + new
`tests/sample.rs` with 3 `EvidentRuntime::sample` tests + 2 CLI smokes).

**Last action:** Replaced the naïve `sample` loop with a real
blocking-clause loop on the cached solver. New
`translate::sample_cached_inner(cached, given, n, ctx) -> Vec<HashMap>`
mirrors `run_cached`'s push/pop pattern but inside the outer push,
loops `check → extract → assert ¬(AND of scalar bindings)` until
either `n` distinct models or UNSAT. The accumulated blocking
clauses live inside the outer push so the cached solver is
unchanged from the caller's perspective when sampling returns.

`EvidentRuntime::sample(name, given, n)` wraps it via the
existing `cache: RefCell<HashMap<String, CachedSchema>>`.
`cmd_sample` in `main.rs` is now a single call to `rt.sample(...)`.

Limitations (v1):
  - Blocking clauses cover only Bool/Int/Str scalar bindings.
    `Var::SeqVar`, `Var::SetVar`, `Var::DatatypeSeqVar`, and
    `Var::PinnedInt` are skipped — schemas whose only varying outputs
    are Seq/Set values will return duplicates. Doc'd at the call site.
  - If a schema has no scalar bindings at all, the loop returns one
    model and bails (no useful blocking clause to add).

**Phase:** v1.2 — CLI mirrors evident.py.
53/53 tests green (5 unit + 39 lib + 9 CLI).

**Last action:** Renamed binary `evident-runtime` → `evident` and
restructured `src/main.rs` to match `evident.py`'s subcommand shape:

  - `query  <files…> <schema> [--given k=v …] [--json]` — accepts
    multiple files (loads in order); JSON mode emits
    `{"satisfied": …, "bindings": {…}}`.
  - `check  <files…>`                 — load + run every loaded
    schema with no givens; print `SAT/UNSAT/ERROR  <name>`.
  - `sample <files…> <schema> [-n N] [--given k=v …] [--json]` —
    real blocking-clause loop (see v1.3 above; was a naïve
    re-query loop in this snapshot).
  - `test   [path]`                   — discovers `test_*.ev` under
    a directory (or one file), runs every `sat_*` / `unsat_*`
    schema, asserts the SAT result matches the prefix. Final summary.
  - `parse  <file>`                   — Rust-only debug helper
    (lists loaded schema names).

Subcommands `execute`, `batch`, `repl` print a clear "not yet
implemented in the Rust runtime — use evident.py" message and exit 2.
They're parked behind the plugin / executor-loop work.

**Phase:** v1.1 — Set sort runtime (membership queries).
49/49 tests green (5 unit + 39 lib + 5 CLI).

**Last action:** `Set(Int)` / `Set(Bool)` / `Set(String)` now declare
as Z3 Set values. `x ∈ s` (with `s` a SetVar identifier) translates
to `set.member(x)`. SetLit-rhs path still works (reduces to OR of
equalities). Set vars don't appear in extracted bindings — Z3 sets
are characteristic functions over the element domain, not finite
containers; iteration / cardinality aren't meaningful without
explicit length tracking. Useful for membership constraints though.

**Phase:** v1.0 — symbolic ∀ bounds via length propagation.
47/47 tests green (5 unit + 37 lib + 5 CLI).

**Last action:** Added the Python-runtime "Pass 1/2/3" length-propagation
shim. New `Var::PinnedInt(i64)` variant lets known-literal int names
participate in compile-time arithmetic — `translate_int` of a PinnedInt
identifier is `Int::from_i64(v)`, and `literal_range` now consults
`translate_int + Z3 simplify` so `∀ i ∈ {0..n - 1}` unrolls when n is
pinned by:
  - `given` (per-query)
  - `n = literal_int_expr` body constraint (build-time)
  - `n = #seq` propagation when `#seq = N` is also pinned
  - any chain of those, iterated to fixed point

3 new tests cover the three pin paths: pinned-via-equality,
length-propagation, and given-value.

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
- **`as_seq()` accessor doesn't match `DatatypeSeqVar`.** When adding
  the composite-element `Var::DatatypeSeqVar` we kept `as_seq()` returning
  only `SeqVar` (the primitive case) so existing branches don't accidentally
  swallow Datatype-element seqs through the wrong code path. The cost is
  every `as_seq()` call site has to pair with an `as_datatype_seq()`
  fallback if the operation should also work for composite seqs.
  `Cardinality(Identifier(name))` is one such site — initially missed,
  added in v1.5. If you add a new translator branch that operates on a
  primitive Seq, decide explicitly whether it also makes sense for
  composite Seqs and add the `as_datatype_seq()` arm.
- **DatatypeBuilder requires globally-unique type names.** Z3's
  `Z3_mk_datatypes` errors on duplicate sort names within the same
  Context. The `DatatypeRegistry` deduplicates per type name within a
  single runtime, so reusing `Point` across two schemas is fine. But
  re-loading a schema with the same type name and a different field
  shape would either bind the wrong shape (we hit the cache before
  rebuilding) or, with `load_source`'s registry flush, produce a Z3
  error on the *next* `Seq(Point)` declaration because the leaked old
  Datatype still owns the name in Z3's context. v1 doesn't exercise
  reload-with-redefinition, so this is theoretical for now.
- **Field types in user-type Datatype are limited.** `get_or_build_datatype`
  rejects fields whose declared type is anything but Int/Nat/Pos/Bool/
  String. Nested user types or `Seq`/`Set` element fields would need
  recursive Datatype building (z3 supports it via `Z3_mk_datatypes` taking
  multiple builders together) — not done in v1. The branch logs a warning
  and skips the whole Seq declaration.

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
- [x] **CLI** — `evident` binary mirrors `evident.py`'s subcommand
      shape: `query`, `check`, `sample`, `test`, `parse`, `execute`
      (headless, since v1.4). `batch` / `repl` still print a clear
      "use evident.py" message and exit 2.
- [x] **Symbolic ∀ bounds via length propagation.** `Var::PinnedInt`
      variant + `collect_pinned_ints` / `collect_seq_lengths` /
      `apply_pinned_ints` pre-pass. `literal_range` reduced to
      `translate_int + Z3 simplify`. Iterates to fixed point so
      chains like `n = #s ∧ #s = 4 ∧ k = n - 1` all resolve.
- [x] **Set sort runtime** for primitive element types. `x ∈ s` uses
      Z3's `set.member(x)`. No iteration / cardinality (Z3 sets are
      functions, not finite containers); SetVars don't appear in
      extracted bindings.
- [x] **Real sample loop with blocking clauses.** `sample_cached_inner`
      in translate.rs reuses the cached schema's solver: outer push for
      givens, then a loop of `check + extract bindings + assert ¬(AND
      scalar = value)` accumulated inside the outer push, popped before
      return. `EvidentRuntime::sample(name, given, n)` wraps it.
      `cmd_sample` is now a single call. Scalar-only blocking (Bool/
      Int/Str); Seq/Set/Composite/PinnedInt skipped from the conjunction
      (documented limitation).
- [x] **`execute` subcommand (headless v1).** New `runtime-rust/src/executor.rs`
      with `Plugin` trait + `StdinPlugin` + `StdoutPlugin`. `run_headless(rt,
      input, output)` drives the step loop: read char → build given (plugin
      contributions + current state) → `query_cached("main", given)` → on
      SAT, write `dst.out`, advance state from `state_next.*`. Embedded io
      stdlib (flat type defs for Stdin/Stdout/etc.) auto-loaded by `cmd_execute`.
      Limitations: stdin/stdout only (no SDL/TCP/batch), no `import` directive,
      no `++`/`int_to_str` operators. See "Phase v1.4" above for the full list.

In rough order of leverage:

- [ ] **`batch` subcommand** — stdin → Seq(String) → solve → Seq → stdout.
      Should be small once `execute`'s loop infrastructure is there.
- [ ] **`repl` subcommand** — interactive read-eval-print.
      Less urgent — test-runner + query cover most workflows.
- [x] **Composite element types (`Seq(UserType)`)**. New `Var::DatatypeSeqVar`
      variant + `DatatypeRegistry` (`RefCell<HashMap<String,
      &'static DatatypeSort>>`) on the runtime. `declare_var`'s `Seq(...)`
      branch dispatches on the inner name: primitives → `SeqVar`, user
      types in `schemas` → `DatatypeSeqVar` after building (or looking
      up) a Z3 Datatype via `DatatypeBuilder`. `Expr::Field(Box<Expr>,
      String)` AST + `parse_postfix` chain `[i].field` parses cleanly.
      `translate_int / _bool / _str` route `Field(Index(seq, idx), name)`
      through `resolve_seq_field`, which applies the matching accessor.
      `extract_seq_composite` walks the array element by element and
      reads each field's value; result is `Value::SeqComposite(Vec<HashMap>)`.
      v1 limitation: only flat user structs whose fields are
      Int/Nat/Pos/Bool/String. Nested Seqs and nested user types are
      out of scope (warned + skipped).
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
| `forall_symbolic_bound_via_pinned_var` | `n = 4 ∧ ∀ i ∈ {0..n-1}` unrolls    |
| `forall_symbolic_bound_via_length_propagation` | `n = #s ∧ #s = 3` chains      |
| `forall_symbolic_bound_from_given`   | per-query `given` n=5 unrolls bound    |
| `set_var_membership_int`             | `s ∈ Set(Int) ; x ∈ s` via Z3 member  |
| `set_var_membership_string`          | `name ∈ Set(String)` membership        |
| `seq_composite_field_access`         | `Seq(Point)` + `pts[0].x = 10` per-elem |
| `seq_composite_with_quantifier`      | `∀ i ∈ {0..2} : pts[i].x > 0`         |

**`tests/cli.rs` (10)**:

| `cli_query_sat_prints_bindings`      | KEY=VALUE on stdout                    |
| `cli_query_unsat_exits_1`            | UNSAT path                             |
| `cli_query_with_given`               | `--given key=value`                    |
| `cli_query_examples_scheduling`      | real .ev file via the binary           |
| `cli_query_json_output`              | `--json` shape                         |
| `cli_check_reports_per_schema`       | `check` SAT/UNSAT lines                |
| `cli_test_runs_sat_unsat_claims`     | `test` discovery + result reporting    |
| `cli_execute_echoes_stdin`           | `execute` headless echo automaton end-to-end |
| `cli_batch_says_parked`              | parked `batch`/`repl` emit clear msg   |
| `cli_parse_lists_schema_names`       | `parse` debug helper                   |

**`src/executor.rs` unit tests (6)**:

| `executor_echoes_input`              | `dst.out = src.char` copies stdin → stdout |
| `executor_state_increments`          | `state_next.n = state.n + 1` advances 0→1→2 |
| `executor_state_gated_output`        | `state.n = 2 ⇒ dst.out = "X"` fires once |
| `executor_unsat_step_is_skipped`     | UNSAT step doesn't crash, output empty |
| `detect_state_pairs_basic`           | state pair detection accepts foo/foo_next |
| `detect_state_pairs_excludes_io_types` | excludes Stdout-typed pairs           |

**`tests/cli.rs` (4)** — spawns the compiled binary:

| `cli_query_sat_prints_bindings`      | KEY=VALUE lines on stdout              |
| `cli_query_unsat_exits_1`            | UNSAT path: stdout "UNSAT", exit 1     |
| `cli_query_with_given`               | `--given key=value` flag               |
| `cli_parse_lists_schema_names`       | `parse` subcommand lists names         |
