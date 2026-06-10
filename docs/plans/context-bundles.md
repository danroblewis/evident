# Context bundles — collapse the shared parse context into one header slot

**Status:** approved 2026-06-10. Unblocks clean de-prefixing (operator
chose "option A": clean short full-word names via bare-mention hiding).
Core mechanism (record-typed header slot) probed working 2026-06-10
(`/tmp/ctx_probe.ev`: `consumer(ctx ∈ ParseCtx, …)` with `ctx.field`
access compiled + ran correct through the frozen oracle).

## Problem

To convert a component from `..`-lift to bare-mention (which hides its
internals so they can have clean unprefixed names), its header must list
its ENTIRE interface — every name it reads from outside plus every name
it exposes. Measured on DriverGroup: ~26 names, of which ~12 are
**ambient context** — the token window (`tok0..tok7`) and parse gate
(`parse_mode`, `in_parse`, `tok_ready`, `work_nil`) — shared by 10–13 of
the 24 components. Listing those 12 in every header is the bloat that
sank the first de-prefix attempt (Symtab's 64-slot header was mostly
this). The claim-headers plan named this as open question #1.

## Design

One record type bundles the ambient context; each component takes it as
a SINGLE header slot and reaches fields by dot-access.

```evident
type ParseCtx(
    tok0, tok1, tok2, tok3, tok4, tok5, tok6, tok7 ∈ Token,
    parse_mode ∈ Int,      -- the prev-tick dispatch mode (_parse_mode)
    in_parse, tok_ready, work_nil ∈ Bool)

fsm DriverGroup(ctx ∈ ParseCtx, … component-specific interface …)
    active ∈ Bool = (ctx.in_parse ∧ (ctx.parse_mode = 9)
        ∧ ctx.work_nil ∧ ctx.tok_ready)
    …
```

`driver_main` builds it once per tick and passes it to each component:

```evident
parse_ctx ∈ ParseCtx(tok0 ↦ tok0, …, tok7 ↦ tok7,
    parse_mode ↦ _parse_mode, in_parse ↦ in_parse,
    tok_ready ↦ tok_ready, work_nil ↦ work_nil)
…
DriverGroup(ctx ↦ parse_ctx, …)
```

The `parse_mode` field carries the PREV value (`_parse_mode`) because
that is what components dispatch on — "what mode were we in at this
tick's start." driver_main builds the bundle fresh each tick (it is an
input, not carried state), so no carry-dual is needed for the bundle
itself; the window tokens are current-tick, the mode is prev-tick, which
is exactly the dispatch context.

## Follow-on bundles (not in v1)

The component-to-component signals also cluster. A second bundle —
`ClassifyResult` (the per-line classification: `line_sort`, `line_is_mem`,
`mem_tyname`, the `enter_*` dispatch flags) — would collapse most of the
remaining cross-component header slots the same way. Design after
ParseCtx lands; ParseCtx alone takes DriverGroup from ~26 to ~14 slots.

## Plan

1. Define `ParseCtx` in `compiler2/driver_ir.ev`; build `parse_ctx` in
   driver_main.
2. Pilot ONE component (DriverGroup): bare-mention against
   `(ctx ∈ ParseCtx, …)`, every `tok*/in_parse/parse_mode/tok_ready/
   work_nil` → `ctx.*`, internals hidden + renamed to clean full words
   (operator naming rule: full words, short when possible; abbreviations
   only when they name a real entity, e.g. `Nat`). Gate + show.
3. Scale component-by-component once the pilot is approved. Each is
   independently gated (units, functionization GREEN, conformance
   153/154, 0 timeouts) and the hidden internals cannot collide across
   components (the point of hiding), so naming is local.
4. `ClassifyResult` bundle as a follow-on to shrink the remaining slots.

## Verification discipline (learned 2026-06-10)

"Gates green" is necessary but not sufficient for a rename/hiding change
— a latent name collision can pass conformance (the `setvar_slot` bug).
For each component: (a) flatten must succeed (a missed interface name
becomes an unbound hidden internal → loud failure, good); (b) oracle
emit must succeed (no "output had no covering assignment"); (c) the
component's behavior fixture + full conformance unchanged. Pilot one,
prove it, then scale.

## Discovered oracle gaps (2026-06-10, blocking clean option A)

Building the DriverGroup pilot surfaced a cluster of frozen-oracle limits.
Some were probed working, several block the clean-names refactor:

WORKS (probed):
- Record-typed header slot with SCALAR fields (`ctx.a + 1`, `ctx.b ? …`):
  compiles + runs correct.
- Hidden carried internal state, SEPARATE-decl form (`n ∈ Int` then
  `n = (is_first_tick ? 0 : _n + 1)`): carries across ticks (probe → exit 3).

RESOLVED (oracle f767cd5, 2026-06-10):
- **`matches` on an enum-typed record field** (`ctx.tok0 matches Ident(_)`):
  the `Expr::Matches` Ctor arm in `translate/exprs/bool.rs` guarded its
  scrutinee on `!n.contains('.')`, so a dotted name — record-field access —
  fell through and the constraint dropped vacuously-SAT. `declare_var_named`
  already flattens an enum-typed record field into an env entry keyed by
  the dotted name (`c.t` → its `EnumVar`), exactly as scalar field access
  relies on; dropping the dot guard lets `env.get("c.t")` resolve it (the
  `Var::EnumVar` match remains the real gate). Bugfix-to-spec; conformance
  fixture `155-matches-on-record-field` (red→green via the oracle, exit 0);
  `compiler2/driver.ev` `driver_main` emit byte-identical before/after
  (inert on existing source). Pin lineage `292c7ef → f767cd5`. NOTE: the
  frozen `compiler.smt2` predates this fix, so fixture 155 remains a
  compiler2 conformance gap until the wave-5 rebuild (same status as
  fixtures 142-148). The oracle — used by the compiler2 emit/driver-decomp
  gates — now handles it correctly.
- **Record construction pinning an enum field to a constructor literal**
  (`Win(t0 ↦ Ident("x"))`): "field doesn't exist / shape" error. (Pinning
  to a variable — `ParseCtx(tok0 ↦ tok0)` — works.)
- **Autocarry combined-decl gap**: `n ∈ Int = (…_n…)` (decl+assign on one
  line) does NOT get its `_n` dual synthesized; only the separate-decl form
  does. Minor, but a real autocarry transform bug.

Consequence: clean option A (bare names via hiding + ctx bundle) for the
stateful, token-`matches`-ing components is gated on the oracle growing
`matches`-on-field-access (and the autocarry combined-decl fix). Until
then, only stateless non-token components convert cleanly; the rest need
option B (qualified names, keep `..` lift) or the oracle work.
