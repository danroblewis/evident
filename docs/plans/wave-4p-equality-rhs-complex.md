# Wave 4p — `name = <complex expr>` equality assertions

Generalises the wave-4j bare-`=` fix (`name = atom`) to a
right-hand-side that is a **complex expression** — constructor
applications, parenthesised ternaries, `matches` predicates, and
negative-int literals. These shapes were silently dropped by the
self-hosted compiler, so every contradictory `unsat_*` lang claim
that used them reported SAT under the seam
(`EVIDENT_SELF_VIA_SMT2=1`). Wave 4o's lang-seam baseline was
130 / 164 (79.3%); every one of the 34 failures was a dropped
constraint.

## What the seam actually uses

The lang seam runs through `scripts/sample-via-smt2.sh`, which
drives **`sample.smt2`** (built from `compiler/sample.ev`), NOT
`compiler.smt2`. Both drivers share the per-membership step claims
(`compiler/parse_body*.ev`), so the step-claim fixes are common; the
two drivers each needed the dispatch wiring (`cts_bare`) applied.

## Items landed

### Item 1 — payload-ctor equality `name = Ctor(args)`  ✅

Bare `r = Ok(5)` / `e = Exit(0)` / `list = Cons(7, Cons(8, Nil))`
after a prior `name ∈ EnumType`. Before: the bare-atom path read the
ctor name as a plain atom (`(= r Ok)`, dropping `(args)`) and the
driver mis-routed the `t3=LParen` line to `SeqMembershipStep`.

Fix: `compiler/parse_body_ctor.ev`'s `CtorMembershipStep` now
self-discriminates the bare form (`cs_bare_ctor`: t1=Eq, t2=ctor
Ident, t3=LParen) and renders the RHS through the existing
`translate_ctor.ev` `RenderExprToks` (L3 depth — handles the nested
ctor corpus: `EOp(OpAdd, ELit(1), ELit(2))`, `AVia(BVia(CVia(ALeaf)))`,
`JArray(JCons(JNum(1), JEmpty))`). Emits a single
`(assert (= name <rhs>))` with no decl/field; the driver
(`compiler/compiler.ev` + `compiler/sample.ev`) reads the new `bare`
output and sets `pinned=false` so the chunk is exactly the assertion.

Closes (bare-ctor only): all of `test_enums_mutual` (11),
`test_enums_payload` (8 + the composition one), `test_kernel_enums::unsat_exit_wrong_payload`.

### Item 2 — parenthesised ternary `name = (cond ? a : b)`  ✅

`x = (flag ? 7 : 99)`. `compiler/parse_body.ev`'s `MembershipStep`
gains `ms_is_barety` (t2=LParen, single-token cond at t3,
t4=Question) → `(assert (= name (ite cond then else)))`. Branch
atoms render int / ident / string / bool. Compound conditions
(`n > 3 ?`) and nested else-ternary are out of scope (the corpus's
failing case is the simple form; the sat nested/comparison cases pass
trivially). Closes `test_ternary::unsat_int_pinned_to_other_branch`.

### Item 5 — `matches` predicate `scrut matches Ctor(_)`  ✅

Standalone `c matches Green` / `s matches Circle(_)`. Before: the
driver read it as a lone composition name and dropped the predicate.
`CtorMembershipStep` now self-discriminates `cs_bare_matches`
(t1=KwMatches, t2=ctor Ident) and emits the Z3 recognizer
`(assert ((_ is Ctor) scrut))` (payload binds ignored, per the
matches-as-Bool contract). Closes
`test_matches::unsat_matches_wrong_nullary`,
`test_matches::unsat_matches_payload_wrong_variant`.

### Item 6 (bonus) — negative-int RHS `name = -N`  ✅

`pos_x = -1` lexes as `Minus IntLit(N)`; the atom helpers saw only
`Minus` (→ ""), dropping the value. `MembershipStep` now detects
`ms_bare_neg` and renders `(- N)`, advancing the rest by one extra
token. Closes `test_chained_membership::unsat_lower_bound_violated`,
`unsat_range_below`.

## Perf note (wave-4m caution)

`sample.smt2` instantiates every step claim per membership across all
claims simultaneously, and `RenderExprToks` (L3) is the heaviest
claim. The three new RHS renders (scalar / bare-ctor / bare-matches)
are folded into ONE conditional `RenderExprToks` call
(`cs_rhs_start`), so the step's model footprint is unchanged — the
rebuilt artifacts actually shrank (260k → 228k lines).

## Deferred (separate waves)

- **Item 3 — record-literal equality.** The failing
  `test_record_lit_arg` claims are record literals **as claim
  arguments** (`use_color(Color(10,20,30), s)`), i.e. composition +
  positional/`↦` binding — NOT the `c = Color(...)` shape the brief
  described. Needs the claim-call argument expander, not a bare-RHS
  fix.
- **Item 4 — match-result equality** (`unsat_match_result_pinned_wrong`).
  Needs three things `MatchMembershipStep` doesn't do: the BARE form
  (t1=Eq), a payload-binding body (`Ok(n) ⇒ n * 10`, needs the
  `Ok__f0 r` accessor), and a non-wildcard second arm (`Err(_)`).
- **Multi-name range** (`unsat_multi_name_range_violation`:
  `0 < x, y, z ∈ Int < 10`) — range-prefix handles only a single name;
  also needs the `x = -5` negative (now fixed) AND multi-name range.
- **Composition + chain** (`unsat_chain_via_composition_violates`,
  `unsat_weekend_via_claim_wrong`) — depend on the lone-name inliner
  fully reproducing the callee's chained/range body.

## Fixtures

- `tests/kernel/test_compiler_driver_eq_ctor.ev` (Item 1)
- `tests/kernel/test_compiler_driver_eq_ternary.ev` (Item 2)
- `tests/kernel/test_compiler_driver_eq_matches.ev` (Item 5)

All three are isolation tests (hand-built TokenLists → the step
claim), validated via bootstrap `emit` + kernel run. Kernel suite:
111 tests, 0 failed (was 108).
