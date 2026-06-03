# Blocked: wave-4 item 3 — slot-binding composition `Claim(slot ↦ value)`

Status: **deferred, not attempted in code.** Items 1+2 (constructor
application + nested Seq literals) landed; item 3 is blocked on
infrastructure the task spec explicitly forbids building if it balloons
("Implementing things item-3 needs (real subschema inline, per-tick scope
resolution) if they balloon the task"). It does balloon. This note records
why and what would unblock it.

## The shape

From `tests/kernel/test_fti_stack.ev`:

```evident
Stack(depth ↦ depth, prev_depth ↦ _depth, is_init ↦ is_first_tick,
      pushing ↦ pushing, popping ↦ popping)
```

This is composition mechanism #5 (CLAUDE.md): a bare body-item that calls the
`Stack` claim, binding each of its first-line params to a value in the
caller's scope.

## Why it's not a small extension of items 1+2

Items 1+2 are a pure **expression renderer**: tokens in, an SMT-LIB string
out, no knowledge of any other claim. A `Claim(slot ↦ value)` body item is
fundamentally different — it is an **inline of another claim's body**, which
the bootstrap translator performs by:

1. **Resolving the callee.** `Stack` is a `claim` defined elsewhere in the
   program. Inlining it requires the compiler to hold a **registry of all
   top-level claims** (name → its body-item list). `compiler.ev` today
   parses exactly **one** top-level item and builds no such registry
   (wave 3.5's documented single-item restriction).

2. **α-renaming the callee's body locals** so they don't collide with the
   caller's (the exact hazard in memory
   `[[project_claim_composition_leaks_body_locals]]`).

3. **Substituting the slot bindings** — every reference to a callee param
   (`depth`, `prev_depth`, …) is rewritten to the caller's bound expression
   (`depth`, `_depth`, …), then the substituted body is translated and
   emitted into the caller's constraint set.

Steps 1 and 3 are precisely the "real subschema inline / per-tick scope
resolution" the spec lists as forbidden-if-ballooning, and step 1 also
requires **multi-top-level-item parsing**, which is independently out of
scope (wave 4b/5; it is also the dominant blocker the
`grammar-wave4.md` smoke test surfaced).

## The discriminator alone is cheap; the body is not

Recognising a slot-bind site is easy (head token is `Ident`, second token is
`LParen` instead of `∈`, then `id ↦ expr , …` pairs up to `)`). A
`SlotBindStep` could parse the `↦` pairs into `(slot, rendered-expr)` tuples
using the wave-4 `RenderExprToks` for each value. But there is **nothing to
emit** without the callee's body: a slot-bind site contributes the callee's
*constraints*, instantiated — not a single `(declare-fun …)` /
`(assert …)` line. Emitting just the parsed bindings would be a no-op that
silently drops the FTI's semantics (the MArm-style silent-drop failure mode
from wave 3.5, in a new guise).

## What would unblock it (the dependency chain)

1. **Multi-top-level-item parsing** in `compiler.ev` (parse N top-level
   `enum`/`claim`/`type` items, not one). This is the wave-4b prerequisite
   and is also required to make the `test_hello` smoke test meaningful.
2. **A claim registry** built during that parse (name → `BodyItemList`).
3. **A body-inline pass**: α-rename + slot-substitute + translate the
   callee's body. The substitution needs the string-decomposition /
   name-rewrite primitive that the generics/desugar self-host sessions
   repeatedly flagged (memory `[[project_string_ops_landed]]` added the
   substr/replace ops that make this tractable, but it has not been wired
   into a compiler-driver inline pass yet).

Once (1) and (2) exist, item 3 becomes a focused pass; until then it cannot
be expressed faithfully, and a partial attempt would regress the
single-writer / silent-drop invariants. Deferred to wave 4b/5, after
multi-item parsing lands.

## Wave-4b update

Dependency (1) — **multi-top-level-item parsing — now exists**:
`compiler/compiler.ev`'s phase-2 `pmode` loop (see
`docs/plans/grammar-wave4b.md`, item 1) walks a sequence of top-level
items, so dependency (2) — **a claim registry** — is now tractable: collect
each claim's `(name → BodyItemList)` during the DISPATCH pass. That clears
the two prerequisites this note called out.

What remains for item 3 is still dependency (3), the body-inline pass:
α-rename the callee's body-locals (the
`[[project_claim_composition_leaks_body_locals]]` hazard) and substitute
each slot binding into the callee's translated body. That needs the
name-rewrite primitive (`[[project_string_ops_landed]]` added the substr/
replace ops, but they are not yet wired into a compiler-driver inline
pass), plus the robust nested-parse work the wave-4b enum/claim machines
still lack (compound field types, first-line params — see
`docs/plans/blocked-grammar-wave4b.md` blockers 2–3). Slot-bind stays
deferred to a focused later session; wave 4b did not attempt it in code.
