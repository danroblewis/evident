# AP-005: no-ignore-in-rust-tests

**Status:** active

**Pattern.** A `#[test]` in `runtime/tests/**.rs` is annotated with
`#[ignore]`.

**Why.** Same family as AP-004. `#[ignore]` skips a test by
default; nobody runs `cargo test -- --ignored` regularly. The
test rots; if it ever gets re-enabled it usually fails for
unrelated reasons because the surrounding code has moved.

**Fix.** Either fix the test, delete it, or — if it's testing a
genuinely-broken runtime feature — file the gap in
`examples/COUNTEREXAMPLES.md` and delete the test.

**Detection.** grep

**Pattern (grep).** `#\[ignore\]` in `runtime/tests/**.rs`. Also
`#\[ignore\s*=\s*"..."\]` (the variant with a reason string —
which is the *least* harmful form but still doesn't run).

**Scope.**
  - Apply to: `runtime/tests/**/*.rs`.
  - Do NOT apply to test functions inside `runtime/src/` `#[cfg(test)]`
    modules (those are unit tests next to the code, follow the
    same rule but the path scope here is the integration tests
    where most ignores have appeared historically). Future
    extension: same rule for `runtime/src/**` `#[cfg(test)]`
    blocks.

**Exceptions.** None.

**Examples.**
  - No known active offender today; rule is preventive.
