# Findings: runtime/src/commands/desugar.rs

Reviewed against `lints/rules/` as of HEAD (188c682).

## Violations of existing rules

None of AP-001 through AP-008 fires on this file.

  - **AP-001** (no library-specific in language-core): `commands/`
    is explicitly out of scope per AP-001's "Scope" section, and a
    grep for the forbidden token classes (`SDL_`, `Sdl[A-Z]`,
    `\bGl[A-Z]`, `Glsl`, `Audio[A-Z]`, `\.dylib`, `\.framework/`,
    `/opt/homebrew/lib/`, `/usr/lib/lib`) returns zero hits in
    this file. Clean.
  - **AP-002 / AP-003 / AP-006 / AP-007 / AP-008**: scope is
    `examples/`. Doesn't apply.
  - **AP-004**: scope is `tests/conformance/`. Doesn't apply.
  - **AP-005**: scope is `runtime/tests/`. Doesn't apply.

## Per-file-invariant violations (from `runtime-invariants.md`)

The brief for `commands/{infer_types,desugar}.rs` lists four
invariants. One is materially violated, one is partially honored,
the other two are honored.

### Missing CLI half of the dual role
**runtime-invariants.md says:** "Two CLI subcommands that double as
libraries used by other parts of the CLI… The dual role is the
file's defining property: each exposes both a `pub fn cmd_<name>`
for the user-facing subcommand AND a `pub fn auto_apply_*` (plus
supporting types like `Inference` or `Rewrite`) for use as a
library from sibling `cmd_*` files."

**Observed at desugar.rs:** the file defines `Rewrite` (line 47),
`collect_passthrough_rewrites` (line 58), and `auto_apply_desugar`
(line 114) — only the **library half**. There is **no
`pub fn cmd_desugar`** in the file, and `runtime/src/main.rs` has
no `"desugar"` arm in its dispatch (verified via
`grep -n cmd_desugar` — zero hits anywhere; `main.rs` lines 24-37
list query/check/sample/test/effect-run/lint only). The sibling
`infer_types.rs` is symmetrically missing its `cmd_infer_types` /
dispatch arm too — `infer-types` even appears in `main.rs`'s
help-banner on line 9 but isn't in the `match` block — but this
finding is about `desugar.rs`, not `infer_types.rs`.

This means the file is currently a **library-only** module
masquerading as a dual-role one. A user typing `evident desugar`
gets "unknown subcommand". The invariants doc treats the dual
role as the file's defining property; the CLI half being absent
is a structural drift away from the brief, not a stylistic nit.

The file's own doc comment (lines 1-33) frames the file as the
"self-hosted desugar pipeline" + "proof-of-concept of the desugar
shape, not a payoff in lines-of-code reduction" without
mentioning the missing CLI verb — the omission appears
unacknowledged rather than deliberate.

### Cross-language contract: matches the .ev file (clean)
**runtime-invariants.md says:** "These files are coupled to the
specific pass `.ev` files they load. The pass files' structure
(claim names, expected query shape, output Datatype shape) is
part of the contract — if either side changes, both must change."

**Verified:** `desugar.rs:42-43` references `STDLIB_AST` =
`stdlib/ast.ev`, `DESUGAR_PASSTHROUGH` =
`stdlib/passes/desugar_passthrough.ev`, and `RULE_NAME` =
`is_passthrough_at_index`. The pass file
`stdlib/passes/desugar_passthrough.ev:35` defines
`claim is_passthrough_at_index` with the expected parameters
(`target_idx ∈ Nat`, `target_name ∈ String`) and the body item
shape `body[target_idx] = BIConstraint(EIdentifier(target_name))`.
The Rust side's `given.insert("target_idx", …)` (line 92) and
`qr.bindings.get("target_name")` (line 98) match the pass's pinned-
input / free-output convention. Cross-language contract is
honored.

