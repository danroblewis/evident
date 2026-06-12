# Porting lower-bounded-seq.sh to Evident — continuation plan

Scaffold + simplest rules landed (this session). The Evident port lives in
`scripts/passes/lowerseq_{scan,plan,emit,lib}.ev`, compiled to committed
`.smt2` artifacts by `scripts/passes/build-lowerseq.sh`, gated byte-identical
against the awk reference by `scripts/passes/lowerseq-equiv.sh`.

## Architecture (mirrors the autocarry port)

```
source ──▶ lowerseq_scan ──records──▶ lowerseq_plan ──registry line──▶ lowerseq_emit ──▶ lowered source
              (one fwd pass)            (resolve gbnd/hasLen)     (+ source again)
```

- `lowerseq_scan` — one forward pass, emits tagged registry records
  (`K`/`T`/`Q`/`D`/`B`/`L`). Reuses `autocarry_lib` scan helpers
  (`AcWsSkip`, `AcWordEnd`, `AcCommentStep`) wholesale.
- `lowerseq_plan` — accumulates records, then (EOF→phase 13) resolves the
  global registry: a name registers iff its decl base is **scalar**
  (Int/String/Bool) AND a `#name ≤ N` bound shares the decl's claim
  (the R0 opt-in gate). Emits ONE line: `⟦name⟧base⦂N⦂hasLen…`.
- `lowerseq_emit` — reads the registry (line 1) then the source; lowers
  decl/dual/literal/hold/bound lines via a per-slot emit sub-loop
  (phase 4, one line per tick).
- `lowerseq_lib` — `LsCommaPos` / `LsCountElem` / `LsNthElem` / `LsTrimL` /
  `LsTrimR` (flat comma split for literal payloads; `AcParseInt` from
  autocarry_lib parses N).

## Ported + byte-identical (15 fixtures green)

- **R0** opt-in gate (scalar base + same-claim `#name ≤ N`; global
  registration; unbounded Seqs pass through verbatim).
- **R1** scalar decl → `xs_0..xs_{N-1} ∈ El` (+ `xs_len ∈ Int` / `0 ≤ xs_len`
  only when hasLen).
- **R5** scalar literal (`xs ∈ Seq(El) = ⟨…⟩` and `xs = ⟨…⟩`) → per-slot
  pins + `xs_len = ne`; Int/String/Bool elements; empty literal; zero-default
  fill for unfilled slots.
- **R7** hold (`xs = (is_first_tick ? ⟨⟩ : _xs)`) → per-slot holds + len
  hold; dual decl `_xs ∈ Seq(El)` → `_xs_k` (+ `_xs_len`); the `#xs ≤ N`
  bound directive: vanishes when ¬hasLen, card-rewrites to `xs_len ≤ N`
  when hasLen.
- **R16** literal index `xs[lit]` → `xs_lit` (anywhere, comment-INCLUSIVE);
  literal-arith fold `xs[2*3 + 1]` → `xs_7` (`+ - *`, `*` binds tighter,
  left-assoc sum — `LsIdxEval` matches awk `idx_eval`); `.field` and literal
  `[sub]`-index (`xs[1].bar[2]` → `xs_1_bar_2`).
- **R18** card `#xs` → `xs_len` (anywhere, comment-INCLUSIVE).

