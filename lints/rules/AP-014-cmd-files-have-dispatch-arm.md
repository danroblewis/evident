# AP-014: cmd-files-have-dispatch-arm

**Status:** active

**Pattern.** A file `runtime/src/commands/<X>.rs` (where `<X>` is
neither `common` nor `mod`) exists in the codebase, but either:
  - it does not declare `pub fn cmd_<X>(...)`, OR
  - `runtime/src/main.rs` does not have a match arm dispatching the
    verb form of `<X>` (underscores → hyphens) to
    `commands::<X>::cmd_<X>`.

In other words: every CLI subcommand file is wired end-to-end —
file exists, function exists, dispatch arm exists.

**Why.** This was the canonical Pattern A bug surfaced in the
agent dependency-graph sweep: `commands/desugar.rs` and
`commands/infer_types.rs` had `pub fn cmd_desugar` /
`pub fn cmd_infer_types` written and documented, but `main.rs`'s
match block had no arm dispatching `"desugar"` /
`"infer-types"` to them. The subcommands were unreachable from
the CLI, but the codebase claimed they existed. Fixed in
`0ddf6a6`. Codify so a new `commands/X.rs` file can't ship
without its dispatch arm.

**Fix.** Add the missing piece. Three locations stay in lockstep:
  1. `runtime/src/commands/X.rs` declares `pub fn cmd_X(...)`.
  2. `runtime/src/commands.rs` has `pub mod X;`.
  3. `runtime/src/main.rs` has `"<verb>" => commands::X::cmd_X(...)`
     where `<verb>` is `<X>` with underscores replaced by hyphens
     (`effect_run` → `effect-run`, `infer_types` → `infer-types`).

**Detection.** grep (cross-file shell loop)

**Pattern (grep).** For each `runtime/src/commands/X.rs` (skipping
`common.rs` and `mod.rs`):
  - Verify `grep "pub fn cmd_<X>" runtime/src/commands/X.rs`
    matches.
  - Verify `runtime/src/main.rs` contains a literal `"<verb>"`
    (the kebab-case verb form) — the dispatch arm.

**Scope.**
  - File enumeration: `runtime/src/commands/*.rs`.
  - Dispatch check: `runtime/src/main.rs`.

**Exceptions.**
  - `common.rs` (shared helpers, not a subcommand) and `mod.rs`
    (module organization) are exempt.

**Examples.**
  - Pre-`0ddf6a6`: `commands/desugar.rs` had `pub fn cmd_desugar`,
    but `main.rs` had no `"desugar" => commands::desugar::cmd_desugar(...)`
    arm. Same for `commands/infer_types.rs` and `"infer-types"`.
    Both were dead code from the binary's perspective. The fix
    added the two arms; the lint catches the next time anyone
    forgets.
