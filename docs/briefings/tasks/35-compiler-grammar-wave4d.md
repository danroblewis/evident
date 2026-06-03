# Task: compiler.ev grammar — wave 4d (emit prelude: Result + last_results)

## Why

Wave 4c closed the dominant Seq/effects encoding blocker. The
smoke test surfaced a NEW dominant blocker, documented in
`docs/plans/blocked-grammar-wave4c.md`:

> *"Blocker A (NEW, now DOMINANT) — the emit prelude is not produced.
> The kernel ALWAYS pins `last_results__len` and decodes
> `last_results` as an `(Array Int Result)`. Bootstrap's `emit.rs`
> hand-writes this prelude — `result_and_last_results_decls()`
> emits the `Result` datatype + `(declare-fun last_results () (Array Int Result))`,
> and... `compiler.ev` doesn't."*

`compiler.ev` can now compile a user claim with kernel-compatible
Seq output (per wave 4c's empirical round-trip), BUT it doesn't
emit the kernel-required infrastructure declarations the kernel
always expects to find.

This task is the focused port of that prelude. Mechanical work;
the blocker doc names exactly which bootstrap function to mirror.

After this lands the smoke test should go green:
`scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello`
→ exit 0. That is the deletion-readiness signal.

## Authorisation

Edit `compiler/*.ev`, `tests/kernel/*.ev` (new fixtures), and
`docs/`. No `bootstrap/`, no `kernel/`, no Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/blocked-grammar-wave4c.md` — the precise blocker
   spec (Blocker A).
3. `docs/plans/grammar-wave4c.md` — what wave 4c landed
   (Seq+max-effects).
4. **`bootstrap/runtime/src/emit.rs`** — find
   `result_and_last_results_decls()` (or whatever it's named —
   the function that emits `Result` datatype + `last_results`
   declarations). Read it carefully; port its exact output.
5. **`kernel/src/tick.rs`** — for what the kernel actually
   expects to find in the body (the `last_results__len` /
   `last_results` reads).
6. `compiler/compiler.ev` — the canonical driver where you'll
   add the prelude emission.
7. `tests/kernel/test_hello.ev` — the smoke-test target.

Cite #2, #4, and #6 in your report.

## Scope

### Item 1: emit the Result-and-last_results prelude

`compiler.ev` already emits the manifest header. After the
manifest, before the user's claim body's declares/asserts,
emit the prelude that bootstrap's
`result_and_last_results_decls()` produces:

- The `Result` datatype declaration (variants: `NoResult`,
  `IntResult(Int)`, `StringResult(String)`, `RealResult(Real)`,
  `EofResult`, `ErrorResult(String)`, plus any others — read
  bootstrap's source for the exact list).
- `(declare-fun last_results () (Array Int Result))`.
- `(declare-fun last_results__len () Int)`.
- Possibly an `is_first_tick` declaration if the user didn't
  declare one explicitly.
- Any other kernel-required pins bootstrap hand-writes.

Look at what bootstrap actually emits for `test_hello.ev` —
diff the output against what compiler.ev produces today to find
the exact missing lines.

### Item 2: smoke test

After item 1:

```bash
scripts/build-compiler-smt2.sh
scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello
```

**This is the deletion-readiness signal.** If exit 0, declare
victory in the report.

If exit non-zero, identify the NEXT blocker precisely (kernel
error message, byte diff, whatever surfaces) and document in
`docs/plans/blocked-grammar-wave4d.md`.

## Acceptance

1. compiler.ev emits the Result datatype + last_results
   declarations as bootstrap does.
2. **Smoke test from item 2 exits 0** OR a precise blocker doc
   identifies the next gap.
3. `./test.sh` is fully green in all 3 functionizer modes.
4. All previous-wave fixtures still pass.
5. Diff scoped to `compiler/*.ev` + new fixtures (if any) +
   `docs/plans/grammar-wave4d.md`.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Adding Python.
- Tackling other items beyond the prelude — if other gaps
  surface, document them and stop.
- Implementing the Seq-as-interface design (post-deletion).

## Known gotchas

- Op/Token/Expr variant names are globally unique.
- Composition leaks callee body-local names — prefix all locals.
- Bootstrap's `match` over composed `MArm(_, b)` silently drops;
  use inline token assembly if you hit that pattern (wave 3.5
  finding).

## Reporting back

- Branch pushed (`agent-35-compiler-grammar-wave4d`).
- Item 1 status.
- **Item 2 smoke test result — the headline.**
- Test count delta (current: 93+).
- Cite docs.

Be terse.
