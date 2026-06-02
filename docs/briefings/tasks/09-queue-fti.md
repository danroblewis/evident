# Task: Implement `stdlib/fti/queue.ev` — Queue FTI (FIFO)

## Why

Companion to `stdlib/fti/stack.ev` (just landed via task #05).
Stack is LIFO; Queue is FIFO. Both will be used by compiler passes
(translator work-stacks, parser worklists). Pattern is the same —
just `tail(_contents)` instead of `init(_contents)` for the
"remove" transition.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/architecture-invariants.md` (FTI rules, especially
   the single-channel + ternary-with-literal pattern that Stack
   used).
3. `stdlib/fti/stack.ev` (the working pattern you're cloning).
4. `tests/kernel/test_fti_stack.ev` (the test pattern).
5. `legacy-python/docs/fti-composition.md` §"Queue."

Cite #3 and #5 in your report.

## What you're building

`stdlib/fti/queue.ev`: a Queue FTI structurally identical to Stack
but with FIFO semantics. Backing memory via `libc::malloc` etc.,
same as Stack. The legal-transition disjunction is:

- `contents = _contents` (no-op)
- `contents = _contents ++ ⟨x⟩` (enqueue)
- `contents = tail(_contents)` (dequeue — drop head)

Plus `tests/kernel/test_fti_queue.ev` — same shape as the Stack
test fixture but uses FIFO: enqueue 3 values, dequeue 2; print
`len(contents)` evolving 0 → 1 → 2 → 3 → 2 → 1.

## Acceptance

1. `stdlib/fti/queue.ev` exists.
2. `tests/kernel/test_fti_queue.ev` exists with `-- expect:` lines.
3. `./test.sh` green (current: 62 kernel tests; new total: 63).
4. The diff touches only `stdlib/fti/queue.ev` and
   `tests/kernel/test_fti_queue.ev`. No frozen paths.

## Forbidden

- Editing `kernel/`, `bootstrap/`, `compiler/`, anything in
  `stdlib/` other than the new `queue.ev`.
- Multi-channel `*_effects` (use the single-channel + ternary
  pattern from `stack.ev`).
- `__mem__`-style synthetic libraries.
- Calling `.simplify()`.
- Adding new Python.

## Reporting back

- Branch pushed.
- One line: did the Queue FTI work, yes/no?
- Files added.
- Test output (the 0→1→2→3→2→1 sequence).
- `./test.sh` final line.
- Cite docs.

Be terse.
