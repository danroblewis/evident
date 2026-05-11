# Findings: runtime/src/translate/eval.rs

Reviewed against `lints/rules/` as of HEAD (188c682).

## Violations of existing rules

None of the eight active rules (AP-001 through AP-008) target this file
directly with a violation. AP-001 (no-library-specific-in-language-core)
applies to `runtime/src/translate/*.rs` but nothing in this file mentions
SDL/GL/Audio/dlopen paths — clean on AP-001. The remaining rules
(AP-002 through AP-008) scope to `examples/`, `tests/conformance/`, or
`runtime/tests/` and do not apply here.

## Per-file-invariant violations (from `runtime-invariants.md`)

The runtime-invariants brief for `translate/eval.rs` lists three
structural invariants the file MUST honor. Two are violated; one is
partially honored.

### Mid-file `use` statements split the dependency surface
**runtime-invariants.md says:** "Scatter `use crate::*` / `use super::*`
imports through the file body — all crate-internal imports go at the
top of the file where any reader can see the dependency surface at a
glance" — listed under "what it must NEVER do."

**Observed at eval.rs:5-7 and eval.rs:103-108:**
> ```rust
> // Lines 5-7 (top):
> use std::collections::{HashMap, HashSet};
> use z3::ast::{Ast, Bool, Int, Real, String as Z3Str};
> use z3::{Context, Params, SatResult, Solver};
>
> // Lines 9-101: helper fns (real_from_f64, f64_to_int_rational,
> // real_value_to_f64, apply_solver_tuning, populate_enum_variants)
>
> // Lines 103-108 (mid-file, after ~95 lines of helpers):
> use crate::ast::*;
> use super::types::{CachedSchema, DatatypeRegistry, EnumRegistry, EvalResult, Value, Var};
> use super::declare::declare_var;
> use super::extract::{assert_seq_given, extract_seq, extract_seq_composite, unescape_z3_string};
> use super::inline::inline_body_items;
> use super::preprocess::{apply_pinned_ints, apply_seq_lengths, collect_pinned_ints, collect_seq_lengths};
> ```

The crate-internal imports (`crate::ast::*` and the five `super::*`
imports) sit on lines 103-108, after the numeric/solver helpers and
the `populate_enum_variants` helper. The std/z3 imports are at the
top. This is exactly the "two clean sections separated by a block
of helpers" shape the invariant forbids — a reader cannot see the
file's full dependency surface without scrolling past 100 lines of
function bodies. The fix per the invariant is one block of imports
at the top of the file.

Note: `populate_enum_variants` (lines 75-101) refers to
`super::types::Var` via the fully-qualified `super::types::Var` path
— it works without the line-104 import because the path is spelled
out. Same for `EnumRegistry` in its signature. So the mid-file
imports aren't required for the helpers to compile; they're an
ordering convenience that violates the invariant.

### No `// ──` section markers between the four sub-concerns
**runtime-invariants.md says:** "The file has four distinct
sub-concerns that must stay cleanly sectioned (with `// ──` headers
between them) and ordered…"

**Observed:** `grep -n "^// ──"` over the file returns zero hits.
There are NO section markers anywhere. The four sub-concerns are
delineated only by function boundaries; a reader has no signal
where one section ends and the next begins.

### Sub-concern ordering: borderline; one helper crosses sections
**runtime-invariants.md says:** "ordered so each section depends only
on those above it: (1) numeric and solver-tuning helpers; (2) the
cached-query path…; (3) the one-shot evaluate variants…; (4) local
model-extraction helpers used by both query paths."

