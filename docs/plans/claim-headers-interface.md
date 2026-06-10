# Claim headers as interface — header-join, explicit-only mapping, punning

**Status:** LANDED 2026-06-10 (approved by operator the same day).
Conformance 142–148 pin the semantics; oracle pin `292c7ef`; compiler2
frame join-set + punning + seam decls; autocarry threads carried header
slots (`tests/fsm_compose/counter_*_header.ev`). Open question 1
(context bundles) remains open — see
`docs/plans/blocked-recs-header-slot.md`. Open question 2 decided: a
carried header slot's `_x` dual is appended to the header (interface).
The frozen `compiler.smt2` seam keeps pre-header semantics (142–146 +
148 allowlisted for IMPL=selfhost) until the wave-5 rebuild.

## Problem

Names-match pass-down is dynamic scoping: a child claim's free names
resolve in the CALLER's environment, so an unrelated name collision
silently becomes a semantic join (the `NATURAL JOIN` footgun). Today every
variable a child mentions is implicitly interface — the parent cannot
prevent its own `name` from being constrained by a child that happens to
use `name` internally. The hand-rolled `bcast_`-style prefixes all over
compiler2 are the symptom: people namespace manually when scoping is
dynamic. The 2026-06-10 hiding fix (conformance 139/140/141) freshens
child names the parent does NOT declare; this plan closes the remaining
hole — child names the parent declares *coincidentally*.

## Design

The child's **header** declares its interface, exactly as `type` already
does (`type FtiBuffer(base, count, cap ∈ Int)` — header names fields,
body states invariants):

```evident
fsm DriverBroadcast(on ∈ Bool, slot ∈ Int, name ∈ String, body ∈ Expr)
    phase ∈ Int            -- body membership = INTERNAL, never joins
    ...
```

Composition semantics (final table):

| Form | Joins on | Internals |
|------|----------|-----------|
| `Child` (bare) | child's **header** names only | hidden |
| `Child(a ↦ x, name)` | **exactly** the listed slots (punning: bare `name` = `name ↦ name`) | hidden; unmapped header slots fresh |
| `b ∈ Child` + `b.f = …` | nothing implicit; receiver wiring | receiver-scoped |
| `..Child` | everything (the deliberate lift) | shared |

Rules:

1. **Header = interface.** Body memberships never join with the parent,
   regardless of name collisions. (Generalizes the hiding fix: the
   `already_bound` test consults the header list, not the parent env.)
2. **Bare mention = natural join on the header.** Predictable: the parent
   author reads one line to know what a bare mention can touch.
3. **Any mapping ⇒ explicit-only.** The moment you wire one slot, you own
   the wiring: unmapped header slots become fresh internals (the "I don't
   care about the child's `name`" case). Punning keeps it cheap where
   names already agree.
4. **`..` is the lift, unchanged.** It is the deliberate everything-shared
   form; wide-context components (the driver's token window / parse gate)
   legitimately keep it until a context-bundle record exists.
5. **Backward compatible.** A header-less claim keeps today's semantics
   (whole body implicitly interface). Headers opt in per claim.

Relational reading: claims are relations, composition is a join;
joins operate on the declared schema, never on incidental column names.
`(slot ↦ value)` is ρ (rename); unmapped-fresh is projection of the
join surface.

## Fixture sketches (number on implementation; one behavior each)

- **header-join-sat/unsat**: header `n`; parent `n` joins through bare
  mention (sat and unsat variants — the 094/095 pair with a header).
- **header-internal-no-capture**: child header `(out ∈ Int)`, body
  internal `name`; parent declares unrelated `name = "x"`; bare mention →
  parent's `name` unconstrained (sat where capture would be unsat).
- **mapped-explicit-only**: child header `(on ∈ Bool, name ∈ String)`;
  parent maps only `on ↦ x`; parent's own `name` unconstrained; child's
  `name` fresh (sat).
- **punning**: `Child(on ↦ x, name)` ≡ `Child(on ↦ x, name ↦ name)`.
- **headerless-compat**: a header-less claim behaves per 094/095/139/140/141
  (already pinned; re-run is the check).
- **positional-alignment**: `(a, b) ∈ Child` binds the header slots in
  order (today's "first-line params" made precise).

## Implementation surfaces

1. **Oracle** (`oracle` branch; commit must cite the fixtures):
   - parser: claim/fsm header params (reuse the `type` header parse);
     punning in mapping lists (`Ident` where `Ident MapsTo value` expected).
   - inline: `inline_claim_call`'s `already_bound` consults the header
     set; bare mention with a header joins header names against the
     parent env; unmapped header slots and all body memberships get the
     per-call fresh consts. Header-less claims: current behavior.
2. **compiler2**:
   - claim-index/classify: record header names per claim (the claimidx
     registry gains an interface list).
   - compose: bare splice with a header = α-prefix + auto-binds for
     header∩parent names; mapped splice = binds from the mapping list
     only (+ punning); `name_outer` (the no-redeclare pass-down rule)
     restricted to header names when the callee has a header.
3. **expand-fsm-autocarry**: fsm headers must thread carries — a header
   slot that is carried state in the child needs its `_x` dual mapped at
   the seam (the existing slot-bind `_x ↦ _y` injection generalizes).
4. **Docs**: CLAUDE.md composition table + a style note (prefer headers
   for components; reserve `..` for deliberate context sharing).

## Migration

- Land semantics with zero in-repo headers (everything header-less =
  no behavior change; gates prove it).
- Add headers component-by-component in compiler2, deleting the manual
  prefixes as each component's interface becomes checked (this is the
  Phase-3 / task #33 direction; `DriverBroadcast` is the pilot).
- The seam path (`compiler.smt2`) is frozen and keeps old semantics
  until the wave-5 rebuild loop closes — known divergence, same as the
  hiding fix.

## Open questions

- **Context bundles**: the driver's cross-cutting state (token window,
  parse gate) shared by ~25 components — one record instance passed as a
  single slot would collapse those wide headers. Depends on record-slot
  mapping ergonomics; design later, do not block headers on it.
- Whether `fsm` headers should auto-expose `_x` duals for carried slots
  or require the parent to map them explicitly (autocarry interaction,
  surface 3) — decide during implementation with a fixture either way.
