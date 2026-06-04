# STATE

_Output of `scripts/check-deletable.sh`._

```
BOOTSTRAP NOT YET DELETABLE.

Blockers:

test.sh still invokes bootstrap. Switch its 'evident' binary path
    to use kernel + compiler.smt2.
bootstrap/ directory still exists (11385 lines of Rust).
    When every blocker above is cleared, run: rm -rf bootstrap/

See CLAUDE.md, section 'The deletion path,' for how to clear these.
```

## Real blocker (the check-deletable.sh script doesn't catch this yet)

Wave 4i (`docs/plans/blocked-bootstrap-cutover.md`) revealed an
architectural gap that gates the cutover:

**The self-hosted toolchain implements only the `emit` verb.** The
bootstrap binary has four (`emit`, `sample`, `sample --all`, `run`).
`scripts/run-lang-tests.sh` drives every `tests/lang_tests/*.ev`
through `evident sample <file> --all --json`. Under
`EVIDENT_SELF_VIA_SMT2=1`, all 11 lang files fail at load:

```
evident-self(smt2): only 'emit' is supported, got 'sample'
```

That's ~190 claim assertions across 11 files — the project's only
language-semantics coverage. Per the freeze rules, dropping the
lang phase or rewriting the fixtures is forbidden; the right move
is to add the missing capability.

Required for the next wave to land cutover:

- `compiler/compiler.ev` must accept a claim-name selector (today
  it emits the *last bare-head* claim — see
  `compiler/compiler.ev:291-296`). Without this, even per-claim
  emit-and-check is infeasible.
- A `sample` execution path. Two candidate shapes (see
  `docs/plans/blocked-bootstrap-cutover.md` for the analysis):
  1. A `sample.ev` driver compiled to `sample.smt2` — iterates a
     `.ev` file's top-level claims, solves each, prints
     `{"name":bool,...}` JSON matching bootstrap's contract.
  2. Per-claim emit + kernel sat-probe (requires both compiler
     claim-selection AND a kernel `--sample` flag, plus proving the
     exit-code proxy is faithful).
- The `run` verb (`emit + exec`) is also unimplemented; this is
  smaller — it's literally what the self-hosted wrapper already
  does for one claim. Only needed if anything outside lang_tests
  uses it.

Then the cutover steps (test.sh rewrite, drop bootstrap branch
from `scripts/evident-self`, `rm -rf bootstrap/`) become mechanical.

## What IS done

- Self-hosted compile of the FULL flattened `test_hello` (1436
  ticks) succeeds and matches bootstrap semantically (wave 4h;
  `--semantic` exit 0 — the first full-corpus self-host smoke pass).
- 100 kernel tests green; test.sh green default mode and under
  `FUNCTIONIZE=0`.
- `compiler.smt2` is built and tracked (226,984 lines).
- The seam (`EVIDENT_SELF_VIA_SMT2=1`) is wired into
  `scripts/evident-self`. Tests/scripts that drive `evident emit`
  are now testable against the self-hosted path.
- All grammar/datatype blockers from waves 4b-4h are closed.

## Where to pick up

Read `docs/plans/blocked-bootstrap-cutover.md`. Pick a path:
add a `sample` driver in Evident (option 1) or extend the kernel
+ compiler.ev for per-claim emit+sat-probe (option 2). Land that
as wave 4j, then re-run wave 4i's probe to verify cutover viable.
