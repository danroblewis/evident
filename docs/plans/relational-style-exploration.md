# Relational style — census, worked rewrites, proposed standards

> Status: exploratory analysis + proposal for operator review
> (2026-06-10). Changes no source; every experiment lives in /tmp.
> Verification backend: `kernel + .goalpost/artifacts/compiler2-stage1.smt2`
> (the conformance gate's compiler) unless marked *frozen path*
> (`scripts/evident-self emit/sample`, i.e. `kernel + compiler.smt2`).

## 0. The question

From the operator (system-dynamics / control-theory framing): *the
best equations relate many variables together — they express more
relational information. Most Evident programs are
`LHS-variable = RHS-expression`, which is imperative assignment in
disguise (`x ∈ Int = 500` ≡ `int x = 500;`). What does a GOOD
set-theoretic constraint model look like in Evident?*

This document (1) measures how true the premise is, (2) rewrites six
real exemplars maximally relationally and tests them, (3) drafts the
"what good looks like" standards that survived contact with the
toolchain, (4) names where relational style cannot work today and what
would unlock it, and (5) proposes what the critic can rule on now.

## 1. Census — the premise is true, except in type bodies

Method: every `.ev` body line, continuation lines joined, classified
by its top-level operator (script: `/tmp/relx/census.py`). A line is
an **assignment** when its top operator is `=` with a bare-variable
LHS (including `x ∈ T = expr` pins and match-RHS); **relational** when
it is a comparison/implication relating ≥2 variables, a quantifier, a
membership condition, or a claim composition (a join). `decl` = bare
membership, excluded from the ratio. Residual `other` (mis-parses,
header continuations) is under 2% everywhere.

| tree | decl | pin-const | assign-expr | effects | bound | relation | quantifier | compose | total | **assign%** | **rel%** |
|---|---|---|---|---|---|---|---|---|---|---|---|
| compiler2 | 604 | 55 | 1412 | 36 | 29 | 77 | 72 | 273 | 2575 | **76.3%** | 22.9% |
| conformance | 182 | 180 | 103 | 154 | 72 | 56 | 7 | 49 | 825 | **68.0%** | 28.6% |
| lang_tests | 171 | 310 | 51 | 3 | 38 | 34 | 1 | 15 | 631 | **79.1%** | 19.1% |
| stdlib | 2 | 63 | 10 | 108 | 11 | 29 | 5 | 0 | 242 | **75.4%** | 18.8% |
| **type bodies (all trees)** | 19 | 6 | 6 | 0 | 23 | 11 | 0 | 0 | 65 | **26.1%** | **73.9%** |

(assign% = (pin-const + assign-expr + effects) / (total − decl).)

Nuances that matter before judging the 76%:

- **Not all assignments are free choices.** Of compiler2's 1,412
  `assign-expr` lines, 241 are `is_first_tick` carry hold chains (the
  kernel's covered-write requirement — every carried field needs
  exactly one covering write per tick) and 36+108 (stdlib) are
  `effects` writers (the single-writer funnel). Those are
  **kernel-mandated shapes**, not stylistic preference. But that still
  leaves **959 plain `y = f(x…)` lines + 212 free-choice ternaries in
  compiler2 alone** — directed definitions where nothing in the kernel
  forces direction. The operator's observation is quantitatively
  correct.
- **Type bodies are the inversion.** The one place the corpus already
  writes membership conditions (`0 ≤ count ≤ cap`,
  `sol > 0 ⇒ (ctx > 0 ∧ cfg > 0)`) is 74% relational. The language's
  own "a type is its constraints" doctrine produces exactly the style
  the operator is asking for — wherever that doctrine applies, the
  ratio flips. The shortest path to a more relational corpus is to
  give the same doctrine to more surface area (see §3).
- stdlib's tiny relational share is misleading: its genuinely
  relational core (`Permutation`, `Toposort`, `distinct`,
  `∀ x ∈ p : x ∈ s`) is small in line count but is the *whole point*
  of those files. See E5 for the bad news about it.

## 2. Worked rewrites

Each exemplar: real site → maximally-relational rewrite → (a)
same-model test (verdicts must match), (b) functionizer report, (c)
readability judgment. Experiment files under `/tmp/relx/exp/`;
compile+run via the stage1 compiler2 artifact (`c2run.sh`), claims
exercised with sat AND unsat/wrong-value twins so a silent drop cannot
masquerade as success (a drop reads as vacuous-SAT; only the unsat
twin catches it).

### E1. Ternary assignment → guarded implications (lang_test exemplar)

Site: `tests/lang_tests/test_ternary.ev` `sat_in_arithmetic`.

```evident
-- before (directed)                   -- after (relational)
flag = true                            flag = true
x = (flag ? 10 : 20) + 5               flag ⇒ (x = 10 + 5)
                                       (¬flag) ⇒ (x = 20 + 5)
```

- **(a) Same model: YES.** Through compiler2: sat form exits 0 with
  `x = 15` confirmed (`ok` gate), contradiction twin (`x = 25`) exits
  2 — both spellings, all four verdicts match.
- **(b) Functionizer: IDENTICAL.** Both: `6 total / 3 JIT / 1 interp /
  1 residual; 0.0 ms z3`. For a *non-carried* output whose guards
  cover, the implication pair costs nothing today.
- **(c) Reads better? Mixed.** For one boolean the ternary is honest
  and compact. The implication pair wins when the cases are *named
  events* rather than a single test, and it generalizes to n cases
  without nesting — it is the form the §3.4 pin-pair ruling already
  prefers for value selection. Verdict: blessed alternative, not a
  mandate.

### E2. The fsm transition: hold chain → difference equation (the core exemplar)

Site shape: every counter/latch in compiler2 (the §4.1 Carry Latch —
241 occurrences), miniaturized to a runnable 6-tick machine.

```evident
-- before (the blessed hold chain)
fsm main
    n ∈ Int
    n = (is_first_tick ? 0 : _n + 1)
    effects ∈ Seq(Effect) = (n = 5 ? ⟨Exit(0)⟩ : ⟨LibCall("libc", "getpid", ⟨⟩)⟩)

-- after (a relational transition system)
fsm main
    n ∈ Int
    is_first_tick ⇒ (n = 0)
    (¬is_first_tick) ⇒ (n - _n = 1)
    effects ∈ Seq(Effect) = (n = 5 ? ⟨Exit(0)⟩ : ⟨LibCall("libc", "getpid", ⟨⟩)⟩)
```

`n - _n = 1` is the difference equation — variables on both sides,
the tick relation stated as a relation. This is the system-dynamics
form the operator is describing.

- **(a) Same model: YES.** Both run to completion through compiler2:
  exit 0 after counting to 5. The autocarry pass synthesizes `_n` for
  the relational form exactly as for the chain.
- **(b) Functionizer: THE CLIFF, measured.**
  - hold chain: `5 total / 1 JIT / 1 interp / 2 residual; 0.0 ms z3`
  - relational: `not functionized (an output had no covering
    assignment); 6 total / 0 JIT / 0 interp / 6 residual; 3.2 ms z3`
  The relational machine solves with Z3 **every tick**. At 6 ticks
  that is 3.2 ms; at compiler scale it is the measured 19 s → timeout
  class of regression (CLAUDE.md, 2026-06-09). The covered-output
  rule is the single wall between today's corpus and relational
  surface (§4.1).
- **(c) Reads better? Unambiguously yes.** `(¬is_first_tick) ⇒
  (n - _n = 1)` states *what is invariant about adjacent ticks*; the
  hold chain states *how to compute the next value*. The relational
  form also decomposes: each guarded implication is one fact, where
  the chain is one expression whose guard order is load-bearing and
  unsplittable.
- **Side finding (silent drop):** the multi-writer guarded form
  `(n = 5) ⇒ (effects = ⟨Exit(0)⟩)` — documented in CLAUDE.md's
  single-writer rule as legal — is **silently dropped by compiler2**
  (emitted unit has no effects constraint at all; kernel dies with
  `effects var not in model`). Only the single ternary funnel works.
  V2-class gap, previously undocumented.

### E3. Keyed projection: pin pair vs covered chain (registry exemplar)

Site: `compiler2/driver_calllower.ev:55-62` (and ~10 sibling sites),
miniaturized to a carried 2-entry registry + lookup, exit code = the
looked-up value.

```evident
-- relational surface (the blessed §3.4 pin pair)
∀ e ∈ xs : ((e.name = key) ⇒ (out = e.val))
(¬(∃ e ∈ xs : e.name = key)) ⇒ (out = 0)

-- directed encoding (the V9 chain)
out = (xs[0].name = key ? xs[0].val : xs[1].name = key ? xs[1].val : 0)
```

- **(a) Same model: YES.** Both exit 9 (= the value keyed by "beta").
- **(b) Functionizer: IDENTICAL** — `11 total / 3 JIT / 5 interp /
  2 residual; 0.0 ms z3` for both. Reason: `scripts/passes/
  lower-bounded-seq.sh` **already lowers the pin pair into exactly the
  covered chain** before the oracle sees it (verified: the two
  fixtures' emitted `.smt2` units are byte-identical modulo comments). For bounded-Seq registries,
  *keep-the-surface-change-the-lowering is not an aspiration; it is
  running code*. The CLAUDE.md perf trap ("implication-defined outputs
  go residual") describes what happens when the pin pair reaches the
  oracle raw — the pass is precisely the lowering that prevents that.
- **(c) Reads better? Yes, and it is already the ruled form** (§3.4:
  chains in source are V9; the pin pair is the surface). This exemplar
  upgrades that ruling from aesthetic to *measured-free*.
- **Caveat (pass gap):** an exact-length pin `#xs = 2` lowers to
  `xs_len = 2` with **no `xs_len` declaration** → malformed unit
  ("invalid argument"). The corpus convention — `#xs ≤ N` bound +
  index-written slots, no exact length — avoids it. Pass bug worth a
  note in the bounded-Seq catalog.

### E4. Chained membership (declare + constrain in one relation)

```evident
-- before                              -- after
x ∈ Nat                                1 ≤ x ∈ Nat ≤ 5
1 ≤ x ≤ 5
```

- **(a) Same model: YES on the frozen path** (`evident-self sample`):
  sat/unsat verdicts identical for both spellings (x=5 sat, x=6
  unsat).
- **(b) BUT: the two-sided form does not compile through compiler2**
  (`0 ≤ n ∈ Int ≤ 5` → "Error: invalid argument" unit). One-sided
  chains (`n ∈ Int < 6`) and pins (`x ∈ T = v`) work. So §2.3 blessed
  grammar is only partially implemented in the current-generation
  compiler — and the conformance corpus contains **zero** two-sided
  chained-membership fixtures, which is why nothing noticed.
- **(c) Reads better? Yes** — the range *is* the membership condition;
  separating them is pure ceremony. Recommendation: a conformance
  fixture pinning the two-sided form, then prefer it.

### E5. The search-style claim (Toposort) — the relational crown jewel is currently dead code

Site: `stdlib/toposort.ev` — already maximally relational
(`Permutation`, `distinct(p)`, `∀ e ∈ edges : position_of(sorted,
e.from) < position_of(sorted, e.to)`). Planned rewrite: index pins
(`edges[0] = Edge<Int>(10, 30)`) → Seq literal of records
(`edges = ⟨Edge<Int>(10, 30), …⟩`).

**Could not verify — on either path.**

- *Frozen path:* `sample` returns **SAT for the suite's own unsat
  claims** (`unsat_three_cycle:true`, `unsat_p_has_duplicates:true`,
  …). `emit unsat_three_cycle` produces **zero asserts** beyond the
  prelude — the entire relational core (Set membership over elements,
  `distinct`, `position_of`, generic instantiation) is silently
  dropped by `compiler.smt2`.
- *compiler2 path:* the file does not compile ("invalid argument" —
  generics, `Set(T)`, `distinct`, `position_of` are all outside
  stage1's grammar).

Finding: **the most relational code in the repo is unverifiable on the
self-hosted toolchain today** — every `sat_*`/`unsat_*` in
`stdlib/toposort.ev` and `stdlib/combinatorics.ev` currently passes
vacuously. The relational roadmap (§4) is not only about new style; it
is about reviving the style the stdlib already has.

### E6. The type body: anemic registry row → cross-field relation

Site: `compiler2/driver_ir.ev` `type SetVar(name, kidx, elems, count)`
— bodyless, but its writers (`driver_setvar.ev:73-75`) maintain a real
invariant: `elems` holds exactly one 32-byte `FtiNameEntry` row per
counted element. Proposed body:

```evident
type SetVar(name ∈ String, kidx ∈ Int, elems ∈ String, count ∈ Int)
    count ≥ 0
    #elems = 32 * count
```

- **(a) Invariant enforced: YES (2-field form verified).** Through
  compiler2, a `Pack(s, count)` with `#s = 4 * count`: consistent
  instance exits 0; `count = 1` with a 2-char string exits **2**
  (UNSAT) — the relation binds, including the string-length lift.
  `0 ≤ count ≤ cap` (`FtiBuffer`'s body) likewise verified
  (sat 0 / violation 2).
- **(b) Functionizer: fine** — `5 total / 1 interp / 1 residual;
  0.0 ms z3` on the consistent instance.
- **(c) Reads better? This IS the house doctrine** ("a type is its
  constraints"). The relation `#elems = 32 * count` converts a wire
  fact currently living in comments into a kernel-checked invariant.
- **Gap: 4-field types fail — and it is pure arity, not the body.**
  Bisected: 3-field type WITH the relational body compiles and runs
  (c13, exit 0); 4-field type WITHOUT any body fails (c12, "invalid
  argument"); neutral field names change nothing (c11). Direct record
  instances appear capped at 3 fields
  (`FtiBuffer`-sized) in the current driver; registry rows like
  `SetVar` are only exercised through the bounded-Seq lowering, never
  as direct instances. So the SetVar body is **proposable but not yet
  landable** without either the lowering instantiating type bodies
  over flattened rows (the `RecTypeEntry` GAP note, `driver_ir.ev:85`)
  or wider direct-instance support.

## 3. Proposed positive standards (for operator review — exact wording)

Not applied to `docs/evident-purism.md`; proposed text follows. The
purism doc is the right home because all three are surface judgments.

### 3.1 Candidate §1.7 — relate, don't assign, where no direction is inherent

> 7. **Relate, don't assign, where no direction is inherent.** An `=`
>    with a bare-variable LHS is a *directed* claim: it nominates an
>    output. Use it where the kernel demands direction (the covering
>    write of a carried field; the `effects` funnel) or where the value
>    genuinely is a derived view. Where the truth is symmetric — a
>    range, a conservation law, a cross-field relationship, a
>    transition relation — state the relation:
>    `1 ≤ x ∈ Nat ≤ 5`, not `x ∈ Nat` + a bound spelled as ceremony;
>    `#elems = 32 * count` in the type body, not a comment;
>    `lo ≤ cursor ≤ hi`, `a + b = total`, `sol > 0 ⇒ ctx > 0`.
>    A schema whose body is 100% directed assignments is a function in
>    disguise (§1.1 — we don't do functions); its relational content is
>    zero and its name should probably be a `Build*` sugar or a lowered
>    artifact. (Severity: NOTE, density heuristic — see §5.)

### 3.2 Blessed relational idioms (additions to the §2/§3 catalog)

1. **The guarded case pair / case family** (verified E1, cost-free):
   ```evident
   cond ⇒ (out = a)
   (¬cond) ⇒ (out = b)
   ```
   for value selection among *named events*; the ternary stays blessed
   for the single-test view and the carried hold chain (§3.4 stands).
2. **The keyed-projection pin pair** (verified E3, cost-free vs the
   chain): already §3.4 doctrine; upgrade its status from "preferred
   surface" to "preferred surface, measured equivalent under the
   bounded-Seq lowering."
3. **The relational type body as the default registry-row contract**
   (verified E6 at ≤3 fields): every registry row type states at least
   one cross-field relation; a bodyless row type is V7 with a named
   reason or a TODO citing the arity/lowering gap.
4. **The difference-equation transition** (verified E2, *currently
   residual*): `(¬is_first_tick) ⇒ (x - _x = δ)` and friends are the
   ideal surface for carried state, **blocked on §4.1**; until then
   they are legal but go Z3-residual, so hot carried state keeps the
   hold chain. Cold/few-tick machines may use them today.

### 3.3 The perf-tension note (candidate addition to §1.5)

> The relational surface and the directional lowering are a
> **long-term contract**: the surface states relations; transforms
> solve them into directed, covered, functionizable form. This is not
> hypothetical — `lower-bounded-seq.sh` already turns the keyed
> pin-pair into the covered chain byte-for-byte (E3, 2026-06-10), and
> the autocarry pass already synthesizes carry duals for relational
> transition claims. Where the lowering does not yet exist (the
> covered-output rule, §4.1), the *surface form is still the ideal*;
> writing the lowered encoding by hand in source is V3. Perf walls are
> walls of the lowering, never grounds to flag the relational surface.

## 4. Open problems — where relational style cannot work today

1. **The covered-output rule on carried state** (the big one, measured
   in E2). The functionizer extracts one covering assignment per
   output; a transition stated as implications has none, so the whole
   machine goes Z3-residual every tick (0 JIT / 0 interp / all
   residual). *Unlock:* **relational extraction at load** — a
   functionizer (or pre-oracle) pass that solves simple relations into
   directed assignments: guard-partition `g₁ ⇒ (x = e₁), …` into a
   covering ite when the guards provably cover and are disjoint
   (is_first_tick/¬is_first_tick is decidable syntactically), and
   isolate `x` in linear relations (`x - _x = 1` → `x = _x + 1`).
   The E2 pair is the natural regression fixture: the relational form
   must reach the hold chain's profile (`≥1 JIT, 0.0 ms z3`).
2. **The kernel's step semantics force directional seams.** `effects`
   is one funnel; every carried field needs a per-tick write; effect
   results land a tick later (`last_results`). These make *some*
   assignments structural. A relational program can shrink the
   directed surface to exactly these seams, not below. Additionally,
   the documented guarded multi-writer form for `effects`
   (`cond ⇒ effects = …`) is **silently dropped by compiler2** —
   either implement or de-document it (it is the one relational form
   the spec promises for effects).
3. **The frozen standalone path drops most of the relational
   fragment** (probed, /tmp/relx/exp/probes): arithmetic inside
   ternary arms (`_n + 1` → `_n`), trailing arithmetic after a
   parenthesized ternary, bare-Bool antecedents (`flag ⇒ …` vanishes),
   guarded and ternary `effects` writers, and the whole
   Set/generics/distinct layer (E5). Until wave-5 rebuilds
   `compiler.smt2` from `compiler2/`, "verify by unsat twin" is
   mandatory discipline for any relational rewrite: a SAT verdict
   alone is evidence of nothing.
4. **Grammar/lowering gaps found en route** (each needs a fixture or a
   catalog note): two-sided chained membership fails in compiler2
   (E4); `#xs = N` exact-length pins emit an undeclared `xs_len` (E3);
   direct record instances cap at 3 fields and type bodies are not
   instantiated over lowered registry rows (E6; `driver_ir.ev:85`'s
   GAP note is the same hole from the other side).
5. **Carried-Seq registries are write-relational only halfway.** The
   `∀ e ∈ xs : e.f = (… ? … : _e.f)` write surface is still a per-field
   directed covering write. A genuinely relational registry write
   ("after an alloc tick, xs′ = xs ∪ {row}") has no surface or
   lowering today; it collides with both the covered-output rule and
   FSM finiteness, and likely needs the same relational-extraction
   machinery as (1).

## 5. Critic integration proposal

**Rule-able now (no new doctrine required):**

- **Assignment-density heuristic, NOTE severity.** Per schema: if
  every body constraint line (excluding decls, the effects funnel, and
  carry hold chains) is a bare-LHS `=`, emit NOTE: "this claim is 100%
  directed assignments — a function in disguise (§1.1). If it computes
  a value from inputs, should it be a `Build*` sugar / a lowered
  artifact? If it models a thing, where are its relations?" This is a
  direct corollary of the existing no-functions ruling; the census
  classifier (`/tmp/relx/census.py`) is a working prototype.
- **Anemic-row-type sharpening of V7 (already WARN).** For a `type`
  used as a registry row (`xs ∈ Seq(T)`): "no cross-field relation in
  the body" cites E6's verified pattern (`#packed = stride * count`)
  as the model, with the arity-gap escape hatch.
- **Unsat-twin discipline for relational rewrites (process note, not a
  lint).** Any PR claiming a relational rewrite must show the wrong-
  value twin going UNSAT on the path it targets — vacuous-SAT is this
  language's signature failure mode (V2) and E5 shows it can swallow
  whole files.

**Must wait for operator approval:**

- §1.7 itself (§3.1 wording above) — it adjusts the philosophy
  section, which is operator territory by construction.
- Blessing the difference-equation transition (§3.2.4) above NOTE
  severity — it inverts today's practical guidance for hot carried
  state until §4.1's relational extraction lands; premature blessing
  would invite timeout regressions the conformance gate only catches
  as bails.
- Any upgrade of guarded-case pairs over ternaries from "blessed
  alternative" to "preferred" — E1 shows equivalence, not superiority,
  and the corpus has 212 free-choice ternaries that a preference would
  instantly turn into findings.

## Appendix: experiment inventory

| id | claim | backend | result |
|---|---|---|---|
| E1 a/b/c/d | ternary vs implication pair, sat+unsat | compiler2 | verdicts match (0/0/2/2); functionizer identical |
| E2 before/after | hold chain vs difference equation | compiler2 | both exit 0; chain: 1 JIT/0.0 ms z3 — relational: 6/6 residual/3.2 ms z3 |
| E3 pinpair/chain | keyed projection, carried registry | compiler2 | both exit 9; functionizer identical (pass lowers pin-pair → chain) |
| E4 | separate vs chained membership | frozen sample / compiler2 | frozen: verdicts match; compiler2: two-sided chain = compile error |
| E5 | toposort index-pins vs Seq literal | both | unverifiable: frozen drops all relational asserts (unsat claims read SAT); compiler2 can't compile generics/Set |
| E6 | relational type body (`#s = k·count`) | compiler2 | enforced at ≤3 fields (sat 0 / violation 2); 4-field types fail to compile |
| probes p1–p7, c1–c13 | grammar-envelope bisection | both | see §4.3/§4.4 drop list |
