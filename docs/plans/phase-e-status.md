# Phase E status — bootstrap acceptance

See `docs/plans/completion-roadmap.md` for the parent plan. Phase E
gates final cutover (Phase F: delete `runtime/`).

## E1 — self-compile demonstration  ✓ (demo form)

Status: **DEMO form landed.** A real `.ev`-on-disk self-compile is
gated on broader translator coverage (Phase C residuals).

`tests/kernel/test_e1_self_compile_demo.ev` builds a SchemaDecl AST
in memory:

    SDecl(KKClaim, "hello",
          BILCons(BIMembership("x", TName("Int")), BILNil))

walks the body, runs each item through `DeclareFromMembership`
(stdlib/translate_declare.ev), wraps the result with the
`ManifestHeader` (stdlib/translate_manifest.ev), prints the SMT-LIB,
and asserts via `starts_with` / `str_contains` that the output is
well-formed (starts with `";; manifest:"`, contains the declare-fun
line). All checks run on the kernel.

This proves the seam: AST → SMT-LIB → kernel-runnable, end-to-end,
without touching `runtime/`.

## What E1 deliberately skips

- **Parsing.** The AST is built by Evident literal-constructor
  expressions; the self-hosted lexer/parser (Phase A/B) are not
  driving it. A real self-compile pipes `.ev` source through them.
- **Multi-item body walks.** Body has a single BIMembership; we
  use a depth-1 unroll (head match). Stretching to N items needs
  the bounded-unroll-ladder pattern from `RenderFieldList`, or a
  multi-tick FSM walker (`test_ast_walker.ev` shape).
- **Non-membership body items.** BIConstraint, BISubclaim,
  BIClaimCall — Phase C5/C6/C8 territory. Out of E1's scope.
- **Re-running the emitted SMT-LIB through the kernel.** The
  diagnostic does string-shape checks only. Eval-via-Z3 of the
  emitted bytes is E2.

## E2 — diff-test  (TODO)

Wire the Rust `evident emit` output and the self-hosted emit
output through the same kernel; on the kernel-test corpus the two
must produce equivalent stdout/exit. Blocked on Phase C coverage
across all `.ev` constructs the corpus uses (enums, match, Seq,
quantifiers, generics).

## E3 — performance  (TODO)

Self-hosted compile of a 100-line `.ev` file → `.smt2` in <60s on
the kernel. Likely gated on the runtime-tier choice (the nested-FSM
plan) and on disk-caching compiled functions.

## Acceptance ladder

- E1 (demo)     ✓  in-memory AST → SMT-LIB on the kernel
- E1 (real)     —  parse a `.ev` file → SMT-LIB on the kernel
- E2            —  byte-equivalent or behaviour-equivalent on corpus
- E3            —  perf budget met
- F             —  `runtime/` moves to `bootstrap/`
