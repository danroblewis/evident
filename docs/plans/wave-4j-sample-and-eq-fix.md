# Wave 4j — `sample` verb + enum-equality assertion fix

Closes the two gaps wave 4i (`docs/plans/blocked-bootstrap-cutover.md`)
flagged as gating the bootstrap cutover. Status of each item below.

## Item 1 — bare `name = expr` assertion fix  ✅ LANDED

**Bug:** a line whose operator is `=` / `≠` (not `∈`) after a prior
`name ∈ Type` membership was mis-parsed as a *new* chained-membership
decl. `MembershipStep` (`compiler/parse_body.ev`) assumed t1 was the `∈`
operator and read t2 as the *type*, emitting `(declare-fun today () Mon)`
(and polluting the manifest `state-fields`) instead of
`(assert (= today Mon))`.

**Fix:** `compiler/parse_body.ev` now detects `ms_is_bare` (t1 is `Eq` or
`OpNe`). For a bare line it lowers the t2 atom via `AtomSmtlib` and emits a
single assertion chunk — `(assert (= name rhs))`, or
`(assert (not (= name rhs)))` for `≠` (matching bootstrap's
`translate_bool` OpNeq → `(not (= l r))`) — with **no** declaration and
**no** manifest field. The var was already declared by the earlier
membership.

**Proof:** `tests/kernel/test_compiler_driver_eq_assertion.ev` exercises
`a ∈ Int · b ∈ Int · a = b` → exact output
`(declare-fun a () Int) / (declare-fun b () Int) / (assert (= a b))`,
`state-fields = a:Int b:Int`. Passes exact-match via the kernel-test
runner logic (bootstrap emit + kernel run). Confirmed on the rebuilt
`compiler.smt2` against the wave-4i probe: `today = Mon` / `today = Tue`
now emit assertions, `state-fields = today:Day` (no pollution).

**Scope of the fix:** bare-`=` / bare-`≠` with a single atom RHS. Bare
lines with compound RHS (binop, ternary, implication) are NOT covered —
see Item 5 / the blocker doc.

## Item 2 — claim-name selection in compiler.ev  ✅ LANDED

`compiler/compiler.ev` now reads an optional target claim name from
`/tmp/compiler-target-claim.txt` via a SECOND `ReadFile` on the first
tick (into `last_results[1]`). When that file holds a name, only the
matching bare-head claim is translated; the rest take the existing
skip path. When the file is absent/empty the `ReadFile` yields an
`ErrorResult`, the `target_read` match falls to `""`, and dispatch
reverts to the corpus convention (last bare-head claim wins) — so all
existing fixtures / the build path are unaffected (backwards compat).

Dispatch gate: `enter_claim` now also requires `name_selected`
(`¬has_target ∨ name_matches`); non-matching bare-head claims join
`enter_skip`.

**Proof:** `tests/kernel/test_compiler_driver_claim_select_by_name.ev`
(compiler.ev with an inline 2-claim source + inline target `"beta"`),
expecting only `beta`'s `(declare-fun y () Int) / (assert (= y 2))`.
Live `compiler.smt2` confirmed: target `alpha`→`x`, `beta`→`y`,
`gamma`→`z`; no target → last (`gamma`).

## Item 3 — sample wrapper  ✅ LANDED

`scripts/sample-via-smt2.sh`: for each top-level claim, write the
target name, run `kernel + compiler.smt2` to emit that claim's SMT-LIB,
strip the `[functionizer]` + `;; manifest:` lines, append `(check-sat)`,
and run standalone `z3` (resolved via `command -v z3`). sat→`true`,
unsat→`false`. Supports `<file> --all [--json]` and `<file> <claim>
[--json]`. Output `{"name":bool,…}` matches bootstrap's
`sample --all --json` JSON shape.

**Proof:** on a 2-claim enum file (`sat_pin` / `unsat_two`, the exact
wave-4i bug shape), bootstrap emits `{"sat_pin":true,"unsat_two":false}`;
the wrapper agrees. (See blocker doc for the timing.)

## Item 4 — wire into evident-self  ✅ LANDED

`scripts/evident-self`'s `emit_via_smt2_wrapper` (the `bin` seam under
`EVIDENT_SELF_VIA_SMT2=1`) now `exec`s `scripts/sample-via-smt2.sh` for
the `sample` verb. `run-lang-tests.sh` resolves its binary through this
seam, so `EVIDENT_SELF_VIA_SMT2=1` routes its `sample --all --json`
calls to the self-hosted path with no change to the runner.

## Item 5 — lang probe  ⛔ BLOCKED

See `docs/plans/blocked-sample-and-eq-fix.md`. Two independent walls:
a per-claim recompile cost of ~90s (Z3-bound, kernel-side) that makes
the 11-file / ~190-claim suite take hours, and unsupported claim-body
shapes (`⇒`, multi-name `a, b ∈ T`, claim composition) that the
self-hosted compiler still cannot translate. Items 1–4 are correct and
verified in isolation; the full suite cannot go green on the
self-hosted path yet.
