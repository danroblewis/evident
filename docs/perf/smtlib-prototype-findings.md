# Evident → SMT-LIB → Z3 prototype — findings

> **Status:** real, exercised path (session smtlib-frontend, 2026-05), built on
> the original prototype (session SMTLIB-proto). The first concrete evidence for
> the north star in
> [`docs/design/smtlib-as-compile-target.md`](../design/smtlib-as-compile-target.md).
> Additive and flag-gated — the default translate/query path is untouched.

> **What changed this session (smtlib-frontend):** the prototype that "proved
> the round-trip in memory but emitted no files and replaced nothing" is now a
> real, inspectable path:
> 1. **`evident dump-smtlib <file> <claim> [--given k=v …] [-o out.smt2]
>    [--solve]`** emits a runnable `.smt2` artifact (and, with `--solve`, runs it
>    back through Z3 and prints sat/unsat + model). See
>    [`runtime/src/commands/dump_smtlib.rs`](../../runtime/src/commands/dump_smtlib.rs).
> 2. **Committed `.smt2` snapshots** for the covered corpus under
>    [`runtime/tests/fixtures/smtlib/`](../../runtime/tests/fixtures/smtlib/),
>    regenerated + diffed + **cross-checked vs the C-API path** by
>    [`runtime/tests/smtlib_snapshots.rs`](../../runtime/tests/smtlib_snapshots.rs).
>    Every fixture also parses + solves under the standalone `z3` binary.
> 3. **Subset grown to string builtins** (`substr`, `index_of`, `replace`,
>    `char_at`, `str_from_int`, `str_len`/`#`, `starts_with`, `ends_with`,
>    `str_contains`, infix `sub ∈ text`) → Z3 `str.*` — which makes the four
>    string claims of the real example file `examples/test_39_string_ops.ev`
>    transpile and solve identically to the C-API path.

## What this is

The north star says: **Evident compiles to SMT-LIB**, Z3 ingests that text via
`parse_smtlib2_string`, and the functionizers optimize the resulting AST
regardless of where it came from. The current `translate/` layer builds the Z3
AST through the C API. This prototype builds an **alternate path** that emits
SMT-LIB *text* for a subset of claims, loads it into Z3
(`Z3_solver_from_string`), solves, and proves the result **matches** the C-API
path.

- Emitter: [`runtime/src/translate/smtlib.rs`](../../runtime/src/translate/smtlib.rs)
  — `schema_to_smtlib(&SchemaDecl) -> String` and `solve(&SchemaDecl) -> SmtSolveResult`.
- Round-trip proof + cost harness:
  [`runtime/tests/smtlib_roundtrip.rs`](../../runtime/tests/smtlib_roundtrip.rs).
- Reached Z3 via the `z3` crate's `Solver::from_string` (which wraps
  `Z3_solver_from_string`). The wrapper *swallows* parser errors, so the
  prototype checks `Z3_get_error_code` afterward (via `z3-sys`) to catch a
  malformed emit instead of silently solving an empty problem.

### Gating

The path is **additive**: nothing on the default `translate`/`query` path calls
into `smtlib.rs`. It is reachable only from the round-trip test (the dedicated
test entry) and via `smtlib::is_enabled()` (`EVIDENT_SMTLIB=1`) for anyone who
wants to wire it elsewhere. `./test.sh` exercises the default path unchanged.

## The subset that transpiles today

