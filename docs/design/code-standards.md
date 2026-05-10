# Code organization standards

**Status: proposal — review before enforcement.**

This document specifies what code belongs in which directory, why,
and how those rules will be mechanically enforced. Goals:

  1. **Stop erosion.** Quick fixes that put library-specific code
     into language-core files (the `SdlVertex` intrusion in `ast.rs`
     was the canonical example) corrode the project. Linters should
     refuse those PRs.
  2. **Make adding a new C library obvious.** A contributor adding
     SDL_Audio support should know exactly where each piece goes
     without reading prior PRs to find out.
  3. **Force a conscious decision when crossing layers.** The right
     thing to do is sometimes to add a generic primitive (option B
     of the "remove SdlVertexBuf" question) rather than a one-off
     library-specific entry. The linter rejecting the one-off makes
     the conscious decision necessary.

## The layer model

```
┌─────────────────────────────────────────────────────────────┐
│  L3   examples/, tests/lang_tests/                          │
│       Pure Evident programs. NO FFI, NO library symbols.    │
├─────────────────────────────────────────────────────────────┤
│  L2   modules/<library>/                                    │
│       Per-library Evident wrappers. LibCall + dylib paths   │
│       LIVE HERE (and only here).                            │
├─────────────────────────────────────────────────────────────┤
│  L1   runtime/src/event_sources/<library>.rs                │
│       Per-library Rust bridges. SDL/GL/AppKit-specific Rust │
│       code lives in dedicated modules under event_sources/. │
├─────────────────────────────────────────────────────────────┤
│  L0   runtime/src/{ast,lexer,parser,translate,runtime,…}.rs │
│       Language + library agnostic. Knows nothing about SDL, │
│       GL, audio, or any specific C library. If `runtime/`   │
│       had only L0 + L1's registry plumbing, the language    │
│       would still build and run programs.                   │
└─────────────────────────────────────────────────────────────┘
```

**Each layer may call into layers below it. None may call up.**

## Proposed rearrangement (BEFORE enforcement)

These moves bring the tree into a state the linter can enforce. Some
are renames; some are file splits. Each is independently justifiable.

### 1. Split `runtime/src/event_sources.rs`

Currently 1390 lines containing 9 distinct sources, all
library-specific (FrameTimer, SigintSource, StdinSource,
FileLineReader, WallClockSource, FileWatcherSource, OneShotShellSource,
SdlWindowSource, GlProgramSource). Split into:

```
runtime/src/event_sources/
  mod.rs                  EventSource trait, SchedulerEvent enum,
                          WriteQueue helpers (the only generic stuff)
  frame_timer.rs          FrameTimer — periodic ticks
  sigint.rs               SigintSource — Ctrl-C handler
  stdin.rs                StdinSource — line reader
  file_reader.rs          FileLineReader, FileWatcherSource — fs watching
  wall_clock.rs           WallClockSource
  shell.rs                OneShotShellSource — synchronous shell exec
  sdl_window.rs           SdlWindowSource (window + GL context + VAO + viewport)
  gl_program.rs           GlProgramSource (shader compile + link)
```

Each file owns one bridge. Adding a new bridge = new file under
`event_sources/`, new entry in `fti.rs::INSTALLERS`. The linter
enforces: nothing else under `runtime/src/` may contain
library-specific symbols.

### 2. Tier `stdlib/`

Today `stdlib/` mixes two distinct categories:

| Category | What | Files today |
|---|---|---|
| **Language core** | Types every Evident program transitively depends on; passes the Rust binary loads | `runtime.ev`, `ast.ev`, `passes/*.ev` |
| **Library wrappers** | FFI to specific C libraries | `posix.ev`, `sdl/*.ev`, `shader/*.ev` |

Split top-level:

```
stdlib/                   Language core. Every program transitively
                          imports stdlib/runtime.ev.
  runtime.ev              Effect, Result, ArgList, FTI types
  ast.ev                  AST representation (used by self-hosted
                          passes that the Rust binary runs)
  passes/                 Self-hosted compiler passes loaded by
                          `evident infer-types`, `desugar`, `lint`

modules/                  Per-library wrappers. Opt-in: a program
                          imports only what it needs.
  posix/posix.ev          libc (was stdlib/posix.ev)
  sdl/                    SDL2 wrappers (was stdlib/sdl/)
    window.ev
    render.ev
    gl.ev
  shader/program.ev       OpenGL shader (was stdlib/shader/program.ev)
```

