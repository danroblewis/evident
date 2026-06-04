# Task: claim composition inliner + range-prefix + Nat desugar (wave 4l)

## Why

Wave 4k (`docs/plans/wave-4k-membership-walk-shapes.md`) landed
multi-name decls, `⇒`, chained `< N`, chained `≠`. Three Wall-2
shapes remain:

1. **Bare-`ClaimName` composition** (Item 5 of 4k was detection
   only — consumed cleanly, but body constraints dropped). Sites
   in lang: `is_weekend_rule`, `bounded_score`, `is_ok_value`.
2. **Range-prefix shapes** — `0 < x ∈ Int < 10`,
   `0 ≤ score ∈ Nat ≤ 100`, `0 ≤ score ≤ 100`. Head-is-literal so
   never reaches MembershipStep — needs a head-parser prefix scan.
3. **`Nat` → `Int` + `(>= name 0)`** desugar. `Nat` is `Int ∪ {0..}`
   per CLAUDE.md; bootstrap emits an Int decl plus a non-negativity
   assertion. Wave 4k's `< N` / `≠ N` codepaths can be reused for
   the `(>= name 0)` part.

After 4l lands, Wall-2 is closed. Wall-1 (per-claim recompile
~5h/lang-pass) is the FINAL remaining gate; it gets a dedicated
wave (lex-once-multi-claim sample.smt2 — wave-4i Option 1).

## Authorisation

