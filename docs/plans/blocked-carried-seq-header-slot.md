# Blocked: a carried bounded Seq cannot be a claim-header slot

**Status:** BLOCKED (frozen-pipeline limitation, measured 2026-06-10).
Discovered while de-prefixing DriverPosBind and DriverCompose to bare
mention + claim headers (de-prefix batch 2).

## The rule discovered

Two facts about bare-mention composition under the frozen pipeline:

1. **A bare-mention-hidden internal cannot carry.** A body membership of
   a bare-mentioned child that references its own `_x` carry dual is
   *silently dropped pre-oracle* — the `_x` is undeclared, the covering
   `=` "can't be expressed as a Z3 Bool," and the constraint vanishes (or,
   in the driver, the kernel functionizer refuses tick-0 eval and the
   whole fast path collapses → compile timeout). Therefore **carried state
   must be header interface, not hidden internal.** (This is the working
   pattern used for DriverEmit's `unit_ptr`/`rendered` and DriverPratt's
   `expr_stack`/`op_stack`/… — header the carried internals.)

2. **A *bounded Seq* — carried OR pure — cannot be a header slot.**
   Putting `xs ∈ Seq(T)` (with `#xs ≤ N`) in a claim/fsm header makes
   `scripts/passes/lower-bounded-seq.sh` refuse:

   ```
   lower-bounded-seq: unsupported use of bounded Seq `xs` survives lowering:
       claim Child(out ∈ Int, xs ∈ Seq(T), …)
   ```

   The bounded-Seq lowering does not rewrite fsm/claim **headers** — it
   only lowers Seq element-writes/reads that appear as top-level
   memberships written where declared. A header-slot Seq is past what the
   pass handles. This holds whether the Seq is *carried* (autocarry also
   appends an `_xs` dual to the header, which likewise fails) or *pure
   per-tick interface* (measured both: identical refusal). So putting a
   bounded Seq in the interface is impossible today, which is what
   collides with rule (1): the carried registry needs to be interface
   (rule 1) but cannot be (rule 2).

## Isolated reproduction

`fsm Child(out ∈ Int, reg ∈ Seq(PbStr), cnt ∈ Int)` with
`#reg ≤ 4`, `∀ k ∈ {0..3} : reg[k].s = (… _reg[k].s)`, `cnt = (… _cnt+1)`,
bare-mentioned from `main` → `lower-bounded-seq: unsupported use of
bounded Seq reg survives lowering`. The scalar carry (`cnt`) headers fine;
only the Seq blocks.

## What it blocks

Two components in de-prefix batch 2 own carried bounded-Seq registries and
therefore could **not** convert to bare mention:

- **DriverPosBind** — `param_names ∈ Seq(PbStr)` (#≤6),
  `param_types ∈ Seq(PbStr)` (#≤4), `bindzip_binds ∈ Seq(Bind)` (#≤6).
- **DriverCompose** — the bind tape: `binds ∈ Seq(Bind)` (#≤12),
  `slot_names ∈ Seq(CSlot)` (#≤6), `bind_stage ∈ Seq(Bind)` (#≤6),
  plus `slot_handles`/`type_pin_suffixes`.

Both stay on `..DriverPosBind` / `..DriverCompose` (whole-body implicit
interface) until this lands. The other four batch-2 components
(DriverSymLookup, DriverEmit, DriverPratt, DriverClassify) converted —
none owns a carried bounded Seq.

## Why the obvious workaround is wrong

The DriverWindow precedent (batch 1) *moved* its carried bounded Seq
(`lat_tags`/`lat_pays`) out of the component into driver_main as a
top-level membership written where declared. Doing that for PosBind/Compose
would move the registries' **allocation + keyed-update writes** — the
component's core logic, not glue — into the driver entry point, splitting
each registry from its writers (purism §6.4 "registries sit with their
writers"; a split-brain). That trades a hand-prefix smell for a worse one.

## The fix (lowering, not source — purism §1.5)

`scripts/passes/lower-bounded-seq.sh` should recognize a bounded-Seq
header slot (and its autocarry `_xs` dual) and lower the element
forms through the seam the same way it does for a top-level membership.
Equivalently, the seam/compose layer could thread a carried bounded-Seq
header slot as an Array+len pair the lowering already understands. Either
keeps the surface (`xs ∈ Seq(T)` in the header) and fixes the transform.

Until then: carried bounded-Seq registries keep `..`-lift; this is a
named gap, not a source defect.