**Observed sequence:**
- (1) `real_from_f64` 25, `f64_to_int_rational` 37, `real_value_to_f64` 54, `apply_solver_tuning` 58 — all numeric/solver helpers, contiguous.
- `populate_enum_variants` 75 — used by sections 2 AND 3 (both `build_cache` and every `evaluate*` call it). It's a shared helper, not a numeric/solver-tuning helper. Per the invariant's category (1), it doesn't fit; per category (4), it sits in the wrong place (4 should follow 3). It currently lives between (1) and the mid-file imports.
- (2) cached-query path: `build_cache` 122, `sample_cached_inner` 192, `run_cached` 331 — contiguous.
- (3) one-shot evaluate variants: `evaluate` 434, `evaluate_with_extra_assertion` 606, `evaluate_with_extra_assertions` 663, `evaluate_with_program_and_body` 742, `evaluate_with_core` 824 — contiguous.
- (4) local extraction helpers: `extract_binding` 930, `extract_enum_value` 992 — contiguous, at the end.

The big-picture order (1 → 2 → 3 → 4) is honored. The wrinkle is
`populate_enum_variants`, which is called by both section 2 and
section 3 entries and conceptually belongs with section 4 (it's a
shared helper used by both query paths) — but currently sits inside
the section 1 region. Without `// ──` markers, the misplacement
isn't visible; with markers, the placement-vs-section mismatch
would force a decision (move it to (4), or extend (1)'s scope to
"numeric + shared-init helpers").

## Other observations

### Unused `HashSet` import
**Observed at eval.rs:5:**
> ```rust
> use std::collections::{HashMap, HashSet};
> ```

`grep -n "HashSet"` shows the symbol appears only in the import line
itself — nothing in the file uses it. `cargo clippy` would catch
this; flagging here for completeness.

### Stale module doc comment
**Observed at eval.rs:1-3:**
> ```rust
> //! The four public orchestrator entry points: `evaluate` (one-shot
> //! query), `build_cache` + `run_cached` (per-step cached query for the
> //! executor), `sample_cached_inner` (n-distinct-models for sampling).
> ```

The module doc claims "four public orchestrator entry points" but the
file actually exposes nine: `evaluate`, `evaluate_with_extra_assertion`,
`evaluate_with_extra_assertions`, `evaluate_with_program_and_body`,
`evaluate_with_core`, `build_cache`, `run_cached`, `sample_cached_inner`,
plus the private `extract_binding` / `extract_enum_value` /
`populate_enum_variants` helpers. The runtime-invariants brief lists
"`evaluate, build_cache, run_cached, sample_cached_inner`, plus
`_with_extra_assertion` / `_with_core` variants" — which matches
reality better than the file's own doc comment. Review-only.

### Detached doc comment at the top of the helper block
**Observed at eval.rs:9-14:**
> ```rust
> /// Set `smt.arith.solver` to `arith_solver` on `solver`. Pass `0` to
> /// skip (lets Z3 use its built-in default). The chosen value depends
> /// on workload — the runtime's auto-tuner decides which to use; this
> /// helper is the dumb mechanism. See `runtime::SolveHistory` for the
> /// policy.
> /// Build a Z3 Real literal from an f64 source value.
> ```

The first five lines (9-13) are the doc comment intended for
`apply_solver_tuning` (defined at line 58), but they're attached to
`real_from_f64` (line 25) because there's no blank line separating
them from the next `///` line. Result: `apply_solver_tuning` has no
doc comment, and `real_from_f64`'s doc comment opens with an
unrelated paragraph about `smt.arith.solver`. Review-only — not a
rule violation, but the kind of artifact that suggests sections were
moved without the doc comments being repaired.

## Candidate new rules

### Suggested AP-009: top-of-file-imports-only
**Pattern observed at eval.rs:5-7 vs eval.rs:103-108:**
> Two `use` blocks in one file, separated by ~95 lines of function
> definitions, neither inside a `mod` block.

