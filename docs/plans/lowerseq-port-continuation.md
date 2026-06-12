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

## Ported + byte-identical (8 fixtures green)

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

## NOT ported (continuation, priority order)

### Tier 1 — R16/R18 default-path substitution (the next step)
The single highest-value gap. The awk runs `subst_index` (R16: `xs[K]`→`xs_K`,
incl. literal-arith fold `xs[2*3+1]`→`xs_7`) + `subst_card` (R18: `#xs`→`xs_len`)
on **every default-path line, INCLUDING COMMENTS** (measured: a comment
`-- note xs[0] and #xs` is rewritten). Hard parts:
  - must run on the raw line (comment-inclusive), at fixpoint;
  - literal-index arithmetic folding (`idx_eval`: `+ - *` over int literals);
  - the dynamic-index select chains (`subst_dyn`: `xs[i]`/`xs_k_accs[j]`→covered
    chain) and `subst_dyn_fix` fixpoint.
  Design: a phase-5 sub-loop in `lowerseq_emit` doing one leftmost rewrite per
  tick until stable, then print. A single-replacement primitive (`LsSubstOne`)
  in the lib finds the leftmost `#name`/`name[lit]` of a registered seq and
  rewrites it. Until this lands, gate fixtures must avoid surviving seq
  index/card uses (the current 8 do).

### Tier 2 — record-element decls + the `∀`/`∃`/member unrolls
  - record-`type` element decls (`xs ∈ Seq(R)` → `xs_k_fj ∈ Tj`; the
    `emit_field_decl`/`emit_field_hold` helpers, incl. Seq-typed type-body-bounded
    fields → per-subslot Int);
  - `∀ x ∈ xs : P` (Int, len-guarded ∧-unroll) and record element `∀ e ∈ xs`;
  - `y ∈ xs` member (Int, len-guarded ∨-unroll);
  - `(∃ i ∈ {0..#xs-1} : P)` / `(∃ e ∈ xs : P)` (`subst_exists`);
  - the range-∀ slot instantiation + the recursive nested-∀ unroll
    (`expand_range_forall`) + the fold shape (multi-line balanced-paren body join).
  Requires PASS0 record-type + enum scan (`tfield`/`ttype`/`fbound`/`enums`).

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
- `lowerseq-equiv.sh` diffs **stdout only**. Tiers 3/4 need stderr+exit
  comparison for the refusal fixtures — extend the harness before porting
  refusals.
- Build the per-rule intermediate by running the flatten prefix
  (walk + autocarry + flatten-body-records) and feeding it to BOTH the awk
  pass and the Evident pipeline. The current gate feeds raw fixtures (which
  have no imports needing the prefix); real `tests/seq/*.ev` fixtures import
  `stdlib/kernel.ev` and must go through the prefix first.
