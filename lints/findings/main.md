# Findings: runtime/src/main.rs

Reviewed against `lints/rules/` as of baf8078.

## Violations of existing rules

None. The active rulebook (AP-001 through AP-008) targets
language-core leakage, example-file constraints, conformance
skips, and Rust-test ignores. None apply to a 38-line argv
dispatch file.

## Per-file invariant check

The invariants doc (`lints/runtime-invariants.md`, "Group 7 —
Top-level / `runtime/src/main.rs`") requires:

  - reads argv, dispatches to `cmd_<name>`, returns ExitCode
  - no command logic
  - no `EvidentRuntime` construction
  - no subcommand-specific flag parsing
  - no state
  - no reach into runtime internals (uses `commands::*` only)
  - no print except for usage / unknown-subcommand
  - the `match` block is the file's only logic
  - "Adding a subcommand means: add a `commands/<name>.rs` …,
    add a `pub mod <name>;` line to `commands.rs`, add a
    match arm here. Three files, mechanical."

Walked the file: argv read at line 19, empty-args usage at
20-23, `match args[0].as_str()` at 24-37 with one arm per verb
calling `commands::<name>::cmd_<name>`, help/unknown-subcommand
fallthroughs print usage and return `ExitCode::from(2)`. No
`EvidentRuntime`, no flag parsing, no state, no `crate::*` /
internal imports. Only printing is the usage banner and the
"unknown subcommand" line. The match block IS the only logic.

The single observable defect is the dispatch-table /
documentation gap described below.

## Dispatch-table completeness defect

**File-level claim vs. dispatch-table reality.**

The doc comment at the top of `main.rs` (lines 1-10) lists
`infer-types` as a subcommand:

> ```
> //!   infer-types <file> [--strict]
> ```

But the `match args[0].as_str()` block (lines 24-37) has no
arm for `infer-types`. It also has no arm for `desugar`, even
though `commands/desugar.rs` exists and is `pub mod`'d in
`commands.rs`. Six arms are present: `query`, `check`,
`sample`, `test`, `effect-run`, `lint` (plus the
`help`/`--help`/`-h` triplet and the `unknown subcommand`
fallthrough).

Confirmed in `commands/`:

  - `commands/infer_types.rs` — exists, `pub mod`'d, exposes
    only library entries (`collect_inferences` line 80,
    `auto_apply_inferences` line 171, `unambiguous_inferences`
    line 205). No `pub fn cmd_infer_types`.
  - `commands/desugar.rs` — exists, `pub mod`'d, exposes only
    library entries (`collect_passthrough_rewrites` line 58,
    `auto_apply_desugar` line 114). No `pub fn cmd_desugar`.

So the user-typed string `evident infer-types …` (which the
doc comment promises) falls through to the
`unknown subcommand: infer-types` branch, returning
`ExitCode::from(2)`. The doc comment is a lie. Same outcome
for `evident desugar`, which is undocumented but plausible
given the file's existence.

This is consistent with the per-file invariant brief in
`lints/runtime-invariants.md`'s "Group 6 — CLI" entry for
`infer_types.rs` / `desugar.rs`, which describes them as
"two CLI subcommands that double as libraries used by other
parts of the CLI" exposing both `cmd_<name>` AND
`auto_apply_*`. Today only the library half exists; the
`cmd_<name>` half is missing on both files, and `main.rs`
correspondingly has no arm to dispatch to. So the gap is
two-sided: each `commands/<name>.rs` is missing its
`cmd_<name>`, and `main.rs` is missing its arm.

The fix is two-sided too: either add the missing
`cmd_infer_types` / `cmd_desugar` functions (with usage
strings, flag parsing, and `commands.rs::common` use as the
other `cmd_*` files do) and the matching `main.rs` arms — OR
delete the `//!   infer-types …` line from the doc comment if
the verb isn't actually meant to be user-callable. Whichever
direction is chosen, the doc + dispatch + per-file `cmd_*`
must be in lockstep.

## Candidate new rules

### Suggested AP-009: main-doc-comment-matches-dispatch-arms

**Pattern observed at runtime/src/main.rs:1-10 vs. lines 24-37:**
> ```
> //!   infer-types <file> [--strict]
> ```
> (no matching `"infer-types" => …` arm in the `match` below)

**Why it might be bad.** `main.rs` is the binary's contract
with the user — what shows up in `--help` and what the
`match` actually accepts must be the same set, or users
pay the cost. The two diverge silently because the doc
comment is prose and the match is code, with nothing tying
them together. This is the same family of issue as a
README's "supported commands" list drifting from what's
shipped, and the fix shape is identical: keep both in
lockstep, ideally by deriving one from the other or by
checking the relationship in CI. Past pattern in this
repo: AP-008 catches the same shape between a registry
file and the file it's supposed to mirror (examples/ vs.
EXPECTATIONS in `runtime/tests/demos.rs`). This would be
the analogue for `main.rs` doc comment vs. `match` arms.

