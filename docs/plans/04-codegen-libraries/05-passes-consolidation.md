# Phase 4.5: Inference / desugar passes consolidation

## Goal

Slim the Rust glue code in `commands/infer_types.rs` (513 lines) and
`commands/desugar.rs` (143 lines). Move more of the logic into
`stdlib/passes/`.

## Prereqs

- Phase 3 done — passes that need recursive AST walks become
  feasible in pure Evident.

## What to build

The passes today are inference rules (find a fact); the runtime
glue applies the inferred fact. With recursive claims + unbounded
output, we can have passes that produce a list of (claim_idx,
body_idx, transformed_item) triples directly — the Rust glue
becomes a simple "apply each rewrite" loop.

Audit `commands/infer_types.rs` and `commands/desugar.rs`. For each
function, decide:
- Move to `stdlib/passes/` if it's pure logic.
- Keep in Rust only if it needs runtime API access (mutation, I/O).

Estimated drop: ~300 Rust lines moved to ~400 Evident lines.

## Files touched

- `runtime-rust/src/commands/infer_types.rs` — slimmed
- `runtime-rust/src/commands/desugar.rs` — slimmed
- `stdlib/passes/*.ev` — extended

## Acceptance

- [ ] Inference still works on existing programs
- [ ] Desugar still works on existing programs
- [ ] LOC: -~300 Rust, +~400 Evident
