# Code organization standards

**Status: proposal — review before enforcement.**

This is a rewrite of an earlier draft that started rule-first
("MUST / MAY / MUST NOT" per directory). That was the wrong shape:
"MAY" is useless because it doesn't enforce anything, "MUST" is hard
to mechanically check without coverage tooling we don't have, and
the rules read as arbitrary because they weren't grounded in WHY.

This rewrite goes purpose → taxonomy → anti-patterns → rules.

## What this codebase is

Evident is a constraint programming language. This repo's job is to
**parse Evident source, translate it to Z3 constraints, solve, and
run multi-FSM programs that talk to C libraries via FTI bridges.**

Concretely, four end-user verbs map to what `runtime/` does:

  * `evident query` / `check` / `sample` — solve constraints in a `.ev` file.
  * `evident test` — discover and run `sat_*`/`unsat_*` claims.
  * `evident effect-run` — execute a multi-FSM program against the OS.
  * `evident lint` — surface translator-level diagnostics.

Everything in this repo serves one of those four verbs. Files that
don't serve any of them are dead weight; files that serve more than
one are usually mis-categorized.

## Why have code-organization standards at all

Two reasons, both negative:

**1. To prevent erosion via local quick fixes.** The canonical
example: SDL needed a vertex buffer, so an `SdlVertex` struct got
added to `runtime/src/ast.rs` (a 5-line change that "worked"). It
turned the language-core AST module into a place that knows about
SDL's struct layout. The fix is much bigger than the original 5
lines. A rule against library-specific identifiers in `ast.rs`
catches this kind of intrusion before it merges.

**2. To force conscious decisions when crossing concerns.** Some
quick fixes are right. Some indicate the structure is wrong and a
generic primitive should be added instead. The rule itself doesn't
make that decision — it makes the decision necessary instead of
sliding past unnoticed.

These rules are not about taste. Naming, formatting, comment style,
import ordering — those are `cargo clippy` and `cargo fmt` and
team convention. The rules here are about **layering**: which file
is allowed to know about which other file's concerns.

## Taxonomy: what kinds of files exist

A file's *role* is a stable answer to "what concern does this file
address." It's almost never the directory it's in (we've moved
files around three times this week). The role is what should
determine the rules.

### Roles in `runtime/src/`

| Role | Files (today) | Concern |
|---|---|---|
| **Language definition** | `ast.rs`, `lexer.rs`, `parser.rs` | What an Evident program IS (data shape + how text becomes that shape). Knows nothing about Z3, nothing about the runtime, nothing about C libraries. |
| **Translation** | `translate/eval.rs`, `inline.rs`, `exprs.rs`, `encode_ast.rs`, `decode_ast.rs` | AST ↔ Z3. Owns the mapping from Evident expressions/types to Z3 constraints. Knows the AST and Z3; nothing about effects, schedulers, or C libraries. |
| **Execution** | `runtime.rs`, `effect_loop.rs`, `effect_dispatch.rs` | Top-level API + multi-FSM scheduler + Effect → I/O dispatch. Knows about Effects but not about specific Effect implementations. |
| **FFI plumbing** | `ffi.rs` | libffi marshaling. Knows about C calling conventions but not about specific C libraries. |
| **Static analysis** | `subscriptions.rs` | Read/write-set inference per claim. Pure AST analysis. |
| **Library bridges** | `event_sources.rs` (currently 1390 lines, 9 sources) | Per-library typed-resource lifecycles. THE ONLY place library-specific Rust code may live. |
| **Bridge registry** | `fti.rs` | Dispatch table mapping Evident type names → bridge install functions. The boundary where execution-layer code names library-bridge code. |
| **CLI surface** | `commands/*.rs` | One file per `evident <subcommand>`. Wires lower layers to user-facing arguments. |
| **Pretty-printing** | `pretty.rs` | AST → string for diagnostics. Pure. |

The big code smell visible in this table: **`event_sources.rs` is
one role but nine concerns** (FrameTimer, SigintSource, StdinSource,
FileLineReader, WallClockSource, FileWatcherSource,
OneShotShellSource, SdlWindowSource, GlProgramSource). It violates
"one file per concern" by mixing every C-library lifecycle in one
place. That's a structural error, not a style one. Fixing it is
prerequisite to lintability.

