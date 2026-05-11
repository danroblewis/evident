# Findings: runtime/src/lib.rs

Reviewed against `lints/rules/` as of HEAD (188c682).

## Violations of existing rules

None of the existing AP rules (AP-001 through AP-008) target `lib.rs`-style
public-API hygiene, so no current rule fires. Brief check:

- **AP-001** (no library-specific code in language core): `lib.rs` lists no
  library-specific module — clean.
- **AP-002 / 003 / 004 / 005 / 006 / 007 / 008** all scope to examples,
  conformance, or test files — out of scope for `lib.rs`.

## Per-invariant audit (the brief's actual question)

The runtime-invariants brief for `lib.rs` says: "A sub-module is `pub mod`
only if external callers need to reach into it; otherwise it's `mod`
(crate-internal)." External callers = `runtime/src/commands/*`,
`runtime/tests/*`, embedders. I cross-referenced every `pub mod` against
`grep -rn "evident_runtime::<name>"` over `runtime/tests/` and
`runtime/src/commands/`.

Results:

| Module declared `pub mod` | External-caller use found? | Justified? |
|---|---|---|
| `ast` | yes — `commands/test.rs`, `commands/check.rs`, `commands/desugar.rs`, `commands/infer_types.rs`, `tests/desugar_passthrough.rs` | ✓ |
| `effect_dispatch` | yes — `commands/effect_run.rs`, `tests/effect_loop.rs`, `tests/scheduler_delta.rs`, `tests/multi_fsm.rs` (via `effect_dispatch::DispatchContext`) | ✓ |
| `effect_loop` | yes — `commands/effect_run.rs`, `tests/effect_loop.rs`, `tests/scheduler_delta.rs`, `tests/multi_fsm.rs` | ✓ |
| `ffi` | **no external use found** | flag (see below) |
| `lexer` | **no external use found** | flag |
| `parser` | **no external use found** | flag |
| `pretty` | yes — `commands/test.rs` (`use evident_runtime::pretty`) | ✓ |
| `translate` | yes — `commands/test.rs` (`translate::preprocess_api`), `tests/roundtrip_ast.rs` (`translate::ast_decoder`), and re-exports declared in `translate.rs` brief | ✓ |
| `runtime` | **no external use of `evident_runtime::runtime::*` found** — only `pub use runtime::{EvidentRuntime, QueryResult, Value}` at crate root | flag (see below) |
| `subscriptions` | yes — `tests/subscriptions_demo.rs` (`use evident_runtime::subscriptions::world_access_sets`) | ✓ |
| `event_sources` | **no external use found** | flag |
| `fti` | **no external use found** | flag |

### Findings — overpermissive `pub mod`

**`pub mod ffi` at lib.rs:10**
> `pub mod ffi;`

Per the runtime-invariants doc: "`ffi.rs` … Importers: `effect_dispatch`
(the only caller), and any test that exercises the FFI primitive
directly." No test under `runtime/tests/` imports `evident_runtime::ffi`
today; `effect_dispatch` is in the same crate and would access via
`crate::ffi`. Should be `mod ffi;`.

**`pub mod lexer` at lib.rs:11**
> `pub mod lexer;`

Per invariants: "Importers: `parser.rs` … transitively, anyone parsing."
All consumption is through `parser.rs`; no external caller imports
`evident_runtime::lexer`. Should be `mod lexer;`.

**`pub mod parser` at lib.rs:12**
> `pub mod parser;`

Per invariants: "Importers: `runtime.rs` (top-level `load_source`)."
External callers go through `EvidentRuntime::load_source(...)`; none import
`evident_runtime::parser` directly. Should be `mod parser;`.

**`pub mod runtime` at lib.rs:15**
> `pub mod runtime;`
> `pub use runtime::{EvidentRuntime, QueryResult, Value};`

The line below already publishes the canonical facade types via `pub use`.
No caller writes `evident_runtime::runtime::*` — they write
`evident_runtime::EvidentRuntime`. The `pub mod` is redundant: `mod
runtime;` plus the existing `pub use` would expose exactly what's used.
Leaving the path public also leaks the internal module name as a
back-channel, contradicting the invariant that "niche internal types
remain accessible only through their owning module's path."

**`pub mod event_sources` at lib.rs:17**
> `pub mod event_sources;`

Used internally by `effect_loop` and `fti`. No external caller imports
`evident_runtime::event_sources`. Per the bridges-layer invariants
(Group 5), bridge files communicate with the scheduler only through the
`EventSource` trait surface — not by being publicly addressable from
outside the crate. Should be `mod event_sources;`.

**`pub mod fti` at lib.rs:18**
> `pub mod fti;`

Per invariants: "fti is the registry, nothing more." Used internally by
`effect_loop` (the scheduler walks `INSTALLERS`) and references
`event_sources` install fns. No external caller imports
`evident_runtime::fti`. Should be `mod fti;`.

### Net effect

Six of the eleven `pub mod` declarations (`ffi`, `lexer`, `parser`,
`runtime`, `event_sources`, `fti`) appear to be unjustified. Tightening
them to `mod` would shrink the public API surface to exactly what
external callers reach for today, matching the brief's intent.

## Candidate new rules

### Suggested AP-009: `pub mod` only when external callers exist

**Pattern observed at runtime/src/lib.rs:7-18:**
> ```
> pub mod ast;
> pub mod effect_dispatch;
> pub mod effect_loop;
> pub mod ffi;
> pub mod lexer;
> pub mod parser;
> pub mod pretty;
> pub mod translate;
> pub mod runtime;
> pub mod subscriptions;
> pub mod event_sources;
> pub mod fti;
> ```

Every internal module is published by default. Six of the eleven have no
external caller; they're `pub` by accumulation, not by design. This is
the failure mode the runtime-invariants doc names directly: "A wide
`lib.rs` is a sign the public API has accumulated rather than been
designed."

**Why it might be bad:** A module published as `pub mod` becomes part of
the crate's API contract. Once external callers (or worse, downstream
embedders) start importing `evident_runtime::ffi::Foo`, the runtime can
no longer rename / reshape / split that module without a breaking
change. Defaulting to `pub mod` flips the burden the wrong way: the
intentional choice should be to publish, not to hide.

**Suggested fix:** Audit `pub mod` declarations in `runtime/src/lib.rs`
periodically. For each, confirm at least one consumer under
`runtime/src/commands/`, `runtime/tests/`, or a known external embedder
imports `evident_runtime::<name>::<something>`. If none does, demote to
`mod <name>;`. Re-export specific facade items via `pub use` at the
crate root only when they're part of the canonical public API.

**Detection idea:** Scriptable. For each `pub mod NAME;` line in
`runtime/src/lib.rs`, grep for `evident_runtime::NAME` in
`runtime/tests/` and `runtime/src/commands/`. Zero hits ⇒ flag. The
check has false positives (an embedder outside the repo could be using
the module), so it should be a soft warning the maintainer reviews, not
a hard CI fail. Could live as a `check_pub_mod_unused` shell function in
`lints/checks.sh`.

This candidate clears the bar (observable in concrete syntax,
specific fix, likely to recur as new modules get added). I'm leaving it
as a candidate rather than landing it — adding a rule + check that
hand-encodes the consumer-set requires a maintainer call on whether
"unused outside the crate" is the bright-line definition (vs., e.g.,
"intentionally documented as part of the public API").