Renaming captures real semantic difference: `stdlib/runtime.ev` is
mandatory for any FSM program; `modules/sdl/window.ev` is only there
if you want a window.

(Other names considered: `bindings/`, `wrappers/`, `plugins/`. Going
with `modules/` because it matches what users in other ecosystems —
Python, Node, Rust — would expect.)

### 3. No other moves

`examples/`, `tests/conformance/`, `tests/lang_tests/`, `runtime/`,
`docs/` all stay as-is. They were just sorted out in the previous
restructure.

## Per-directory rules

Each row lists what MUST be there, what MAY be there, what MUST NOT,
and the lint-check pattern.

### `runtime/src/{ast,lexer,parser,translate,runtime,pretty,effect_loop,effect_dispatch,ffi,subscriptions,fti}.rs` — L0 core

| | |
|---|---|
| **Purpose** | Language and runtime infrastructure. Generic, library-agnostic. |
| **MUST** | Compile and pass tests if every L1 file (event_sources/*) were deleted, modulo trivial registry-table changes. |
| **MAY** | Reference L1 modules by name in the FTI registry table (`fti.rs`). |
| **MUST NOT** | Contain identifiers matching `SDL`, `Sdl`, `Gl[A-Z]`, `Glsl`, `Audio`, `glClear`, `glProgram`, etc. — anything specific to a real C library. The exception: literal example strings inside doc comments are OK. |
| **MUST NOT** | Contain `#[repr(C)]` structs that mirror a specific C library's struct layout. (Generic `#[repr(C)]` for libffi marshaling primitives is fine.) |
| **MUST NOT** | Contain hardcoded dylib paths (`/opt/homebrew/`, `.dylib`, `.framework/`, etc.). |
| **Lint** | grep-based file scanner, runs as part of `cargo test`. Fails on any of the patterns above appearing outside L1 paths. |

### `runtime/src/event_sources/*.rs` — L1 bridges

| | |
|---|---|
| **Purpose** | Per-library FTI bridge implementations. One file per bridge. |
| **MUST** | Implement the `EventSource` trait. |
| **MUST** | Be the ONLY place outside `commands/` that imports `libloading` or `libffi` directly for opening a specific C library. |
| **MAY** | Contain library-specific symbols, `#[repr(C)]` structs, hardcoded dylib paths, OS-conditional code. |
| **MAY NOT** | Reach into other L1 files unless going through a public trait method. (No `crate::event_sources::sdl_window::SdlWindowSource::__internal`.) |
| **Naming** | One `pub struct <Library>Source` per file. File named after the resource (`sdl_window.rs`, `gl_program.rs`). |
| **Lint** | (a) verify each file declares exactly one `pub struct *Source`; (b) verify each is registered in `fti.rs::INSTALLERS`; (c) verify imports stay within the layer. |

### `runtime/src/fti.rs` — the registry

| | |
|---|---|
| **Purpose** | Single dispatch table from Evident type name → install function. |
| **MUST** | Be the only place that names L1 modules from L0. |
| **MUST** | Mirror every entry in `INSTALLERS` with a `type` declaration in `stdlib/runtime.ev`. |
| **Lint** | Cross-check: walk `INSTALLERS`, load `stdlib/runtime.ev`, verify each name is a declared type. (AST-based test, not grep.) |

### `runtime/src/commands/*.rs` — CLI subcommands

| | |
|---|---|
| **Purpose** | One file per CLI subcommand. |
| **MUST** | Be the only place containing `cmd_*` entry points. |
| **MAY** | Reference any layer below — they're the user-facing surface that wires everything. |
| **Lint** | Each file matches `cmd_<command>.rs` shape (declares exactly one `pub fn cmd_<name>`). |

### `runtime/tests/*.rs` — test harness

| | |
|---|---|
| **Purpose** | Rust integration tests. |
| **MUST** | Reference test fixtures via `../tests/lang_tests/` or `../examples/` paths only. |
| **MUST NOT** | Embed Evident source as Rust string literals beyond ~20 lines (factor into a fixture file). |
| **MAY** | Spawn the binary via `Command::new(env!("CARGO_BIN_EXE_evident"))`. |

### `stdlib/` (post-tier) — language core

| | |
|---|---|
| **Purpose** | Mandatory or near-mandatory Evident files. |
| **MUST** | Live at top level of `stdlib/` OR under `stdlib/passes/`. |
| **MAY** | Contain `LibCall` to libc/system functions ONLY if necessary for core runtime support (e.g. if any). |
| **MUST NOT** | Contain library-specific imports beyond the core types. |
| **Lint** | Whitelist: `stdlib/runtime.ev`, `stdlib/ast.ev`, `stdlib/passes/*.ev`. Anything else under `stdlib/` is a lint error (move to `modules/`). |

### `modules/<name>/*.ev` — per-library wrappers

| | |
|---|---|
| **Purpose** | FFI wrappers for one C library each. |
| **MUST** | Live under `modules/<name>/` (a directory, even for one-file modules — leaves room for growth). |
| **MAY** | Contain `LibCall`, `FFICall`, `FFIOpen`, `FFILookup`, hardcoded dylib paths. |
| **MUST** | Be importable as `import "modules/<name>/<file>.ev"`. |
| **MUST** | Have all top-level claims be `Effect`-builders (`out ∈ Effect` last param) — they don't define FSMs themselves; they build effects FSMs emit. |
| **Naming** | Claims named `<verb>_<noun>` snake_case (`sdl_create_window`, `gl_clear_color`, `render_present_after`). |
| **Lint** | (a) every file under `modules/` follows the directory rule; (b) no non-FFI files lurking under `modules/` (e.g. someone putting an FSM here); (c) claim-name regex check via grep. |

### `examples/test_NN_<name>.ev` — worked examples + integration tests

(Already documented in CLAUDE.md "Conventions for `examples/`". Below
restates as enforceable rules.)

| | |
|---|---|
| **Naming** | `test_NN_<name>.ev`, sequential N, lowercase + underscore name. |
| **MUST** | Contain at least one FSM-shape claim (state pair + ResultList + EffectList). |
| **MUST** | Contain at least one `claim sat_*` or `claim unsat_*` test. |
| **MUST** | Have a row in `runtime/tests/demos.rs::EXPECTATIONS` (unless explicitly skipped — see "interactive only" tag below). |
| **MUST NOT** | Contain `LibCall`, `FFICall`, `FFIOpen`, `FFILookup`, or any hardcoded dylib path. |
| **MUST NOT** | Contain library symbol strings (`"SDL_PumpEvents"`, `"glClear"`, etc.) — those go in `modules/`. |
| **MAY** | Be tagged `-- interactive` in the file's header to opt out of the EXPECTATIONS requirement (e.g. `test_15_signal` waits for SIGINT). |

### `tests/lang_tests/*.ev` and `tests/lang_tests/multi_fsm/*.ev` — Rust regression fixtures

| | |
|---|---|
| **Purpose** | Inputs to specific Rust integration tests. |
| **Naming** | Numbered (`NN_<name>.ev`) under `multi_fsm/`; `test_<feature>.ev` at top level. |
| **MAY** | Use any language feature, including ones we don't expose to demo writers (they're testing the runtime). |
| **MUST** | Be referenced by a Rust test under `runtime/tests/`. |
| **Lint** | Cross-check: every `.ev` under `tests/lang_tests/` appears in at least one `.rs` file under `runtime/tests/`. |

### `tests/conformance/*.py` — black-box CLI conformance

| | |
|---|---|
| **Purpose** | Spec the language behavior independent of implementation. |
| **MAY** | Run subprocess invocations of `evident` only. |
| **MUST NOT** | Import from `runtime/` (as Python) or any other implementation internal. |
| **MUST NOT** | Contain `pytest.skip` / `xfail` markers. (If a test fails, fix it, file the bug in `examples/COUNTEREXAMPLES.md`, or delete it.) |

## Lint implementation

Two flavors:

### A. Grep-style scanners — `tests/lints/*.sh`

Fast, run from `test.sh` or as a `cargo test` driver. One script per
rule. Each prints offending file:line and exits non-zero on failure.

Examples:

```bash
# tests/lints/no_ffi_in_examples.sh
violations=$(grep -rln 'LibCall\|FFICall\|FFIOpen\|FFILookup\|\.dylib\|\.framework/' examples/ 2>/dev/null)
if [ -n "$violations" ]; then
  echo "FAIL: examples/ contains FFI primitives or dylib paths:"
  echo "$violations" | xargs -I{} grep -nE 'LibCall|FFICall|FFIOpen|FFILookup|\.dylib|\.framework/' {}
  exit 1
fi
```

```bash
# tests/lints/no_library_specific_in_l0.sh
# Forbidden: SDL, Gl<UPPER>, Glsl, Audio in any runtime/src/ file
# OUTSIDE runtime/src/event_sources/ and runtime/src/commands/
PATTERN='SDL[A-Z_]|Sdl[A-Z][a-zA-Z]|^[^/]*Gl[A-Z][a-zA-Z]|Glsl|^[^/]*Audio'
violations=$(find runtime/src -name '*.rs' \
  -not -path 'runtime/src/event_sources/*' \
  -not -path 'runtime/src/commands/*' \
  | xargs grep -lE "$PATTERN")
...
```

Wired into `test.sh` as Phase 0 — they run before everything else
because they're cheap (~50ms) and a layering violation should
short-circuit the rest.

### B. AST-based — `runtime/tests/lints.rs`

Slower but precise. Uses the runtime's own parser/loader to walk
Evident source ASTs. Used for rules grep can't express:

  * **EXPECTATIONS coverage**: every `examples/test_*.ev` has a row in
    `runtime/tests/demos.rs::EXPECTATIONS` OR is tagged interactive.
  * **FTI registry coverage**: every name in `fti.rs::INSTALLERS` is a
    declared type in `stdlib/runtime.ev`.
  * **`examples/` shape**: each file declares ≥1 FSM-shape claim AND
    ≥1 `sat_*`/`unsat_*` claim.
  * **No L1 reach**: imports between L1 files only via the trait.

Each rule is a `#[test] fn lint_*` in `runtime/tests/lints.rs`. Cargo
test runs them as part of the standard suite. Failures are normal
test failures with a clear message ("examples/test_42_foo.ev has no
sat_*/unsat_* claim — required for L3 demos").

### Wiring

`test.sh` gains a Phase 0:

```
── Phase 0: lints ──
✓ no_ffi_in_examples
✓ no_library_specific_in_l0
✓ examples_have_sat_claims
... 12 lints passed
```

Phase 0 failures fail the run before any compilation happens.
Iteration: `./test.sh --lints-only` for fast feedback while writing
new code in a sensitive directory.

## Examples of rules biting

These would have caught real issues from the past few sessions:

  * **SdlVertexBuf intrusion in `ast.rs`** — `no_library_specific_in_l0`
    catches `SdlVertex`, `SdlVertexBuf` in `runtime/src/ast.rs`.
  * **`LibCall` in `examples/test_16_sdl_red.ev` (initial draft)** —
    `no_ffi_in_examples` catches it.
  * **`/opt/homebrew/lib/libSDL2.dylib` in a demo file** —
    `no_dylib_paths_in_examples` catches it.
  * **An example missing inline `sat_*` claims** — `examples_have_sat_claims`
    catches it.
  * **A new `pub struct FooSource` in `event_sources.rs` not registered
    in `fti.rs`** — `fti_registry_coverage` catches it.

## What this doc does NOT enforce (and why)

  * **Naming style of internal Rust functions.** `cargo clippy` covers
    this. We don't reinvent it.
  * **Code formatting.** `cargo fmt` covers it.
  * **Comments / docstrings.** Reasonable judgment; the rules in
    CLAUDE.md ("default to no comments; explain WHY when non-obvious")
    are guidance, not lint-enforced.
  * **Test coverage percentages.** Coverage tools catch real holes; the
    `EXPECTATIONS` table catches *demo* coverage which is the part that
    actually matters here.

## Staging

If we agree on this doc:

  1. **Rearrange** (one PR per move): split `event_sources.rs` into
     `event_sources/`; tier `stdlib/` into `stdlib/` + `modules/`. Each
     of these is mechanical and breaks existing imports — fix the
     imports, run `./test.sh`, commit.
  2. **Implement Phase 0 lints** (one script per rule). Wire into
     `test.sh`. Fix any violations the lints surface in current code.
  3. **Implement AST-based lints** as `runtime/tests/lints.rs`.
  4. **Add the rule list to CLAUDE.md** so agents see it in the
     context window without having to read this doc fully.

If we disagree on this doc, push back on the lines that don't fit and
we'll revise before any rearrangement happens.