| Category | Supported | Lowering |
|---|---|---|
| Scalar sorts | `Int`, `Nat`, `Pos`, `Bool`, `Real`, `String` | `declare-const`; `Nat`→`(>= x 0)`, `Pos`→`(> x 0)` |
| Arithmetic | `+ - * /` | `+ - *`; `/` → `div` (Int) or `/` (Real), sort-inferred |
| Comparison | `= ≠ < ≤ > ≥` | `= < <= > >=`; `≠` → `(not (= ..))` |
| Logic | `∧ ∨ ¬ ⇒` | `and or not =>` |
| Membership (as constraint) | `x ∈ {a,b,c}`, `x ∈ {lo..hi}` | `(or (= x a) …)`, `(and (>= x lo) (<= x hi))` |
| Conditional | `(c ? a : b)` | `(ite c a b)` |
| String concat | `++` | `str.++` |
| String slice / search | `substr`, `index_of`, `char_at`, `replace` | `str.substr`, `str.indexof`, `str.at`, `str.replace` |
| String length | `str_len(s)`, `#s` | `str.len` |
| String predicates | `starts_with`, `ends_with`, `str_contains`, infix `sub ∈ text` | `str.prefixof`, `str.suffixof`, `str.contains` |
| Int → String | `str_from_int(n)` | `str.from_int` + sign reattach (matches C-API) |
| Pinned input | `--given k=v` (CLI) | `(assert (= k <lit>))` |

`int_lit` wraps negatives as `(- n)`; `real_lit` guarantees a decimal point;
`str_lit` double-quotes and doubles internal `"`. Sort inference (`sort_of`)
picks `div` vs `/` and validates branches.

### What does NOT transpile yet (reported as `SmtLibError`, never mis-emitted)

- **Containers**: `Seq(T)`, `Set T` declarations and their operators
  (`#`, indexing `[]`, concat into a Seq).
- **Quantifiers**: `∀` / `∃` — the prototype is the quantifier-free fragment.
  (A var declared scalar but used under a `∀` constraint is still rejected at
  the constraint, so the boundary is exact.)
- **Algebraic data**: enums, records, `match`, `matches`, `Cons/Nil` literals.
- **Composition**: passthrough `..C`, `ClaimCall`, subclaims, `halts_within`,
  `run(fsm)` — i.e. anything that isn't a flat `Membership` + `Constraint` body.
- **Pins** on a scalar membership.

These are the natural next slices (records → datatype `declare-datatypes`,
quantifiers → `forall`/`exists`, Seq → Z3 seq theory), and each is independent.
The boundary is enforced positively: the emitter returns an error the moment it
sees something out of subset, so a partial transpile can never silently drop a
constraint (the Evident "missing constraint is a silent bug" failure mode).

## Corpus coverage (which `examples/*.ev` the path handles end-to-end)

The SMT-LIB path now solves, matching the C-API path, **every claim in the
covered subset** — verified both ways (`smtlib::solve` ⟷ `EvidentRuntime::query`)
by `runtime/tests/smtlib_snapshots.rs`.

| Source | Claims solved via SMT-LIB == C-API |
|---|---|
| `examples/test_39_string_ops.ev` | `sat_split_head`, `sat_substitute`, `sat_prefix_and_len`, `unsat_wrong_arg` — **all 4 static claims**, via the string-builtin lowering. The fsm `string_demo` is out of subset (enum-typed state + `match` + `Seq(Effect)`). |
| Inline scalar/string corpus | 19 snapshot fixtures (scalar arithmetic/logic, set/range membership, ternary, `--given` pins, the full string-op family). |

**The rest of `examples/` (test_01–test_38) is out of subset and reported as
such** — every one is an FSM/effect demo whose static claims pin enum-typed
state, `Seq(Result)` / `Seq(Effect)` records, `match` arms, or embed-call
(`ClaimCall`) bodies. None of those constructs transpile yet, so the emitter
errors on them (it never mis-emits). `test_39` is the lone example whose static
claims live entirely in the scalar/string fragment, and it is fully covered.

To grow example coverage further the subset needs **enums + `match`** (→
`declare-datatypes` + tester/accessor `ite`), which would unlock the
state-machine claims of `test_02`/`test_20`/etc. — but those *also* use
`Seq(Effect)` and `ClaimCall`, so enums alone are necessary-not-sufficient.
The independent, single-construct wins (each unlocking simple inline claims)
remain: enums, records-as-flattened-leaves, finite-range `∀` unrolling, Seq.

