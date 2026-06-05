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

Next wave should target the multi-line enum variant class first (single fix, 9 wins). Then record-lit (3 wins) and composition+chain (3 wins). After ~95% pass, cutover is mechanical.