R16/R18 are implemented as a phase-5 single left-to-right token walk over the
RAW default-path line, one token-unit per tick (the two token shapes `#name` /
`name[` are disjoint, so one combined pass mirrors awk's `subst_index ∘
subst_card`). A clean default line (no `#` or `[`) fast-paths verbatim. New lib
helpers: `LsIdxEval` / `LsStripWs` / `LsAllDigits` / `LsOnlyIdxChars` /
`LsDigitEnd`. Fixtures: `r16_index` / `r16_arith` / `r16_field` / `r16_comment`
/ `r18_card`.

- **subst_dyn (single-ident)** dynamic index `tok[idv]` over a registered
  seq → the covered select chain `(idv = 0 ? tok_0 : … : tok_{N-1})`
  (+ `.field` → `_field` per arm, + literal `[sub2]` left bracketed for the
  dynfam pass). Inline in the same phase-5 walk: when the `LsIdxEval` literal
  fold fails but the inner is a single ident (`LsIsIdent`), `LsDynChain` emits
  the chain (N ≤ 16). Fixtures `r17_dyn_index` / `r17_dyn_mixed`.

- **member (Int)** whole-line `lhs ∈ xs` over a registered Int seq → the
  len-guarded ∨-unroll `(((0 < xs_len) ∧ (lhs = xs_0)) ∨ …)`. Detected
  independently of the lead-token registry hit (the lead is the element var,
  not the seq); requires the whole line to be exactly `lhs ∈ xs`, rhs a
  registered **Int**-element seq, lhs a single token. New lib `LsMemberChain`.
  Fixture `member_int`. (String/Bool members are NOT lowered — matching awk's
  `elemOf[rhs]=="Int"` guard — they fall to awk's completeness refusal, a
  Tier 4 item the port does not yet reproduce.)

- **∀ over Int seq** `∀ x ∈ xs : P` → the len-guarded ∧-unroll
  `(((0 < xs_len) ⇒ (P[x→xs_0])) ∧ …)`. Implemented as emit **phase 6**:
  an outer slot cursor over N, an inner token-walk over the predicate doing
  the whole-token `bvar → sname_k` substitution (token-boundary aware, like
  the phase-5 walk; subst_tok ONLY, no index/card lowering inside P — matching
  awk's Int-∀ branch). Refs `sname_len` unconditionally. Fixtures
  `forall_int` / `forall_multitoken` / `forall_boundary`.

## NOT ported (continuation, priority order) — parity status: 20/20 byte-equiv

The gate (`scripts/passes/lowerseq-equiv.sh`) now compares 20 fixtures
byte-identically (was 16/16): the original 16 + `member_int`,
`forall_int`, `forall_multitoken`, `forall_boundary`, and (Tier-4
prerequisite landed this session) the harness was extended to diff
**stderr + exit** for any fixture marked `-- expect: flatten-error` — so
refusal fixtures can be ported faithfully once the refusals exist. No
refusal fixtures exist yet (the rules below that REFUSE are not ported).

The wire-in into `scripts/flatten-evident.sh` remains **DEFERRED**: the
port is not at full parity (the rules below — ∃, record-element, keyed
projection, pin family, enum/record refusals, completeness sweep — are not
ported), so swapping out the awk reference would silently drop behavior.

### Tier 1c — the Seq-field DYNFAM dynamic sub-index
The remaining `subst_dyn` shape: `xs_k_accs[j]` (a flattened Seq-typed record
field family) → its own per-subslot chain. Needs the record-element / Seq-field
registration (PASS0 below), so it folds into Tier 2.

### Tier 2 — record-element decls + the remaining `∀`/`∃` unrolls
  DONE this session: scalar `∀ x ∈ xs : P` (Int) and `y ∈ xs` member (Int).
  REMAINING:
  - record-`type` element decls (`xs ∈ Seq(R)` → `xs_k_fj ∈ Tj`; the
    `emit_field_decl`/`emit_field_hold` helpers, incl. Seq-typed type-body-bounded
    fields → per-subslot Int) — needs PASS0 record-type + enum scan
    (`tfield`/`ttype`/`fbound`/`enums`) threaded scan→plan→emit (THE big lift);
  - record element `∀ e ∈ xs : P` (uses `e.f` / `_e.f`) — needs PASS0;
  - `(∃ i ∈ {0..#xs-1} : P)` / `(∃ e ∈ xs : P)` (`subst_exists`). MEASURED
    2026-06-12: the awk `subst_exists` fires ONLY on a **parenthesized**
    `(∃ … : P)` group (it requires an opening paren immediately before `∃`);
    a BARE `∃ x ∈ xs : P` is NOT expanded and falls to awk's completeness
    refusal (exit 1) — so the supported surface is the parenthesized
    `flag = (∃ i ∈ {0..#xs-1} : xs[i] = v)` form. The index-form expansion
    substitutes the bvar to a **literal** k (`xs[i]`→`xs[0]`) and the
    EXISTING phase-5 index walk then lowers `xs[0]`→`xs_0`, so the ∃-expand is
    a PRE-phase to phase 5 (a phase-5.5 that splices the ∨-chain in, then
    re-routes the line into the phase-5 walk). The arm is
    `((k < xs_len) ∧ (P[i→k]))` (len-guarded always for `{0..#xs-1}`;
    unguarded for the literal `{0..N}` and the element-form-without-len cases).
    The integration handoff (5.5 → 5) is the implementation cost.
  - the range-∀ slot instantiation + the recursive nested-∀ unroll
    (`expand_range_forall`) + the fold shape (multi-line balanced-paren body join).

### Tier 3 — the keyed-projection PAIR + guarded pin FAMILY
The set-theoretic registry-read lowering. Hard parts:
  - recognizing the PAIR (pin + `¬∃`-default) across source order, element /
    index / windowed-index forms, then assembling the covered select chain;
  - the pin-FAMILY assembly (member ordering, the negated-disjunction default
    guard syntactic check) and its **LOUD REFUSALS** (exit 1) — and the
    refusal MESSAGE byte-fidelity (the awk prints exact multi-line
    `expected:`/`found:` diffs to stderr; matching those byte-for-byte is its
    own sub-task, since the equivalence bar includes stderr/exit for refusals).

### Tier 4 — the enum/record refusals + the completeness sweep
  - enum-element refusals on literal/hold/append (exit 1, exact message);
  - the post-rewrite **completeness check**: any surviving bare `xs`/`_xs`
    token for a registered seq → exit 1 with the exact message. This is the
    safety net that turns silent oracle drops into loud failures; it must be a
    final pass over the emitted output (a second scan), and its messages need
    byte-fidelity.

## Effort estimate
- Tier 1 (R16/R18 subst): ~1 focused session (the substitution sub-loop +
  literal-arith fold + dynamic chains + fixpoint; comment-inclusive is the
  subtlety). Unlocks most of `tests/seq/*` index/card fixtures.
- Tier 2 (record + ∀/∃): ~2 sessions (PASS0 record/enum scan is new; the
  nested-∀ recursion and fold-body join are fiddly).
- Tier 3 (projection + pin family): ~2–3 sessions; the family default-guard
  check + refusal-message byte-fidelity is the long pole.
- Tier 4 (refusals + completeness): ~1 session, but gated on stderr/exit
  byte-equivalence harness support (the current gate diffs stdout only).

## Harness notes for the continuation
- `lowerseq-equiv.sh` now diffs stderr+exit for REFUSAL fixtures (those whose
  header has `-- expect: flatten-error`): it compares (awk stderr + awk exit)
  against (port stdout + port exit) — both message text and exit code. The
  port has no stderr channel, so a refusal must `BuildPrintln` the exact awk
  diagnostic and `Exit(1)`. Non-refusal fixtures still diff stdout. (Landed
  2026-06-12 — Tier 4 prerequisite.) Matching the awk's multi-line
  `expected:`/`found:` diff text byte-for-byte is still the long pole of the
  pin-family refusals (Tier 3).
- NOTE for the runner: in a git worktree the kernel may live only in the main
  checkout; set `EVIDENT_KERNEL=<main>/kernel/target/release/kernel`.
- Build the per-rule intermediate by running the flatten prefix
  (walk + autocarry + flatten-body-records) and feeding it to BOTH the awk
  pass and the Evident pipeline. The current gate feeds raw fixtures (which
  have no imports needing the prefix); real `tests/seq/*.ev` fixtures import
  `stdlib/kernel.ev` and must go through the prefix first.
