# expr_as_var port — findings and block (2026-06-07)

Task: port bootstrap's `expr_as_var` compound-expression slot binding
(commit `c817c6c`) into the self-hosted compiler source so that

```evident
AcceptBool(p ↦ (x > 3), out ↦ result)
```

resolves in the sat-check path, proven by rebuilding `sample.smt2`
from `compiler/sample.ev` and running
`tests/lang_tests/test_expr_slot_binding.ev` (all `sat_*` sat, all
`unsat_*` unsat).

**Status: the port is IMPLEMENTED in source (uncommitted, see §2) but
CANNOT be proven, because rebuilding `sample.smt2` from source is
blocked — not by the port, but by two pre-existing walls (§4). Per the
session's honesty rule, nothing was committed.**

## 1. What the verdicts actually are today

`scripts/sample-via-smt2.sh tests/lang_tests/test_expr_slot_binding.ev
--all` against the COMMITTED `sample.smt2`:

| claim | expected | committed sample.smt2 |
|---|---|---|
| sat_pass_true_comparison | sat | sat (vacuous) |
| unsat_pass_true_comparison_wrong_witness | unsat | **sat — wrong** |
| sat_pass_false_comparison | sat | sat (vacuous) |
| sat_pass_conjunction | sat | sat (vacuous) |
| unsat_pass_conjunction_witness_wrong | unsat | **sat — wrong** |
| sat_pass_int_sum | sat | sat (vacuous) |
| unsat_pass_int_sum_wrong_witness | unsat | **sat — wrong** |
| sat_pass_int_difference / negation / nested_arith / ternary / ternary_negative_branch | sat | sat (vacuous) |

Note: the task briefing assumed the committed `sample.smt2` HAS the
capability ("baked in from deleted bootstrap Rust"). It does not —
and `c817c6c`'s own commit message says so explicitly: *"Seam path
(kernel + sample.smt2) still shows sat for the unsat tests … the fix
lives in bootstrap's Rust expr_as_var … compiler/sample.ev catching
up is a follow-on."* The capability only ever existed in bootstrap's
direct `query` path. Confirmed three ways:

- the verdict table above (3 wrong unsat verdicts);
- `sample.smt2`'s declared symbol set is exactly the current
  `compiler/sample.ev` + imports (no `cca_*`/inline-table machinery);
- `compiler.smt2`'s inliner asserts decode to exactly the current
  `compiler/compiler.ev` lone-name inliner (`r_t1_blocks` includes
  `LParen`, so `Name(…)` lines never inline).

Deeper than the briefing assumed: the artifacts have NO `slot ↦ value`
call inlining AT ALL (not even for plain names). `compiler.ev`'s
pmode-4 inliner covers ONLY the bare lone-name (names-match) form;
`sample.ev` has no inliner whatsoever. The `sat_*` claims above pass
vacuously because the walk hits the call line, `MembershipStep`'s
`ms_is_lone` eats one token, the next tick fails, `claim_done` fires
and the REST of the claim body (including the `result = …` witness
pin) is junk-drained. That is also why the composition entries in
`run-lang-tests.sh`'s `DEFAULT_KNOWN_FAILS` exist.

## 2. The port (implemented, in working tree)

Three changes, all in Evident source:

1. **`compiler/translate_scalar_expr.ev` (new)** — depth-unrolled
   scalar expression renderer over the lexer TokenList, sibling of
   `RenderExprL0..L3`: `SAtomR` (leaf), `SOpSmt` (operator →
   SMT-LIB spelling), `SPrim0/1/2` (`[¬] (atom | "(" chain ")")`),
   `SChain0/1` (binop / bool-split / ternary chains of up to four
   primaries). `SPrim2` covers every shape in the acceptance test:
   `(x > 3)`, `(x > 0 ∧ y > 0)`, `(a + b)`, `(¬(x > 10))`,
   `(a * (b + 1))`, `(x > 0 ? x : (0 - x))`. Sort-agnostic SMT text
   means bootstrap's "try bool, then int/real/string" collapses to
   one shape-driven renderer with the same first-success semantics.

2. **`compiler/parse_body_call.ev` (new)** — `CallArgsStep` parses
   `Name(slot ↦ value, …)` (≤3 bindings) off the walk head, rendering
   each value via `SPrim2`; emits a substitution table
   `|slot#rendered;…` plus the callee name and the post-`)` tail.
   `ok=false` for anything it can't render (record-literal args,
   positional calls, tuple-∈ forms keep their current behaviour).
   `SlotSubst` is the table lookup.