## The round-trip proof

Two test files exercise the path:

- **`runtime/tests/smtlib_roundtrip.rs`** — the original in-memory parity +
  cost harness, now extended with the string-builtin family.
- **`runtime/tests/smtlib_snapshots.rs`** — emits the runnable `.smt2`
  artifacts, diffs them against committed fixtures, AND cross-checks each
  against the C-API path (including real `examples/test_39` claims and a
  `--given`-pinned case).

Both assert sat/unsat parity (and, where the model is forced, model equality):

- SAT: `n > 5`; `a+b=10 ∧ a,b>0`; `p=true ∧ p⇒q` (q forced true); `name="hello"`;
  `k=7`; `x=-5`; `x+x=3.0` (x=1.5); `q=17 ∧ r=q/5` (r=3, confirms `div`);
  `m ∈ {2,4,6} ∧ m>3`.
- SAT (strings): `substr("Edge<Rect>", 0, indexof "<") = "Edge"`;
  `replace("Seq(T)","T","Rect") = "Seq(Rect)"`; `#"Edge<Rect>" = 10`;
  `str_from_int(-42) = "-42"`; `char_at("abc",1) = "b"`;
  prefix/suffix/contains predicates.
- UNSAT: `n>10 ∧ n<3`; `a=3 ∧ b=3 ∧ a≠b`; `m ∈ {2,4,6} ∧ m>10`;
  `starts_with("world.pos","local.")`; `"xyz" ∈ "abc"`; `n>5` pinned `n=3`.
- Boundary: `Seq(Int)`, a `∀`, an enum-typed var, and an out-of-subset
  `dump-smtlib` claim each assert the emitter errors (out of subset).

**All parity cases pass** — the SMT-LIB-authored Z3 AST solves identically to
the C-API-authored one. This is the "it really works" evidence for gate #3's
premise that both authoring routes converge on the same Z3 AST.

### A real divergence the prototype surfaced

`x > 0 ∧ x*x = 2` (satisfiable; x = √2) is the one case where the two paths
**disagree**, and the SMT-LIB path is the correct one:

- **SMT-LIB path → SAT.** A plain `Solver::new` lets Z3 route nonlinear real
  arithmetic to `nlsat`, which decides it.
- **C-API path → "not satisfied".** The default path runs a *tuned tactic
  chain* (`translate/eval/solver.rs`) that returns `Unknown` here, and
  `evaluate` maps `satisfied = matches!(result, SatResult::Sat)` — so `Unknown`
  becomes a reported non-solution.

This isn't a prototype bug; it's a finding about the default solver
configuration. It is pinned by `real_nonlinear_smtlib_decides_capi_does_not`
(the test fails if the default path ever starts solving it, prompting a doc
update). It also hints that "authoring the AST via SMT-LIB" and "authoring it
via the tuned C-API pipeline" are *not bit-identical* — the tactic/tuning layer
lives outside the AST and would need to be reapplied to an SMT-LIB-fed solver.

## The cost reality (north-star gate #1)

Measured on the 7-claim corpus, 200 iterations each (1400 solves/path), release
build, Apple Silicon. Numbers are noisy on the Z3 side (±40% run-to-run on
parse/solve); the *shape* is stable. Two representative runs:

| Step | Run A | Run B |
|---|---|---|
| (1) C-API **warm** query (cached plan + JIT + value cache) | 99.5 µs | 97.0 µs |
| (2) **emit** SMT-LIB text only (Evident AST → string) | 1.3 µs | 1.1 µs |
| (3) `smtlib::solve` — **fresh** Z3 `Context` + parse + solve | 4019.7 µs | 4853.9 µs |
| (4) parse + solve on a **shared** `Context` | 1399.6 µs | 2384.7 µs |
| context-creation cost (3 − 4) | ≈ 2620 µs | ≈ 2469 µs |

