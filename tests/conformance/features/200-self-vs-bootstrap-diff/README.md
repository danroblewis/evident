# 200-self-vs-bootstrap-diff — the cutover equivalence gate

This feature is the runner-integrated front for the **self-hosted vs
bootstrap** equivalence check. Its job is to give the conformance runner
two honest signals about the cutover state:

- **`IMPL=bootstrap`** (what `./test.sh` runs today): compile `source.ev`
  with the bootstrap runtime and assert the manifest + `Exit` translation
  in `expected/smt2-contains`. This passes today — it pins the reference
  output the self-hosted compiler must eventually reproduce.
- **`IMPL=selfhost` / `IMPL=both`**: the runner compiles via
  `kernel + compiler.smt2`. While `compiler.smt2` does not exist (the
  committed state — wave 3 has not landed), the runner reports this
  feature **BLOCKED**, not failed. That is the correct pre-cutover state.

## The authoritative byte-diff: `check.sh`

`source.ev` only pins *substrings* (what the standard runner supports).
The real per-source equivalence proof is a full byte-diff of the two
emitted `.smt2` files, which lives in `scripts/diff-vs-bootstrap.sh`.
`check.sh` runs it over the fixtures in `fixtures/`:

```sh
tests/conformance/features/200-self-vs-bootstrap-diff/check.sh
```

`check.sh` is **not** auto-run by `runner.sh` / `test.sh` (the runner only
globs `source.ev`), so it never affects the suite's exit code. Run it by
hand, or wire it into the cutover gate once `compiler.smt2` exists.

### Today's expected result

`diff-vs-bootstrap.sh` **SKIPs** (exit 0) until `compiler.smt2` is built,
so `check.sh` is green-by-skip today. Once `compiler.smt2` exists it will
report **DIFFER** for these fixtures until `compiler/compiler.ev` emits a
*complete* kernel program — the bootstrap kernel-emit path produces the
full `Effect`/`Result` datatype preamble, `last_results`, the `effects`
array, and `_<name>` state-carry, none of which the wave-1+2 grammar yet
produces. A clean `check.sh` is therefore a precise statement of "the
self-hosted compiler is feature-complete for this source" — i.e. exactly
the condition that lets `bootstrap/` be deleted (CLAUDE.md, "The deletion
path").

The `fixtures/` are minimal kernel programs (a membership + `effects`) so
the bootstrap leg always emits; they deliberately stay inside the grammar
`compiler/compiler.ev` is growing toward.
