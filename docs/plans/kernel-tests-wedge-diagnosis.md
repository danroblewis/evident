# Phase 5 kernel-tests "wedge" — diagnosis (2026-06-07)

## Symptom

`./test.sh` phase 5 (`scripts/run-kernel-tests.sh` over
`tests/kernel/*.ev`) started at 13:42 and produced **zero log
output for 3+ hours** before the operator killed it. Phases 1–4
behaved (conformance and lang_tests completed, with their own
mass failures — see "Root regression" below).

## Verdict

**Not a hang. An invisible grind — and the kill didn't take.**
Three compounding mechanisms, all evidence-backed:

1. The regenerated `compiler.smt2` makes every
   `test_compiler_driver_*` fixture's *emit* step spin to the
   kernel's 100,000-tick limit at ~14–16 ms/tick ≈ **25–31 minutes
   per fixture** (and even "fast" fixtures take minutes or fail
   on broken output).
2. `scripts/run-kernel-tests.sh` has **no timeout of any kind** —
   no per-fixture `timeout`, no phase deadline. The only backstops
   are the kernel's own 100k-tick limit and mem-cap's 12 GB RSS
   cap (which never triggered: grinder RSS is ~100–500 MB).
3. The script **buffers all progress output** into a mktemp file
   and only `cat`s it after the entire 119-fixture sweep finishes
   (`run-kernel-tests.sh` lines 200–205:
   `xargs -P 4 … > "$output_file"` … `cat "$output_file"`). So a
   sweep making slow progress is *indistinguishable in the log*
   from a dead one.

Bonus finding: **the "killed" run survived its kill.** At 16:53 —
more than two hours after the operator's kill — the sweep's
`xargs -P 4` (PID 22854, started 13:42) and four in-flight
fixture pipelines were still alive and still appending to the
buffered results file. Killing the foreground `test.sh | tee`
does not reap the `xargs` job tree. (The orphan tree — xargs at
3h14m elapsed plus four in-flight fixture pipelines
(`multiname`, `mvp`, `multi_toplevel` at 24 min,
`multiline_payload_tag` at 18 min) — was killed at ~17:00 during
this investigation.)

## Forensic evidence

### The buffered results file of the wedged run

`/tmp/evident_kernel_results.7oFLXs` — this is the run's
`output_file` mktemp; it had 26 completed fixture results at
16:39 (27 by 16:53; **all ✗**). Two failure classes:

**Class A — emit grinds to the tick limit (~27 min each), 11 of
the first 26:**

```
✗ test_compiler_driver_canonical_comment.ev: emit failed:
  [functionizer] 7852 total / 810 JIT / 6450 interp / 45 residual;
  1665565.0 ms total (1114104.0 ms func / 0.0 ms z3)
  kernel: tick limit (100000) reached
```

Identical lines (1,531,826–1,861,074 ms total ≈ 25.5–31 min) for:
`canonical_enum`, `canonical_match`, `canonical_seq`,
`claim_select_by_name`, `claim_selection`, `ctor_app`, `ctor_l3`,
`effect_array`, `effect_seq`, `enum_seq_payload`.

**Class B — emit "succeeds" but the artifact is broken (fails in
seconds-to-minutes at the kernel-run step):**

- Empty emit → `kernel: var `effects__len` not in model`
  (`arith`, `bool_ops`, `chain_bound`, `comparisons`, `enum`,
  `eq_assertion`, `implication`, `match`, `multi_member`, …)
- Malformed emit → Z3 parse errors in the *emitted* smt2:
  `unknown constant ite (Bool WorkList)` (`test_ast_to_text`),
  `unknown constant ite (Bool ExprList)` (`test_ast_walker`),
  `invalid function application, arguments missing`
  (`eq_ctor`, `eq_matches`),
  `unknown constant TLCons (Token)` (`eq_ternary`),
  datatype-parameter error (`test_comment_lexer`).

### Live process table at 16:53 (2h+ after the "kill")

```
22854  xargs -P 4 -I{} bash -c run_one …          (started 13:42)
43654  kernel compiler.smt2  86% CPU  36:34 CPU-min   ← emit of test_compiler_driver_matches.ev (since 16:12)
87296  kernel compiler.smt2  73% CPU  16:09 CPU-min   ← emit of test_compiler_driver_multi_toplevel.ev (since 16:33)
…      + multiline_payload_tag (16:39), multiname (16:53)
```

### Controlled reproduction (same wrapper, same env, timeout-wrapped)

Run exactly as `run_one` does (`scripts/evident-self bin` wrapper →
flatten | mem-cap kernel compiler.smt2), with `timeout 300` and
`EVIDENT_PHASE_TRACE=1`:

**Fixture #1 in sorted order, `test_ast_to_text.ev`** — emit alone
was still ticking at tick 21,000 when the 300 s timeout killed it
(rc=124, ~14 ms/tick). (In the wedged run its emit did eventually
finish — then the kernel-run step failed on the malformed output
above.) Trace signature:

```
[phase +0.1s] body parsed: 7851 asserts
[phase +1.8s] functionize done (functionized: true)
[phase +1.8s] tick 0
[phase +3.1s] tick 100
…ticks advance forever at ~14 ms/tick…
```

The time is all in the compiler FSM's tick loop — the kernel is
healthy; the *program* (compiler.smt2 compiling the fixture) never
reaches its Exit effect.

