# legacy-python/ — read-only reference from the tiny-runtime branch

These markdown files are design documents copied from the
`tiny-runtime` branch (tip `418eaf5`, 2026-06-01). They are the
load-bearing ideas behind FTI, the minimal-runtime architecture, and
the unimplemented Z3-via-libcall mechanism.

## Why these are here

The coordinator session that explored `tiny-runtime` (report at
`docs/notes/python-branch-techniques.md`) determined that the Python
source code is reproducible from short excerpts and does not need to
live in this repo, BUT the design docs contain the genuinely novel
idea — *"Z3 is a library reached through FFI, not a runtime"* — that
exists nowhere else. Without these docs, that idea is lost.

A coordinator guarantee accompanies these files: every subordinate
session whose task touches FTI design, the Formula-builder
architecture, the kernel-effects model, or the minimal-runtime
direction MUST cite which doc here justified the approach. Sessions
that don't cite are rejected on review.

## The four docs and what they cover

| File                            | Topic                                                                                                   |
| ------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `runtime-architecture.md`       | The trampoline + LibCall + state-pair model. Synchronous semantics, no intra-tick iteration, composition over scheduling. The "minimal runtime is trampoline + libcall" framing. |
| `fti-composition.md`            | How FTIs (Foreign Type Interfaces) get inlined at compile time into the host FSM. Namespaced sub-fields, namespaced `*_effects` channels, the legal-transition disjunction pattern. |
| `fti-z3.md`                     | The Z3-via-libcall FTI: build Z3 ASTs by FFI calls to libz3, so the SMT-LIB body the kernel solves stays tiny. The Formula datatype + the `materialize` recursion. **Unimplemented in tiny-runtime; the most important single idea for our self-hosting target.** |
| `fti-z3-m6-extensions.md`       | Extensions to the Z3 FTI design — `ArgRef` + tick-local scratchpad, `@push`/`@pop` RPN evaluation, `define-fun-rec` for the materializer.                                          |
| `evident-language-spec.md`      | The language grammar from `tiny-runtime`. Smaller language than current Evident (no enums, no generics, no multi-FSM scheduler). Reference for the grammar shape, not the current spec.                |

## How these relate to the current repo

The current `kernel/` (Rust) implements the trampoline + LibCall +
state-pair model from `runtime-architecture.md`, with one effects
channel and no `__mem__` or Z3-via-libcall. The shipped FTI design
from `fti-composition.md` assumes:

- **Multiple namespaced `*_effects` channels** — current kernel has
  one.
- **The `__mem__` synthetic library** — current kernel doesn't have
  this.

So `prelude/stack.ev` and `prelude/queue.ev` from tiny-runtime are
NOT directly transcribable into `compiler/` without kernel
additions. That tradeoff is the subject of a kernel-extension
proposal in `docs/plans/kernel-extension-effects-and-mem.md`.

The Z3-via-libcall FTI (`fti-z3.md` + `fti-z3-m6-extensions.md`) is
the path our self-hosting target should aim toward: it makes the
compiler a Formula-builder rather than an SMT-LIB-string emitter,
which fits the "minimal kernel" goal more cleanly. It also requires
the `ArgRef` + scratchpad addition to the kernel.

## What is NOT here

- No Python source from `tiny-runtime` (per the freeze; we are
  removing Python from this project, not adding it).
- No tiny-runtime test fixtures or examples.
- Nothing copied that exists in current form in this repo's
  `bootstrap/runtime/src/` (no duplication of reference material).

## Related reference: `legacy-rust/functionizer/`

The Z3 **macro-finder functionizer** (the post-load optimizer that
turns determined constraint bodies into callable functions) is Rust,
not Python, so it lives in a sibling tree: `legacy-rust/functionizer/`,
extracted from branch `feat/compile-constraints-to-programs`. See
`legacy-rust/README.md` and `docs/plans/functionizer-integration.md`.
Same read-only freeze applies. The symbolic-regression and LLM
functionizer variants were deliberately NOT extracted.

## Freeze status

`legacy-python/` is **read-only reference**. Same freeze rules apply
as `bootstrap/`: do not edit, do not bug-fix, do not extend. When
all referenced ideas have been transcribed into `compiler/*.ev` (or
explicitly rejected), the directory may be removed.
