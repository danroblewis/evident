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

---

## 4. Would it have caught our two real bugs?

Both bugs cost ~a day each, debugged inline in `driver_main`. Honest
assessment of whether the proposed standalone formal validation would
have caught each *before* it cost that day.

### Bug 1 — buffer overrun (resume point 12) → **YES**

The lexer/symtab/claim-index FTIs were sized for conformance fixtures
(4096 tokens, 1024 symbols, 256 claims). `sample.ev` (6283 lines) lexes
to tens of thousands of tokens and the writes ran off the buffer end,
corrupting the heap / segfaulting. The fix was the bounds membership
(`lx_count ∈ Int < 65534`, etc.) — which is *exactly* PROOF 1/PROOF 3 in
`buffer_safety.ev`.

Would standalone validation have caught it? **Yes, with high confidence.**
A standalone FTI buffer model with a `capacity` slot and the
`write_slot < capacity` invariant, exercised by a proof that drives
`count` up to and past `capacity`, makes the overrun UNSAT — and the
*absence* of such a bound is visible the moment you write the proof: you
cannot state `unsat_write_into_full_buffer` without naming `capacity`,
and naming it forces the question "is the buffer big enough for the real
workload?" The bug was fundamentally "a bound that wasn't in the model."
A discipline that requires every FTI to ship its bounds-safety proof
makes that omission a missing-test failure, not a production segfault.
Caveat: the proof proves *a* capacity is respected; choosing the *right*
capacity for `sample.ev` is a sizing decision the proof surfaces but does
not make for you. Still — it converts a silent heap corruption into a
loud "you have no capacity bound," which is the day-saving move.

### Bug 2 — Z3 AST use-after-free (missing inc_ref) → **PARTIAL / mostly NO**

A `Z3_ast` built via the C API starts at refcount 0 and Z3 GCs it under
memory pressure unless `inc_ref`'d. The driver builds ~142k ASTs carried
as `Int` handles; a long-lived one got reclaimed mid-build and Z3
segfaulted. The fix was a **kernel** policy (`inc_ref` every AST-returning
`libz3` builder); the git log records that **a model-side attempt failed.**

Would standalone validation have caught it? **Partial, leaning no**, and
§1.2 is why. The thing that went wrong is Z3's *internal* refcount — a
number invisible to Z3-the-solver. A modeled `rc ∈ Int` ownership
discipline (the `Liveness` claim sketch) can prove "a use after a modeled
free is UNSAT," but the bug was not a violation of a modeled discipline —
it was the *absence* of any refcount tracking at all, in a layer (the C
heap) the model cannot observe. Writing a liveness proof would have
**raised the question** "who owns these AST handles and when do they
die?" — which is real value, and might have prompted the `inc_ref` policy
earlier. But the proof itself cannot discharge against the external heap,
so it would not have *mechanically* caught the bug the way bug 1's bound
does. Honest verdict: formal validation would have improved the odds of
*noticing the lifetime question* during design, but would NOT have given
a red/green proof signal on the actual fault. This is the boundary of
what a constraint model over external memory can do.

### Summary

| Bug | Caught? | Why |
|---|---|---|
| Buffer overrun | **Yes** | Bounds safety is decidable arithmetic over `Int` metadata; the proof can't be written without naming the missing capacity bound. |
| AST use-after-free | **Partial / no** | External-heap liveness is invisible to Z3; a modeled refcount proves discipline self-consistency, not actual-heap safety. The fix was necessarily a kernel policy. |

One-for-two on mechanical catch — but the one it catches is the *class*
of bug (bounds/sizing) that recurs every time the FTI meets a bigger
workload, and the one it misses is a one-time lifetime fix now closed by
kernel policy. That asymmetry matters for the cost/benefit call.

---

## 5. Benefit / cost, and the before-or-after-compiler2 call

### Cost to build standalone validated FTI models

| Item | Effort |
|---|---|
| Buffer/stack/queue invariant + step helpers (the §2 shape) | ~0.5 day |
| Proof suite (bounds, region, aliasing, underflow, non-vacuity) per FTI | ~0.25 day (the worked file is the template) |
| Wire proofs into `test.sh` as a phase / `goalpost` measure | ~0.25 day |
| A `Liveness` discipline model + its honest-caveat proof | ~0.25 day (optional; documented value-add, not a bug-catcher) |
| **Total for a validated buffer FTI used as the driver's spec** | **~1–1.5 days** |

