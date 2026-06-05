# Task: equality-RHS-record + match-result + composition gaps (wave 4q)

## Why

Wave 4p (`docs/plans/wave-4p-equality-rhs-complex.md`) closed 4 of 6
classes of `name = <complex RHS>` shapes that were dropping
constraints in lang_tests: bare ctor application, ternary, matches
predicate, negative ints. It DEFERRED 4 more classes that still
cause `unsat_*` claims to report sat under the seam:

| Shape | Example | Sites |
| ----- | ------- | ----- |
| Record literal equality | `c = Color(r ↦ 1, g ↦ 2, b ↦ 3)` | `test_record_lit_arg.ev::unsat_positional_color` etc. |
| Match-result equality | `r = match e (Ok(v) ⇒ v; Err(_) ⇒ 0)` | `test_match.ev::unsat_match_result_pinned_wrong` |
| Multi-name range / chain | `0 < a, b ∈ Int < 10` (LHS only) | `test_chained_membership.ev::unsat_multi_name_range_violation` |
| Composition + chain | `chain_via_composition_violates` | `test_chained_membership.ev` |

This wave closes those. After it lands, the lang phase should be
≥98% pass and the cutover gate truly clears.

## Authorisation

Edit `compiler/*.ev`, `tests/kernel/*.ev` (fixtures), `docs/`.
Forbidden: `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
`tests/conformance/`, Python.

## Required reading

1. `CLAUDE.md` — "Composition mechanisms", "Records and lift forms",
   "Match and `matches`".
2. `docs/plans/wave-4p-equality-rhs-complex.md` — what 4p landed and
   the design pattern for adding new bare-`=` shapes.
3. `compiler/parse_body.ev` — `ms_is_bare` and the dispatch.
4. `compiler/parse_body_ctor.ev` — `CtorMembershipStep` (4p's
   `cs_bare_ctor` lives here).
5. `compiler/parse_body_match.ev` — match-pattern parser.
6. `compiler/translate*.ev` — translation rules.
7. `tests/lang_tests/test_match.ev`, `test_record_lit_arg.ev`,
   `test_chained_membership.ev` — the failing fixtures.
8. Bootstrap's `runtime/src/translate/expr.rs` (or similar) —
   reference for the match-result and record-lit lowerings.

## Scope

### Item 1: record-literal equality

`c = Color(r ↦ 1, g ↦ 2, b ↦ 3)` where `Color` is a record type
with fields `r, g, b ∈ Nat`. Lower to:

```
(assert (= (Color__r c) 1))
(assert (= (Color__g c) 2))
(assert (= (Color__b c) 3))
```

Or one composite assertion using a Color constructor:

```
(assert (= c (Color 1 2 3)))
```

Pick whichever bootstrap emits (the composite is more compact; the
per-field form composes better with partial record-lits).

Also handle positional form `c = Color(1, 2, 3)`.

Fixture: `tests/kernel/test_compiler_driver_eq_record_lit.ev` with
both positional and `↦` forms.

### Item 2: match-result equality

`r = match e (Ok(v) ⇒ v; Err(_) ⇒ 0)`. Lower to an ITE chain over
the enum's recognizers:

```
(assert (= r
   (ite ((_ is Ok) e) (Ok__f0 e)
   (ite ((_ is Err) e) 0
   <fallback>))))
```

The fallback for a complete match is the last branch's value. For
incomplete match (no wildcard), Z3 picks unconstrained → use
some default or refuse on partial match.

Fixture: `tests/kernel/test_compiler_driver_eq_match.ev` with both
complete (wildcard) and pattern-by-pattern matches.

### Item 3: multi-name range — LHS-prefix only

`0 < a, b ∈ Int < 10`. The current multi-name decl handler
(wave 4k Item 1) handles `a, b ∈ T` but not the prefix `0 <`
applied to all names.

Extend the head parser to recognize `<lit> <cmp> <name1>, <name2>, ...`
as a prefix bound applied to each name:

```
(assert (< 0 a))
(assert (< 0 b))
(assert (< a 10))
(assert (< b 10))
```

(Wave 4l's range-prefix handles single-name `0 < x ∈ Int < 10` —
this extends to the multi-name case.)

Fixture: `tests/kernel/test_compiler_driver_multiname_range.ev`.

### Item 4: composition + chain (`chain_via_composition_violates`)

Look at `test_chained_membership.ev` to see the exact shape.
Likely: a claim's body inlines another claim via composition, and
the composed body uses a chained bound that the bare-`ClaimName`
inliner from 4l doesn't fully translate (the bound assertions get
dropped).

Verify the precise drop:
1. Run bootstrap on the file, capture the SMT-LIB for the failing claim.
2. Run self-hosted, capture its SMT-LIB.
3. Diff to find the missing assertions.
4. Fix in the appropriate pass.

Fixture: `tests/kernel/test_compiler_driver_composition_chain.ev`.

### Item 5: verify

```bash
EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang
```

**Expected: ≥160 / 164 (≥97%).** Document any remaining failures.

## Acceptance

1. All 4 items land with kernel fixtures.
2. `EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang` ≥97% pass.
3. `./test.sh` (default) green; no regression.
4. Compiler.smt2 + sample.smt2 rebuilt + tracked.
5. Wave doc.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
  `tests/conformance/`.
- Adding Python.
- Hardcoding lang_test claim names.

## Known gotchas

- Sample.smt2 must be rebuilt (it embeds compiler.ev's translate
  rules). `scripts/build-sample-smt2.sh`.
- Record types and match patterns interact with the wave-4g/4l
  field-type table. Extend that table to record types.
- Lang phase under seam is ~1.5h wall. Budget accordingly; don't
  iterate the full probe more than twice.

## Reporting back

- Branch (`agent-48-equality-rhs-record-match`).
- Items 1-4 status.
- Lang pass rate (headline).
- Test count delta (current: 111).
- Cite docs.

Be terse.