Edit `compiler/*.ev`, `tests/kernel/*.ev` (fixtures), `docs/`.
Forbidden: `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
`tests/conformance/`, Python.

## Required reading

1. `CLAUDE.md` — "Chained membership" (range form), `Nat` typing
   facets, "Composition mechanisms" (the table — 7 forms; we are
   adding the bare-`ClaimName` form here).
2. `docs/plans/wave-4k-membership-walk-shapes.md` — what 4k landed
   AND the sketch for Item-5 composition (driver-level claim-table
   scan of `_fwd`).
3. `docs/plans/blocked-sample-and-eq-fix.md` — Wall 2's full
   shape table, including the three remaining ones this wave
   tackles.
4. `compiler/parse_body.ev` — 4k's `ms_is_implln` etc. are here;
   the head-parse and MembershipStep dispatch start here.
5. `compiler/compiler.ev` — driver loop; this is where the
   claim-table that Item 1's inliner walks must live (sketch in
   the 4k doc).
6. `tests/lang_tests/test_enums_basic.ev` — `is_weekend_rule` is
   the canonical composition example.
7. `tests/lang_tests/test_chained_membership.ev` — range-prefix
   shapes.
8. Bootstrap's parser for the same shapes —
   `bootstrap/runtime/src/parser/` — port equivalents.

## Scope

### Item 1: bare-`ClaimName` composition (inline body)

Per CLAUDE.md's "Composition mechanisms" table, a bare `ClaimName`
on its own line in a claim body inlines the named claim's body
via names-match (no explicit slot binding).

Today (after 4k): detection consumes the line cleanly so the walk
keeps going, BUT the named claim's body constraints are NOT
emitted. Sat-check verdicts that depend on the composition flip.

Fix (sketch from 4k doc): build a program-wide claim table in
`compiler.ev` (during the multi-top-level walk), then in
MembershipStep / driver, when a bare-Ident composition line is
detected, look up the named claim's body and inline its
translated constraints into the current claim's output (with
prefixing — see gotcha).

**Gotcha** (per memory:
[[project-claim-composition-leaks-body-locals]]): the callee's
body-local names UNIFY with the caller's. Prefix all locals when
inlining. The compiler's other composition paths already do
something here; verify the prefixing is wired for the bare-Ident
path.

Fixture: `tests/kernel/test_compiler_driver_composition_bare.ev`
proves a 2-claim composition matches bootstrap byte/semantic.

### Item 2: range-prefix shapes

CLAUDE.md "Chained membership":

```
0 < x ∈ Int < 10       -- declare + range
```

Today: the head-parser sees a literal (`0`) before the name, gets
confused, doesn't reach MembershipStep. The whole line is
mis-handled.

Fix: extend the head-parser to recognize `<lit> <cmp> <name>` as
a prefix BEFORE the membership operator. Emit:
- the regular `(declare-fun <name> () <Type>)`
- one `(assert (<cmp-op> <lit> <name>))` per prefix
- 4k's tail logic handles the `< 10` suffix

Variants to handle: `<`, `≤`, `>`, `≥` on both prefix and suffix
sides. The asymmetric forms `0 < x ∈ Int` (prefix only) and
`x ∈ Int < 10` (suffix only, already wave 4k) both work.

Also handle the form WITHOUT `∈ Type`:

```
0 ≤ score ≤ 100
```

This is a chained-bound on an already-declared name. Same
codepath without the decl emit.

Fixture: `tests/kernel/test_compiler_driver_range_prefix.ev` with
4 sub-cases (`< x ∈ T`, `x ∈ T < N`, `< x ∈ T < N`, `< x ≤ N`).

### Item 3: `Nat` → `Int` + `(>= name 0)`

CLAUDE.md describes `Nat` as nonnegative integers. Today the
self-hosted compiler likely emits `(declare-fun name () Nat)`
which is invalid SMT-LIB (`Nat` isn't a Z3 sort).

Fix: when the type token is `Nat`, emit `(declare-fun name () Int)`
plus `(assert (>= name 0))`. Reuse 4k's chained-tail codepath for
the assertion (it already knows how to emit comparisons against
zero or any literal).

Bootstrap source for the exact emit shape: grep for `"Nat"` in
`bootstrap/runtime/src/translate/`.

Fixture: `tests/kernel/test_compiler_driver_nat_desugar.ev`.

### Item 4: lang single-file probe

After Items 1-3:

```
EVIDENT_SELF_VIA_SMT2=1 scripts/evident-self bin > /tmp/wrap.sh
chmod +x /tmp/wrap.sh
/tmp/wrap.sh sample tests/lang_tests/test_enums_basic.ev --all --json > /tmp/probe.json 2>&1
bootstrap/runtime/target/release/evident sample tests/lang_tests/test_enums_basic.ev --all --json > /tmp/baseline.json 2>&1
diff /tmp/probe.json /tmp/baseline.json
```

Time budget: ~19 claims × 60s ≈ 20 min for the probe.

**Expected: empty diff.** If `test_enums_basic.ev` matches
bootstrap end-to-end, the shape coverage is COMPLETE for the
simplest lang file. (Other lang files may use more shapes
documented in 4j's blocker doc; they get their own waves if so.)

If diff non-empty: document the precise diverging claim in
`docs/plans/blocked-shape-tail.md` with the byte diff.

### Item 5: rebuild + commit compiler.smt2

After all items pass and `./test.sh` is green:

```
scripts/build-compiler-smt2.sh
```

(This regenerates `compiler.smt2` to embed Items 1-3.) Commit
the rebuilt artifact.

## Acceptance

1. All 3 items land with fixtures matching bootstrap.
2. `./test.sh` green default + under `FUNCTIONIZE=0` kernel
   phase.
3. `test_enums_basic.ev` lang probe (Item 4) byte-equal to
   bootstrap OR precise blocker doc.
4. No regression on wave 4g/4h/4j/4k fixtures.
5. `compiler.smt2` rebuilt + committed.
6. Diff scoped to `compiler/*.ev` + new fixtures + wave doc +
   compiler.smt2.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
  `tests/conformance/`.
- Adding Python.
- Implementing lex-once-multi-claim (Wall 1 — separate wave).
- Skipping the Item 4 probe.

## Known gotchas

- Composition leaks body-local names — prefix unification is the
  recurring bug class.
- The naming collision from 4k (`ms_is_impl` vs the existing
  pin-op flag) — search for any name you introduce in
  `compiler/*.ev` before assigning it.
- The lang probe is ~20 min wall-clock; don't iterate it more
  than twice.
- `(>= x 0)` for `Nat` is the SAME shape as 4k's `< N` / `≠ N`
  tail codepaths; reuse the existing assertion-emit helper.

## Reporting back

- Branch (`agent-43-claim-composition-and-shape-tail`).
- Items 1-3 status.
- Item 4 lang-probe headline.
- Test count delta (current: 105).
- Cite docs.

Be terse.
