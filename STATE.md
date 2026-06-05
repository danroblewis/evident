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

**THE single ctor-arg blocker — now confirmed to wipe the kernel phase too**

`compiler/translate_ctor.ev`'s `RenderExprL0` only handles atomic args —
bare names, simple ints. Anything else inside a constructor call is
silently dropped:

| Source                              | Bootstrap                                | Self-host       |
| ----------------------------------- | ---------------------------------------- | --------------- |
| `Exit(3 + 4)`                       | `(Exit (+ 3 4))`                         | `(Exit 3)`      |
| `Exit(0)` literal int               | `(Exit 0)`                               | constraint dropped — output stops at `is_first_tick` decl |
| `LibCall("libc", "puts", ⟨…⟩)`     | full LibCall ctor + ArgStr seq           | constraint dropped |

Bootstrap on `tests/kernel/test_hello.ev` emits ~20 lines incl. the
full effects body + `(assert (= effects__len 2))`. Self-host on the
same file emits ONLY 11 lines, stops after `is_first_tick`. Kernel:
`var effects__len not in model` → exit 3.

**This means the kernel phase under seam is ~0% pass, not 100%.**
The "111 kernel tests green" line earlier in this file was always the
bootstrap path (the default `./test.sh --kernel`). Until translate_ctor
renders expression args, no real kernel-test fixture survives the seam.

### Cutover blockers in dependency order

1. **Fix `compiler/translate_ctor.ev`**: `RenderExprL0` must recurse
   into expression arguments — Int literal, String literal, `⟨…⟩` Seq
   literal, nested ctor call, arithmetic. The lang + conformance +
   kernel suites all need this.
2. Rebuild `compiler.smt2` from the fixed source.
3. Re-verify lang + conformance + kernel under seam (
   `EVIDENT_SELF_VIA_SMT2=1 bash test.sh --kernel|--lang|--conformance`).
   With the mem cap landed (`scripts/mem-cap.sh`, default 3 GB) and
   fanout dropped to 4 (was sysctl `hw.activecpu` ≈ 12), the host
   stays usable.
4. Switch `test.sh` to drop phases 1+2's bootstrap build and
   `scripts/evident-self` to drop the bootstrap branch.
5. `rm -rf bootstrap/`.

### Safety landings this session (kept on main)

- `scripts/mem-cap.sh` — polling watchdog that SIGKILLs any kernel
  child whose RSS exceeds `MEM_CAP_MB` (default 3000). macOS doesn't
  honor `ulimit -v` (RLIMIT_AS), hence the polling shim. Wired into
  the `EVIDENT_SELF_VIA_SMT2=1` seam wrapper.
- `scripts/run-{lang,kernel}-tests.sh` default parallelism: 4 (was
  sysctl `hw.activecpu` ≈ 12). Each kernel-on-compiler.smt2 child can
  briefly grow >3 GB; 12 in parallel swapped the host on this Mac.
- `tests/conformance/features/runner.sh` known-fails allowlist for
  `IMPL=selfhost` (16 entries today — every ctor-arg case).

When item 1 lands, the conformance allowlist + every lang ctor-arg
fail + nearly every kernel fixture close simultaneously. It is THE
remaining correctness blocker for the cutover.

### Why my session's mechanical-cutover attempt was wrong

I had been treating "lang phase 93.9% under seam" as evidence the
seam was nearly ready. It was not — the lang phase happens to use
mostly `unsat_*` tests where a dropped constraint still produces an
SMT-LIB program that solves (just to the wrong answer that the
wrapper then maps `unsat→sat=fail`). The kernel phase fails harder
because the dropped constraint leaves the program with no `effects`
array at all, which the kernel rejects at load time. Two phases, same
root cause, very different surface symptoms.

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
