# Wave 4k — membership-walk shape coverage (5 shapes)

Closes **Wall 2** of `docs/plans/blocked-sample-and-eq-fix.md`: the
self-hosted compiler silently dropped five claim-body shapes the lang
corpus uses, each of which flips a sat/unsat verdict. All five are
membership-walk extensions in `compiler/parse_body.ev::MembershipStep`
(no driver change needed for Items 1–4). Wall 1 (per-claim recompile
cost) is a separate later wave and is untouched here.

Each landed item ships a `tests/kernel/test_compiler_driver_*.ev`
fixture (self-contained FSM that lexes → parses → translates an embedded
source and prints the SMT-LIB, checked against `-- expect:` headers via
`scripts/run-kernel-tests.sh` — the bootstrap-emit + kernel-run loop).

## Item 1 — multi-name decl `n1, n2 [, n3] ∈ T [pin]`  ✅ LANDED

`ms_is_multi` (t1 = Comma). 2-name (`a, b ∈ Day`) and 3-name
(`x, y, z ∈ Int < 5`) supported, with an optional shared pin/bound
(`= v`, `< v`, `≤ v`, `> v`, `≥ v`, `≠ v`) applied per name. Emits one
`(declare-fun n () T)` per name and, when pinned, one assertion per name.
Fixture: `test_compiler_driver_multiname.ev`.

**Out of scope:** the range-PREFIX form `0 < x, y, z ∈ Int < 10` — its
head token is a literal, not an Ident, so `is_name` is false and the walk
never enters MembershipStep. See "Remaining gaps" below.

## Item 2 — implication `LHS ⇒ RHS`  ✅ LANDED

`ms_is_implln` (OpImplies at t3). A line `name op a ⇒ name2 op2 b` lowers
to `(assert (=> <lhs> <rhs>))`. Detected BEFORE the bare-`=` path (which
alone consumed `name = a` and dropped the consequent — the Wall-2 bug).
LHS/RHS each render any comparison op (`=`, `≠`, `<`, `>`, `≤`, `≥`);
`true`/`false` (which lex as KwTrue/KwFalse) render as the SMT-LIB Bool
literals. Fixture: `test_compiler_driver_implication.ev`.

**Out of scope:** parenthesised / compound consequents (`A ⇒ (B ∧ C)`).
The corpus uses the atom-op-atom shape (test_enums_basic).

## Item 3 — chained suffix bound `name ∈ T cmp atom`  ✅ LANDED

`ms_is_chain` (t1 = ∈, t3 ∈ {`<`,`>`,`≤`,`≥`,`≠`}). Emits the decl AND a
bound assertion. Covers `pos_x ∈ Int < 100`. (Shares the fixture with
Item 4.)

## Item 4 — chained `≠` in decl `name ∈ T ≠ atom`  ✅ LANDED

Same `ms_is_chain` path; `≠` lowers to `(assert (not (= name atom)))`
(matching bootstrap `translate_bool` OpNeq). Also widened the BARE line
handler (`ms_is_bare`) from `=`/`≠` to the full comparison set so a
standalone `r < 10` on a prior decl lowers correctly. Fixture:
`test_compiler_driver_chain_bound.ev` (covers `< 100`, `≠ 0`, and bare
`r < 10`).

## Item 5 — bare `ClaimName` composition  ⚠️ DETECTION ONLY

`ms_is_lone` (a lone Ident line whose next token is not an operator we
recognise) is now **cleanly consumed** (rest = l1, no decl, no field) so
it no longer corrupts the walk — the pre-4k path mis-read it as
`name ∈ <next-token>` and emitted `(declare-fun name () )`, derailing
every subsequent membership.

**The composed claim's CONSTRAINTS are still dropped** (a documented
wrong verdict for the three composition claims `sat_weekend_via_claim`,
`unsat_weekend_via_claim_wrong`, `sat_chain_via_composition` /
`unsat_chain_via_composition_violates`). True inlining is **driver-level**
and cannot live in MembershipStep:

- MembershipStep only sees the CURRENT claim's `rem`. The named claim's
  body tokens live elsewhere in the forward token list (`_fwd`, which the
  driver already carries as state after REVERSE).
- The inliner belongs in `compiler/compiler.ev`'s claim sub-machine: on a
  lone-name membership, (1) scan `_fwd` for `KwClaim Ident(name)`,
  (2) collect that claim's body tokens up to the next top-level keyword,
  (3) splice them onto the front of `rem` and continue the walk. Because
  composition is names-match, the spliced body-locals refer to the SAME
  vars as the caller — NO prefixing (the inverse of the
  `project_claim_composition_leaks_body_locals` hazard, which only bites
  explicit slot-bound composition).
- This is a multi-tick sub-FSM (scan + collect + splice) and is only
  testable via a full `compiler.smt2` rebuild (the kernel fixtures
  exercise MembershipStep, not the driver's cross-claim scan). Deferred
  to a follow-on wave.

## Remaining gaps (range-prefix family, head-is-literal)

These all begin with a literal + comparison BEFORE the name, so the
membership walk's `is_name` head check fails and they never reach
MembershipStep. They need a head-parser extension (recognise a leading
`lit cmp` prefix, emit the lower-bound constraint, then fall into the
normal name handling):

- `0 < pos_x ∈ Int`            (lower bound only)
- `0 < pos_x ∈ Int < 10`       (two-sided range)
- `0 ≤ score ∈ Nat ≤ 100`      (le range)
- `0 < x, y, z ∈ Int < 10`     (range + multi-name)
- `0 ≤ score ≤ 100`            (bare double-bound, head-is-literal)

Also `Nat` is emitted verbatim as a sort (`(declare-fun x () Nat)`),
which is not valid SMT-LIB — bootstrap maps `Nat`→`Int` + `(>= x 0)`.
That `Nat`/`Pos` desugar is a separate `translate_declare.ev` concern,
orthogonal to the walk shapes.

## Implementation note (the bug that ate the first pass)

The new implication discriminator was initially named `ms_is_impl` —
colliding with the PRE-EXISTING `ms_is_impl ∈ Bool = (t5 matches
OpImplies)` (the pin-operator classifier). Evident unifies same-named
body memberships, so the two definitions became one constraint
(`(t5 = ⇒) = (is_name ∧ t3 = ⇒)`), forcing UNSAT on every implication
tick. Renamed to `ms_is_implln`. (Cf. memory
`project_claim_composition_leaks_body_locals` — the same name-unification
mechanism, here within a single claim.)
