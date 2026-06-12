# A′ — declaration pre-scan: retire the hoist passes by fixing the compiler

**Type:** design / compiler change. Not started in code (scoped 2026-06-12).
**Goal:** make the self-hosted compiler register every declared name BEFORE it
builds any body, so declaration-before-use is automatic — fixing the
bodyless-record ordering cluster natively AND letting `scripts/passes/
hoist-decls.sh` + the cross-lift hoist (awk) be **deleted**. This is the
"fix the lowering in the compiler (Evident), not in a bash pre-pass"
direction — it *shrinks* the awk surface instead of growing it.

## Why this exists

The oracle is whole-program: it parses everything, so every const is known
before it emits, and a use-before-decl never bites. The self-hosted compiler
(`compiler2/driver.ev`) is a **one-pass incremental Z3-AST builder** (a tick
machine — it materializes each Z3 const as it walks the source). So a name
used before its declaration line is processed gets a **0 handle** → the
`rc=9` / `rc=70/71` failures.

Today we paper over this with awk: `hoist-decls.sh` reorders declarations to
the front of each claim, and the cross-lift hoist copies a `..`-lifted child's
declarations into the caller. This works for `driver_main` but (a) is more awk,
and (b) does NOT move ASSIGNMENT-declared field-consts — which is why bodyless
records (`line.at_eof` from `ClassifiedLine`, `_qloop.on` from `QLoop`, …) still
fail in the unit wrappers: a bodyless record's fields are declared by their
`r.field = …` assignments, which the hoist can't move (they have RHS deps).

## The fix — a two-pass build

Currently the build phase (`phase = 3`) walks the token buffer once, classifies
each line, and emits work-items: declarations → `C2DeclConst` (which mints the
Z3 const via `mksym`/`mkconst` and writes the symtab entry, see driver.ev:811+),
bodies/constraints → `C2Process` + assert.

Split phase 3 into two sub-passes over the SAME token buffer:

- **3a — declaration scan.** Walk the buffer; for every line that DECLARES a
  name (membership decls, the `__len` duals, the `_x` carries, AND the
  field-const targets of `r.field = …` assignments on a record membership),
  emit ONLY its `C2DeclConst` (mint const + register symtab). Skip all
  bodies/constraints. This is the "declare all consts first" the oracle does
  implicitly.
- **3b — body build.** Reset the token cursor; walk again; emit the bodies
  (`C2Process` + assert). Every name now resolves from the symtab regardless
  of source order — including a sibling-lifted module reading a name a
  later-lifted module declares.

Key insight for bodyless records: in 3a, treat a record membership `r ∈ T` as
declaring `r.f` for every field `f` of `T` (read the field list — the same
field list the body-record broadcast already walks). Then `line.at_eof`
resolves without any flattening pass.

## Insertion points (driver.ev)

- `phase` transition (driver.ev:763): add the 3a→3b sub-state; gate the build
  on which sub-pass.
- The token cursor / FTI walk: needs a reset between 3a and 3b (re-seek to the
  first body line). The lexer already keeps `tok_buf.base`; the build cursor is
  the work-item walk — a second pass means re-driving it from the start.
- The line classifier (`..DriverClassify`): emit decls-only in 3a (suppress the
  `C2Process`/assert items), bodies-only in 3b (suppress `C2DeclConst`, since
  3a already declared — the existing `is_redeclared` no-op handles any overlap).
- The record field-const declaration in 3a needs T's field list. Either reuse
  the recs registry (cap 3 — too small; the wall we hit before) OR read the
  field names from the type decl / claimindex header. The latter avoids the
  registry-cap rework and is the cleaner path.

## Payoff

- Fixes the bodyless-record cluster (≥13 unit modules) natively.
- **Deletes** `scripts/passes/hoist-decls.sh` (~130 lines awk) and the cross-lift
  logic — net awk reduction.
- Makes the compiler whole-program-aware for declarations (matches the oracle),
  removing a whole class of ordering fragility.
- Likely lets `flatten-body-records.sh` shrink too (the record gap was also
  partly an ordering/registration problem).

## Risk / why it's a focused fresh effort

This touches the compiler's hottest dispatch (the work-item phase state). A
half-done split leaves the build broken (conformance red). It needs: the
two-pass phase state, the cursor reset, the classifier gating, and the
field-list source for 3a — then re-gate (units + conformance + the self-compile
sweep). Do it as a dedicated effort, gated per step, NOT interleaved.

## Sequencing

1. Add the 3a/3b phase split with the classifier gating (decls-only / bodies-only),
   field-consts still from assignments only (no record-field pre-decl yet).
   Gate: conformance unchanged (this alone should be behavior-neutral for
   already-ordered programs).
2. Add record field-const pre-declaration in 3a (read field list from the type
   decl/claimindex). Gate: the bodyless-record sweep modules go clean.
3. Delete `hoist-decls.sh` + cross-lift; re-gate conformance + self-compile sweep.
4. (Stretch) revisit whether `flatten-body-records.sh` can shrink/go too.
