# BLOCKED: lang probe (wave 4j, Item 5)

**Status: Items 1–4 LANDED and verified in isolation. Item 5
(`EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang` green) is BLOCKED on two
independent walls. Cutover NOT performed; nothing in `bootstrap/`,
`kernel/`, `stdlib/`, or `tests/lang_tests/` was touched.**

The `sample` verb now exists on the self-hosted path (wrapper + claim
selection + the bare-`=` assertion fix), and produces verdicts
byte-equal to bootstrap **on the shapes it supports** (proven: a 2-claim
enum file `{"sat_pin":true,"unsat_two":false}` matches bootstrap). But
the lang suite cannot go green, for two reasons that are both real and
neither of which is a `sample`-wrapper bug.

---

## Wall 1 (DECISIVE for in-session completion): per-claim recompile cost

The self-hosted `sample` checks one claim per `kernel + compiler.smt2`
run. Measured cost of a single such compile:

| input                                  | wall  | breakdown                         |
| -------------------------------------- | ----- | --------------------------------- |
| 3-claim file, select claim 1 (`alpha`) | 92 s  | 87.6 s Z3 / 40676 residual / 0 JIT |
| 2-claim file, select claim 2 (`beta`)  | 64 s  | Z3-bound, not functionized        |

`[functionizer] not functionized (extract_program: an output had no
covering assignment); … 40676 residual; … 87609.9 ms z3`. Every compile
tick goes to Z3; nothing functionizes for the compiler-FSM shape on these
inputs.

The lang suite is 11 files × ~19 claims ≈ **~190 claims**, each a
separate ~60–90 s compile (later claims pay extra for skipping the
earlier ones). That is **~4–6 hours** of wall time for one `--lang`
pass — and `--kernel` / `--conformance` under the seam are each similarly
hours, as wave 4i already noted. This is a kernel-side per-tick solve
cost (`kernel/` is frozen); it is not addressable from `compiler/*.ev`
or the wrapper.

**Architectural implication for the coordinator:** the per-claim
*recompile* model is correct but impractical. A faithful, fast `sample`
wants the wave-4i Option 1 instead — a single `sample.smt2` (or a
compiler mode) that lexes the file ONCE and emits/solves every claim in
one kernel run, amortising the lex+parse over all claims rather than
paying it ~190 times. The wrapper here is the Option-2 stopgap; it
proves the verdict path is correct but should not be the cutover vehicle.

---

## Wall 2 (independent): unsupported claim-body shapes

Even with unlimited time, no lang file goes green: every file uses
claim-body shapes the self-hosted compiler still cannot translate. The
bare-`=`/`≠` fix (Item 1) was necessary but covers only a fraction of the
surface. Concretely, per `tests/lang_tests/`:

| shape                              | example (file:line)                                   | self-hosted behaviour today |
| ---------------------------------- | ----------------------------------------------------- | --------------------------- |
| multi-name decl `a, b ∈ T`         | `a, b ∈ Day` (test_enums_basic.ev:98)                 | t1 is `Comma`, not `∈`/`=` → `MembershipStep` mis-declares `(declare-fun a () b)` and the walk STOPS (next head is `OpIn`, not an Ident) → rest of claim dropped |
| implication `⇒` (29 occurrences)   | `today = Sat ⇒ is_weekend = true` (test_enums_basic.ev) | bare-`=` handler consumes `today = Sat`, leaves `⇒ …` dangling → walk stops, consequent + remaining lines dropped |
| chained bound `∈ Int < N` / range  | `pos_x ∈ Int < 100` (test_chained_membership.ev:27)   | no chain desugar in the membership walk → constraint dropped |
| chained `≠` in decl `∈ Int ≠ 0`    | `pos_x ∈ Int ≠ 0` (test_chained_membership.ev)        | not the bare form; not handled in the `∈` path |
| claim composition (lone name)      | `is_weekend_rule` (test_enums_basic.ev:131), `bounded_score`, `is_ok_value` | a bare `ClaimName` line is an Ident with no `∈`/`=` → mis-read / walk stops |
| `Set(T)` / `Real` / `match` / `matches` in lang context | test_chained_membership, test_match, test_matches, … | partially wired for `emit`, untested for sat-equivalence |

The bare-`=`/`≠` Item-1 fix is correct for the shapes it claims
(`name = atom`, `name ≠ atom` after a prior membership) — but most lang
claims combine it with the above, and dropping any constraint silently
flips a sat/unsat verdict.

---

## What IS proven (Items 1–4)

- **Item 1** — `tests/kernel/test_compiler_driver_eq_assertion.ev` exact-
  matches (`a = b` → `(assert (= a b))`, no spurious decl/field). The
  wave-4i probe (`today = Mon`/`= Tue`) now emits assertions on the
  rebuilt `compiler.smt2`.
- **Item 2** — `tests/kernel/test_compiler_driver_claim_select_by_name.ev`
  + live `compiler.smt2`: target `alpha`→`x`, `beta`→`y`, `gamma`→`z`;
  no target → last bare-head (`gamma`). Backwards-compat preserved.
- **Item 3** — `scripts/sample-via-smt2.sh`: `{"sat_pin":true,
  "unsat_two":false}` matches bootstrap on the enum bug shape.
- **Item 4** — `scripts/evident-self` routes `sample` to the wrapper
  under `EVIDENT_SELF_VIA_SMT2=1`.

## The unblock (a future wave)

1. **Fold the recompile wall** — build wave-4i Option 1: lex once,
   sat-check every claim in one kernel run (amortise the ~190×).
2. **Grow the membership walk** to multi-name decls, chained bounds/`≠`,
   `⇒`-guarded lines, and bare `ClaimName` composition — the shapes the
   lang corpus actually uses. Each is a `compiler/parse_body*.ev`
   extension with a `tests/kernel/` fixture, same as Item 1.

Until both land, the lang phase cannot run green on `kernel +
compiler.smt2`, and the cutover cannot make `./test.sh` green.
