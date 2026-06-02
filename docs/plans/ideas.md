# Ideas — deferred until after bootstrap deletion

Things we want to do *after* the deletion path is complete. Not on
the critical path; not blocking; not to be picked up by sessions
until `scripts/check-deletable.sh` exits 0.

## BNF parser-generator in Evident

**Source:** user, mid-session ~task #18.

**Idea:** describe Evident's grammar in BNF (one file, e.g.
`compiler/evident.bnf`), and build a generic BNF parser-generator
in Evident that:

1. Reads a BNF file.
2. Emits a working lexer + parser as Evident code (or runs them
   directly as interpreters over the grammar).
3. Works for any BNF grammar, not just Evident's.

**User rationale:**

> *"It would be really cool if we could make a BNF parser in
> Evident, if we could describe our Evident grammar in BNF then
> use the BNF file to generate a parser and lexer and work from
> that. A generic BNF in Evident that we could use for any BNF
> grammar. I think future agents trying to modify the grammar
> rules and syntax might have an easier time working on BNF than
> on Evident code describing the grammar."*

**Why defer:**

- Not on the bootstrap-deletion critical path. The current
  hand-written `compiler/lexer.ev` and `compiler/parser.ev` are
  good enough to produce `compiler.smt2` once they compose; a
  BNF-driven equivalent is a refactor of working code.
- A parser-generator is substantial — easily its own
  multi-session arc. Spawning it now risks contention with the
  critical-path work.
- After bootstrap is deleted, the grammar surface is whatever the
  self-hosted compiler accepts. The BNF + generator can replace
  the hand-written passes cleanly with no
  bootstrap-equivalence concern.

**When to pick this up:** after Phase 5 of
`docs/plans/DELETION-CHECKLIST.md` (bootstrap severed from all
test paths). At that point, this becomes a clean follow-up.

**Likely shape when implemented:**
- `compiler/evident.bnf` — Evident's grammar in BNF.
- `compiler/bnf_lexer.ev` — generic BNF tokeniser.
- `compiler/bnf_parser.ev` — generic BNF parser, producing a
  grammar AST.
- `compiler/bnf_generate.ev` — emits a lexer + parser specialized
  to a given grammar AST. Either as Evident source (compile-time
  generation) or as a runtime interpreter (generic but slower).
- New tests that demonstrate the generator on at least two
  grammars (Evident's own + one other, e.g. JSON or arithmetic).

## Replace Cons-lists with Seqs (constraint-model fit)

**Source:** user, mid-session ~task #18.

**Idea:** the current invariants point sessions at cons-list state
(`enum WorkList = WLNil | WLCons(WorkItem, WorkList)`) for bounded
data, because cons-lists functionize cleanly today via the
macro-finder while Z3 Seqs are opaque to it. But cons-lists are
imperative-shaped — they carry a "first/rest" verb structure —
and that doesn't naturally appear in constraint-model thinking.
Seqs are the more constraint-native shape.

**User rationale:**

> *"I don't like using the Cons things because we never seen Cons
> in constraint system models. Seq made more sense, and I would
> like to see if we can replace Cons with Seq, even if it has to
> be some rewrite rules."*

**Why defer:**

- The functionizer's `recompose_record_seqs` path — the legacy
  Rust feature that makes record-typed Seqs functionize cleanly
  — was explicitly deferred in task #18. Until that lands,
  switching to Seqs regresses perf on every recursive walker.
- The shift is a sweeping rewrite: every `WorkList` /
  `WLCons`-style pattern in `compiler/translate_*.ev`,
  `stdlib/fti/*.ev`, and the AST walkers would change.
- Doing it before bootstrap deletion adds risk to an
  already-large refactor.

**Likely path when picked up:**

1. Land the `recompose_record_seqs` functionizer extension
   (`legacy-rust/functionizer/src/z3_eval.rs` has the original;
   compare to what shipped in `kernel/src/functionize/`).
2. Add a Seq-based work-stack pattern alongside the cons-list
   one. Prove it functionizes equivalently on a small fixture.
3. Sweep the codebase replacing cons-list state with Seqs,
   one pass at a time, verifying each retains its
   `tests/conformance/features/` equivalence.
4. Drop the cons-list pattern from the invariants doc.

**When to pick this up:** after Phase 5 of
`docs/plans/DELETION-CHECKLIST.md` (bootstrap severed). Or sooner
if the cons-list pattern starts creating noticeable maintenance
friction.

## (Add more ideas here as they surface)
