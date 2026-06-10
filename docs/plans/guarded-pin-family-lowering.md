# Guarded-pin-family lowering — retiring the V9 value-selection chains

**Status:** proposed (2026-06-10). Targets the V9 WARN class of the
compiler2 baseline review (`docs/critic-reports/compiler2-baseline.md`:
38 value-selection / case-code ternary chains, the S2 systemic
finding).

## Ruling and precedent

The operator has ruled ternary chains out of source
(`docs/evident-purism.md` §1.4): set-theoretic surfaces — keyed-
projection pin pairs, guarded implications — are the preferred form;
**covered select chains belong ONLY in lowered artifacts**. §3.4 draws
the line precisely: the carried-write **hold chain**
(`x = (is_first_tick ? init : event ? v : … : _x)`) is BLESSED and
stays in source; ≥2-test **value-selection** chains are WARN with the
durable fix being "a lowering or registry change, not acceptance of
the chain" (§1.5).

The lowering half already exists for one shape: the **keyed-projection
PAIR** in `scripts/passes/lower-bounded-seq.sh` (header, REWRITE
RULES) recognizes

```evident
∀ e ∈ xs : ((e.F = KEY) ⇒ (OUT = e.V))
(¬(∃ e ∈ xs : e.F = KEY)) ⇒ (OUT = DEF)
```

together and lowers the pair to the covered select chain
`OUT = (((0 < xs_len) ∧ (xs_0_F = KEY)) ? xs_0_V : … : DEF)`, because
implication-pins alone are bare disjunctions the functionizer cannot
extract (measured 2026-06-09: fixture-001 19 s → >300 s timeout — the
CLAUDE.md covered-output trap). Its semantics note carries over
verbatim to this plan: the chain is FIRST-match-wins where the
constraint reading pins ALL matches.

This plan generalizes the PAIR from "pins quantified over one Seq" to
**any family of guarded pins on one output**, so the 38 chains whose
tables are data can be written as guarded implications.

## The rule

### Surface form (what source says)

A **pin family** for output `out`: N guarded pin lines plus exactly
one default line, in one claim scope —

```evident
g1 ⇒ (out = v1)
g2 ⇒ (out = v2)
…
gN ⇒ (out = vN)
(¬(g1 ∨ g2 ∨ … ∨ gN)) ⇒ (out = d)
```

where each `gi` is a Bool expression and each `vi`/`d` an expression
of `out`'s type. Quantified pin lines from the existing PAIR rule
(`∀ e ∈ xs : ((e.F = KEY) ⇒ (out = e.V))`) may be **members of the
same family** (the mixed floor-case form below); their contribution
to the default's disjunction is the matching `∃`.

### Lowered form (what the oracle sees)

```
out = (g1 ? v1 : g2 ? v2 : … : gN ? vN : d)
```

One covering assignment — the functionizer-safe shape. Quantified
members expand to their per-slot guarded arms at their textual
position (exactly as the PAIR does today).

### Recognition

- **Same-out grouping.** Lines group by the pinned name `out` within
  one claim body. `out` must have no other covering write in scope
  (a family for an `out` that also has a bare `out = …` line is an
  error — two covering writes).
- **Order = priority.** The textual order of the pin lines is the
  chain order. This matters only where guards overlap — see
  semantics.
- **The default is mandatory and syntactically checked.** The default
  line's guard must be the negated disjunction of the family's guards
  (for quantified members, the corresponding `¬∃`). A family with no
  default **fails the pass loudly** — lowering it would either leave
  bare implications (the measured perf cliff) or invent a value;
  the existing completeness-check precedent in lower-bounded-seq.sh
  (loud exit 1 on unsupported uses) applies. The default existing is
  also what makes the surface honest set theory: the family states a
  total case analysis.

### Semantics note (disjointness / first-match-wins)

As constraints, overlapping guards with different values are UNSAT
(both pins assert). The lowered chain is first-match-wins. The
lowering is therefore faithful exactly when guards are pairwise
disjoint or overlapping guards agree on the value — the same contract
the PAIR documents ("use only where keys are unique"). The rule
inherits it: **the pin family asserts disjoint cases; priority is not
a semantics you may lean on.** Where priority IS the semantics
(an event hierarchy), the construct you want is the blessed hold
chain or a `match` — not this rule.

### What must NOT be rewritten

- **Carried-write hold chains are out of the rule's domain** (purism
  §3.4 blesses them in source as ternary). The recognizer must
  additionally refuse a pin family whose default value is the
  output's own carry dual (`… ⇒ (out = _out)`): that is a hold
  written in pin-family clothing, and hold guards are *prioritized
  events* that overlap by design — lowering would silently bake in
  textual priority where the constraint reading is UNSAT. Loud
  refusal, with a message naming the hold-chain form.
- Single conditionals (`cond ? a : b`) and capture-or-carry views —
  blessed (§3.4), untouched.

### Floor-case mixes (the matchpin shape)

driver_matchpin.ev:187–192 today is half pin-pair, half chain:

```evident
fold_tester1_user ∈ Int
∀ e ∈ user_variants : ((e.name = fold_ctor1) ⇒ (fold_tester1_user = e.tester))
(¬(∃ e ∈ user_variants : e.name = fold_ctor1)) ⇒ (fold_tester1_user = 0)
fold_tester1 ∈ Int = (fold_ctor1 = "IntResult" ? z_intres_test
    : fold_ctor1 = "StringResult" ? z_strres_test
    : fold_tester1_user)
```

The mixed family makes the intermediate `_user` const and the chain
disappear — floor literals and the registry scan are one case
analysis over one output:

