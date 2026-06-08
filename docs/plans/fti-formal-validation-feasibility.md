# FTI formal validation — feasibility study

Status: feasibility analysis + worked proof (2026-06-08).
Subject: can FTI ("Foreign Type Interface") memory-safety properties be
**formally validated in-language** — expressed as Evident constraints
and discharged by Z3 — so that the FTI subsystem that keeps breaking
compiler2 can be unit-tested in isolation instead of debugged inline in
the 6277-line `driver_main`?

Worked evidence lives in `tests/fti_proofs/`. Every verdict in this
document was produced by `/usr/local/bin/evident-oracle` (the reference
compiler) and, for the kernel-run proofs, the release kernel binary.

## TL;DR verdict

- **Provable in-language today, cleanly:** bounds safety (no write past
  capacity), region containment (every address inside the malloc'd
  bytes), no-aliasing (distinct slots → distinct addresses), no-underflow
  (no pop below zero), and address arithmetic correctness. These are
  arithmetic predicates over `Int` state, and Z3 (LIA) decides them
  outright. The driver's existing `lx_count < 65534` bound is one
  instance; this study generalizes it and shows the universal form
  (`∀ base,cap,count` the write stays in-region) also discharges.
  **All five obligations discharge** — see §3.
- **Provable as a *modeled discipline*, with an honest gap:**
  no-use-after-free / no-double-free / liveness (the refcount bug). A
  linear/affine ownership discipline CAN be written as constraints and an
  illegal use proven UNSAT *relative to a modeled refcount*. But Z3
  cannot observe the actual Z3-context heap, so the model proves "IF the
  program threads the refcount as specified THEN no use-after-free" — it
  cannot prove the FFI calls in C-land actually maintain it. That last
  mile is the kernel's `inc_ref` policy, not a constraint. See §1.2.
- **Worked proof discharged:** yes. `sample --all` reports the five
  obligations as designed; the kernel forces `UNSAT on tick 0` (exit 2)
  on the overrun and `exit 0` on the in-bounds write. See §3.
- **Recommendation:** build a small standalone validated FTI buffer model
  **now, before finishing compiler2**, using the oracle as the build
  tool. It is ~1–1.5 days; the FTI is the thing that keeps costing days;
  and the model is the spec the driver's inline FTI must match. See §5.

---

## 1. What can be formally expressed

The FTI pattern keeps bulk data in libc-`malloc`'d memory (reached by the
kernel's `__mem.read_long`/`write_long` deref primitive) and carries only
a little `Int` metadata — `base` (the malloc pointer) and `count`/`depth`
(occupancy) — in Z3 state. Memory safety is therefore a property of that
metadata and the address arithmetic `addr = base + slot*8`. The question
is which safety properties are predicates Z3 can decide over that
metadata, and which need a notion of *heap state over time* that Evident's
constraint model does not have.

The formal-methods taxonomy that applies: **bounds/region** properties
are first-order arithmetic (decidable in LIA); **lifetime/ownership**
properties are the province of **separation logic** and **linear/affine
type systems**, which reason about a mutable heap and resource
consumption — neither of which Evident models directly.

### 1.1 Maps cleanly to Evident-as-it-is (decidable arithmetic)

These are all predicates over the `Int` metadata; Z3's linear-integer
arithmetic decides them, and an `unsat_*` claim is a total proof (not a
bounded check).

| Property | Formal statement | Evident encoding | Foundation |
|---|---|---|---|
| **Bounds safety** | `write_slot < capacity` for every reachable write | a membership constraint `write_slot ∈ Int < capacity` in the FTI claim; the violating state has no model | array-bounds / refinement type `{i : 0 ≤ i < n}` |
| **Region containment** | `base ≤ addr < base + capacity*8` | derive `addr = base + slot*8` from `0 ≤ slot < capacity`; prove the negation UNSAT | spatial separation-logic footprint `base ↦ _ * … ` collapsed to an interval |
| **No aliasing** | `i ≠ j ⇒ base+i*8 ≠ base+j*8` | injectivity of `slot ↦ base+slot*8`; prove the alias UNSAT | separation-logic `*` (disjointness of cells) |
| **No underflow** | `count ≥ 0` invariant; a pop sets `count' = count−1` | `0 ≤ count` membership makes a pop from 0 UNSAT | refinement `{n : n ≥ 0}` |
| **Address arithmetic** | the slot→addr map is the one the kernel will deref | compute `addr` in the model exactly as the FTI emits it | — |

