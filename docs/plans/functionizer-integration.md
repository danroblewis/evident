# Functionizer integration — the macro-finder version

**Status:** research + extraction complete; implementation deferred
(see §5). Source extracted to `legacy-rust/functionizer/` from branch
`feat/compile-constraints-to-programs` (tip `5f0e066`), rounds 22–24.

This doc is the coordinator-readable summary. The deep design is
`legacy-rust/functionizer/docs/compile-claims-to-functions.md`.

---

## 1. What the macro-finder version does

The functionizer turns a constraint body — whose every output is
*uniquely determined* by its inputs — into a callable function, so
the kernel can evaluate it directly instead of asking Z3 to solve it
each tick. It is exactly the "promote a constraint-defined symbol to a
function definition" idea Z3's `macro-finder` tactic embodies; the
shipped pipeline reaches it through the general tactic chain rather
than the single tactic.

Pipeline (`legacy-rust/functionizer/src/z3_eval.rs`):

1. **Normalize.** `simplify_assertions` runs Z3's tactic chain
   `Tactic::new(ctx,"simplify").and_then("propagate-values")` over the
   body's Bool assertions (a `Goal` → `apply` → `list_subgoals`). It
   *deliberately omits* `solve-eqs` — that tactic substitutes
   equality-defined variables away into a model-converter the Rust z3
   binding doesn't expose, destroying the `(= var expr)` shape we need.
   Folding to literal `false` (or a `decided_unsat` subgoal) is a cheap
   UNSAT short-circuit.
2. **Extract.** `extract_program` partitions the simplified assertions,
   keyed by output-variable name, into a `Z3Program`
   (`src/z3_program.rs`):
   - `(= var expr)` where `var ∈ outputs` and `expr` doesn't mention
     `var` → a `Z3Step::Scalar` assignment.
   - `(= var__len N)` + `(= (select var i) elem)` → `Z3Step::Seq`
     (a length pin plus literal-indexed element pins).
   - `(or (not P) Q)` → `Z3Step::Guarded` (`P ⇒ Q`, i.e. a `match`/
     `Implies` arm with a free scrutinee); `eval` walks branches in
     order. `P` is typically a datatype recognizer `((_ is Init) state)`.
   - non-equality leftovers (`(>= n 0)`, `(< x 5)`) → `predicates` that
     must hold at eval time (skipped if they reference an unbound var —
     Z3 already vetted satisfiability).
   - If any output lacks a defining assignment → return `None`,
     **fall through to a full Z3 solve.** Soundness is preserved by
     always being able to refuse.
3. **Gate (per component).** `decompose.rs` runs union-find over the
   free variables to split the body into independent sub-models; each
   is judged by a **2-copy uniqueness check** — `∃ x,y₁,y₂. body(x,y₁) ∧
   body(x,y₂) ∧ y₁≠y₂` UNSAT ⇒ function-shaped. Search-shaped
   components stay on Z3; function-shaped ones get compiled.
4. **Compile / eval.** `eval_program` interprets the `Z3Program`
   natively (the always-available slow path). `cranelift.rs` is the
   fast backend: it lowers the program's ASTs to Cranelift IR → native
   machine code (`fn(*const i64, *mut i64)`), used per component.

Measured: 242× vs Z3 on a 4-op arithmetic claim (native eval); a
further 3.4× from Cranelift over native eval on a 3-step chain. The
expensive Z3 work happens **once at build**, not per tick.

---

## 2. How it wires into the current architecture

Today the kernel (`kernel/src/tick.rs`) does:

```
parse SMT-LIB body  (Z3_parse_smtlib2_string)      → cached body ASTs   [~line 113]
ONE pre-loop .simplify() pass (Z3_simplify each)                        [~line 130]
per-tick loop:
    fresh solver (mech A) | persistent solver (mech B)                  [~line 176]
    apply pins (_<name> = prev, is_first_tick, given)                   [~line 228]
    Z3_solver_check → get_model → read state-next + effects             [~line 238]
    walk effects, dispatch, carry state                                 [tick.rs]
```

