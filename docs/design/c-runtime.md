# `runtime-c/` — a minimal SMT-LIB-first Evident runtime in C++

> **Status:** session `c-runtime` (2026-05). A new, self-contained, **additive**
> runtime under [`runtime-c/`](../../runtime-c/). It does **not** touch the Rust
> `runtime/` (the language spec + cross-check oracle), which stays green. This
> doc is the honest account: the architecture, what's implemented, the
> C-vs-Evident-vs-SMT-LIB split, the Z3 dependency, and the roadmap.

## Why this exists

This is the first real implementation of the north star in
[`smtlib-as-compile-target.md`](smtlib-as-compile-target.md): **Evident compiles
to SMT-LIB, Z3 runs it.** The Rust runtime builds the Z3 AST through the C API;
this runtime instead emits **SMT-LIB text** and hands it to Z3's own parser. It
is the C-shell-over-Z3 vision of [`minimal-runtime.md`](minimal-runtime.md) made
concrete — as little native code as possible, everything that can be a
constraint expressed as SMT-LIB (and, eventually, as Evident).

The Rust prototype `runtime/src/translate/smtlib.rs` proved this round-trips for
a scalar subset *inside* the Rust process. This runtime is the standalone
native seed: its own lexer + parser + emitter + Z3 binding, no Rust.

## Architecture: the irreducible native seed

The pipeline, all in C++ (`runtime-c/src/`):

```
source text
  → lexer.cpp      Unicode operators + indentation → tokens   (mirrors runtime/src/lexer.rs)
  → parser.cpp     recursive descent → AST (ast.h)            (mirrors runtime/src/parser/)
  → smtlib.cpp     AST → SMT-LIB text                         (mirrors runtime/src/translate/smtlib.rs)
  → solve.cpp      SMT-LIB → Z3 (parse) → check → model       (Z3 C API)
  → main.cpp       CLI: read → parse → emit → solve → print
```

| File | Lines¹ | Concern |
|---|---|---|
| `src/ast.h`     | ~150 | AST node types (Expr, BodyItem, SchemaDecl, EnumDecl, …) |
| `src/lexer.{h,cpp}`  | ~310 | UTF-8 decode + tokenize; Unicode operators; indentation |
| `src/parser.{h,cpp}` | ~720 | recursive-descent parser, faithful to the Rust grammar |
| `src/smtlib.{h,cpp}` | ~330 | AST → SMT-LIB text (the translate layer, as text generation) |
| `src/solve.{h,cpp}`  | ~140 | Z3 C-API binding: parse text, solve, extract scalar model |
| `src/value.h`   | ~50  | runtime value extracted from a model |
| `src/main.cpp`  | ~150 | CLI |
| `src/z3_link_proof.cpp` | ~80 | M0 — the native↔Z3 seam, standalone |

¹ approximate, at time of writing.

### Why C++ (not C)

The spec allows "C/C++". A hand-written lexer/parser/AST with Unicode handling
and string emission is dramatically smaller and safer in C++ (std::string,
std::vector, std::shared_ptr, std::optional) while still being a thin native
seed with no framework dependencies. The Z3 dependency is its C API regardless.

## The C / SMT-LIB / Evident split

This is the whole point — what is irreducibly native, what is the portable IR,
and what is (eventually) self-hosted.