This is small because the worked example already de-risked the hard
parts: the proof vehicle (`sample --all`), the state-scoping shape
(Probe-C helpers), and the kernel-run form all exist and discharge.

### Benefit

- **Recurring class avoided.** Bug 1's class — an FTI bound that fits the
  fixtures but not the real workload — recurs at every scale jump
  (4096→65536 tokens was one; the next corpus is another). Each instance
  has historically cost ~a day of segfault archaeology. A standalone
  model with bounds proofs turns each into a one-line capacity edit
  guarded by a green/red proof.
- **Reproducible boundary conditions, cheaply.** Today the only way to
  hit an FTI boundary is to run the whole driver on a big input. A
  standalone model reproduces "buffer full," "pop empty," "address at
  region end" in milliseconds, in isolation — the thing the mission notes
  as the missing capability.
- **The model becomes the spec.** The driver's inline FTI must match the
  standalone invariant; divergence is a test failure. This is the
  isolation/unit-test the inline 6000-line FTI structurally cannot have.

### The honest limit on benefit

The use-after-free class (bug 2) is **not** covered mechanically (§4), and
that was the more time-consuming, harder-to-reason bug. So formal
validation does not insure against the *whole* FTI failure surface — it
insures against the bounds/sizing half, which is the *recurring* half.
The lifetime half is now closed by kernel policy and is unlikely to recur
unless the FFI surface grows.

### Before or after compiler2? — **Before. Do a thin slice now.**

The operator's framing: compiler2 is ~one hard bug from compiling
`sample.ev`, and its FTI is the thing that keeps breaking.

**Recommendation: build the thin standalone FTI buffer model + bounds
proofs NOW (before finishing compiler2), using the oracle as the build
tool — but keep it thin (the ~1 day core, defer the liveness model).**

Reasoning:

1. **The FTI is on the critical path and is the active failure source.**
   Two of the recent day-costing bugs were FTI bugs; the remaining "one
   hard bug" is plausibly another FTI/sizing issue. Hardening the exact
   subsystem that keeps breaking, before pushing more weight onto it, is
   the textbook "stop digging" move.
2. **The oracle makes "before" cheap and unblocked.** Standalone FTI
   models are plain `.ev` compiled by `evident-oracle` — they do **not**
   depend on compiler2 being finished. There is no sequencing blocker:
   "before" costs nothing in tooling that "after" would save.
3. **The model is the spec the driver needs anyway.** The driver's inline
   bounds (`lx_count < 65534` etc.) were added reactively, one segfault at
   a time. A standalone model gives a place to decide capacities
   deliberately and prove them, then port the bound into the driver with
   confidence — directly serving the "compile sample.ev" goal rather than
   detouring from it.
4. **It is genuinely small** (§ cost) and the worked example has already
   discharged the methodology risk. This is not a research detour; it is a
   day of writing proofs whose shape is already validated.

**But scope it thin:** build the buffer bounds/region/aliasing/underflow
proofs (the bug-1 class, high ROI, fast). **Defer** the `Liveness` /
refcount discipline model — §4 shows it would not have caught bug 2
mechanically, so it is documentation-grade value, not critical-path
insurance; do it after compiler2 if at all. Spending the "before" budget
on the half that mechanically catches the recurring bug, and deferring
the half that doesn't, is the highest-ROI split.

**What would change the call to "after":** if the remaining compiler2 bug
turns out to be non-FTI (a parser/translate gap), then finishing it
first is fine — the FTI isn't the blocker in that case. The trigger to do
the model *first* is specifically: the next bug is again an FTI
sizing/bounds fault. Given the recent history (two of two), that is the
way to bet.

---

## 6. Artifacts

- `tests/fti_proofs/buffer_safety.ev` — 5 discharged sat/unsat
  obligations (bounds, universal region theorem, aliasing, underflow,
  non-vacuity). Run: `evident-oracle sample … --all`.
- `tests/fti_proofs/overrun_unsat.ev` — kernel-run overrun → exit 2.
- `tests/fti_proofs/safe_exit.ev` — kernel-run in-bounds write → exit 0.
- Reference: `docs/plans/driver-subsystem-map.md` §3 (state-scoping
  verdict the §2 model shape respects); `kernel/src/libcall.rs` (the
  `__mem` primitive + the `inc_ref` AST-lifetime policy that closes the
  bug-2 gap a constraint cannot).
