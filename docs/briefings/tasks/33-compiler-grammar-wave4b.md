# Task: compiler.ev grammar — wave 4b (multi-top-level + L3 nesting + slot-bind + semantic diff)

## Why

Wave 4 (task #32) landed constructor application + nested Seq
literals. The smoke test (`scripts/diff-vs-bootstrap.sh`) DIFFERS
from bootstrap, with three named causes (per wave-4 report):

1. **Multi-top-level-item dispatch** — `flatten-evident.sh` inlines
   stdlib/kernel.ev's enum decls BEFORE the user's claim. `compiler.ev`
   handles ONE top-level item; it doesn't walk a sequence.
2. **Seq encoding** — bootstrap emits `(Array Int Effect) + __len`,
   self-hosted emits `seq.++`. Different bytes, same semantics.
3. **`max-effects` derivation** — bootstrap auto-derives 16; self-hosted
   hardcodes 0.

Plus wave-4 explicitly deferred:

4. **Slot-bind composition** (`Claim(slot ↦ value)`) — see
   `docs/plans/blocked-grammar-wave4-slot-bind.md`.
5. **L3+ constructor nesting** — current renderer is L2-only.

This task closes those gaps and arrives at deletion-readiness.

## Coordinator decision: semantic equivalence, not byte equivalence

The `diff-vs-bootstrap.sh` test today does a byte diff of two
`.smt2` files. That's the wrong test. We don't care if the
SMT-LIB is byte-identical; we care if **running it on the kernel
produces the same observable output**. The kernel can run either
`seq.++` or `(Array Int Effect) + __len`; both are valid SMT-LIB.

So a key part of this wave is extending `diff-vs-bootstrap.sh` to
compare KERNEL OUTPUTS (stdout + exit code) instead of `.smt2`
bytes. This change alone resolves gap #2 (Seq encoding) and gap
#3 (max-effects) at the test layer — they're semantically
equivalent.

## Authorisation

Edit `compiler/*.ev`, `tests/kernel/*.ev` (new fixtures),
`scripts/diff-vs-bootstrap.sh`, and `docs/`. No `bootstrap/`, no
`kernel/`, no Python.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/grammar-wave4.md` — what landed and what's left.
3. `docs/plans/blocked-grammar-wave4-slot-bind.md` — the slot-bind
   blocker doc.
4. `scripts/diff-vs-bootstrap.sh` — the harness you'll extend.
5. `compiler/compiler.ev` — the canonical driver.
6. `tests/kernel/test_hello.ev` — the smallest real kernel program;
   the smoke-test target.
7. `tests/kernel/test_fti_stack.ev` — uses slot-bind composition;
   the canonical example.

Cite #2, #3, and #4 in your report.

## Scope (in priority order)

### Item 1: Multi-top-level-item dispatch (DOMINANT)

Today `compiler/compiler.ev` reads one top-level form. After
flatten, the input is a sequence like:

```
-- import "..."          (comment, skipped)
enum Effect = ...
enum LibArg = ...
type ... 
claim main
    ...
```

The driver needs to walk a list of top-level items and dispatch
each via head-token. Wave 3.5 added head-token dispatch for the
`enum` vs `claim` case at the SINGLE-item level; extend it to a
LOOP over all top-level items.

Concretely:
- Outer FSM scans top-level forms in source order.
- Each form is one of: enum decl, claim decl, type decl, comment
  line (`--`), or blank.
- Emit the per-form SMT-LIB in source order: enums first
  (datatype decls), then claim body's declares/asserts.

Test fixture: `tests/kernel/test_compiler_driver_multi_toplevel.ev`
— compile a source with 2 enum decls followed by a claim,
verify the .smt2 has both `(declare-datatypes …)` blocks before
the claim's `(declare-fun …)`.

### Item 2: Semantic diff harness

Extend `scripts/diff-vs-bootstrap.sh` to support a
`--semantic` flag (default off for backwards compat). When set:

1. Compile via both paths → `/tmp/orig.smt2` and `/tmp/self.smt2`.
2. Run BOTH through the kernel: `kernel /tmp/orig.smt2` → capture
   stdout + exit; `kernel /tmp/self.smt2` → capture stdout + exit.
3. Compare the two CAPTURES, not the .smt2 bytes.
4. Exit 0 if stdouts match AND exit codes match.

The original byte-mode behavior stays as default (or switch the
default once item 1 lands and the byte mode is no longer
meaningful — pick one and document).

### Item 3: L3+ constructor nesting

Wave 4's renderer is L2-only (`Exit(0)`, `LibCall("lib","fn",⟨⟩)`).
For `tests/kernel/test_hello.ev` you need L3:

```evident
LibCall("libc", "puts", ⟨ArgStr("hello")⟩)
                       └─────┴ L3: ctor inside Seq inside ctor's payload
```

Options the wave-4 session noted:
- Extend depth-unrolled renderer to L3 (constraint count grows
  ~6×/level, so this is the last manual unrolling).
- Switch to a token work-stack walker (handles arbitrary depth;
  proper structural pattern).

Recommendation: **token work-stack walker.** The cons-list/Seq
work-stack pattern is already documented in
`docs/plans/architecture-invariants.md` and used by
`translate_arith.ev`'s recursive walker. The depth-unrolled
approach gets unwieldy past L3.

Test fixture: `tests/kernel/test_compiler_driver_ctor_l3.ev`.

### Item 4: Slot-bind composition (`Claim(slot ↦ value)`)

Per `docs/plans/blocked-grammar-wave4-slot-bind.md`, this needs:
- A claim registry (the compiler must know what slots `Stack`
  declares).
- Body-inline-with-substitution (when a host writes
  `Stack(depth ↦ d, …)`, the compiler inlines Stack's body with
  parameter substitution).
- Multi-top-level-item parsing (so the `Stack` declaration is
  available in the registry when we encounter its call site).

**Item 1's multi-top-level-item dispatch makes the registry
trivial** (just collect claim decls during the outer walk). So
this item is much more tractable after item 1.

If it still balloons even after item 1, document remaining
blockers and stop after items 1+2+3.

Test fixture (if item 4 lands): use `tests/kernel/test_fti_stack.ev`'s
shape — a host that calls `Stack(depth ↦ depth, ...)`.

### Item 5: Smoke test signal

After items 1-3 (item 4 is stretch), run:

```bash
scripts/build-compiler-smt2.sh
scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello
```

**This should now exit 0.** If it does, we are DELETION-READY.

If it doesn't, identify the remaining gap precisely (kernel error
message, byte diff, whatever surfaces) and document in
`docs/plans/blocked-grammar-wave4b.md`.

## Acceptance

1. Items 1, 2, 3 landed.
2. Item 4 either landed (with test fixture) or has a written
   blocker doc explaining what's still needed.
3. **`scripts/diff-vs-bootstrap.sh --semantic tests/kernel/test_hello.ev hello`
   exits 0** — this is the deletion-readiness signal.
4. `./test.sh` is fully green in all 3 functionizer modes.
5. All previous-wave fixtures still pass byte-identical.
6. Diff scoped to `compiler/*.ev` + new tests +
   `scripts/diff-vs-bootstrap.sh` + new
   `docs/plans/grammar-wave4b.md`.

## Forbidden

- Editing `bootstrap/`, `kernel/`, `stdlib/`.
- Adding Python.
- Tackling wave 5 (quantifiers, generics, records, subclaims).
- Implementing import RESOLUTION (flatten already handles that).

## Known gotchas

- Op/Token/Expr variant names are globally unique.
- Composition leaks callee body-local names — prefix all locals.
- Bootstrap's `match` over composed `MArm(_, b)` silently drops
  the constraint (wave 3.5 finding). Use inline token assembly
  if you hit it.

## Reporting back

- Branch pushed (`agent-33-compiler-grammar-wave4b`).
- Per-item status (1-5).
- **Smoke test #5 result** — this is the headline.
- Test count delta (current: 91).
- Cite docs.

Be terse.
