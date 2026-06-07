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

## The ladder (UPDATED 2026-06-07 evening: the oracle)

Operator decision: the deleted bootstrap serves as a build-time
ORACLE — `scripts/build-oracle.sh` builds it from pinned history
(c218dca^) OUTSIDE the tree, keeps only the binary
(`/usr/local/bin/evident-oracle`), sunset the day compiler2
self-compiles. The oracle compiles FULL Evident in seconds
(validated: expr-slot-binding verdict table perfect; smoke/hello
emits run exit 0; artifact regeneration reproducible).

```
oracle ──compiles──▶ compiler2.ev (FULL Evident, no subset contortions)
compiler2.smt2 ──compiles──▶ everything, eventually compiler2.ev itself
```

Closing that last arrow is genuine self-hosting — which even the
bootstrap era never had. The fossil-subset constraint is GONE from
the critical path.

FALLBACK (proven, shelved): the stage-0 stitch architecture —
hand-written capture-driver shell + fossil-compiled dispatch claims
+ stitcher — ran end-to-end at toy scale (tests/seam/stage0_toy/,
'(= x 5)' exit 0) and projects to 800-1,000 lines total. See
docs/plans/stage0-sizing.md. Use only if the oracle path fails.

## Phases and parallelization

P0 (DONE, merged): fossil subset rules → fossil-subset.md (+ the
   corrections appendix — read it; several verdicts were confounded).
P1 (DONE, merged): stdlib/z3_{core,ops,seq,datatypes}.ev — ~60
   BuildZ3* claims, every group proven by a green kernel fixture.
P2 (NEXT): compiler2's translate passes in FULL Evident (oracle
   compiles them — no subset discipline), one agent per pass
   (bool → ternary/record → seq/string → ctor/match → quant), each
   an FSM walking parsed expressions and emitting BuildZ3* effects,
   handles in Int state. Per-pass fixtures compiled by the oracle,
   run by the kernel.
P3: compiler2.ev driver (lexer/parser + FTI tokens + new translate
   + solver-to-string emit). Oracle compiles it (seconds);
   EVIDENT_TICK_LIMIT override available for self-compile runs.
   Acceptance: beats the fossil on the conformance census; then
   compiles itself (oracle sunset).
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