The functionizer slots in as a **fourth pre-loop step and a per-tick
fast path**:

- **After parse + pre-loop simplify, before the loop:** run
  `extract_program` over the cached, simplified body assertions,
  partitioned by the manifest's `state-fields` + `effects` as the
  output set (`kernel/src/manifest.rs` already lists these). Result:
  an `Option<Z3Program>` cached for the program's life (the body is
  fixed; only pins change per tick). On `None`, the functionizer is
  simply absent and the existing loop runs unchanged.
- **Per tick:** if a `Z3Program` was extracted, pack the pins
  (`_<name>` carries, `is_first_tick`, any given) into the input slots,
  `eval_program` (or the JIT'd function) to produce state-next +
  effects, and **skip `Z3_solver_check` entirely**. Any output the
  program couldn't define, or any predicate that fails, falls back to
  the Z3 solve for that tick. This is the same fall-through discipline
  the extractor already enforces.

Concrete seam, files that change (kernel — gated, see §4 freeze note):

- `kernel/src/tick.rs`: add the post-simplify extraction call and a
  per-tick branch `if let Some(prog) = &functionized { eval … } else {
  Z3_solver_check … }`.
- new `kernel/src/functionize.rs` (or a sibling crate): the extractor +
  `Z3Program` + evaluator. The extracted `z3_eval.rs`/`z3_program.rs`
  use the **high-level `z3` crate** (`Tactic`, `Goal`, `Bool`,
  `Dynamic`); the kernel is raw `z3-sys`. So this is either (a) a
  raw-`z3-sys` re-port of `simplify_assertions`/`extract_program`, or
  (b) the kernel gains the `z3` crate alongside `z3-sys`. (a) keeps the
  kernel minimal; (b) is faster to land. Both are real work.
- `kernel/src/manifest.rs`: no change — it already names the outputs.

Note the architecture-invariants alignment: the pre-loop simplify is
the *one* simplify invariant #4 already blesses; extraction reuses its
output, adding no per-tick simplify. The functionizer is a per-tick
*reader* of a build-once artifact — it does not rebuild the Z3 model
in the body (invariant honored).

---

## 3. What makes an FSM "functionize cleanly" — shape guidelines

These are the implementation-choice rules. Future sessions choose the
shape on this axis, not on Z3 solve speed (see
`architecture-invariants.md`).

- **Bounded cons-list datatypes beat Z3-native `Seq`.** The extractor
  handles a `Seq` output *only* when its length pins to a literal `N`
  and its elements are literal-indexed (`(= (select arr 0) …)`). A
  `Seq` whose length or indices are symbolic is opaque — extraction
  returns `None` and the whole tick falls back to Z3. An enum cons-list
  (`enum LL = Nil | Cons(Int, LL)`) is a datatype: its recognizers
  (`(_ is Cons)`) and accessors (`Cons__f0`) fold under `simplify` and
  appear as `Guarded` branches the extractor *does* capture, and the
  Cranelift backend can emit them. Prefer cons-lists for bounded work
  stacks / AST traversal (reinforces task #13's finding).
- **Fixed-arity `match` beats variadic `Seq` ops.** A `match` over an
  enum with a free scrutinee normalizes to `(or (not ((_ is V) s)) Q)`
  clauses — directly captured as `Z3Step::Guarded`. Variadic
  `Seq`-fold / `#seq` / `++` operations do not reduce to per-output
  equalities and read as opaque residual predicates.
- **Scalar `var = expr` chains are the sweet spot.** Arithmetic /
  comparison / ternary chains (`sum = a+b`, `next = match state …`)
  extract as `Scalar` steps and JIT to `iadd`/`isub`/`imul`/`sdiv`/
  ITE at native speed. Keep tick bodies as determined scalar
  assignments wherever possible.
- **Recognizer/accessor datatypes are fine; nested-Seq-in-ctor is not.**
  `b = Many(⟨Red,Green,Blue⟩)` (a `SeqLit` payload inside an enum
  constructor) was a known translator-gap that hard-exits — keep enum
  payloads scalar or cons-list, not literal `Seq`.
- **Avoid free `∀ x ∈ range`/`seq[i]` quantifiers in tick bodies.**
  They don't reduce to per-output equalities; they keep the component
  search-shaped and pin it to Z3. (Matches the existing
  "range-of-indices quantifier" idiom-to-avoid.)
- **Determinism is the gate.** If a body's outputs are uniquely
  determined by its inputs (2-copy UNSAT), it functionizes. If the body
  genuinely searches (multiple valid models), it cannot — and *should*
  stay on Z3. Don't try to make a search-shaped FSM look functional.

Rule of thumb: **cons-list over Seq, fixed-arity match over variadic
Seq op, determined scalar assignment over quantified constraint.**

---

## 4. Estimated effort + risk

**Effort: large — multi-session (rough order 8–15 sessions), not one.**
The extracted code is ~3.3k lines of Rust against the *old* `runtime/`
crate's `Value`/`DatatypeRegistry`/high-level-`z3` types, none of which
exist in the kernel. Re-targeting it to `kernel/`'s raw `z3-sys` +
SMT-LIB pipeline is the bulk of the work.

Rough chunking:

1. Port `simplify_assertions` + `extract_program` + `Z3Program` to the
   kernel's value/model types and (likely) raw `z3-sys`. Gate behind an
   env flag, **off by default** (mirrors round-23's
   `EVIDENT_FUNCTIONIZE_Z3=1`). Native `eval_program` only — no JIT yet.
   ← first PR-sized chunk if we implement.
2. Wire the per-tick fast path + fall-through in `tick.rs`; prove
   byte-identical state-next/effects vs the Z3 path on the kernel test
   fixtures.
3. (Later, optional) Cranelift backend for hot components.

Risks:

- **Kernel freeze.** `tick.rs`/`manifest.rs` are `kernel/` — FROZEN by
  default. This needs a written proposal in `docs/plans/` and explicit
  user approval per edit. It also enlarges the "minimal kernel"
  (Cranelift is a heavy dep). The minimal-runtime tension is real and
  must be decided before chunk 1.
- **The extractor uses the high-level `z3` crate**; the kernel uses
  `z3-sys`. Recognizer-variant extraction already relies on parsing the
  *formatted* application string `((_ is Init) state)` because the
  binding doesn't expose datatype params — fragile, and the raw port
  must reproduce it.
- **Fatal-exit translator gaps.** The old `build_cache` did
  `process::exit(1)` on unrepresentable shapes; that footgun must not
  be reproduced — extraction must *refuse* (return `None`), never exit.
- **Soundness.** The 2-copy gate + always-available fall-through is the
  safety net; any port must keep "refuse and let Z3 solve" as the
  default, and verify equivalence against Z3 on every fixture.

---

## 5. Implement now or defer?

**Defer.** Reasons:

- The kernel is FROZEN and the project's live priority is *deleting
  bootstrap*, not adding a kernel optimizer. A functionizer that edits
  `tick.rs` is off the critical deletion path.
- The functionizer's value right now is as a **decision oracle**, not
  running code: it tells current sessions which Evident shapes to write
  (§3) so the eventual self-hosted compiler emits functionizable
  bodies. That value is captured by this doc + the invariants section —
  no code needed.
- The extracted source targets a `runtime/` crate that is itself slated
  for deletion; a real implementation should target the kernel +
  SMT-LIB pipeline, which is still settling.

**When we do implement,** the first PR-sized chunk is §4 chunk 1:
port `simplify_assertions` + `extract_program` + native `eval_program`
into a kernel-side module behind an off-by-default env flag, prove it
produces a `Z3Program` for the determined-scalar kernel fixtures, and
fall through to Z3 on everything else — zero behavior change with the
flag off. That requires a `kernel/` edit proposal + user approval
first.