### What the numbers say

1. **String generation is not the bottleneck.** Emitting the SMT-LIB text is
   ~1 µs — three orders of magnitude under the solve. Gate #1's "SMT-LIB
   generation is string-heavy" worry is really about **Z3's parser ingesting
   the text** and about **context lifecycle**, not about Evident *producing* it.
   The string-theory blowup that bit the leaf passes was about *solving over
   string values*; *emitting* SMT-LIB doesn't touch that at all.

2. **Z3 `Context` creation dominates the cold path** (~2.5 ms, ~60% of a fresh
   solve). A real pipeline reuses one context, which roughly halves the cost
   (step 4). This is an implementation detail of the prototype (`solve` makes a
   fresh context per call), not an inherent cost of SMT-LIB.

3. **Parse + solve on a shared context is ~1.4–2.4 ms vs the warm C-API's
   ~0.1 ms** — ~14–25×. But this compares the **cold, uncached** SMT-LIB path to
   the C-API's **fully-warm** steady state (cached `ClaimPlan`, JIT-compiled
   components, value cache). It is therefore a *conservative* (worst-case)
   number for SMT-LIB, and it is exactly the comparison the north star says is
   the wrong one to optimize: **gate #3 designates SMT-LIB as a compile-once /
   AOT path, not a per-tick path.** Steady-state still emits the compiled
   artifact and skips the text round-trip.

### Verdict on the gates

- **Gate #1 (string-theory perf):** *Generation* is cheap (1 µs). The cost is in
  Z3's parser + context, not in producing the text. The original concern was
  mis-aimed for the *emit* direction — it's a real concern for *parsing Evident*
  and for *string-valued solves*, but not for "AST → SMT-LIB text."
- **Gate #3 (compile-time not run-time):** confirmed real and necessary. A
  per-solve SMT-LIB round-trip is 14–50× the warm C-API path; this only makes
  sense as a compile-once step whose output is then functionized to native.
- **Gate #2 (AOT-compile the front end):** untouched by this prototype; it's the
  next and largest piece (functionizing a whole translator program).

## What this proves, and what it doesn't

**Proves:** Evident claims in a scalar/string quantifier-free subset *do*
round-trip through SMT-LIB **text** into Z3 and solve identically to the C-API
path — now with **real `.smt2` artifacts** (`evident dump-smtlib`), committed
snapshots, and a verified-equal cross-check that includes a real example file
(`examples/test_39_string_ops.ev`). The authoring route is swappable; emit cost
is negligible (~1 µs); every artifact is independently runnable under the
standalone `z3` binary.

**Doesn't prove:** that the *whole* translator is expressible as SMT-LIB
(containers, quantifiers, datatypes, FSM lowering all remain — only `test_39`'s
static claims of the example corpus transpile so far), nor that this should be
the *default* translate path (the per-solve serialize+parse cost is 14–50× the
warm C-API path, so it stays gated — a compile-once/AOT step, not per-tick), nor
that the self-hosting bootstrap (gate #2) closes — that needs the AOT
functionizer on a translator-sized program. It also surfaced that the
tuning/tactic layer lives *outside* the AST, so a faithful cutover must carry
that over, not just the constraints.

## Repro

```sh
cd runtime
cargo test --release --test smtlib_roundtrip                 # parity + boundary + strings
cargo test --release --test smtlib_snapshots                 # .smt2 snapshots + C-API cross-check
cargo test --release --test smtlib_roundtrip cost_comparison -- --nocapture   # cost breakdown

# Emit a real artifact + solve it back through Z3:
./target/release/evident dump-smtlib ../examples/test_39_string_ops.ev sat_split_head --solve
# Regenerate snapshots after an intentional emitter change:
EVIDENT_UPDATE_SNAPSHOTS=1 cargo test --release --test smtlib_snapshots
```
