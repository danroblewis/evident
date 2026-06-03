# Task: compiler.ev grammar — wave 3.5 (integrate enum/match/Seq into the monolithic driver)

## Why

Wave 3 (task #29) landed the translator passes for enum, match,
matches, Seq. **But only `matches` is fully integrated into the
canonical `compiler/compiler.ev` driver.** Enum / match / Seq are
proven in dedicated test fixtures that inline the same pipeline,
but the monolithic driver doesn't dispatch to them yet.

That means `compiler.smt2` built TODAY (via bootstrap from
`compiler.ev`) would NOT compile real `.ev` files that use enum,
match, or Seq. The deletion-path cutover requires compiler.smt2
to handle those shapes.

The wave-3 session called this restructure "wave 3.5" and
documented the integration gaps in
`docs/plans/grammar-wave3.md` §"Integration honesty." Two
concrete gaps:

1. **`enum` is a top-level form**, sibling to `claim`. The driver
   today assumes every top-level item is a claim with a body.
   Needs head-token dispatch at the top level.
2. **`match` / `Seq` in membership RHS positions** need a
   per-membership sub-walk. The single-tick `MembershipStep`
   doesn't host them; the driver needs to recognize when a
   membership's RHS is a match expression or Seq literal and
   invoke the right pass.

## Authorisation

You may edit `compiler/*.ev`, `tests/kernel/*.ev` (new + existing),
and `docs/`. No `bootstrap/`, no `kernel/`, no Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/grammar-wave3.md` — what wave 3 delivered and what
   it explicitly deferred. **READ THIS FIRST** — it documents the
   exact integration gaps.
3. `compiler/compiler.ev` — the canonical monolithic driver.
4. `compiler/parse_body.ev` — `MembershipStep` + `TLHd`/`TLTl`,
   the structure you'll extend.
5. `compiler/parse_body_seq.ev` (added in wave 3) — the worked
   pattern for per-membership Seq handling.
6. `tests/kernel/test_compiler_driver_enum.ev`,
   `test_compiler_driver_match.ev`, `test_compiler_driver_seq.ev`
   — the wave-3 dedicated fixtures showing the passes work; you
   need to make `compiler.ev` produce the same output without the
   fixture's inline plumbing.
7. `compiler/translate.ev` (enum), `compiler/translate_match.ev`,
   `compiler/translate_seq.ev` — the per-pass implementations.

Cite #2 and #4 in your report.

## Wave 3.5 scope

### Item 1: top-level head-token dispatch for `enum`

When `compiler.ev`'s top-level walker sees a `KwEnum` head token,
it should dispatch to `translate.ev`'s `EnumDeclSmtlib` (or
similar) instead of the claim/body path. Mirror the bootstrap
parser's `program.rs` structure: each top-level item is one of
`claim` / `type` / `schema` / `fsm` / `enum` / `import`, dispatched
by head token.

Today's driver effectively assumes `claim foo` + body. Generalize
to: read head token → if `KwEnum`, handle enum; if `KwClaim`/etc,
handle schema with body.

### Item 2: per-membership RHS sub-walk

When `MembershipStep` (or its successor) sees a membership like
`n = match e { … }` or `xs ∈ Seq(Int) = ⟨1,2,3⟩` or
`ys = xs ++ ⟨4⟩`, the RHS Expr needs to be walked by the
appropriate translator pass. The existing arithmetic RHS path
(`translate_arith.ev`'s recursive walker) is the model.

Concretely:
- `EMatch(scrutinee, arms)` → invoke `translate_match.ev`
- `ESeqLit(items)` → invoke `translate_seq.ev`'s literal path
- `EBinOp(OpConcat, ...)` → invoke `translate_seq.ev`'s concat path
- `EBinOp(OpLen, ...)` → invoke `translate_seq.ev`'s length path

If the Expr ASTs for these don't exist yet in `compiler/parser.ev`,
add them (wave 3 noted: `EMatch`, `ESeqLit`, `OpConcat`, `OpLen`
may or may not be present). Check first.

### Item 3: regression — wave 3's fixtures should now also work via the canonical driver

Adapt the wave-3 fixtures so they use the canonical
`compiler/compiler.ev` (or a fresh fixture proving the integration
without changing the existing wave-3 fixtures). Specifically: add
fixtures like `test_compiler_driver_canonical_enum.ev` and
`test_compiler_driver_canonical_match.ev` that drive the
unmodified `compiler.ev` on enum / match / Seq inputs and verify
byte-identical SMT-LIB to bootstrap.

### Item 4: smoke test — `compiler.ev` self-compile still green

After integration, run:

```
scripts/flatten-evident.sh compiler/compiler.ev > /tmp/flat.ev
bootstrap/runtime/target/release/evident emit /tmp/flat.ev main -o /tmp/orig.smt2
```

This should remain exit 0 (bootstrap compiles the flattened
self-hosted compiler). Include the line count of the flattened
output for comparison with wave 3's number (2281 lines).

## Acceptance

1. `compiler/compiler.ev` dispatches `enum` at top level.
2. `compiler/compiler.ev` dispatches `match`/`Seq` in membership
   RHS positions.
3. At least 3 new "canonical" fixtures that prove the integration
   (one per shape, driving the unmodified `compiler.ev`).
4. All wave-1 / wave-2 / wave-3 / MVP fixtures still pass.
5. `./test.sh` is fully green in all 3 functionizer modes.
6. Smoke test (item 4) is exit 0.
7. Diff scoped to `compiler/*.ev` + new tests + new
   `docs/plans/grammar-wave3.5.md`.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Adding Python.
- Tackling wave 4 (quantifiers, generics, records, subclaims,
  imports-as-runtime-resolved).
- Removing or weakening the wave-3 dedicated fixtures (they prove
  the per-pass mechanics; keep them).

## Known gotchas (from wave 2 + 3)

- Op/Token/Expr variant names are globally unique. New variants
  (OpConcat, OpLen, EMatch, ESeqLit, etc.) may already exist —
  check first.
- Composition leaks callee body-local names into the caller.
  Prefix all locals in new pass-claims.
- The work-stack recursive-walk pattern is the canonical shape.

## Reporting back

- Branch pushed (`agent-31-compiler-grammar-wave35`).
- Per-item status (1: enum dispatch, 2: RHS sub-walk for each of
  match/Seq variants, 3: canonical fixtures, 4: smoke).
- Test count delta (current: 85).
- Smoke test output (exit code + flattened line count).
- Cite docs.

Be terse.
