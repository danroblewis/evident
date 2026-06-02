# Round 23 — Z3 functionizer integration (off by default)

**Outcome:** STRUCTURAL. The Z3-AST functionizer pipeline from
Round 22 is now wired into `rt.query` behind an env gate
(`EVIDENT_FUNCTIONIZE_Z3=1`). Off by default while we widen the
static gate that protects against the Z3 translator's
fatal-exit-on-failure policy.

## What this round adds

### Guarded equality recognition

Z3's `simplify` doesn't fold `match`/`Implies` constructs when
the scrutinee is a free variable. Instead it emits implication
clauses of the form:

```text
(or (not ((_ is Init) state)) (and (= effects__len 2) ...))
```

This is `state = Init ⇒ effects = [...]`. Round 23 extends
`extract_program` to recognize these:

- `try_guarded(a)` matches `(or (not P) Q)` → returns `(P, Q)`.
- `classify_guarded_consequent` inspects Q:
  - `(= var expr)` → scalar guarded assignment for `var`.
  - `(and (= var__len N) (= (select var i) elem) ...)` → seq
    guarded assignment with N elements.
  - `(= var__len 0)` → empty-seq guarded assignment.
- New `Z3Step::Guarded` variant holds the branches.
- `eval_program` walks branches in order, picking the first
  whose guard evaluates to true.

### Recognizer evaluation

Z3 represents `((_ is Init) state)` as a `DT_IS` decl whose
"is_<Variant>" parameter is buried in the FuncDecl's parameters
(not its name). The Rust z3 0.12 binding doesn't expose those
parameters, so we extract the variant name from the formatted
application string `((_ is Init) state)` — fragile but
sufficient for v1.

### Predicate handling

Bodies often have non-equality assertions left over after
simplify: `(>= n 0)` from Nat bounds, `(< x 5)` from explicit
range constraints, full `(or ...)` implications. `extract_program`
collects these as `Z3Program::predicates`. At eval time they
must evaluate to `true`; `false` returns `None` (UNSAT). When
the predicate can't be evaluated (references a var not in env),
it's SKIPPED — Z3 already vetted the body's satisfiability;
unevaluable residual assertions are at worst tautological and
shouldn't drive a false UNSAT.

### UNSAT short-circuit

`simplify_assertions` now returns a `SimplifyResult { formulas,
unsat }` record. If any subgoal is `decided_unsat` or any
assertion folded to literal `false`, the function-izer returns
`SAT=false` immediately without going through the slow path.
Cheap detection of contradictions like `x = 3 ∧ x = 4`.

### Integration in `rt.query`

```rust
let z3_fz_on = functionize_on && std::env::var("EVIDENT_FUNCTIONIZE_Z3")
    .map(|s| s != "0").unwrap_or(false);
if z3_fz_on {
    if let Some(result) = self.try_functionize_z3(name, schema, given) {
        return Ok(result);
    }
}
// fallthrough to existing Evident-AST functionizer / Z3 full solve
```

Per (claim, sorted given_keys), the program is built once and
cached. Eval is per-call.

## Why off by default

`build_cache` (the Z3 translation pipeline) uses
`std::process::exit(1)` on dropped constraints — i.e., body
shapes Z3 can't express as Bool. Examples:

- Enum ctor with Seq payload: `b = Many(⟨Red, Green, Blue⟩)`.
- Encoded ASTs: `expected = MakeProgram(__Cell_SchemaDecl(...))`.
- FFI install Seqs in `external type` bodies.

These cases exit the process before my Z3 functionizer can fall
through to the slow path. The Round 22 prototype was naive about
this — Round 23's static `has_known_translator_gap` check catches
the SeqLit-in-ctor case, but doesn't catch the others.

Two paths forward:

1. **Make `translate_bool` return `Result`** instead of fatal-
   exiting. Big refactor, but unblocks the function-izer to be
   enabled by default.
2. **Widen `has_known_translator_gap`** to cover all known gap
   shapes. Whack-a-mole; new gaps will surface as future tests
   are added.

Recommend (1) for Round 24 along with Cranelift codegen.

## Test coverage

- `runtime/tests/z3_eval_hello.rs` — hello with `state = Init`
  pinned, verifies state_next + effects extracted correctly.
- `runtime/tests/z3_eval_unpinned.rs` — hello with state FREE,
  verifies guarded branches work for both `state=Init` and
  `state=Done` from a single extracted program.
- All 444 cargo + 119 conformance tests pass with default
  setting (EVIDENT_FUNCTIONIZE_Z3 unset).
- With `EVIDENT_FUNCTIONIZE_Z3=1`, the function-izer fires for
  schemas whose bodies translate cleanly; falls through to the
  Evident-AST function-izer or full Z3 solve otherwise. Mario
  still works (`EVIDENT_FUNCTIONIZE_Z3=0` is the default on the
  scheduler path too).

## What's measurable

Cache hit on a stable (claim, given_keys) — chain eval at
microsecond scale, native walk of Z3 ASTs. Cache miss cost is
build_cache + simplify (~ms for small claims).

When Z3 simplifies the body to a single `(= var constant)`
(state-pinned hello), the program reduces to a no-op constant
lookup. When Z3 leaves the dispatch open (unpinned state), the
program is a chain of Guarded branches walked at runtime.

Both are fast. The expensive Z3 work happens once at cache
build, not per tick.

## Round 24 plan

1. Refactor `translate_bool` (and the inline pipeline) to
   return `Result<Bool, TranslateError>` instead of
   `std::process::exit`. Most call sites can `?` the result; a
   few top-level entry points need to handle the error (print
   message, return None / propagate).
2. Once safe, flip `EVIDENT_FUNCTIONIZE_Z3` default to ON.
3. Bench Mario. Expect significant speedup from native walking
   of pre-simplified Z3 ASTs vs full Z3 solve per tick.
4. THEN add Cranelift codegen from the Z3 ASTs. Per-component
   JIT compile. That's the "actual native code" the user asked
   for.
