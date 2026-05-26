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
- **Evident (the self-hosting half — first proof reached at M5):** an AST
  transform can be expressed as an Evident program (the AST is a recursive
  `enum`, the pass is a `claim` relating `input` to `output`) and the seed *runs*
  it — reflecting the input AST, solving, and reifying the output AST — with no
  pass-specific logic in the seed's C++. M5 (below) demonstrates this on identity,
  operand-swap, and Mul→Add rewrites; the same `passes.ev` runs on the Rust
  runtime and yields the identical output AST. What's still ahead: a C↔Evident
  reflection bridge so the seed can parse an arbitrary program, reflect *its* AST
  into the enum value (rather than pinning it in source), run the pass, and reify
  back — the full `stdlib/passes/`-style loop.

## Milestones reached

| Milestone | Status | Evidence |
|---|---|---|
| **M0** — Z3 link proof | ✅ done | `z3_link_proof` builds, links libz3, solves a hardcoded SMT-LIB string, prints `n = 6` |
| **M1** — parser (subset) | ✅ done | `seed_tests` lexer+parser cases; parser mirrors the Rust grammar (see below) |
| **M2** — AST → SMT-LIB | ✅ done | `schema_to_smtlib`; `--smtlib` flag dumps it; `seed_tests` emit cases |
| **M3** — end-to-end + cross-check | ✅ done | `evidentc <file> <claim>`; `crosscheck.sh` — verdicts + forced models agree with the Rust runtime |
| **M4a** — enums (Z3 datatypes) | ✅ done | `declare-datatypes`; nullary + payload ctors, `match` → nested `ite`, `matches` recognizer, recursive enum model extraction; `enums.ev` cross-checks |
| **M4b** — finite quantifier unrolling | ✅ done | `∀ v ∈ {lo..hi} : body` → `and` over the constant range (`∃` → `or`); constant-fold bounds, symbolic bounds rejected; `quantifiers.ev` cross-checks |
| **M4c** — records | ✅ done | per-field Z3 leaves (`v.x`, `p.pos.y`); field access, named/positional pins, record literals, componentwise `= ≠ < ≤ > ≥` + bounding-box lifts, **arithmetic broadcast** (`c = a + b`, scalar `v * dt / 1000`), **local invariants** (refined records, rebound per instance); `records.ev` cross-checks. Record-valued ternary is out of subset (oracle-boundary). |
| **M4d** — Seq | ◑ partial | Z3 sequence theory (SMT-LIB text): `Seq(Int)`/`Seq(Bool)` decl, `#` → `seq.len`, `xs[i]` → `seq.nth`, equality, model extracts as `[e0, …]`; `seqs.ev` cross-checks (incl. whole-seq value parity). `++` runtime concat + `Seq(Real)` out of subset (oracle drops both); `Seq(String)` emits but Z3 returns `unknown` (documented divergence). |
| **M5** — push one transform to Evident | ✅ proof-of-concept | the seed RUNS an Evident transform pass: AST-as-`enum`, pass-as-`claim`, seed reflects input → solves → reifies `output`. The same `passes.ev` runs on the Rust runtime with the identical output AST. `passes.ev` cross-checks. |

### The subset that transpiles today (M3 + M4a + M4b)

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
| Enums (M4a) | `enum`, payload + recursive variants, `match`, `matches` | `declare-datatypes`; `match` → nested `ite` over `(_ is Ctor)`; `matches` → recognizer |
| Quantifiers (M4b) | `∀ v ∈ {lo..hi} : body`, `∃ v ∈ {lo..hi} : body` | unroll → `and`/`or` over the constant range; bound var substituted per iteration |
| Records (M4c) | `type IVec2(x, y ∈ Int)`, field access `v.x`, pins, `IVec2(a, b)` literals, `a = b` / `lo ≤ p ≤ hi`, `c = a + b` / `v * dt / 1000` | per-field leaf consts (`v.x`); comparison/equality lift componentwise (`and` of per-axis; `≠` → `or` of per-axis `not =`); arithmetic broadcasts per-axis (zip records, broadcast scalars) |
| Seq (M4d) | `xs ∈ Seq(Int)` / `Seq(Bool)`, `#xs`, `xs[i]`, `a = b` | `(declare-const xs (Seq Elem))`; `#` → `seq.len`, `[]` → `seq.nth`; model walked from `seq.++`/`seq.unit` → `[e0, …]` |

