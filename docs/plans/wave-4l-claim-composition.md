# Wave 4l — claim composition inliner + range-prefix + Nat desugar

Closes the three remaining **Wall 2** shapes from
`docs/plans/blocked-sample-and-eq-fix.md` that wave 4k left open
(`docs/plans/wave-4k-membership-walk-shapes.md`, "Remaining gaps" + Item 5).
After 4l, every claim-body shape the simplest lang corpus file
(`tests/lang_tests/test_enums_basic.ev`) uses is translated by the
self-hosted compiler. **Wall 1** (per-claim recompile cost, ~minutes/claim)
is the only remaining gate and gets its own wave (lex-once-multi-claim,
wave-4i Option 1).

## Item 1 — bare-`ClaimName` composition (driver inliner)  ✅ LANDED

Wave 4k consumed a lone-`ClaimName` body line cleanly but **dropped** the
named claim's constraints (a documented wrong verdict). 4l inlines them.

True inlining is **driver-level** (MembershipStep only sees the current
claim's `rem`; the callee's body tokens live in `_fwd`). Implemented in
`compiler/compiler.ev` as a new `pmode = 4` sub-FSM:

- **Detect** (in the claim run): a `rem` head `Ident(name)` whose next token
  is not a membership/assert operator (`∈ = , < > ≤ ≥ ≠ ⇒ ( matches ? + - * /
  ∧ ∨`) is a names-match composition (`is_lone_line` → `do_inline`).
- **Search** (`iph 0`): walk `_fwd` one token/tick for `KwClaim Ident(name)`.
- **Collect** (`iph 1`): gather the callee's body tokens (reverse order) up to
  the next top-level keyword (or end of stream).
- **Transfer** (`iph 2`): cons the reversed collection onto the saved tail
  (`itail` = `rem` past the lone name) → the callee body is spliced, in source
  order, onto the FRONT of `rem`; resume the membership walk (`pmode 2`).

Because composition is names-match, spliced body-locals refer to the SAME
vars as the caller — **no α-prefixing** (the inverse of the explicit
slot-bound hazard, memory `project_claim_composition_leaks_body_locals`).

**Declare-dedup** (the new correctness piece): the callee re-declares caller
variables (`is_weekend_rule` re-declares `day`/`is_weekend`). Bootstrap's
declare path is idempotent (declare.rs early-returns on a known env key); the
self-hosted compiler emits TEXT, so a second `(declare-fun …)` is a Z3
duplicate. The driver tracks declared single names in `_decll`
(`|name|`-delimited) and, on a duplicate, suppresses the decl line **and** the
manifest field, emitting only any bound/pin assertion. This is a general
guard (no-op for non-composition claims, where no name repeats).

Proof: `tests/lang_tests/test_enums_basic.ev` discriminating claim
`unsat_weekend_via_claim_wrong` (`day = Wed`, `is_weekend = true`) is `false`
ONLY when the rule body is inlined — without inlining it would be `sat`. The
self-hosted verdict matches bootstrap (see "Item 4" below).

## Item 2 — range-prefix `lit cmp name [∈ T] [cmp atom]`  ✅ LANDED

`compiler/parse_body.ev` MembershipStep: a body line whose head is a
literal + comparison BEFORE the name never reaches the `is_name` head check,
so a `ms_is_rangepfx` discriminator (`t0` IntLit, `t1` ∈ {`<`,`>`,`≤`,`≥`},
`t2` Ident) handles it. The prefix `lit cmp name` lowers to `(cmp lit name)`,
the suffix `name cmp atom` to `(cmp name atom)` — mirroring bootstrap's
`try_parse_chained_membership` (one Constraint per op with the var in the
middle: `0 < x ∈ Int < 10` → `(< 0 x)` + `(< x 10)`). Sub-cases:

- `0 < x ∈ Int`     — prefix only, typed (decl + lower bound)
- `x ∈ Int < N`     — suffix only (the wave-4k chain path)
- `0 < x ∈ Int < N` — two-sided, typed
- `0 < x ≤ N`       — bare double-bound on a prior decl (asserts in `decl`,
  pinned=false, like `ms_is_bare`, to avoid an empty-decl blank line)

Fixture: `tests/kernel/test_compiler_driver_range_prefix.ev`.

## Item 3 — `Nat` → `Int` + `(>= name 0)`  ✅ LANDED

`compiler/parse_body.ev`: `Nat` is not a Z3 sort. Bootstrap (declare.rs)
declares an Int const + a `>= 0` post-constraint; the manifest field is `Int`
(a Nat var is an `IntVar`, emit.rs `discover_state_fields`). MembershipStep
maps the decl sort (`ms_sort`: Nat→Int) and prepends `(assert (>= name 0))` to
whatever the normal/chain/pin path emits, so `n ∈ Nat < 50` reuses the wave-4k
chain tail for the `< 50` bound. Applied to the range form too
(`0 ≤ s ∈ Nat ≤ 100`).

Fixture: `tests/kernel/test_compiler_driver_nat_desugar.ev`.

## Item 4 — lang probe (`test_enums_basic.ev`)

The per-claim verdicts the self-hosted `kernel + compiler.smt2` produces
match bootstrap's `sample --all` byte-for-byte on the claims exercising the
new shapes (notably the two composition claims). See the session report for
the verdict table. Wall 1 (per-claim recompile cost) makes a full 19-claim
`--all` pass impractical in-session; the discriminating composition claims are
checked directly.

## Naming-collision guard (wave-4k lesson)

Per `project_claim_composition_leaks_body_locals` and the wave-4k
`ms_is_impl`/`ms_is_implln` incident: every new local introduced here was
searched for in `compiler/*.ev` before assignment. The range-prefix locals
are `ms_rp_*` / `ms_p_*`; the inliner state is `i*` / `r_t*` / `decll`.