**Suggested fix.** A small CI/test check that:

  1. Greps the doc-comment block at the top of `main.rs` for
     lines of the form `//!   <verb>` (two spaces after
     `//!`, then the verb name, then space-or-EOL).
  2. Greps the `match` block for arms of the form
     `"<verb>" => commands::…`.
  3. Asserts the sets are equal (modulo `help`/`--help`/`-h`,
     which is inherently a meta-verb and doesn't need to
     appear in the doc-comment list).

Failure message: "main.rs doc comment lists `<verb>` but no
match arm dispatches it" or "main.rs match arm dispatches
`<verb>` but doc comment doesn't list it."

**Detection idea.** Shell-level grep is sufficient:

```bash
# Verbs claimed in the doc comment (lines starting with `//!   `,
# three spaces, taking first word).
doc_verbs=$(awk '/^\/\/!   [a-z]/ {print $2}' runtime/src/main.rs | sort -u)
# Verbs dispatched in the match block.
arm_verbs=$(awk '/=> *commands::/ {gsub(/[" ]/,""); split($0,a,"=>"); print a[1]}' \
              runtime/src/main.rs | sort -u)
diff <(echo "$doc_verbs") <(echo "$arm_verbs")
```

Mechanizable; lives naturally in `lints/checks.sh`. Bar:
clears observable-in-syntax (concrete grep), specific fix
(add arm OR delete doc line), likely to recur (every new
subcommand is a fresh chance to skip one side), no overlap
with AP-001..008.

I have NOT created `lints/rules/AP-009-…md` or added the
check to `checks.sh` — only proposed it here, since the
current finding is one observed instance and the user has
the standing rule that I report and they decide.

### Suggested AP-010: commands-mod-mirrors-match-dispatch (review-only)

**Pattern observed.** `commands.rs` has `pub mod desugar;` and
`pub mod infer_types;`, but `main.rs` doesn't dispatch to
either. The orphan-module case: a file under `commands/`
that has no corresponding `cmd_<name>` arm is either
(a) a library helper that shouldn't be `pub mod`'d at the
crate-binary level — promote to a library-only module, OR
(b) an unfinished subcommand whose dispatch wasn't wired up.

**Why it might be bad.** Cargo will compile `pub mod
infer_types;` regardless of whether main.rs uses it, so the
binary ships with dead-from-CLI code and no warning. A
future reader has to grep both files to know which modules
under `commands/` are actually subcommands.

**Suggested fix.** Either move "library-only" command-helper
modules out of `commands/` (since the per-file invariant
says `commands/` files should have a `pub fn cmd_<name>`),
or wire up the missing match arms.

**Detection idea.** Cross-check `commands.rs`'s `pub mod`
list against `main.rs`'s match arms; flag any module that
appears in one but not the other (excluding `common`). This
is a stricter form of AP-009 that catches the case where
the doc comment is also missing. Could mechanize but the
two checks together start feeling like one bigger check
with two failure modes.

**Filing as review-only**, since (a) it overlaps with AP-009
above and (b) the right shape might be one combined check
("verb-set agrees across all three places: doc comment,
match arms, `commands.rs` pub mods, `commands/<n>.rs`
defines `pub fn cmd_<n>`"). Want one solid rule rather than
two adjacent ones.

## Clean

The dispatch logic itself is clean — every arm calls
`commands::<name>::cmd_<name>(&args[1..])` with no
intermediate work, no flag parsing, no state, no internal
reach. The defects are at the edges: one missing arm pair
(`infer-types`, `desugar`) plus the doc-comment drift.