```evident
fold_tester1 ∈ Int
(fold_ctor1 = "IntResult") ⇒ (fold_tester1 = z_intres_test)
(fold_ctor1 = "StringResult") ⇒ (fold_tester1 = z_strres_test)
∀ e ∈ user_variants : ((e.name = fold_ctor1) ⇒ (fold_tester1 = e.tester))
(¬((fold_ctor1 = "IntResult") ∨ (fold_ctor1 = "StringResult")
    ∨ (∃ e ∈ user_variants : e.name = fold_ctor1))) ⇒ (fold_tester1 = 0)
```

Disjointness holds by the repo's collision-freedom fact (builtin/floor
names vs user variant names; noted at driver_exprdecomp.ev's header).
This is what unblocks baseline burndown item **W4** ("blocked on a
registry that can hold floor handles" — with mixed families the floor
entries need no registry slot at all).

### Out of scope: case codes that should be enums + match

A chain over a *case-code discriminant* (`recdecl_st`, `parse_mode`,
`fold_arm_n`, `enum_act/step`) is not a keyed data lookup — it is a
`match` waiting for its enum, per the baseline's driver_record finding
(driver_record.ev:185–194: "an RD-step enum + match is the surface")
and burndown items W1–W3. Rewriting those as pin families would be
spelling the same case-code smell in a new notation. The rule of
thumb: **if the guards all test one discriminant against constants,
the alternative is an enum + `match`; if the guards are independent
predicates or registry probes, the pin family is the surface.** The
38-WARN class splits accordingly; this lowering retires the
data-table/floor-mix subset (W4/W5/W6-adjacent), not the enum
deficit.

## Before/after examples (from baseline findings)

**1. driver_symlookup.ev:45–47** (`lookup_handle` floor chain over a
pin-pair base — same shape as matchpin):

```evident
-- before
lookup_handle ∈ Int = (lookup_name = "true" ? z_true
    : lookup_name = "false" ? z_false
    : lookup_enum_val)
-- after (lookup_enum_val's existing pair folds in)
(lookup_name = "true") ⇒ (lookup_handle = z_true)
(lookup_name = "false") ⇒ (lookup_handle = z_false)
∀ e ∈ enum_values : ((e.name = lookup_name) ⇒ (lookup_handle = e.value))
(¬((lookup_name = "true") ∨ (lookup_name = "false")
    ∨ (∃ e ∈ enum_values : e.name = lookup_name))) ⇒ (lookup_handle = 0)
```

**2. driver_matchpin.ev:190–192** (`fold_tester1`) — shown above;
the same rewrite covers its five siblings (fold_tester2/acc1/acc2/
def_acc, baseline rows :196–216).

**3. driver_lex.ev:122–128** (`frac_pow`, literal-code → power
table; duplicated as `real_denom` at driver.ev:831–837):

```evident
-- before
frac_pow ∈ Int = (_frac_digits = 1 ? 10
    : _frac_digits = 2 ? 100
    : _frac_digits = 3 ? 1000
    : _frac_digits = 4 ? 10000
    : _frac_digits = 5 ? 100000
    : _frac_digits = 6 ? 1000000
    : 1)
-- after
(_frac_digits = 1) ⇒ (frac_pow = 10)
(_frac_digits = 2) ⇒ (frac_pow = 100)
(_frac_digits = 3) ⇒ (frac_pow = 1000)
(_frac_digits = 4) ⇒ (frac_pow = 10000)
(_frac_digits = 5) ⇒ (frac_pow = 100000)
(_frac_digits = 6) ⇒ (frac_pow = 1000000)
(¬((_frac_digits = 1) ∨ (_frac_digits = 2) ∨ (_frac_digits = 3)
    ∨ (_frac_digits = 4) ∨ (_frac_digits = 5) ∨ (_frac_digits = 6)))
    ⇒ (frac_pow = 1)
```

(Borderline: all guards test one discriminant, so a literal-pattern
`match _frac_digits` is the *other* admissible rewrite — either
retires the chain; the shared-table dedup with `real_denom` is its
own baseline item.)

**4. Named non-example — driver_window.ev:293–309** (`win_need` over
14 `parse_mode` codes) and **driver_record.ev:185–194**
(`recdecl_st_now` transition table): single-discriminant case codes —
the alternative is W1's enums + `match`, not a pin family.

## Implementation

- **Where:** the same pass family as the PAIR — extend
  `scripts/passes/lower-bounded-seq.sh`'s pair recognizer to (a)
  admit scalar pin lines into a family alongside quantified ones,
  (b) admit pure-scalar families (no Seq member at all). One grouping
  pass keyed on `out`; running it inside the Seq pass avoids an
  ordering problem (a separate later pass would see the PAIR's
  already-lowered chain instead of its pin lines). Mark with the
  standard `# TODO: rewrite in Evident` header; the roadmap's
  passes-in-Evident deliverable inherits it (the rule inventory in
  post-cutover-roadmap.md grows by one family).
- **Fixtures first:** `tests/compiler2_units/seq_lowering/` cases for
  scalar family, mixed family, missing-default (loud fail),
  hold-disguise (loud refusal), overlap UNSAT documentation case;
  plus a `tests/seq/` behavioral pair. Byte-identical flatten output
  on `compiler2/driver.ev` until compiler2 sources adopt the form.
- **Adoption order:** matchpin's six chains first (calibration-pinned
  instances, smallest blast radius), then symlookup/exprdecomp floor
  chains, then the literal tables (frac_pow/real_denom, with the
  dedup). Each adoption gates on the affected isolation units +
  `scripts/functionization-gate.sh` (~20 s); batch conformance per
  the CLAUDE.md fast-gate/slow-gate workflow.
- **Critic loop:** each adopted file re-reviewed by `evident-critic`;
  the V9 rows flip to the blessed-pin-pair verdict the calibration
  already pins for matchpin's :187–189 half.