**First Class-A grinder, `test_compiler_driver_canonical_comment.ev`
(fixture #7)** — with `EVIDENT_TICK_LIMIT=3000` to cap the wait:

```
emit rc=3
[functionizer] 7852 total / 810 JIT / 6450 interp / 2 residual; 40980.9 ms total
kernel: tick limit (3000) reached
smt2 bytes: 0
```

~14 ms/tick × 100,000 ticks = ~23–27 min — matching the wedged
run's recorded 1.5–1.9 M ms per Class-A fixture exactly.

### Arithmetic of the "wedge"

26 fixtures completed in ~177 min (13:42 → 16:39) at `-P 4`.
119 fixtures total, 38 of them `test_compiler_driver_*` (the
Class-A-heavy family), plus lexer-class fixtures whose emits also
grind 10k+ ticks. Projected remaining runtime: **roughly 8–12 more
hours** — after which the phase would have printed everything at
once and reported 119 failures. The sweep was never deadlocked;
it was just unwatchably slow and silent by construction.

## Root regression (why fixtures grind at all)

`compiler.smt2` was regenerated this morning
(`a085f93 compiler.smt2: regenerate via the bootstrap oracle from
current source`, 10:32), alongside kernel functionizer changes
(`c8e7d9b functionizer: cover compiler.smt2`,
`cdfff1f functionizer: capture bare Bool literal pins…`,
`0b181c5 kernel: EVIDENT_TICK_LIMIT override`). The regenerated
artifact is globally broken, not just slow:

- Conformance (phase 3): **124/138 failed** — emitted smt2 missing
  basic forms (`(+ 3 4)`, `str.++`, …), empty emits, Z3 parse
  errors.
- lang_tests (phase 4): 2 unexpected failures.
- Phase 5 merely amplifies the same regression into hours, because
  each kernel-fixture emit is a full kernel+compiler.smt2 run with
  a 100k-tick ceiling instead of a quick wrong answer.

The compiler FSM's non-termination (Class A) and truncated/
malformed output (Class B) are two faces of the same artifact
regression. Note `tick.rs`'s own comment dates the 100k-tick
ceiling problem to today: "established 2026-06-07 — the
expr_as_var port's baseline build died here at ~50 min".

## Ruled out

- **The historical mem-cap stdin bug** (backgrounded child gets
  /dev/null → ReadLine EOF): `scripts/mem-cap.sh` line 31 forwards
  stdin explicitly (`"$@" <&0 &`), and the emit wrapper pipes its
  two stdin lines (`printf '%s\n%s\n' "$FLAT" "$CLAIM" | …`).
  Phase traces show ReadFile/effects flowing; no EOF stall.
- **mem-cap kills**: zero `mem-cap: killed` lines; grinder RSS
  ~100–520 MB vs the 12 GB cap.
- **stdin-reading fixtures** (`test_echo_lines.ev`,
  `test_sample_driver_marker_count.ev`): they get explicit stdin
  from `setup_fixture`, and neither was reached.
- **xargs/pipe deadlock**: the results file kept growing; workers
  were dispatching new fixtures all along.

## Recommended fix (for the follow-up implementation agent)

In priority order:

1. **Per-fixture wall-clock timeout in `run-kernel-tests.sh`**
   (`scripts/`-tree change, transition-only growth is allowed).
   Wrap both the emit and the kernel-run step in `run_one` with
   `timeout "${EVIDENT_KERNEL_TEST_TIMEOUT:-120}"` and print a
   distinct `✗ <name>: timeout after Ns (emit|run)` line. A wall
   timeout is the right lever — a *tick*-count cap is wrong here
   because legitimate compiler emits take tens of thousands of
   ticks; 120 s at healthy speeds is generous, while a Class-A
   grinder dies in 2 min instead of 27. Worst case the full sweep
   is then bounded at ~119 × 240 s / 4 ≈ 2 h even with everything
   broken.
2. **Stream progress instead of buffering.** Replace
   `xargs … > "$output_file"` + final `cat` with `xargs … | tee
   "$output_file"` so each ✓/✗ line is visible the moment a
   fixture finishes (test.sh already tees phase 5 to
   `/tmp/evident-kernel.log`; today that tee received nothing for
   hours). Keep the file for the final ✗ count.
3. **Fix the actual regression** — the oracle-regenerated
   `compiler.smt2` (a085f93) and/or its interaction with the
   functionizer changes. That is a separate, bigger work item;
   phase 5 results are meaningless until conformance is green
   again. Cheap interim gate: have test.sh skip phase 5 (or mark
   it blocked) when phase 3's failure count exceeds a threshold,
   so a broken artifact fails fast at the cheap phase instead of
   burning 12 h at the expensive one.
4. **Optional belt-and-braces:** export a lower
   `EVIDENT_TICK_LIMIT` (e.g. 30,000) for phase-5 *emit* calls
   only, once a healthy emit's tick budget is measured — but only
   as a secondary backstop behind the wall timeout, for the same
   "legitimate emits are tick-hungry" reason.
5. **Process-tree hygiene:** when test.sh is interrupted, the
   `xargs` job tree survives (observed alive 2h+ after the kill,
   still burning ~3 cores). A `trap` in `run-kernel-tests.sh` that
   kills its own process group (or `xargs --process-slot-var` +
   explicit reaping) would make a Ctrl-C/kill actually stop the
   sweep.

## Repro commands (for the implementer)

```bash
EV=$(scripts/evident-self bin)
# Class-A grinder, capped so it fails in ~45 s instead of ~27 min:
EVIDENT_TICK_LIMIT=3000 EVIDENT_PHASE_TRACE=1 \
  "$EV" emit tests/kernel/test_compiler_driver_canonical_comment.ev \
  compiler_driver_canonical_comment -o /tmp/out.smt2
# → exit 3, "kernel: tick limit (3000) reached", 0-byte out.smt2
```
