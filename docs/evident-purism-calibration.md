# Evident purism — calibration appendix

The `evident-critic` procedure (`.claude/skills/evident-critic/SKILL.md`)
run manually against historical code, pinning expected verdicts. A
critic whose judgment diverges from these on analogous cases is
miscalibrated; the calibration wins. Rulebook references are to
`docs/evident-purism.md` (§ numbers).

## Case 1 — the reverted tuple-bind commit (must flag: BLOCKER)

Target: `git show d1be22a -- compiler2/driver_enum.ev` (landed
2026-06-09 with byte-identical flatten output and green gates;
reverted as invalid grammar in `2b0efb2` four minutes later).

| file:line | severity | rule | suggested rewrite |
|-----------|----------|------|-------------------|
| driver_enum.ev:142 | BLOCKER | §4 V1 invented grammar (tuple-bind over a plain Seq, §2.5); fails §5 test 2 — `efs` does not contain (index, element) tuples | `∀ k ∈ {0..2} : VariantFieldType(v ↦ ed_v, idx ↦ k, ty ↦ efs[k].ty)` — the index range survives: the position is wired into the claim (`idx ↦ k`), §3.1 |
| driver_enum.ev:335 | BLOCKER | §4 V1, same form | `∀ k ∈ {0..5} : uev[k].name = (… (uev_cap_d ∧ _ed_vidx = k) ? … : _uev[k].name)` — allocation is the honest positional op (§3.2); the cursor match `_ed_vidx = k` needs the position |
| driver_enum.ev:336 | BLOCKER | §4 V1 | as :335 (`.ctor`) |
| driver_enum.ev:337 | BLOCKER | §4 V1 | as :335 (`.tester`) |
| driver_enum.ev:349 | BLOCKER | §4 V1 | as :335 (`.acc`, cursor `_uev_acc_pend = k + 1`) |

`VIOLATIONS: 5 BLOCKER / 0 WARN / 0 NOTE`
`requires operator ruling: ∀ (k, e) ∈ xs indexed-family binder`
(ruling already given: rejected, `2b0efb2`).

The point this case pins: the commit's own message proves the
laundering — "flatten output … is byte-identical, so behavior is
unchanged by construction." Behavior-identical, gate-green, and still
not Evident. Inadmissible evidence is inadmissible.

## Case 2 — a chain-heavy current file (must WARN on value chains, must NOT flag hold chains)

Target: `compiler2/driver_matchpin.ev` (current worktree state).

Flagged:

| file:line | severity | rule | suggested rewrite |
|-----------|----------|------|-------------------|
| driver_matchpin.ev:190–192 | WARN | §4 V9 value-selection chain (`fold_ctor1` tested against successive literal keys to select a handle) | Fold the IntResult/StringResult floor entries into the same registry the pin pair at :188–189 already reads, so one keyed projection covers floor + user; until a registry can hold the floor handles, the chain stands as tolerated debt — fix the lowering/registry, don't re-spell the chain (§1.5) |
| driver_matchpin.ev:196–198 | WARN | §4 V9 | as above (`fold_tester2`) |
| driver_matchpin.ev:202–204 | WARN | §4 V9 | as above (`fold_acc1`) |
| driver_matchpin.ev:208–210 | WARN | §4 V9 | as above (`fold_acc2`) |
| driver_matchpin.ev:214–216 | WARN | §4 V9 | as above (`fold_def_acc`) |
| driver_matchpin.ev:221–223 | WARN | §4 V9 (case-code dispatch: `fold_arm_n = 0 ? … : fold_arm_n = 1 ? … : …`) | An arm-count enum + `match` (§2.6) is the set-theoretic surface for case dispatch over a code |

Correctly NOT flagged (the blessed idioms this case exists to pin):

- **:110–175 — the carried-write hold chains** (`match_st`,
  `match_name`, … `arm2_expr`): `x = (is_first_tick ? init : event ?
  v : … : _x)`. This is the FSM transition relation in
  prioritized-guard form — the single covering write of a carried
  field, terminating in the `_x` hold (§3.4 blessed). Nothing is being
  looked up; there is no pin form to prefer.
- **:177–186 — single conditionals and capture-or-carry views**
  (`fold_ctor1 = (_tested_arms ≥ 1 ? _arm1_ctor : _pend_ctor)`): one
  test is an ite, not a chain (§3.4).
- **:188–189 (and the parallel pairs through :213) — keyed-projection
  pin pairs** over `user_variants`: the exemplary §2.5 registry read,
  living in the same file as the chains it should replace.
- **`≠` comparisons (`fold_def_bind ≠ ""`, etc.)**: perf concern,
  explicitly out of critic scope (header rule) — never flagged.
- **:39 — the `(ite ((_ is C1) scrut) b1 …)` text inside the module
  comment**: judged an allowed class-3 cross-file contract (it
  documents the lowered shape against `translate2_match.ev`), not a
  banned code-example-in-prose; it is SMT-shaped, not Evident-shaped,
  so the flattened-source false-positive risk the ban exists for does
  not apply.

`VIOLATIONS: 0 BLOCKER / 6 WARN / 0 NOTE`

## Case 3 — known-good conformance sources (must pass CLEAN)

Targets: `tests/conformance/features/094-bare-unconditional-sat/source.ev`
and `tests/conformance/features/140-bare-mention-independent-internals/source.ev`
(at main).

- 094: `claim` for a pure predicate (`IsPositive`) — correct keyword
  (§2.1); chained membership `n ∈ Nat`, pin `n = 5` (§2.3); bare
  mention composing a component (§2.2); effects literal with `Exit(0)`
  (§2.8). No findings.
- 140: two bare mentions (`wants_big`, `wants_small`) whose `secret`
  internals are independent per call site — the exact semantics
  conformance 140 pins (§2.2); `n ∈ Int = 1`; `Exit(n - 1)`. No
  findings. (Enum constructors in expression position are
  constructors, not function calls — §2.6, §1.1 not triggered.)

`CLEAN` (both files)

## Coverage of the required calibration properties

- **True positive:** Case 1, `∀ (k, e) ∈ xs` → BLOCKER (invented
  grammar, transform-laundered).
- **True negative:** Case 3, 094 + 140 → CLEAN.
- **Correctly-NOT-flagged idiom:** Case 2, the carried-write hold
  chains (`is_first_tick`-rooted covering writes ending in `_x`) — the
  blessed §3.4 exception, distinguished from the six value-selection
  chains in the same file that DO warrant WARN.