The quantifier bounds must **fold to integer constants at emit time** (literals +
literal arithmetic, mirroring the Rust path's `literal_range` simplify). A symbolic
bound (`{0..m}` with `m` a free const), tuple binding (`coindexed`/`edges`), or a
Seq/Set range is reported out of subset — never silently mis-unrolled.

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
   *same model value* — scalars (`x=7`, `s="hello"`), enums (`Ok(7)`), records
   (`v.x=3`), quantifier results, whole sequences (`[10, 20, 30]`);
3. for the M5 self-hosted passes, runs each transform on both engines and asserts
   they reify the *same output AST* (`passes.ev`).

All three halves pass. This is the "it's a real runtime" evidence: the
SMT-LIB-authored Z3 solve agrees with the C-API-authored one across enums,
quantifiers, records, sequences — and the same Evident transform pass produces
the same answer on both.

### Known, documented divergences (representation/tactic, outside the AST)

Two places where the seed and the Rust oracle differ — both because the *solver
configuration / value representation* lives **outside** the AST and isn't carried
over the SMT-LIB seam. Noted for honesty, not worked around; the cross-check
fixtures stay clear of them.

1. **Nonlinear real arithmetic.** `x>0 ∧ x*x=2` is SAT on a plain `Solver` (Z3
   routes it to `nlsat`) but "not satisfied" on the Rust runtime's *tuned tactic
   chain* (which returns `Unknown`). This seed uses a plain solver, so it would
   say SAT. The fixtures stay within the linear fragment.
2. **`Seq(String)`.** The seed lowers it to Z3 sequence theory
   (`(Seq String)` + `seq.nth`), which Z3 returns **`unknown`** for on the plain
   solver — confirmed identical on the standalone `z3` CLI, so it's a genuine
   seq-theory incompleteness, not a seed bug. The Rust oracle decides it because
   it represents Seqs as **arrays + a pinned length** (not Z3 seq theory), a
   representation choice that lives outside the AST. `Seq(Int)`/`Seq(Bool)` are
   decidable under both and cross-check exactly (whole-seq value parity included);
   `seqs.ev` therefore omits `Seq(String)`.

## M5 — the self-hosting proof (the seed runs an Evident transform)

The headline. `runtime-c/tests/fixtures/passes.ev` declares a miniature AST as a
recursive Evident `enum` and a set of transform passes as `claim`s:

```evident
enum Expr = Lit(Int) | Add(Expr, Expr) | Mul(Expr, Expr)

claim swap_add_pass
    input ∈ Expr
    output ∈ Expr
    input = Add(Lit(1), Mul(Lit(7), Lit(8)))
    output = match input
        Add(a, b) ⇒ Add(b, a)        -- swap the operands of a top-level Add
        _         ⇒ input
```

The seed is *only the engine*. It reflects the pinned `input` AST into the enum
value (nested constructor calls, M4a), the solver computes `output` from the
transform constraint, and the model extractor reifies the output AST
(`read_ast_value` recursion) — printing `output = Add(Mul(Lit(7), Lit(8)),
Lit(1))`. **No part of the swap lives in the seed's C++**; the transform is
entirely the Evident `claim`. That is the architecture's "push logic into
Evident" half: the same engine runs identity, operand-swap, and a Mul→Add
lowering by changing only the `.ev` source.

The proof is sealed by the cross-check: the *same* `passes.ev` runs on the Rust
runtime and yields the *identical* output AST for every pass. One pass, two
engines (an SMT-LIB-text seed in C++ and the C-API Rust runtime), one answer.

What this is not, yet: the input AST is *pinned in the source*, not reflected
from a parsed program. The full `stdlib/passes/`-style loop — parse an arbitrary
program, reflect *its* C AST into the enum value, run the pass, reify back into a
C AST — needs a C↔Evident reflection bridge. The seam already exists (the AST is
data; the emitter is pure), so that bridge is the natural next step. Transforms
that need nested patterns (e.g. constant folding `Add(Lit(a), Lit(b)) ⇒
Lit(a + b)`) additionally need the one-level `match` restriction lifted.

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

1. **M4a — enums (Z3 datatypes). ✅ DONE.** Emits `declare-datatypes` for `enum`
   decls; nullary ctors → constants, payload ctors → applications, `match` →
   nested `ite` over `(_ is Ctor)` recognizers + accessor binds, `e matches Ctor`
   → a recognizer, and recursive enum model extraction (read the datatype value
   AST recursively). Cross-checks via `enums.ev`.
2. **M4b — finite quantifier unrolling. ✅ DONE.** `∀ x ∈ {lo..hi} : body` →
   conjunction over the constant range (disjunction for `∃`), substituting the
   bound var per iteration — exactly what the Rust translator does
   (`exprs/quant.rs` `literal_range` branch). Bounds must fold to integer
   constants at emit time (`eval_const_int`); symbolic bounds, tuple binding,
   and Seq/Set ranges are reported out of subset. Cross-checks via `quantifiers.ev`.
3. **M4c — records. ✅ DONE.** Represented as **per-field Z3 leaves** (`v.x`,
   `p.pos.y`) rather than datatypes — the same dotted-leaf shape the Rust runtime
   uses, so model values cross-check exactly (a record-as-datatype would print
   `IVec2(3, 4)` where Rust prints `v.x=3`/`v.y=4`). Done: field access (dotted
   identifiers resolve to leaf consts), named/positional pins, record literals in
   expression position, and the componentwise comparison/equality + bounding-box
   lifts. A `type`/`schema` qualifies as a record only if its body is all field
   memberships; one with a local-invariant constraint stays out of subset (the
   invariant would otherwise be silently dropped). **Arithmetic broadcast** lands
   too: `c = a + b` zips record leaves, `v * dt / 1000` broadcasts scalars across
   axes (integer `div` on Int fields), nested forms compose — all cross-check
   exactly vs the oracle. **Local invariants** land too: a refined record
   (`type DateRange(lo, hi ∈ Int)` with `lo ≤ hi`) has its constraints
   instantiated per instance, each field's bare name rebound to its leaf
   (`lo` → `d.lo`) via the same scoped-bind trick `match` uses — so `d ∈ DateRange`
   with an inverted range is UNSAT. **Record-valued ternary** (`c = (flag ? a : b)`)
   is deliberately out of subset: the Rust oracle drops it ("couldn't translate to
   Bool"), so the seed reports the boundary rather than silently exceeding the
   oracle and diverging on the model. Cross-checks via `records.ev`.
4. **M4d — Seq. ◑ PARTIAL.** Z3 sequence theory, lowered as SMT-LIB text:
   `declare-const xs (Seq Int)`, `#` → `seq.len`, `xs[i]` → `seq.nth`, `a = b` →
   seq equality. The model value is walked from its `seq.++`/`seq.unit` AST (no
   seq-specific C API — the homebrew `z3.h` exposes none) and formatted `[e0, …]`,
   matching the Rust CLI exactly. Element types match the oracle's `SeqElem` set
   (`Int`, `Bool`, `String`). Boundaries: `++` runtime concat of opaque Seq vars
   is out of subset (the oracle drops it — only `⟨…⟩` literal flattening, a
   load-time desugar, is supported), `Seq(Real)` is out of subset (the oracle
   drops it), and `Seq(String)` emits correct SMT-LIB but Z3's seq theory returns
   `unknown` on the plain solver (the oracle's array representation decides it —
   see the divergence note). Still TODO: element-iteration quantifiers
   (`∀ x ∈ xs`) over a pinned length. Cross-checks via `seqs.ev`.
5. **Imports.** Resolve `import "..."` relative to the file (currently ignored
   with a note). Needed to run anything that pulls in stdlib.
6. **Tactic parity.** Decide whether to replicate the Rust tuned tactic chain
   (see the divergence above) — it lives outside the AST and would need to be
   reapplied to the SMT-LIB-fed solver for bit-identical behavior.
7. **M5 — self-host one transform. ✅ PROOF-OF-CONCEPT.** The seed runs an Evident
   program that is itself a *transform*: the AST is a recursive `enum`
   (`Expr = Lit(Int) | Add(Expr, Expr) | Mul(Expr, Expr)`), the pass is a `claim`
   relating `input` and `output` (`output = match input  Add(a, b) ⇒ Add(b, a)
   …`), and the seed is the engine — it reflects the pinned input AST into the
   enum value (nested ctor calls, M4a), the solver computes `output` from the
   transform constraint, and the model extractor reifies the output AST
   (`read_ast_value` recursion). **No pass-specific logic lives in the seed's
   C++** — the transform is entirely in the `.ev` file. The same `passes.ev` runs
   on the Rust runtime and produces the *identical* output AST (identity,
   operand-swap, Mul→Add lowering, passthrough) — one pass, two engines, one
   answer. This is the architecture's "push logic into Evident" half, proven on a
   small subset. The next step is a C↔Evident reflection bridge (parse an
   arbitrary program → reflect its AST → run the pass → reify), the full
   `stdlib/passes/`-style loop; the seam already exists (the AST is data, the
   emitter is pure). Nested patterns (`Add(Lit(a), Lit(b)) ⇒ Lit(a + b)` constant
   folding) need the one-level `match` restriction lifted first.

### Non-goals for the seed (stay Rust / stay native)

- Z3 itself and the FFI/IO kernel (real side effects, async, OS bridges).
- The multi-FSM scheduler, effect dispatch, FTI bridges — out of scope; this seed
  is a *query* runtime (parse → solve → print), not an `effect-run` executor.
- The functionizers (Cranelift JIT, GLSL, …). The north star puts these
  *after* SMT-LIB; the seed stops at "Z3 solves the SMT-LIB."
