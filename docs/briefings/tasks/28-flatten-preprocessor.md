# Task: Import-flattening preprocessor (`scripts/flatten-evident.sh`)

## Why

Bootstrap resolves imports natively (`bootstrap/runtime/src/runtime/load.rs`).
Once `compiler.smt2` exists, the kernel + `compiler.smt2` will be
the compiler, but `compiler.smt2` will NOT do import resolution
(that's deferred to a future enhancement). To use `compiler.smt2`
on real `.ev` files (which all have `import` lines), we need a
preprocessor that walks the import graph and concatenates
everything into a single flat `.ev` source.

This is the **bridge step** in the deletion path:

```
compiler.smt2 produced (bootstrap, one last time)
  → wave 2/3 done
  → ↓ THIS TASK ↓
  → flatten-evident.sh exists
  → scripts/evident-self bin rewires to "flatten | kernel compiler.smt2"
  → test.sh phases 4/5 no longer touch bootstrap binary
  → bootstrap deletable
```

This task is **independent of all compiler.ev grammar work** —
the flatten script is pure Bash text-munging over `.ev` source. It
can run NOW in parallel with wave 2.

## Authorisation

You may add `scripts/flatten-evident.sh` and a test fixture in
`tests/kernel/`. You may NOT edit `bootstrap/`, `kernel/`,
`compiler/`, `stdlib/`, or `scripts/evident-self` (we keep the
seam stable until the actual cutover).

## Required reading

1. `CLAUDE.md`.
2. `STATE.md`.
3. `docs/plans/DELETION-CHECKLIST.md` Phase 5.
4. A handful of `.ev` files showing the import syntax —
   `compiler/compiler.ev`, `compiler/parser.ev`,
   `tests/kernel/test_fti_stack.ev`. Note that imports are
   line-oriented: `import "path/to/file.ev"`.
5. `bootstrap/runtime/src/parser/program.rs` (just to confirm the
   import-line syntax bootstrap accepts) — you're MATCHING this
   syntax exactly, not making something new.
6. `bootstrap/runtime/src/runtime/load.rs::resolve_import` (lines
   54-76) — for how paths resolve (project-root vs relative;
   handle whichever bootstrap supports).

Cite #5 and #6 in your report.

## What you're producing

### `scripts/flatten-evident.sh`

A Bash script that:

1. Takes one `.ev` file as `$1`.
2. Walks the import graph rooted at that file:
   - For each line matching `^import "(.+)"`, capture the path.
   - Resolve it the same way bootstrap does (most likely: relative
     to repo root). Read the file. Recurse.
3. Topologically sorts the imports (each file appears AFTER its
   dependencies, ONCE).
4. Concatenates everything in toposort order to stdout, with the
   `import` lines stripped (or commented out — pick one).
5. Detects cycles: if the import graph has a cycle, exit non-zero
   with a clear error.
6. `# TODO: rewrite in Evident` header per the freeze rules.

### Test fixture

Add `tests/kernel/test_flatten_compiler.ev` OR a Bash test
(`tests/conformance/features/0NN-flatten/`) that:

1. Runs `scripts/flatten-evident.sh compiler/compiler.ev` and
   captures the output.
2. Verifies the output:
   - Contains content from every imported file.
   - Has no `import` lines.
   - Each imported file's content appears exactly once.
   - Order respects deps (e.g., `compiler/parser.ev` content
     appears after `compiler/lexer.ev` content, since parser
     imports lexer).

If you go the conformance-feature route, follow
`tests/conformance/features/README.md`'s shape.

### Optional smoke test

Compile the flattened output via bootstrap and verify it's
equivalent to compiling the original via bootstrap:

```bash
bootstrap/runtime/target/release/evident emit compiler/compiler.ev main -o /tmp/orig.smt2
scripts/flatten-evident.sh compiler/compiler.ev > /tmp/flat.ev
bootstrap/runtime/target/release/evident emit /tmp/flat.ev main -o /tmp/flat.smt2
diff /tmp/orig.smt2 /tmp/flat.smt2   # should be empty or whitespace-only
```

Include the diff result in your report.

## Acceptance

1. `scripts/flatten-evident.sh` exists, executable, with `# TODO:
   rewrite in Evident` header.
2. Test fixture proves it on `compiler/compiler.ev` (the most
   complex real import graph in the project).
3. Smoke test: bootstrap-compiling the flattened output produces
   equivalent SMT-LIB to bootstrap-compiling the original (modulo
   whitespace / line number metadata).
4. `./test.sh` is fully green.
5. `scripts/check-deletable.sh` blocker list unchanged (this is
   infrastructure for the cutover, not the cutover itself).

## Forbidden

- Editing `bootstrap/`, `kernel/`, `compiler/`, `stdlib/`.
- Editing `scripts/evident-self` (the cutover is a separate task).
- Adding Python.
- Implementing real import resolution in Evident (defer).
- Handling features bootstrap doesn't (e.g., conditional imports,
  glob imports — these don't exist in Evident).

## Reporting back

- Branch pushed.
- The smoke-test diff output (should be ~empty).
- Number of lines in the flattened `compiler/compiler.ev` vs
  original.
- `./test.sh` final line.
- Cite docs.

Be terse.