3. **`compiler/sample.ev` (driver)** — composes `CallArgsStep`;
   `do_ccall` preempts the lone-name path; a pmode-4 inline machine
   (adapted from `compiler.ev`'s, plus an `iph=5` flat param-list
   skip and per-token `SlotSubst` substitution during transfer)
   splices the callee's body into `rem`. A substituted body line
   `out = p` becomes `Ident("result") Eq Ident("(> x 3)")`, which the
   existing bare-assert path renders as
   `(assert (= result (> x 3)))` — Ident payloads pass through the
   atom renderers verbatim, so no other pass needed changes.

Hand-traced end-to-end for all 14 claims in the acceptance test: the
12 expression-binding claims produce exactly the bootstrap-equivalent
constraints; `sat_pass_ternary_negative_branch` keeps its current
(vacuously correct) verdict because `x ∈ Int = (0 - 7)` is a separate
pre-existing membership gap.

## 3. Harness validation (baseline)

- Acceptance run against committed `sample.smt2`: works, wrong
  verdicts as expected (§1) — harness good.
- `scripts/build-sample-smt2-candidate.sh /tmp/sample_baseline.smt2`
  from UNMODIFIED source: **fails** —
  `kernel: tick limit (100000) reached` after ~50 min wall
  (`[functionizer] 7852 total / 810 JIT / 6450 interp / 45 residual;
  1332372.4 ms`).

## 4. Why the rebuild is blocked (two independent walls)

**Wall A — the seam cannot compile composition lines.** Empirical:
a 259-line flat fixture with one `Helper(p ↦ true, out ↦ r)` call
compiled through `kernel + compiler.smt2` in 36 s; the emitted .smt2
contains `x`/`r` declares but NO trace of the call and NOTHING after
it — `effects = ⟨Exit(0)⟩` on the NEXT line vanished too
(`max-effects = 0`). The walk's `claim_done` junk-drain ate the rest
of the body. Flattened `compiler/sample.ev` contains ~370 composition call lines
(53 in the driver alone); a "rebuilt" sample.smt2 would be a stub
truncated at main's first composition (~40 lines in). The fix for Wall A is the same machinery as this port, but in
`compiler.smt2` — which is the frozen artifact (chicken-and-egg; the
wave-5 plans are the documented exit, see CLAUDE.md "rebuilding them
is blocked on the wave-5 plan").

**Wall B — kernel tick limit.** `kernel/src/tick.rs` hardcodes
`TICK_LIMIT: usize = 100_000` (no env override; kernel is
do-not-edit). Flat `compiler/sample.ev` is ~26.5 k tokens; LEX +
REVERSE + PARSE/skip is ≥ ~3 ticks/token ≈ 80–100 k ticks — the
unmodified-source build died at exactly the limit. Even a
composition-capable compiler.smt2 would need either a higher limit
or fewer ticks/token to recompile the sample driver.

Wall A also explains the kernel driver fixtures
(`tests/kernel/test_compiler_driver_*.ev` compose `MembershipStep`
etc. with `↦` bindings): they could only ever have passed under
bootstrap emit, not under the post-cutover seam. Corroborated by the
full `./test.sh` run in flight during this session
(`/tmp/full_test2.log`): the IMPL=selfhost conformance phase shows
dozens of emit-gap failures (`smt2 missing: (+ 3 4)`,
`smt2 missing: (and`, exit-code mismatches, …) — the seam currently
drops far more than expression slot bindings (several of these are
the known `RenderExprL0` ctor-arg gap already tracked in STATE.md).

## 5. What would unblock the proof

In order of plausibility:

1. **Wave-5d AOT path** (docs/plans/wave-5d-aot-binary-cache.md) —
   the project's own answer to "rebuild the committed artifacts".
2. A one-time, clearly-labeled regeneration of `compiler.smt2` with
   the call-inliner + this port included, via whatever tool next
   gains full-language emit (same wave-5 dependency).
3. Kernel tick-limit raise (kernel edit — currently off-limits) plus
   the Wall-A fix.

The source port in §2 is forward-correct for any of these: it is the
same walk/discriminator/renderer style as the existing waves, and the
acceptance test is already in the corpus to gate it.

Follow-on once provable: mirror the same ~130-line driver patch into
`compiler/compiler.ev` (the emit driver — it has the lone-name
inliner but the same `slot ↦ value` gap), reusing
`compiler/parse_body_call.ev` + `compiler/translate_scalar_expr.ev`
unchanged.

## 6. Artifacts of this investigation

- `/tmp/mini_ccall.ev` (+ flat/out) — Wall-A reproducer.
- Baseline build log: tick-limit failure (task output, §3).
- Symbol-set diffs: `sample.smt2` top-level names == current source.
