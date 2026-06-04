# Task: sample verb + enum-equality fix (wave 4j)

## Why

Wave 4i (`docs/plans/blocked-bootstrap-cutover.md`) revealed the
self-hosted toolchain has only the `emit` verb; `evident sample` is
needed by 11 lang_tests (~190 claim assertions, the project's only
language-semantics coverage). A subsequent probe in this session
revealed a *second* gap: the self-hosted compiler mis-handles
`name = EnumConstant` after a membership decl, emitting
`(declare-fun today () Mon)` instead of `(assert (= today Mon))`.
Both gaps are real, both gate the cutover.

This wave closes both, then re-runs wave 4i's probe to confirm
cutover viability.

## Authorisation

Edit:
- `compiler/*.ev` — the equality-assertion fix lives here.
- `scripts/evident-self` — extend to dispatch the `sample` verb.
- `scripts/sample-via-smt2.sh` (new) — the wrapper.
- `scripts/build-compiler-smt2.sh` — only if a downstream change
  forces it.
- `tests/kernel/*.ev` — new fixtures for both gaps.
- `docs/` — wave doc.

Forbidden: `bootstrap/`, `kernel/`, `stdlib/`, `tests/lang_tests/`,
`tests/conformance/`, Python.

## Required reading

1. `CLAUDE.md` — verb table, definition of done, deletion path.
2. `docs/plans/blocked-bootstrap-cutover.md` — wave 4i's diagnosis;
   the two candidate paths for sample.
3. `STATE.md` — current state + the equality-assertion finding.
4. `scripts/evident-self` — the cutover seam structure.
5. `compiler/compiler.ev` — esp. lines 285-300 (the
   claim-selection comment that explains why per-claim selection
   is missing).
6. `compiler/parse_body.ev` and the `=` handler — where the
   `name = X` shape gets dispatched.
7. `tests/lang_tests/test_enums_basic.ev` — the simplest of the 11
   files; useful as a target.
8. **Bootstrap's `sample`** — `bootstrap/runtime/src/main.rs::cmd_query_or_sample`
   for the contract / output shape.

## Probe of the equality bug (verbatim from this session)

```
echo 'enum Day = Mon | Tue | Wed
claim main
    today ∈ Day
    today = Mon
    today = Tue' > /tmp/probe.ev

scripts/flatten-evident.sh /tmp/probe.ev > /tmp/probe.flat.ev
cp /tmp/probe.flat.ev /tmp/compiler-input.ev
./kernel/target/release/kernel ./compiler.smt2 < /tmp/probe.flat.ev > /tmp/probe.smt2
```

Output (post-prelude):

```
(declare-datatypes ((Day 0)) (((Mon) (Tue) (Wed))))
(declare-fun today () Day)
(declare-fun today () Mon)   ← BUG — should be (assert (= today Mon))
(declare-fun today () Tue)   ← BUG — should be (assert (= today Tue))
```

Manifest also wrong: `state-fields = today:Day today:Mon today:Tue`.

The shape `name = EnumConstant` after a prior `name ∈ Type` is
being parsed as a new chained-membership (declare-and-pin) rather
than an assertion. The fix lives in whichever pass handles bare
`name = expr` lines.

## Scope

### Item 1: fix the enum-equality assertion bug

**Bare `name = expr` after a prior `name ∈ Type` membership must
emit `(assert (= name expr))`, NOT a new declaration.**

Find the relevant handler in `compiler/parse_body*.ev` /
`compiler/translate*.ev`. Fixture:

```
-- tests/kernel/test_compiler_driver_eq_assertion.ev
-- Pre-declared name then equality to enum constant → assertion
```

Pass byte-identical to bootstrap (use
`scripts/diff-vs-bootstrap.sh --semantic`).

### Item 2: claim-name selection in compiler.ev

Today `compiler.ev` emits the LAST bare-head claim (corpus
convention from `compiler/compiler.ev:291-296`). For `sample
<file> <claim>` to work, the compiler must be able to select a
specific claim by name.

