# Foundation briefing — every subordinate session reads this

You are working on the Evident self-hosting project. **Before doing
anything**, read this entire briefing AND `CLAUDE.md` at the repo
root. Then run `scripts/check-deletable.sh` from the repo root and
read its output. That output is the project's current state; it
tells you what is blocking the project's only goal.

## The project's only goal

Delete `bootstrap/`.

`bootstrap/runtime/` is ~10,500 lines of Rust that currently compile
Evident to SMT-LIB. We are transcribing that compiler into Evident
itself. When the self-hosted compiler exists and is verified, we
delete bootstrap. Then the project is done.

You do not "fix bugs in bootstrap." You do not "refactor bootstrap."
You do not "tidy bootstrap." You read it as a reference, you
transcribe a portion of it into Evident, you verify equivalence
with a conformance test, and you delete the portion that's now
replaced. That sequence is the entire job.

## The architectural shape (memorise this)

```
DELETION TARGET:

  source.ev → [kernel + compiler.smt2] → output.smt2 → [kernel] → exit/stdout

  Nothing else. No Rust beyond kernel/. No Python.
```

The kernel (`kernel/`, ~880 LOC Rust) is the minimal native
runtime: trampoline + libffi + Z3 wrapper. **It only reads
SMT-LIB.** Evident source compiles to a Z3 model, which exports as
SMT-LIB. **The compiler-in-Evident, when compiled to
`compiler.smt2`, IS the compiler.** Self-hosting in this project is
trivial in shape: `kernel + compiler.smt2` reads `source.ev` and
emits `source.smt2`. Then `kernel` runs `source.smt2`. That's the
whole pipeline. Bootstrap has no role in this picture.

## The freeze (effective now)

| Path                 | What you may do                              |
| -------------------- | -------------------------------------------- |
| `bootstrap/`         | Read. Delete (when replacement verified). Nothing else. |
| `kernel/`            | Read. Edits require user proposal + approval. |
| `scripts/*.py`       | Read. Delete (when replaced). No new lines.   |
| `tests/**/*.py`      | Same.                                        |
| `compiler/*.ev`      | Grow. This is where the work lives.          |
| `stdlib/*.ev`        | Grow. Library code only (not the compiler).  |
| `tests/**/*.ev`      | Grow.                                        |
| `tests/conformance/features/` | Grow. Implementation-agnostic feature specs. |
| `scripts/*.sh`       | Grow only when Evident cannot yet express it. Add a `# TODO: rewrite in Evident` header. |

If your diff touches a frozen path, **your work is rejected**,
regardless of whether tests pass. Submit a `docs/plans/` note
describing the block instead.

## How a session contributes

The deletion path is sequential per capability:

1. Pick a capability that bootstrap provides (a lexer token, a
   parser production, a translator pass).
2. Implement it in `compiler/*.ev`.
3. Add a conformance test in `tests/conformance/features/` that
   defines the capability as an input/output spec.
4. Run the test under `IMPL=bootstrap`. It should pass (bootstrap
   already does this).
5. Run the test under `IMPL=selfhost` (uses kernel +
   `compiler.smt2`). If `compiler.smt2` doesn't exist yet, this is
   the part you're building toward; document what was needed.
6. When both pass and produce equivalent output, mark the
   capability covered in `docs/plans/DELETION-CHECKLIST.md`.

If you are unable to complete any of these for your assigned
capability, **do not patch bootstrap to make it work**. Write a
note in `docs/plans/blocked-<your-task>.md` describing the block
and stop.

## What "done" looks like for your session

Your session reports back with:

- What capability you addressed.
- Files added under `compiler/`, `stdlib/`, `tests/`.
- New `tests/conformance/features/<feature>/` directory with the
  spec.
- Output of `scripts/check-deletable.sh` (it likely still exits
  1, but the blocker list may be shorter).
- Any `docs/plans/blocked-*.md` you wrote.

Your session does NOT report back with:

- Improvements to bootstrap.
- New Python anywhere.
- Edits to frozen files.
- A "Phase X complete" claim without check-deletable.sh moving.

## What to read before starting

In this order:

1. `CLAUDE.md` (the project's load-bearing rules).
2. `bootstrap/READ-ME-FIRST.md` (why bootstrap is reference, not
   tool).
3. `STATE.md` (the current `check-deletable.sh` output).
4. `docs/plans/DELETION-CHECKLIST.md` (the list of capabilities
   needed).
5. `tests/conformance/features/README.md` (how to write a feature
   spec).
6. **If your task touches FTI design, the Formula-builder
   architecture, the kernel-effects model, the Z3-as-library
   direction, or any minimal-runtime concept** — read:
   - `legacy-python/README.md` (orientation).
   - `legacy-python/docs/runtime-architecture.md` (the trampoline
     + LibCall + state-pair model).
   - `legacy-python/docs/fti-composition.md` (how FTIs inline into
     the host FSM).
   - `legacy-python/docs/fti-z3.md` + `fti-z3-m6-extensions.md`
     (the Z3-via-libcall design — the most important single idea
     for our self-hosting target; unimplemented in tiny-runtime so
     it exists only here).
   Your report back MUST cite which of these docs justified your
   approach. Sessions that don't cite will be rejected.
7. `docs/notes/python-branch-techniques.md` (the coordinator-level
   summary of what was learned from `tiny-runtime`; read this to
   know what the coordinator already knows).
8. Your task spec (passed in alongside this briefing).

Then run `scripts/check-deletable.sh` and start work.