**Why it might be bad:** A reader's first job in an unfamiliar file
is "what does this file depend on?" That answer should fit in a
single screen at the top. When `use` statements are split — top
block for std/external crates, mid-file block for crate-internals —
the reader sees only half the dependency surface and assumes that's
all of it. The mid-file block silently expands later. This is the
same family of "scattered information" anti-pattern as scattered
constants or scattered constants tables; the runtime-invariants doc
calls it out as a per-file rule for `translate/eval.rs` specifically,
but the underlying principle generalizes to every Rust file in the
runtime: imports go at the top, period. Anything else is either
laziness ("I added a new function and pasted its imports next to
it") or a rough draft of a should-be-its-own-file split.

**Suggested fix:** Move all `use` statements to a single block at
the top of the file, before any `fn` / `struct` / `enum`
definition. If the file has so many imports that this is painful,
the file is too big and should be split.

**Detection idea:** grep — for each `*.rs` file under `runtime/src/`,
find the line number of the first `fn`/`struct`/`enum`/`impl`
declaration (the first non-import top-level item). Then look for any
`^use ` line whose line number exceeds that. Any hit is a violation.
A few-line shell pipeline could express this; doable as a
`check_top_of_file_imports` function in `lints/checks.sh`.

(The runtime-invariants brief already states this for eval.rs as a
per-file rule. Promoting it to AP-009 generalizes it to the whole
runtime crate, which is consistent with how every other file's
invariant brief reads — none of them mention scattered imports
because none of them have any.)

### Suggested AP-010: sectioned-file-needs-section-markers
**Pattern observed at eval.rs throughout (zero `// ──` markers):**
> A file's per-file invariant explicitly requires "// ── headers
> between them" between N sub-concerns; the file has zero such
> headers.

**Why it might be bad:** Without markers, the only signal a reader
has that "this is the cached-query section, that is the one-shot
section" is the function names — and even those mix
(`evaluate_with_extra_assertions` and `evaluate_with_program_and_body`
both belong to section 3 but the connection is implicit). Markers
let a reader skim the file's table of contents in seconds. Their
absence makes the four-section invariant aspirational rather than
visible — and aspirational structure tends to drift, because
nobody can see whether a new function is in the right section.

**Suggested fix:** When a file's invariant requires section markers,
add them. Format:
```rust
// ──────────────────────────────────────────────────────────────
// (2) Cached-query path: build, sample, run
// ──────────────────────────────────────────────────────────────
```
or simpler `// ── (2) cached-query path ──`.

**Detection idea:** For each file mentioned in `runtime-invariants.md`
whose brief contains the string "// ──", grep that file for at least
one `^// ──` line. Zero hits = violation. This is a small
review-time shell check; doable as `check_section_markers` in
`lints/checks.sh`.

(This rule is narrow — it only fires on files whose invariant
specifically asks for markers. Today that's `translate/eval.rs`.
Future per-file invariants that grow a sub-concern list will pick
it up automatically.)

### Suggested AP-011: doc-comment-attachment-discipline (review-only)
**Pattern observed at eval.rs:9-14:**
> Two consecutive `///` blocks with no blank line between them
> attach to the next item as one merged comment, even when they
> were intended for different items defined further down the file.

**Why it might be bad:** A doc comment block written for function
B but placed before function A (because A was inserted later, or
B was moved away) silently re-attaches to A and disappears from B.
Cargo doc renders the wrong text; readers learn the wrong thing
about A and nothing about B. Cargo and clippy don't catch this.

**Suggested fix:** Each `///` block must be immediately followed
by the item it documents (no other `///` block in between, with
no blank line between two adjacent doc-comment paragraphs that
were meant for different items). If a doc comment is "between"
items, separate the previous block with a blank line.

**Detection idea:** Hard to mechanize reliably — distinguishing
"intended-for-this-item" from "left-stranded" requires reading
the prose. Review-only.

## Clean

Not clean. Two per-file-invariant violations (split imports,
missing section markers); one borderline placement
(`populate_enum_variants`); plus one unused import, one stale
module doc, and one detached doc comment as review-only notes.
Three new rules proposed (two mechanizable, one review-only).