### "Build the desugar logic in Rust" — clean
**runtime-invariants.md says:** "Must NOT build the desugar logic
in Rust — every rule lives in stdlib/passes/desugar_passthrough.ev"

**Verified:** the only Rust-side rule logic is filter steps that
the brief explicitly carves out: deciding whether `target_name` is
a known schema (line 99 — `if !known.contains(name)`, with the
inline rationale at lines 73-75 + the `.ev` file's own comment
lines 17-21 explaining why this stays in Rust until LinkedList
iteration is supported), and the still-matches sanity check on
line 132 that defends against stale rewrites. Both are
orchestration concerns, not desugar rules.

### "Special-case any specific rule" — clean
**runtime-invariants.md says:** "Special-case any specific rule —
if a rule needs special handling, that's a sign the rule should be
expressed differently in its `.ev` file."

**Verified:** `RULE_NAME` is a single `const &str` constant; the
file only knows how to handle one rule shape (bare-identifier →
passthrough). The shape is generic — it's "load a pass, query for
indices, decode `target_name`, build a `BodyItem::Passthrough`,
apply via `replace_body_item_in_claim`". Adding a second rule
would require generalizing `Rewrite` to carry the new replacement
shape. Today there's only one; nothing is special-cased.

## Other observations

### Hand-waved off-by-one: `body_len` ignored if `Err`
**Observed at desugar.rs:89:**
> ```rust
> let body_len = rt.user_claim_body_len(claim_idx).unwrap_or(0);
> ```

If `user_claim_body_len` returns `Err` for a claim, `body_len` is
silently 0 and the inner loop never runs — the file produces no
rewrites for that claim, with no diagnostic. Same shape on line 88
(`unwrap_or_default()`) for `claim_name`. The sibling `cmd_test`
in `infer_types.rs::collect_inferences` uses the same pattern
(line 141). Review-only — there's no obvious reason
`user_claim_body_len` would fail for a valid `claim_idx` returned
from `user_claim_indices_in_file`, so the `unwrap_or` is benign in
practice.

### Stderr warning shape inconsistent with sibling
**Observed at desugar.rs:121:**
> ```rust
> eprintln!("warning: desugar pipeline failed: {e}");
> ```

vs `infer_types.rs:178-179`:
> ```rust
> eprintln!("warning: inference pipeline failed: {e}");
> eprintln!("(continuing without inferences; pass --strict to suppress this message)");
> ```

The desugar variant prints one line; the inference variant prints
two and references a `--strict` flag. The sibling files document
themselves as having symmetric dual-role responsibilities; the
on-failure UX is divergent. Either both should suggest a
suppression flag (and both should implement it) or neither should.
Review-only.

### Doc comment line 8 references a removed match arm
**Observed at desugar.rs:7-12:**
> ```rust
> //! Currently one rewrite:
> //!   `BodyItem::Constraint(Expr::Identifier(name))` where `name` is
> //!   a known schema → `BodyItem::Passthrough(name)`. Previously this
> //!   was handled by a match arm in `translate/inline.rs:223`; that
> //!   arm is now removed.
> ```

A line-number reference to another file (`translate/inline.rs:223`)
that's unverified at review time and will silently rot. Standard
problem with line-numbered cross-file references in doc comments.
Review-only — same family as AP-12 (self-evident comments)
discussed in the agent prompt as review-only.

### Ad-hoc `std::collections::*` qualified paths instead of `use`
**Observed at desugar.rs:80-81 and desugar.rs:91:**
> ```rust
> let mut indices: std::collections::BTreeSet<usize> =
>     std::collections::BTreeSet::new();
> // …
> let mut given = std::collections::HashMap::new();
> ```

The file imports `HashSet` at the top (line 35) but reaches for
`std::collections::BTreeSet` and `std::collections::HashMap` via
fully-qualified paths in the function body. Same file, two
discordant import styles — looks like
`collect_passthrough_rewrites` was lifted from `infer_types.rs`
(which has the same fully-qualified paths on lines 133-134, 215)
without harmonizing. `cargo fmt` won't catch this; review-only.

