# compiler2 — the Z3-AST rewrite plan

Decision (operator + session, 2026-06-07): stop building SMT-LIB
text by string concatenation. The compiler builds Z3 ASTs in memory
via `BuildZ3*` libcall sugar and asks Z3 to serialize
(`Z3_solver_to_string`) at emit time. This document is the work
plan; companion docs: `z3-sugar-inventory.md` (the API surface),
`fossil-subset.md` (what the committed artifact can compile —
being probed), `expr-slot-binding-port-notes.md` (why rebuilds are
blocked today).

## Why (recap)

- The text path is why translation gaps exist at all: the
  depth-unrolled RenderExprL0/L1/L2 renderers drop compound
  expressions (`(assert (= ok ))`), compositions vanish, escaping
  is a subsystem. With AST building there is no renderer to be
  incomplete — every Z3 op is available by construction.
- The text path is also the state pathology: output accumulated as
  ever-growing String state fields, re-pinned through Z3 every tick
  (O(n²) bytes over a compile). Under the AST architecture the
  output is ONE Int handle; the bytes live in Z3's heap.
- Input side: tokens move to FTI (libc memory + cursor Ints in
  state — `stdlib/fti/token_stack.ev`, proven), folding the
  TokenList pivot plan into compiler2 from day one instead of
  retrofitting.

## The constraint that shapes everything: the fossil

`compiler.smt2` (committed artifact) drops claim-composition lines
wholesale and cannot rebuild ANY current compiler source (~370
composition lines in flattened sample.ev). It is a fossil: built
once by the deleted bootstrap, never able to recompile itself.
Therefore compiler2 must be written in the FOSSIL-COMPILABLE SUBSET
of Evident (flat claims, simple bodies — exact rules from the
fossil-subset probe). The ladder:

```
fossil compiler.smt2 ──compiles──▶ compiler2.ev (subset-disciplined)
compiler2.smt2 ──compiles──▶ everything, eventually compiler2.ev itself
```

Closing that last arrow is genuine self-hosting — which even the
bootstrap era never had.

## Phases and parallelization

P0 (probe agent, running): fossil subset rules → fossil-subset.md.
P1 (4 sugar agents, running): stdlib/z3_core.ev / z3_ops.ev /
   z3_seq.ev / z3_datatypes.ev + kernel fixtures, one inventory
   section each, in isolated worktrees. Orchestrator merges with
   test gates.
P2 (next wave): compiler2's translate passes, one agent per pass
   (bool → ternary/record → seq/string → ctor/match → quant), each
   an FSM walking parsed expressions and emitting BuildZ3* effects,
   handles in Int state. Per-pass fixtures; subset discipline gated
   by compiling each file through the fossil.
P3: compiler2.ev driver (lexer/parser re-expressed in subset + FTI
   tokens + new translate + solver-to-string emit). Fossil compiles
   it once (EVIDENT_TICK_LIMIT raised — override landed 0b181c5).
   Acceptance: beats the fossil on the conformance census; then
   compiles itself.
P4: delete the string machinery (RenderExpr*, escaping, pin caps);
   regenerate the conformance census as the scoreboard.

## Operational notes

- Seam compiles run ~35 s (post-functionizer, c8e7d9b). Parallelize
  the conformance runner (sequential today, 138 × 35 s ≈ 80 min;
  8-way ≈ 10 min) before the next full census.
- The 2026-06-07 census log was lost to a container restart;
  regenerate after the runner is parallelized. Known failure
  classes: compound-expr-in-value-position drops, effects-body
  silent drops, composition-line drops — all text-renderer
  artifacts that P2-P4 eliminate structurally.
- Kernel stays frozen except explicit capabilities (tick-limit
  override, phase trace — both landed). No language features in
  Rust.

## What is explicitly NOT the plan

- No bootstrap resurrection.
- No fixing the fossil's renderers in place (compiler/translate_*.ev
  edits can never take effect — nothing can rebuild that source).
  The existing translate files remain as reference semantics until
  compiler2 supersedes them.
- The manifest header stays text (inventory §13) — it's the
  kernel's wire contract, not part of the model.