Cheapest shape: read the target claim name from a fixed
path (e.g. `/tmp/compiler-target-claim.txt`) the same way it
reads source via `ReadFile`. If the file is empty/absent, fall
back to "last bare-head" (backwards compat).

Then `evident-self`'s sample wrapper writes the target claim name
to that file before invoking kernel + compiler.smt2.

Fixture: `tests/kernel/test_compiler_driver_claim_select_by_name.ev`.

### Item 3: sample wrapper

`scripts/sample-via-smt2.sh` (new). Contract:

```
sample-via-smt2.sh <file.ev> --all --json
```

Reads `<file.ev>`, finds every top-level claim, for each:
1. Sets the target-claim name (Item 2's mechanism).
2. Invokes `kernel + compiler.smt2` with the file as input.
3. Appends `(check-sat)` to the emitted SMT-LIB.
4. Runs `z3` standalone on it.
5. Records sat → `true`, unsat → `false`.

Output: JSON `{"claim_name": bool, ...}` to stdout, matching
bootstrap's `sample --all --json` exactly.

`z3` is at `/opt/anaconda3/bin/z3` (4.16.0); resolve it via
`command -v z3` to avoid hardcoding the path.

Also support the single-claim form:

```
sample-via-smt2.sh <file.ev> <claim> [--json]
```

— emits one entry's JSON or, without `--json`, a human-readable
sat/unsat line.

### Item 4: wire into evident-self

In `scripts/evident-self`:

- Dispatch `sample` to `scripts/sample-via-smt2.sh` under
  `EVIDENT_SELF_VIA_SMT2=1` (and only then; bootstrap fallback
  still active otherwise).
- The wrapper-script generation pattern (`emit_via_smt2_wrapper`)
  can be reused.

### Item 5: re-run wave 4i's probe

```
EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang
```

**Expected: green.**

If any lang test fails:
- If on a NEW compiler bug (e.g. some shape the self-hosted
  compiler still can't handle): document in
  `docs/plans/blocked-sample-and-eq-fix.md` with the precise
  diagnostic and stop. Don't paper over.
- If on a sample-wrapper bug: fix it.

Also re-run kernel and conformance phases under the seam (Items
1+2 may have side effects):

```
EVIDENT_SELF_VIA_SMT2=1 bash test.sh --kernel
EVIDENT_SELF_VIA_SMT2=1 bash test.sh --conformance
```

These are slow (hours). Run last. They tell the coordinator if
any further self-hosted-compiler gaps exist beyond Items 1+2.

## Acceptance

1. `=`-as-assertion fix (Item 1) lands; byte-identical to
   bootstrap on the new fixture.
2. Claim-name selection (Item 2) lands; backwards-compat preserved
   (existing fixtures unaffected when no target file).
3. `scripts/sample-via-smt2.sh` exists and matches bootstrap's
   JSON output on a single lang_test file.
4. `scripts/evident-self` dispatches `sample` under the seam.
5. `EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang` green OR a
   precise blocker doc identifies the next gap.
6. `./test.sh` (default, bootstrap-path) still green.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Adding Python.
- Modifying any test fixture in `tests/lang_tests/` or
  `tests/conformance/` to make the seam happy.
- Skipping Item 5's lang probe.

## Known gotchas

- Each self-hosted compile is ~minute(s); a sample run on an
  11-claim file is ~10 min. Budget accordingly.
- The functionizer diagnostic line `[functionizer] ...` is the
  FIRST line of the kernel's output — strip it before piping to
  z3 (`grep -v '^\[functionizer\]'`).
- The emit prelude (Result, last_results) is shared across all
  claims; that's fine for sat-check, just declares extra
  unconstrained vars.
- `tests/lang_tests/*.ev` files do NOT have a `main` claim or
  `effects`; their claims are pure constraint sets. The compile
  should still succeed (it does on a simple probe per this
  session's testing).

## Reporting back

- Branch (`agent-41-sample-and-eq-fix`).
- Item 1-4 status.
- **Item 5 result — green or precise next blocker.**
- Test count delta.
- Cite docs.

Be terse.