## Candidate new rules

### Suggested AP-009: dual-role-files-have-both-halves
**Pattern observed at desugar.rs (no `cmd_desugar` in the file
nor in `main.rs` dispatch):**
> The runtime-invariants brief explicitly names this file as
> dual-role ("each exposes both a `pub fn cmd_<name>` for the
> user-facing subcommand AND a `pub fn auto_apply_*`"), but the
> file ships only `auto_apply_desugar` — the CLI half is absent.

**Why it might be bad:** A "dual-role" framing in the per-file
invariant promises both a CLI verb and a library hook. When only
one half ships, code that follows the invariants doc to find an
example of the dual-role pattern sees an asymmetric implementation
and learns the wrong shape ("it's fine to ship just the library
half"). The next dual-role file (next self-hosted pass) will
inherit the asymmetry by precedent. The file's own doc comment
(lines 30-33) acknowledges this is a "proof-of-concept" but
doesn't note the missing CLI half — the omission reads as an
oversight rather than a deliberate choice.

The same pattern exists in `infer_types.rs` (no `cmd_infer_types`
either, despite `infer-types` appearing in `main.rs`'s help
banner) — so the rule isn't a one-off; it's a recurring drift
across both files the invariant brief covers.

**Suggested fix:** Either add `cmd_desugar` (and the `main.rs`
dispatch arm) so the file actually fits the dual-role brief, OR
update the invariant brief to acknowledge that `desugar.rs` /
`infer_types.rs` are library-only with the CLI half deferred. The
current state — invariant says X, code says Y, no comment in
either side acknowledges the gap — is the worst of both.

**Detection idea:** grep — for each file the invariants doc
flags as dual-role (currently `commands/{desugar,infer_types}.rs`),
verify the file contains `pub fn cmd_<filename>` AND `main.rs`
contains a dispatch arm matching `<filename>`. Doable as a small
shell check in `lints/checks.sh` keyed off a hardcoded list of
dual-role files (the list is small; auto-discovery would parse
`runtime-invariants.md`).

### Suggested AP-010: stale-line-number-cross-file-refs (review-only)
**Pattern observed at desugar.rs:11:**
> `//! Previously this was handled by a match arm in
> `translate/inline.rs:223`; that arm is now removed.`

**Why it might be bad:** Line numbers in cross-file references go
stale on the first edit to the referenced file. Once stale, the
comment misleads — a reader following the pointer lands on
unrelated code, learns nothing, and doesn't trust any other
pointer in the codebase. The two safer forms are: (a) reference
the symbol name (`translate/inline.rs::translate_passthrough`),
which survives line-number drift, or (b) drop the location
entirely and just describe what was done.

**Why review-only:** Mechanical detection would require either an
AST walk (find file paths followed by `:NNN` in `///` / `//!` /
`//` comments anywhere) or a maintenance bot that re-resolves
references on each commit. Both are heavier than the rule's
payoff. A reviewer noticing a `:NNN` cross-file reference and
asking the author to switch to a symbol name is the right
intervention.

**Suggested fix:** When citing another file from a doc comment,
cite the symbol (function / method / constant / type name), never
the line number.

**Detection idea:** review-only. (Could be a grep
for `[a-zA-Z_]+\.rs:\d+` in `///` / `//!` lines, but the false-
positive rate from legitimate prose mentioning grep output or
build errors would be high.)

## Clean

Not clean. One per-file-invariant violation (CLI half absent
despite the dual-role brief); the cross-language contract,
"don't build logic in Rust" rule, and "no special-case rules"
rule are all honored. Three review-only observations (silent
`unwrap_or`, divergent stderr UX vs sibling, stale line-numbered
cross-file ref, ad-hoc qualified paths) plus one stylistic
nit. Two new rules proposed: AP-009 (dual-role files must ship
both halves — mechanizable) and AP-010 (stale line-numbered
cross-file refs — review-only).
