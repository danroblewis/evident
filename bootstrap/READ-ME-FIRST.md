# bootstrap/ — read this first

**This directory exists to be deleted.**

Everything under `bootstrap/runtime/` is ~10,500 lines of Rust that
implement the Evident compiler. It is **reference material** for the
self-hosted Evident compiler we are building in `compiler/*.ev`.

When the self-hosted compiler is feature-complete and verified
equivalent on the conformance corpus, this entire directory is
deleted in a single commit. That deletion is the deliverable of
this project.

## Rules

- **Do not edit any file here.** Not Rust, not TOML, not READMEs.
  Not even comments. Not even "small cleanups." Not even bug fixes
  — a bug here is a signal to accelerate the replacement in
  `compiler/`, not a signal to patch.
- **You may read files here freely.** Understanding the grammar /
  translator / lexer is the entire point of keeping this around.
  Read it; transcribe it into Evident; verify equivalence; delete it.
- **You may delete files here** once their replacement in
  `compiler/*.ev` has been verified by a conformance test that
  passes under both `IMPL=bootstrap` and `IMPL=selfhost` (see
  `tests/conformance/features/README.md`).

## How to know if it's time to delete

Run from the repo root:

```
scripts/check-deletable.sh
```

If it exits 0 with "BOOTSTRAP DELETABLE NOW," you have permission
to `rm -rf bootstrap/` in your next commit.

If it exits 1, the output tells you exactly what's blocking
deletion. Go work on those blockers — in `compiler/`, in
`stdlib/`, in `tests/conformance/features/`. Not here.

## Why the existing freeze isn't soft

In earlier sessions this directory was framed as "the bootstrap
compiler, frozen, modifications exceptional." That framing was
read as "we still depend on it, edits are sometimes OK," which is
wrong. The project's only goal is `rm -rf bootstrap/`. Treating
the directory as a tool we depend on indefinitely is the failure
mode that put us here. So: **read, transcribe, delete. Nothing
else.**

See `../CLAUDE.md` for the architectural picture (the kernel runs
SMT-LIB; the self-hosted compiler is just an `.smt2` file the
kernel runs) that makes the deletion path obvious.