- **C++ (the bootstrap seed, kept minimal):** lexer, parser, the AST→SMT-LIB
  *emitter*, and the Z3 binding. The parser is the front-end bootstrap (string →
  AST always needs a seed in another language, exactly like any self-hosting
  compiler's front end). The Z3 binding is the one unavoidable native seam.
- **SMT-LIB (the compile target / portable IR):** the *semantics* of every
  supported construct live here as emitted text — `declare-const`, `assert`,
  arithmetic, `ite`, datatype declarations, etc. Z3 ingests it via
  `Z3_solver_from_string`. This is the layer that outlives the solver choice.
- **Evident (the self-hosting half — not yet reached here):** once the seed can
  *run* an Evident program, AST transforms (desugar/inject/…) become Evident
  programs the seed runs, as the Rust runtime does with `stdlib/passes/`. M5
  (below) is the first proof-of-concept; the architecture is built to allow it
  (the AST is data, the emitter is pure), but it is **not implemented yet**.

## Milestones reached

| Milestone | Status | Evidence |
|---|---|---|
| **M0** — Z3 link proof | ✅ done | `z3_link_proof` builds, links libz3, solves a hardcoded SMT-LIB string, prints `n = 6` |
| **M1** — parser (subset) | ✅ done | `seed_tests` lexer+parser cases; parser mirrors the Rust grammar (see below) |
| **M2** — AST → SMT-LIB | ✅ done | `schema_to_smtlib`; `--smtlib` flag dumps it; `seed_tests` emit cases |
| **M3** — end-to-end + cross-check | ✅ done | `evidentc <file> <claim>`; `crosscheck.sh` — 13 verdicts + 5 forced models agree with the Rust runtime |
| **M4** — grow the subset | ⏳ roadmap | enums, quantifiers, Seq, records (see roadmap) |
| **M5** — push one transform to Evident | ⏳ roadmap | the self-hosting half |

### The subset that transpiles today (M3)

Mirrors the Rust prototype's table (`docs/perf/smtlib-prototype-findings.md`):

| Category | Supported | Lowering |
|---|---|---|
| Scalar sorts | `Int`, `Nat`, `Pos`, `Bool`, `Real`, `String` | `declare-const`; `Nat`→`(>= x 0)`, `Pos`→`(> x 0)` |
| Arithmetic | `+ - * /` | `+ - *`; `/` → `div` (Int) or `/` (Real), sort-inferred |
| Comparison | `= ≠ < ≤ > ≥` | `= < <= > >=`; `≠` → `(not (= ..))` |
| Logic | `∧ ∨ ¬ ⇒` | `and or not =>` |
| Membership (as constraint) | `x ∈ {a,b,c}`, `x ∈ {lo..hi}` | `(or (= x a) …)`, range bound |
| Conditional | `(c ? a : b)` | `(ite c a b)` |
| String concat | `++` | `str.++` |
| Chained membership | `0 < x ∈ Int < 5` | declare + per-pair bound (parser desugar) |

The parser additionally *accepts* the full grammar — enums, quantifiers, Seq/Set
literals, records, match, claim composition, generics, FSMs — so the front end is
ahead of the emitter. Anything the emitter can't lower is **reported as an error
the moment it's seen** (`SmtError`), never silently mis-emitted. This preserves
Evident's "a missing constraint is a silent bug" safety: the boundary is exact.

### What is faithfully a mirror of the Rust front end

The lexer and parser were ported construct-by-construct from `runtime/src/lexer.rs`
and `runtime/src/parser/`. In particular the seed reproduces:
- indentation-significant layout (`Indent`/`Newline`, paren-depth newline
  suppression), Unicode operators, `--` comments, `\"`/`\n`/`\t` string escapes;
- the chained-membership desugar (`0 < x ∈ Int < 5`), multi-name shorthand
  (`x, y, z ∈ Int`), chained comparisons (`a ≤ b ≤ c` → pairwise AND);
- precedence climbing identical to the Rust grammar (`⇒` tighter than `∧`, `=`
  tighter than `∧`/`∨`, ternary between `⇒` and `∨`);
- the speculative `<` disambiguation (generic-arg suffix vs comparison operator),
  including the rewind-on-failure behavior.

## Cross-check methodology

The Rust runtime is the oracle. `runtime-c/tests/crosscheck.sh`:
1. runs each fixture through `evidentc --all` and `evident sample --all`, and
   asserts the sat/unsat verdict for every claim matches;
2. for forced-model fixtures (unique solution), asserts both runtimes extract the
   *same model value* (`x=7`, `x=1.5`, `q=true`, `x=-5`, `s="hello"`).

Both halves pass. This is the "it's a real runtime" evidence: the SMT-LIB-authored
Z3 solve agrees with the C-API-authored one, exactly as the prototype found.

### A known, documented divergence (inherited from the prototype)

The prototype found that nonlinear real arithmetic (`x>0 ∧ x*x=2`) is SAT on a
plain `Solver` (Z3 routes it to `nlsat`) but "not satisfied" on the Rust runtime's
*tuned tactic chain* (which returns `Unknown`). This seed uses a plain solver, so
it would also say SAT — the tuning/tactic layer lives **outside** the AST and is
not carried over. The fixtures here stay within the linear fragment to avoid the
divergence; it is noted for honesty, not worked around.

## Z3 dependency

- Built/tested against **Z3 4.15.4** (Homebrew): `/opt/homebrew/include/z3.h`,
  `/opt/homebrew/lib/libz3.dylib`.
- Uses the C API directly: `Z3_mk_config`/`Z3_mk_context`/`Z3_mk_solver`,
  `Z3_solver_from_string` (load SMT-LIB text), `Z3_solver_check`,
  `Z3_solver_get_model` + `Z3_model_eval` + `Z3_get_numeral_int64` /
  `Z3_get_bool_value` / `Z3_get_numeral_string` / `Z3_get_string` (extract).
- Parser errors are swallowed by Z3 into the context error state, so after
  `Z3_solver_from_string` the seed checks `Z3_get_error_code` (same guard the
  prototype needed) — a malformed emit can never silently solve an empty problem.

## Roadmap / TODO (next sessions)

Ordered by value and independence. Each is additive to the seed.

1. **M4a — enums (Z3 datatypes).** Emit `declare-datatypes` for `enum` decls;
   lower nullary ctors to constants, payload ctors to applications, `match` to
   nested `ite` over `(_ is Ctor)` recognizers + accessor binds, `e matches Ctor`
   to a recognizer, and add enum model extraction (reconstruct the datatype sort,
   read the ctor name). High value, cross-checks cleanly (`enum`-heavy claims).
2. **M4b — finite quantifier unrolling.** `∀ x ∈ {lo..hi} : body` →
   conjunction over the constant range (disjunction for `∃`), substituting the
   bound var — exactly what the Rust translator does. Requires constant range
   bounds at emit time. Unlocks a large slice of real claims.
3. **M4c — records.** Single-constructor datatypes; field access via accessors;
   positional/named pins; the record-as-vector lifts (componentwise `=`/`≤`,
   arithmetic broadcast). Larger; depends on M4a's datatype machinery.
4. **M4d — Seq.** Z3 seq theory (`declare-const xs (Seq Int)`), `#` → `seq.len`,
   indexing → `seq.nth`, `++` → `seq.++`. Independent of M4a–c.
5. **Imports.** Resolve `import "..."` relative to the file (currently ignored
   with a note). Needed to run anything that pulls in stdlib.
6. **Tactic parity.** Decide whether to replicate the Rust tuned tactic chain
   (see the divergence above) — it lives outside the AST and would need to be
   reapplied to the SMT-LIB-fed solver for bit-identical behavior.
7. **M5 — self-host one transform.** Once the seed can run an Evident program
   (needs enough of M4 to express a pass), port a trivial desugar/identity pass
   to Evident and have the seed run it — the self-hosting half. The AST is data
   and the emitter is pure, so the seam exists; this is the proof.

### Non-goals for the seed (stay Rust / stay native)

- Z3 itself and the FFI/IO kernel (real side effects, async, OS bridges).
- The multi-FSM scheduler, effect dispatch, FTI bridges — out of scope; this seed
  is a *query* runtime (parse → solve → print), not an `effect-run` executor.
- The functionizers (Cranelift JIT, GLSL, …). The north star puts these
  *after* SMT-LIB; the seed stops at "Z3 solves the SMT-LIB."