The key reason these are *cleanly* expressible: the metadata is `Int`,
the arithmetic is linear, and the property is a one-shot predicate. The
driver already relies on this — `lx_count ∈ Int < 65534`,
`st_cnt ∈ Int < 8192`, `ci_cnt ∈ Int < 2048` are exactly bounds-safety
refinements, and a violating tick goes UNSAT (exit 2) with
`EVIDENT_UNSAT_CORE=1` naming the line. This study's contribution is to
show (a) the *universal* form discharges (not just the concrete sampled
state), and (b) the property can be **isolated into a standalone claim
and unit-tested**, which the inline driver bound cannot be.

### 1.2 Maps only as a *modeled discipline* — the honest gap

No-use-after-free, no-double-free, and liveness are **temporal,
heap-stateful** properties. The formal tools for them are:

- **Linear / affine types** (Wadler; Rust's ownership): a resource is
  used *exactly once* (linear) or *at most once* (affine). "Free consumes
  the handle; a later use is a type error."
- **Separation logic** (Reynolds/O'Hearn): the heap is a partial map of
  addresses to values; `addr ↦ v` is a *consumable* assertion; `free`
  removes it; a subsequent `addr ↦ _` is unprovable.

Evident has **neither**. Its model is a set of constraints over named
variables solved fresh-ish each tick; it has no heap-as-a-value, no
resource that is *consumed*, and `__mem` writes go to an external region
Z3 cannot see. So the honest position is:

**What CAN be expressed:** a *modeled* refcount/ownership discipline. Give
each live handle a refcount `rc ∈ Int` carried in state; declare
`is_use ⇒ rc > 0` and `free ⇒ rc' = 0`. Then "a use on the tick after a
free" is UNSAT *against the model*. This is a genuine proof that the
**discipline is self-consistent** — if you respect the refcount protocol,
you cannot construct a use-after-free state. It is the constraint-language
analogue of an affine type's "use-after-move is rejected."

**What CANNOT be expressed:** that the actual `Z3_ast` in the C heap is
still alive. Z3's GC reclaims an AST whose real refcount hit zero,
regardless of any `rc ∈ Int` the Evident model is tracking. The model's
`rc` and Z3's internal refcount are two different numbers; the constraint
binds the former, the kernel's `inc_ref` policy binds the latter. The
refcount bug (use-after-free at ~142k ASTs) was a divergence between
those two — and **no model-side constraint can close it**, because the
quantity that went wrong (Z3's internal count) is invisible to Z3-the-
solver. This matches the kernel comment in `libcall.rs`: "Refcount
discipline is imperative memory management, not a constraint, so it can't
live in the model cleanly." The git history confirms a model-side attempt
was made and failed; the fix was the kernel `inc_ref` allowlist.

So the verdict on lifetime safety is **partial**: the *discipline* is
expressible and provable self-consistent (useful — it would catch a
logic error where the FTI's own bookkeeping frees-then-uses), but the
*external-heap liveness* is out of reach without a language/kernel notion
of heap ownership. Honest framing for §4's bug-2 assessment.

### 1.3 Address-aliasing across allocations

A subtle case worth calling out: §1.1's no-aliasing is *within one
buffer* (injectivity of one base's slot map). Aliasing *across two
malloc'd regions* (does buffer A's range overlap buffer B's?) is NOT
provable, because the two `base` values come from `libc::malloc` at
runtime — Z3 only knows they are two `Int`s, not that the allocator
returned disjoint regions. The disjointness is an allocator guarantee the
model must *assume* (`base_A + cap_A*8 ≤ base_B ∨ base_B + cap_B*8 ≤
base_A` as an axiom), not prove. This is the same shape as the liveness
gap: facts about the external allocator are inputs to the model, not
theorems of it.

---

## 2. A standalone FTI buffer model sketch

What a separately-testable FTI buffer looks like as an `.ev` file,
respecting the state-scoping constraint proven in
`driver-subsystem-map.md` §3: **carry pairs (`x`/`_x`) must be declared in
the top-level FSM claim** (a composed sub-claim cannot own manifest state)
**; invariants and transition bodies live in pure helper claims** (the
Probe-C pattern).

```evident
-- Pure invariant helper: the safety contract, no carry. Takes the
-- derived state as slots, asserts the memory-safety predicate. This is
-- the unit under test — sat/unsat obligations instantiate it directly
-- (see tests/fti_proofs/buffer_safety.ev), AND the real FSM composes it.
claim FtiInvariant(base ∈ Int, capacity ∈ Int, count ∈ Int,
                   write_slot ∈ Int, write_addr ∈ Int)
    base > 0
    capacity > 0
    0 ≤ count
    count ≤ capacity
    write_slot = count
    write_slot < capacity              -- bounds safety (load-bearing)
    write_addr = base + write_slot * 8 -- region-contained by construction

-- Pure transition helper: next count from the action. No carry; returns
-- the next value through an output slot (Probe-C shape).
claim FtiStep(prev_count ∈ Int, is_push ∈ Bool, is_pop ∈ Bool, next_count ∈ Int)
    is_push ⇒ (next_count = prev_count + 1)
    is_pop  ⇒ (next_count = prev_count - 1)
    ((¬ is_push) ∧ (¬ is_pop)) ⇒ (next_count = prev_count)
    next_count ≥ 0                     -- no underflow

-- The FSM host: carry pairs declared HERE (manifest state), bodies
-- delegated to the helpers. base is threaded two-tick from malloc.
claim main
    count ∈ Int                        -- carry: occupancy
    _count ∈ Int
    base ∈ Int                         -- carry: malloc pointer
    _base ∈ Int
    -- ... action decode, malloc-handle capture, __mem write/read effects,
    --     and a single-writer effects = ⟨…⟩ ++ … schedule ...
    next_count ∈ Int
    FtiStep(prev_count ↦ _count, is_push ↦ pushing, is_pop ↦ popping,
            next_count ↦ next_count)
    count = (is_first_tick ? 0 : next_count)
    waddr ∈ Int
    FtiInvariant(base ↦ base, capacity ↦ 4, count ↦ count,
                 write_slot ↦ count, write_addr ↦ waddr)
```

The crucial property of this shape: `FtiInvariant` and `FtiStep` are
**pure, total functions over their slots**, so they are exactly what a
`sat_`/`unsat_` proof claim instantiates with a chosen state — the unit
test and the production FSM share the identical invariant code. That is
what "separately testable" buys: the property proven in isolation is
*literally the same claim* the driver runs, not a re-implementation that
can drift. The deleted `stdlib/fti/stack.ev` had its `Stack(...)` legal-
transition claim in this shape already; what it lacked was the
accompanying proof claims and the bounds membership.

---

## 3. Proof methodology — demonstrated

The proof vehicle is the project's own `sat_`/`unsat_` convention,
discharged by `evident-oracle sample --all` (a Z3 satisfiability check
per claim) and, for the live form, by emitting to SMT-LIB and running the
release kernel. An `unsat_*` claim reported `false` (UNSAT) is a proof
the violating state is excluded; a paired `sat_*` reported `true` (SAT)
proves the model is not vacuously empty.

### 3.1 Static obligations — `tests/fti_proofs/buffer_safety.ev`

```
$ evident-oracle sample tests/fti_proofs/buffer_safety.ev --all
"FtiInvariant":true
"unsat_write_into_full_buffer":false      ← UNSAT: no write when full ✓
"sat_write_into_nonfull_buffer":true      ← SAT:   in-bounds write reachable ✓
"unsat_write_addr_escapes_region":false   ← UNSAT: ∀ state, addr in-region ✓
"unsat_distinct_slots_alias":false        ← UNSAT: no two slots alias ✓
"unsat_pop_empty_underflows":false        ← UNSAT: no pop below zero ✓
```

All five discharge as designed. The make-or-break one is
`unsat_write_addr_escapes_region`: it leaves `base`, `cap`, `count`
**symbolic** (only the invariant constrains them) and asks Z3 whether ANY
admitted state can land a write outside `[base, base+cap*8)`. UNSAT means
the bounds theorem holds universally — this is a real proof, not a
sampled check of one concrete buffer.

### 3.2 Live obligation — the kernel runs it

To show the property discharges through the **actual kernel** (not just
the oracle's sat-checker), `overrun_unsat.ev` asserts the same bound into
a per-tick FSM query that drives `write_slot = capacity`:

```
$ evident-oracle emit tests/fti_proofs/overrun_unsat.ev main -o overrun.smt2
$ kernel overrun.smt2
kernel: UNSAT on tick 0
EXIT=2
```

The kernel refuses the overrunning tick exactly as it would a real FTI
write past the buffer end — `exit 2`, the project's UNSAT halt code. The
companion `safe_exit.ev` (an in-bounds free slot, `0 ≤ write_slot < 4`)
runs to `exit 0`, confirming the bound rejects only the overrun, not
every write. This is the live analogue of the driver's `lx_count < 65534`
catching the resume-point-12 overrun.

### 3.3 What the methodology does NOT prove

Honesty per §1.2: none of these obligations touch the *external heap*.
They prove the **metadata model** excludes unsafe states. The chain "model
says addr is in-region" ⇒ "the `__mem.write_long` actually lands in the
malloc'd bytes" depends on the kernel dereferencing the same address the
model computed — which it does (`base + slot*8` is passed verbatim as the
`ArgInt`), but that link is an inspection argument, not a Z3 theorem. And
no use-after-free obligation is in this file precisely because §1.2 shows
it can only be proven against a modeled refcount; a `liveness.ev` sketch
is described in §1.2 but its proof would carry the documented caveat.
