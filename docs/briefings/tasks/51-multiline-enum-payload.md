# Task: multiline enum payload variant tag enforcement (wave 4t)

## Why

Lang seam v5 (`/tmp/lang-seam-v5.log`, 145/164 = 88.4% pass after
waves 4q+4s) leaves 19 failures. **9 of those — nearly half — cluster
in a SINGLE file (`test_enums_mutual.ev`) with a single shape: enum
variants declared MULTI-LINE without a `|` separator** where a
payload-equality constraint isn't enforcing the variant tag.

All 9 failures are `unsat_*→sat` — the compiler should reject the
contradictory pin but emits a SAT program. The dropped assertion is
the variant-tag check.

```
enum Expr =
    Lit(Int)
    Add(Expr, Expr)

claim unsat_expr_op_mismatch
    e ∈ Expr = Lit(5)
    e = Add(...)        -- ← contradicts the Lit pin; should be unsat
```

Wave 4g added newline-separated variant parsing for the enum
declaration. Wave 4p added `name = Ctor(args)` equality assertion
for the bare bool case. Neither fully covers the multi-line +
payload-equality combination — the contradiction isn't compiled into
a tag check.

This wave closes that ONE class. Hypothesized result: 9 of 19
failures close. Lang phase → ~154/164 = ~93.9%.

## Authorisation

Edit:
- `compiler/*.ev` — likely `parse_body.ev` and/or
  `parse_body_ctor.ev` and/or the enum-decl translator.
- `compiler.smt2`, `sample.smt2` — rebuild after the source change.
- `tests/kernel/*.ev` — fixture.
- `docs/plans/wave-4t-*.md` — wave doc.

Forbidden: `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
`tests/conformance/`, Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/wave-4q-...` (if it exists) — what 4q tried for
   this class.
3. `docs/plans/grammar-wave4g.md` — newline-separated variants
   landing.
4. `docs/plans/wave-4p-equality-rhs-complex.md` — Item 1's
   `cs_bare_ctor` pattern (the bare-`=` ctor equality fix).
5. `tests/lang_tests/test_enums_mutual.ev` — the 9 failing claims.
   Read each one to identify the EXACT shape.
6. `STATE.md` — current state.

## Scope

### Item 1: identify the missing constraint

For each of the 9 failing claims in test_enums_mutual.ev:
1. Run bootstrap on the file, capture the emitted SMT-LIB for that
   claim.
2. Run self-hosted, capture its SMT-LIB.
3. `diff` to find the missing assertion(s).
4. Identify which compiler pass should have produced it.

Most likely: the `name = Ctor(args)` shape, when applied to a
multi-line variant, doesn't emit `((_ is Ctor) name)` (the
recognizer that contradicts a prior `name = OtherCtor(...)`).

### Item 2: implement the fix

In whichever pass: when emitting an equality assertion `name =
Ctor(args)` for a constructor application, also emit the variant
recognizer `((_ is Ctor) name)` (or the implicit equivalent in the
constructor application's downstream constraints).

OR: the multi-line variant parsing might be silently dropping the
constructor name entirely. In that case the fix is in the parser,
not the translator.

Verify via the diff in Item 1.

### Item 3: fixture

`tests/kernel/test_compiler_driver_multiline_payload_tag.ev`
pinning the smallest reproducer for one of the 9 failing claims.
Must `--semantic` match bootstrap.

### Item 4: rebuild + probe

```bash
scripts/build-compiler-smt2.sh
scripts/build-sample-smt2.sh
EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang
```

Target: ≥154/164 pass (close 9 of 19). Document any new fails.

### Item 5: don't break kernel tests

```bash
./test.sh --kernel
```

Must be 111/111 green.

## Acceptance

1. Item 1's diff identifies the missing assertion(s) precisely.
2. Item 2 fix lands; kernel fixture passes byte/semantic-identical
   to bootstrap.
3. `EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang` shows ≥154/164
   pass (close 9 multiline failures).
4. `./test.sh --kernel` 111/111 green.
5. compiler.smt2 + sample.smt2 rebuilt + committed.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
  `tests/conformance/`, Python.
- Hardcoding test_enums_mutual.ev's specific claim shapes.
- Tackling the other 10 non-multiline failures (separate wave).

## Known gotchas

- `test_enums_mutual.ev` uses both single-line and multi-line enum
  declarations; ensure the fix doesn't regress the single-line case.
- compiler.smt2 + sample.smt2 must be REBUILT after compiler.ev
  edits; the prior wave forgot, causing apparent "no progress."
- Wave 4q already touched compiler.ev/sample.ev (rescued at
  commit `afc7513`); read the diff to understand what's already
  changed before adding more.

## Reporting back

- Branch (`agent-51-multiline-enum-payload`).
- Items 1-4 status.
- Lang pass rate after this wave (the headline; target 154+).
- Test count: should stay 111.
- Cite docs.

Be terse.
