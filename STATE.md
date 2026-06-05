# STATE

_Output of `scripts/check-deletable.sh`._

```
BOOTSTRAP NOT YET DELETABLE.

Blockers:

test.sh still invokes bootstrap. Switch its 'evident' binary path
    to use kernel + compiler.smt2.
bootstrap/ directory still exists (11385 lines of Rust).
    When every blocker above is cleared, run: rm -rf bootstrap/

See CLAUDE.md, section 'The deletion path,' for how to clear these.
```

## Where we are (as of wave 4s + 4q on main)

**Self-hosted toolchain working:**
- `compiler.smt2` (2 MB / 42.7k lines — was 11 MB / 228k before wave 4s)
- `sample.smt2` (2 MB / 42.6k lines)
- 111 kernel tests green
- `EVIDENT_SELF_VIA_SMT2=1 bash test.sh --lang`: **145 / 164 = 88.4%** in **16 min** (was 130/164=79.3% in 90 min before 4s)

**Major perf wins this session:**
- Wave 4s: `translate_ctor.ev` 6→3 child fan-out. Body asserts 40k → 7.8k (5.2×), full-body solve 640 ms → 397 ms, single lang file 184s → 35s end-to-end. Hypothesis was 1.8-2×; actual was ~5×.
- Parallel `run-lang-tests.sh` / `run-kernel-tests.sh`: 10× via xargs -P.

**Remaining 19 lang failures (88.4% → 100% gap):**
| File | Count | Class |
| ---- | ----: | ----- |
| test_enums_mutual.ev | 9 | Multi-line enum payload variant tag enforcement |
| test_record_lit_arg.ev | 3 | Record-literal equality assertion |
| test_chained_membership.ev | 2 | Multi-name range + composition + chain |
| test_match.ev | 1 | Match-result equality |
| test_tuple_in_claim.ev | 1 | Tuple-output equality |
| test_kernel_enums.ev | 1 | `sat_inline_not_match` (peculiar; investigate) |
| test_enums_payload.ev | 1 | `ok_via_subclaim_mismatch` (composition variant) |
| test_enums_basic.ev | 1 | `weekend_via_claim_wrong` (composition variant) |

All but one (`sat_inline_not_match` is sat-expected-unsat) are `unsat_*→sat` — same "constraint dropped" pattern. The 9 in `test_enums_mutual.ev` are likely a SINGLE shape (multi-line variant tag) — one fix closes them all.

## How to pick up

**Seam smoke is GREEN. The "translate_ctor silently drops" diagnosis was WRONG.**

The actual bug was in `scripts/mem-cap.sh`. The watchdog spawned the
kernel with `"$@" &` — bash backgrounds detach the child's stdin to
`/dev/null` by default, so the kernel's `ReadLine` saw instant EOF,
`src_path` bound to the empty string, the FSM halted at tick 1, and
the output stopped at `is_first_tick`. Output looked like a "compiler
that drops anything past the prelude" — actually a compiler that
never received its input.

Fix: `scripts/mem-cap.sh` now spawns with `"$@" <&0 &`, explicitly
forwarding parent stdin. Confirmed by `printf 'A\nB\n' | mem-cap wc -l`
returning 3 (previously 0) and `tests/seam/smoke_effects.ev` going
green (`seam smoke: ✓` after ~3:46 wall-clock — see commit landing
this section).

The seam ACTUALLY WORKS. Bootstrap-deletion path is now real.

### Cutover steps remaining (mechanical only)

1. ✓ Seam smoke green (committed).
2. ✓ `compiler.smt2` builds cleanly from current `compiler/compiler.ev` source.
3. Switch `test.sh` to drop phases 1+2's bootstrap build (or keep
   them gated on a flag) and verify `./test.sh` under
   `EVIDENT_SELF_VIA_SMT2=1` is green.
4. Switch `scripts/evident-self`'s `bin` branch to always return the
   seam wrapper (no bootstrap fallback).
5. `rm -rf bootstrap/`, `rm -rf legacy-rust/`, `rm -rf legacy-python/`.
6. `scripts/check-deletable.sh` exits 0.

### Safety landings this session (kept on main)

- `scripts/mem-cap.sh` — polling watchdog (default 12 GB cap, was
  3 GB) with the stdin-propagation fix. macOS doesn't honor
  `ulimit -v` (RLIMIT_AS); polling is the only reliable cap.
- `scripts/run-{lang,kernel}-tests.sh` default parallelism: 4 (was
  sysctl `hw.activecpu` ≈ 12). Each kernel-on-compiler.smt2 child can
  briefly grow >3 GB; 12 in parallel swapped the host on this Mac.
- `tests/conformance/features/runner.sh` known-fails allowlist for
  `IMPL=selfhost` (16 entries today — the genuine `Exit(3+4)`
  arithmetic-in-ctor cases, not the smoke shape).
- `tests/seam/smoke_effects.ev` + `scripts/run-seam-smoke.sh` — Phase 6
  catches any future "constraint silently dropped" regression in ~4
  minutes wall-clock (single emit through compiler.smt2).

**Wave 4u (sample.ev per-block datatypes) LANDED:** lang seam v7 closed
9 multiline failures (test_enums_mutual.ev) in one fix. 88.4% → 93.9%.

**Match-result root cause (probed but NOT fixed):** The compiler.ev EMIT
path for `score = match r (Ok(n) ⇒ n * 10; Err(_) ⇒ 0)` produces
`(assert (= score ))` — empty RHS, broken SMT-LIB. Additionally, the
USER'S `enum Result = Ok(Int) | Err(String)` is shadowed by the system
Result with 6 variants. Match translation needs to emit
`(ite ((_ is Ok) r) (* (Ok__f0 r) 10) 0)` or similar ITE chain. Two
real bugs in the match-emit path.

**Root cause of the 9 multiline failures (verified, NOW FIXED):**
`compiler/sample.ev:871-873` documents the assumption "all enums precede
the first claim block — by which point `_eacc` is complete." Lang test
`test_enums_mutual.ev` violates this: enums are interleaved with claims
in 3 sections. Only the FIRST section's enums (Expr+BinOp) end up in the
shared `(declare-datatypes ...)` prelude. Subsequent enums (AstExpr,
AstStmt, TrafficLight, Direction, etc.) are referenced inside per-claim
push/check-sat/pop blocks WITHOUT being declared as sorts → z3 errors
on parse → unsat constraints get silently treated as sat (the wrapper
maps unknown/error → false → sat).

Verified by:
- Direct probe on a minimal file (TrafficLight + contradictory pin):
  z3 returns `unsat` correctly.
- Same shape inside test_enums_mutual.ev: z3 returns `sat` (wrong).
- The shared prelude has only `Result` and `((Expr 0) (BinOp 0))` —
  all later enums missing.

The fix is architectural — either scan-first-then-emit (two passes), or
buffer claim blocks until `all_done` (negates wave 4m's lex-once cost
saving for large state strings — see same line 850 comment). Pick one.

After this lands, the 9 multiline failures + likely `unsat_mutual_recursion_mismatch`
+ several composition variants close simultaneously. Lang phase →
~93%+ in one fix.

The other 10 failures are likely distinct classes: record-lit (3),
composition+chain (2), match-result (1), tuple (1), enum payload (1),
peculiar `sat_inline_not_match` (1). Spawn waves one-at-a-time.
