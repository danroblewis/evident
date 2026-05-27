# Plan — gap-closing: bring both SMT-LIB engines to full contract parity

**Mission.** Close the 4 documented boundaries in `runtime-contract/MATRIX.md`
that the SMT-LIB engines (strategy-1 `runtime-smt/` + strategy-2 SMT-LIB path)
don't yet cover, so both reach FULL parity with the Evident/effect-run path:
1. **async event sources** (timer/stdin/signal as world writes)
2. **`last_results` threading** across ticks (effect → result feedback)
3. **FFI effects** (LibCall/FFICall beyond Println/Exit)
4. **mode-2 dispatch** (toposort tie-break / ordered-effect chains)

For each: add `runtime-contract/` fixtures capturing the behavior (from the
Evident path), then make BOTH engines pass them. Update `MATRIX.md` — turn the
documented gaps into ✓ where closed, keep honest where genuinely out of scope.

## Orchestration protocol
One PHASE per gap (P1–P4). Within a phase, parallel subagents: (a) capture the
behavior as new contract fixtures from the Evident path, (b) implement it in
`runtime-smt/`, (c) implement it in the strategy-2 SMT-LIB path. Integrate, run
the contract matrix for that behavior against both engines, gate (`./test.sh`
green + new fixtures pass on both), checkpoint-commit, proceed. Order by leverage:
last_results threading (P1, smallest), FFI effects (P2), async sources (P3,
needs the awaiter — see docs/design/event-sources-as-evident.md), mode-2 (P4).
P5: regenerate `MATRIX.md`, final ./test.sh ×2, push.

## Honest notes
- Some gaps (async sources) may need the generic awaiter from
  `event-sources-as-evident.md` — if a gap is genuinely a bigger sub-project,
  close what's tractable, document the rest honestly in MATRIX.md, don't fake.
- Additive; the Evident path + `./test.sh` stay green. Push-only; don't merge.