### Roles in `stdlib/`

| Role | Files (today) | Concern |
|---|---|---|
| **Core types** | `runtime.ev` | Effect / Result / FFIArg / FTI types. Mandatory — every program transitively imports this. |
| **AST representation** | `ast.ev` | The Evident-side mirror of `runtime/src/ast.rs`, used by the self-hosted compiler passes. |
| **Self-hosted passes** | `passes/*.ev` | Compiler passes that the Rust binary loads at runtime (literal_types, iter_types, propagation, consistency, lint_duplicate_decls, desugar_passthrough). |
| **Library wrappers** | `posix.ev`, `sdl/*.ev`, `shader/program.ev` | Per-library FFI call wrappers. THE ONLY place `LibCall` / `FFICall` / dylib paths may live. |

`stdlib/` mixes two categories: things every program needs (`runtime.ev`)
or that the runtime itself loads (`passes/`, `ast.ev`), and things
specific to one C library (`sdl/`, `shader/`, `posix.ev`). The
contract for those categories is different — see "Reorganization"
below.

### Roles in `examples/`

| Role | Files | Concern |
|---|---|---|
| **Worked examples + integration tests** | `test_NN_<name>.ev` | Each is a multi-FSM program (the demo) plus inline `sat_*`/`unsat_*` static tests. Also the contract that the runtime does what it says it does — failing demos = failing runtime. |
| **Counterexample log** | `COUNTEREXAMPLES.md` | The honest list of runtime gaps surfaced by demo-writing or conformance triage. Not a TODO list of work; a list of things programs can't do today. |

### Roles in `tests/`

| Role | Files | Concern |
|---|---|---|
| **Conformance** | `tests/conformance/*.py` | Black-box CLI tests. Spec the LANGUAGE behavior; should pass against any correct implementation. |
| **Lang fixtures** | `tests/lang_tests/*.ev`, `tests/lang_tests/multi_fsm/*.ev` | Inputs to specific Rust integration tests. May exercise edge cases that demo-writers never would. |

## Anti-patterns observed

This is the input that makes the rules concrete. Each entry is a
real mistake from past sessions, named so we can refer to it.

Add new entries as we discover them. Each entry should be
falsifiable enough to write a test against.

### AP-1. Library-specific identifier in a language-core file

**Example.** `SdlVertex` struct in `runtime/src/ast.rs`,
`SdlVertexBuf` variant in `EffectFfiArg`,
`FfiArg::SdlVertexBuf(Vec<SdlVertex>)` in `runtime/src/ffi.rs`,
`decode_sdl_vertex` in `runtime/src/translate/decode_ast.rs`. Four
files in the language-core role each acquired SDL-specific code.
Comment in `ast.rs:386` literally said "SDL-specific until we have
a general 'packed struct array' FFI primitive" — past-self knew
this was wrong and shipped it anyway.

**Why it's bad.** Couples the language to one library. A second
library wanting a similar feature would have to add its own variant.
Removing or replacing SDL becomes a multi-file refactor.

**Detection.** String search for library names (SDL, Sdl, Gl[A-Z],
Glsl, Audio, etc.) in language-core role files.

### AP-2. Raw FFI primitive in a worked-example file

