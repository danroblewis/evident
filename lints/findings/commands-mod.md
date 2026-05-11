# Findings: runtime/src/commands.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None. The active rulebook (AP-001 through AP-008) targets
language-core leakage (AP-001), example-file constraints (AP-002,
AP-003, AP-006, AP-007, AP-008), conformance-suite skips
(AP-004), and Rust-test ignores (AP-005). None apply to a
14-line CLI module-entry file.

Per-file invariants (from `lints/runtime-invariants.md`):
the file is required to be a small `pub mod` listing, must not
contain implementation, must not re-export internals beyond what
`pub mod` already does, and must not carry state. Checked:

  - Body is one doc comment plus 9 `pub mod` declarations
    (`common`, `check`, `desugar`, `effect_run`, `infer_types`,
    `lint`, `query`, `sample`, `test`). Each corresponds to an
    actual file under `runtime/src/commands/`. No orphaned or
    missing modules.
  - No `pub use` re-exports. No type aliases. No `impl` blocks.
    No `fn` definitions. No `static` / `const` / `lazy_static`.
  - No `use` statements at all — no chance of widening the
    module's external surface accidentally.
  - File length: 14 lines including the doc header and trailing
    blank. Well within the "small" expectation.

The doc comment accurately describes the layout convention
(`cmd_<name>` per file under `commands/`, shared helpers in
`commands/common.rs`).

## Candidate new rules

None. This file has no observable anti-pattern, and the role
("module entry must stay a thin `pub mod` listing") is already
covered by the per-file invariant in
`lints/runtime-invariants.md`. Mechanizing it as an AP rule
would require either:

  - A grep that fails when `commands.rs` (and other named
    "module entry" files like `translate.rs`, `event_sources.rs`)
    contains anything beyond `pub mod` / `mod` / `pub use` /
    doc comments — useful but narrow, and the invariants doc
    already names these files explicitly.
  - An AST check that the file's top-level items are all
    `ItemMod` — same observation in stricter form.

If a future violation appears (someone inlines a helper into
`commands.rs` or starts re-exporting `cmd_*` symbols at the
crate root from here), proposing AP-009 "module-entry files
contain only module declarations" with a grep over a fixed list
of entry-file paths would be the obvious response. Not
warranted today — review-only suffices.

## Clean

This file is clean. No violations, no candidates that clear the
"likely to recur, observable in syntax" bar.