**Example.** Initial draft of `examples/test_16_sdl_red.ev`
contained `LibCall("/opt/homebrew/lib/libSDL2.dylib", "SDL_Init", …)`
inline. The rule exists in CLAUDE.md ("demo files MUST NOT contain
raw FFI calls"). The first SDL demo I wrote violated it, despite my
having written the rule a few commits earlier.

**Why it's bad.** Every program reaching directly into `LibCall`
re-implements the same dylib-path + signature + arg-marshaling work.
The wrapper-claims layer in `stdlib/modules/<library>/` exists to
do that work once. Programs that bypass it duplicate work and
couple to platform paths.

**Detection.** Grep for `LibCall|FFICall|FFIOpen|FFILookup` in
example files.

### AP-3. Hardcoded dylib path or library-symbol string in non-stdlib file

**Example.** `"/opt/homebrew/lib/libSDL2.dylib"` and
`"SDL_PumpEvents"` strings in `examples/*.ev`. The path locks
the program to one platform; the string makes the program know
about one library's symbols by name.

**Why it's bad.** Same family as AP-2. The path/symbol belongs in
a stdlib wrapper, behind a typed claim like `sdl_pump_events(out)`.

**Detection.** Grep for `\.dylib|\.framework/|/opt/homebrew/lib/`
in non-stdlib-wrapper files; grep for known C symbol prefixes
("SDL_", "gl[A-Z]", "ffi_") in example files.

### AP-4. xfail / skip markers as TODO sediment

**Example.** Conformance suite originally had 64 `xfail`-marked
tests. I added them as a "known failing, will fix later" mechanism.
A week later they were still there and no one had looked at them.
The right move was always: fix the test, fix the code, or delete
the test. Marking it as xfail is the third option dressed up as
something more.

**Why it's bad.** A test suite full of "known failures" is one
nobody trusts. New failures hide among old "known" ones.

**Detection.** Grep for `pytest.mark.xfail|pytest.mark.skip` in
conformance tests. Grep for `#[ignore]` in Rust tests.

### AP-5. Test passes via substring match through wrong code path

**Example.** `examples/test_10_spawn.ev` test driver originally
checked stdout contained `"parent spawned worker"`. That string was
the **parent's** Println, emitted regardless of whether the spawn
actually fired. The actual spawned-worker output (`"worker
spawned with id=7"`) never reached stdout — the runtime had a
SpawnFsm-on-Exit-tick bug — but the test passed because the parent's
line was enough to satisfy `contains`.

**Why it's bad.** Test driver passes give a false sense of
verification. The test's stated intent (does SpawnFsm work?) and
the test's actual assertion (does the parent print its line?) are
disconnected.

**Detection.** Manual review when writing assertions; harder to
automate than the others. A weaker proxy: prefer multi-line
ordered assertions ("must contain `A` then `B` then `C`") over
single-substring ("must contain `B`").

### AP-6. Placeholder output instead of computed value

**Example.** `examples/test_02_counter.ev` printing `"tick"` each
frame instead of `"tick 5"`, `"tick 4"`, …. `examples/test_09_two_fsms.ev`
printing `"got n"` instead of the actual value of `n`. The demo's
stated purpose (demonstrate IntToStr / shared world) is satisfied
by printing the value; the placeholder is an unconscious shortcut.

**Why it's bad.** A program "works" without exercising the feature
the demo is supposed to demonstrate. Combined with AP-5, you can
have green tests for a runtime that's silently broken.

**Detection.** Manual review when reading new demo source. A weak
linter heuristic: warn if a demo's stdout is identical across
states/iterations when it claims to be tracking changing values.

### AP-7. Stdlib helper without an `*_after` companion

**Example.** `stdlib/modules/sdl/render.ev` originally had
`set_draw_color(renderer ∈ Int, color, out)` taking a literal Int
renderer. Inside an `Effect::Seq` where the renderer comes from
`SDL_CreateRenderer`'s prior result, the helper was unusable —
you couldn't pass `ArgPriorResult(N)` through a typed `Int` slot.
Demos worked around it by inlining `LibCall` (triggering AP-2).

**Why it's bad.** The wrapper layer's contract is "you never need
raw FFI." When a wrapper helper has a usability gap inside Seq,
demos break the contract. Fixing the wrapper (adding a parallel
`*_after(prior_idx ∈ Int, ...)` variant) restores it.

**Detection.** AST: every `claim X(handle ∈ Int, …, out ∈ Effect)`
in `stdlib/modules/` should also have a sibling `claim X_after(prior_idx ∈ Int, …, out ∈ Effect)`.

### AP-8. Demo file missing `sat_*`/`unsat_*` claims

**Example.** Hypothetical, but easy to slip in: a new demo gets
written, runs end-to-end, ships. No inline static-test claims.
The demo is now an example only, not a test. Drift.

**Why it's bad.** Examples that aren't tests stop catching
regressions. The whole point of `examples/test_NN_<name>.ev` is
that the file is BOTH.

**Detection.** AST: load every `examples/test_*.ev`, verify it
contains ≥1 claim whose name starts with `sat_` or `unsat_`.

### AP-9. Demo file missing FSM-shape claim

**Example.** Hypothetical: someone puts a pure-static-test file
under `examples/`. It passes `evident test` but has no runnable
program — it's in the wrong directory.

**Why it's bad.** Examples are supposed to be runnable programs
with inline tests. Static-only files belong in `tests/lang_tests/`.

**Detection.** AST: load every `examples/test_*.ev`, verify ≥1
claim has the FSM shape (state pair + `last_results ∈ ResultList`
+ `effects ∈ EffectList`).

### AP-10. Demo not in EXPECTATIONS table

**Example.** A demo gets added under `examples/` but not registered
in `runtime/tests/demos.rs::EXPECTATIONS`. The demo doesn't run
in CI; broken state goes uncaught.

**Why it's bad.** The whole "demo IS test" contract relies on the
test driver running each demo. Skipping = unmaintained.

**Detection.** Cross-file: list `examples/test_*.ev`, list
`EXPECTATIONS` rows, set-difference. Allow opt-out via a header
tag (`-- interactive` for demos that need real stdin or SIGINT).

### AP-11. Long single-file module mixing concerns

**Example.** `runtime/src/event_sources.rs` at 1390 lines
containing 9 distinct sources. Each source is its own struct +
impl + EventSource impl + Drop, all in one file.

**Why it's bad.** Hides the "one file per concern" pattern. New
contributors don't know whether to add their bridge to this file
or to make a new one. Code review becomes harder. Imports get
tangled.

**Detection.** Soft heuristic. A `.rs` file declaring more than 2
`pub struct`s with `EventSource` implementations is a candidate
for splitting.

### AP-12. Self-evident comment

**Example.** `// Update the dot's x position by adding velocity * dt to current.`
on a line that says `nxt.pos.x = cur.pos.x + cur.vel.x * input.dt / 1000`.
The comment restates what the names already say.

**Why it's bad.** Costs reader time, decays as code changes.
CLAUDE.md guidance: comment WHY, not WHAT.

**Detection.** Hard to mechanize without false positives. Skip for
now; rely on review.

## Rules derived from anti-patterns

Each rule corresponds to one or more anti-patterns and is checkable.
Each lists what it catches, where it lives, and roughly what it
looks like.

| Rule | Catches | Implementation |
|---|---|---|
| `R1: language_core_is_library_agnostic` | AP-1 | Grep scanner. List of L0 files (everything in `runtime/src/` except `event_sources/`, `commands/`, `fti.rs`). Forbidden patterns: `SDL[_A-Za-z]`, `Sdl[A-Z][a-z]`, `\bGl[A-Z]`, `Glsl`, `Audio[A-Z]`, `glClear`, `glProgram`, `\.dylib`, `\.framework/`, `/opt/homebrew/`. Doc-comment-only mentions are OK; the scanner ignores lines that start with `//` or `///`. |
| `R2: examples_no_raw_ffi` | AP-2 | Grep scanner. Forbidden in `examples/`: `\bLibCall\b`, `\bFFICall\b`, `\bFFIOpen\b`, `\bFFILookup\b`. Comment-only ignored. |
| `R3: examples_no_dylib_paths` | AP-3 | Grep scanner. Forbidden in `examples/`: dylib path patterns + known C symbol prefixes (`"SDL_`, `"gl[A-Z]`, `"ffi_`). Comment-only ignored. |
| `R4: no_xfail_in_conformance` | AP-4 | Grep scanner. Forbidden in `tests/conformance/`: `pytest.mark.xfail`, `pytest.mark.skip`, `pytest.skip`. |
| `R5: no_ignore_in_rust_tests` | AP-4 | Grep scanner. Forbidden in `runtime/tests/`: `#[ignore]`. |
| `R6: examples_have_sat_claims` | AP-8 | AST test (Rust). Load each `examples/test_*.ev`, count claims whose name starts with `sat_`/`unsat_`. Fail if zero. |
| `R7: examples_have_fsm_claim` | AP-9 | AST test (Rust). Load each `examples/test_*.ev`, look for at least one claim with FSM-shape Membership items. Fail if zero. |
| `R8: examples_in_expectations_table` | AP-10 | AST test (Rust). Read EXPECTATIONS rows from `runtime/tests/demos.rs`, list `examples/test_*.ev`. Set-difference. Allow opt-out via `-- interactive` header tag. |
| `R9: stdlib_module_has_after_variants` | AP-7 | AST test (Rust). Walk `stdlib/modules/<lib>/*.ev`. For each claim with shape `(handle ∈ Int, ..., out ∈ Effect)`, verify a sibling `<name>_after(prior_idx ∈ Int, ..., out ∈ Effect)` exists. Fail with the missing list. (May warn-only initially.) |
| `R10: bridge_files_one_concern_each` | AP-11 | Grep scanner. Each file under `runtime/src/event_sources/` declares exactly one `pub struct *Source`. Files declaring more than one fail. |
| `R11: bridge_registered_in_fti` | AP-1 + AP-11 | AST test (Rust). For each `pub struct *Source` declared under `runtime/src/event_sources/`, verify `runtime/src/fti.rs::INSTALLERS` references it via the matching install fn. |
| `R12: stdlib_core_doesnt_libcall` | (preventive) | Grep scanner. `LibCall`/`FFICall` allowed only under `stdlib/modules/`. Files at top of `stdlib/` and under `stdlib/passes/` must not contain them. |

Two anti-patterns (AP-5, AP-6, AP-12) are review-only — too hard to
mechanize without high false-positive rate. Documenting them in this
list at least gives reviewers a checklist.

## Reorganization implied by the taxonomy

To make the rules express-able cleanly:

### 1. Split `runtime/src/event_sources.rs` into `event_sources/`

```
runtime/src/event_sources/
  mod.rs                  EventSource trait, SchedulerEvent, WriteQueue
  frame_timer.rs          FrameTimer
  sigint.rs               SigintSource
  stdin.rs                StdinSource
  file_reader.rs          FileLineReader, FileWatcherSource
  wall_clock.rs           WallClockSource
  shell.rs                OneShotShellSource
  sdl_window.rs           SdlWindowSource
  gl_program.rs           GlProgramSource
```

R10 + R11 become enforceable. New bridges = new file + new
`INSTALLERS` row.

### 2. Tier `stdlib/`

```
stdlib/                   Core. Mandatory or runtime-loaded.
  runtime.ev              Core types
  ast.ev                  AST mirror
  passes/                 Self-hosted passes

stdlib/modules/           Per-library wrappers. Opt-in.
  posix/posix.ev
  sdl/window.ev
  sdl/render.ev
  sdl/gl.ev
  shader/program.ev
```

(Goes under `stdlib/modules/`, not a new top-level `modules/` —
preserves the `import "stdlib/..."` convention. Each library lives
in its own directory even if it's one file, since FFI wrappers
tend to grow.)

R2, R3, R9, R12 become checkable: `examples/` may not contain raw
FFI; `stdlib/modules/<lib>/` is where it lives; `stdlib/` core
must not contain it.

## What this doc doesn't try to enforce

  * **Naming style of internal Rust functions / variables.** `cargo
    clippy` covers it.
  * **Code formatting.** `cargo fmt`.
  * **Comment style.** AP-12 documents the principle; mechanizing
    is too noisy.
  * **Test coverage percentages.** We don't have coverage tooling;
    AP-10 catches the demo-coverage case which is what actually
    matters.
  * **Operator precedence footguns.** CLAUDE.md documents these
    (`=` vs comparisons, `⇒` vs `∧`). Could lint via AST later
    once we have a few real examples.
  * **Comment density / docstring presence.** Reasonable judgment.

## Process

If we agree on this:

  1. Add to this doc any anti-patterns I missed from earlier sessions
     (the AP list is the load-bearing part — every rule traces to
     a real mistake).
  2. Decide naming: `stdlib/modules/` vs `modules/` vs `bindings/`.
  3. Do the rearrangement (one PR per move).
  4. Implement R1-R5 (grep scanners) as a Phase 0 in `test.sh`.
  5. Implement R6-R11 (AST tests) under `runtime/tests/lints.rs`.
  6. Add the rule list to CLAUDE.md so agents see it without
     reading this whole doc.

If we disagree, push back on specific anti-patterns or rules; the
doc is meant to be edited.
