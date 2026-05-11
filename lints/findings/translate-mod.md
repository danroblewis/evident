# Findings: runtime/src/translate.rs

Reviewed against `lints/rules/` (AP-001..AP-008) and
`lints/runtime-invariants.md` as of HEAD (baf8078).

## Violations of existing rules

None. AP-001 (no library-specific identifiers) is clean — no
`SDL_`, `Sdl[A-Z]`, `Gl[A-Z]`, `Glsl`, `Audio[A-Z]`, dylib paths,
or platform-specific tokens. AP-002..AP-008 do not apply (this
file is neither under `examples/` nor a test file).

## Violations of per-file invariants (`runtime-invariants.md`)

The invariant for `translate.rs` enumerates the allowed
re-exports literally: "*the small set of public items
(`evaluate`, `build_cache`, `run_cached`, `sample_cached_inner`,
`Value`, `EvalResult`, `FieldKind`, `DatatypeRegistry`,
`CachedSchema`, `structural_names`, `structural_signature`)*"
and says the file "*Never widen the re-export list to expose
translate-internal types*".

The current file widens the surface in several ways:

### Re-export widening at translate.rs:40-46

> ```rust
> pub use eval::{build_cache, evaluate, evaluate_with_core, evaluate_with_extra_assertion,
>                 evaluate_with_extra_assertions,
>                 evaluate_with_program_and_body,
>                 run_cached, sample_cached_inner};
> pub use preprocess::{structural_names, structural_signature, StructuralSignature};
> pub mod preprocess_api { pub use super::preprocess::collect_referenced_names; }
> pub use types::{CachedSchema, DatatypeRegistry, EnumRegistry, EvalResult, FieldKind, Value};
> ```

Items re-exported beyond the documented allow-list:

  * `evaluate_with_core` — internal evaluate variant for unsat-core
    extraction.
  * `evaluate_with_extra_assertion` and `evaluate_with_extra_assertions`
    — internal evaluate variants for extra-assertion solving.
  * `evaluate_with_program_and_body` — internal evaluate variant
    used by `runtime.rs:866/900/1008` for body-replacement queries.
  * `StructuralSignature` — type, used by `runtime.rs` for cache
    keying. The invariant lists the *functions* `structural_names`
    and `structural_signature` but not the type.
  * `EnumRegistry` — used by `runtime.rs` (e.g. lines 61, 232, 421,
    1084). Lives in `translate/types.rs`; arguably translate-
    internal as the invariant for `types.rs` says "Defines the
    typed bindings shared by every other file in the translate
    pipeline" — i.e. internal to translate.

### New `pub mod` sub-modules at translate.rs:19-36 and 45

> ```rust
> pub mod ast_decoder {
>     pub use super::decode_ast::{decode_program, decode_effect, decode_effect_list,
>                                   decode_ffi_arg, decode_arg_list,
>                                   decode_result, decode_result_list,
>                                   DecodeError};
> }
>
> pub mod ast_encoder {
>     pub use super::encode_ast::{encode_program, encode_body_items_into_seq,
>                                  encode_effect_result, encode_effect_result_list,
>                                  EncodeError};
> }
> ```

> ```rust
> pub mod preprocess_api { pub use super::preprocess::collect_referenced_names; }
> ```

Three `pub mod` blocks each containing additional `pub use`
statements. The invariant says the body should be "`mod x;` and
`pub use`" — sub-modules with their own `pub use` lists are a
mechanism to expose translate-internal symbols (decode_ast,
encode_ast, preprocess) without those symbols appearing in the
allow-listed top-level re-exports. The boundary the invariant
describes ("the boundary exists on purpose") is being routed
around by namespacing.

The `ast_encoder`/`ast_decoder` modules in particular expose
~13 functions plus 2 error types from `encode_ast.rs` /
`decode_ast.rs`. Those modules' invariants describe them as
producing/consuming Z3 Datatype values for self-hosted compiler
passes — externalizing that many entry points means external
callers can drive the round-trip directly rather than going
through the runtime facade.

### Module-level doc comment references PROGRESS.md at translate.rs:6

> ```rust
> //! See `runtime/PROGRESS.md` for the layout rationale.
> ```

Not a rule violation, but worth flagging: `runtime/PROGRESS.md`
does not exist in the tree (only `runtime-rust/` historical
references and the path doesn't resolve). A doc-comment pointer
to a missing file is dead-link rot.

## Candidate new rules

### Suggested AP-009: no-pub-mod-in-module-entry-files

**Pattern observed at translate.rs:19, 29, 45:**
> ```rust
> pub mod ast_decoder { pub use super::decode_ast::{...}; }
> pub mod ast_encoder { pub use super::encode_ast::{...}; }
> pub mod preprocess_api { pub use super::preprocess::collect_referenced_names; }
> ```

**Why it might be bad.** Module-entry files (the ones whose
invariant says "Module entry — should be small (mod declarations
+ pub use re-exports)") are supposed to publish a flat,
allow-listed surface. Wrapping internal items in `pub mod
foo { pub use ... }` is a way to route around the allow-list:
the wrapper namespaces the symbols so they look "scoped" but
still makes them callable from outside the crate. Concretely,
`commands/test.rs:18` reaches in via
`use evident_runtime::translate::preprocess_api::collect_referenced_names`
to use a translate-internal helper. Once one such pseudo-namespace
exists, more accrete; the boundary the invariants describe
becomes vacuous.

**Suggested fix.** A module-entry file holds `mod x;` and
`pub use x::{specific, items};`. If a logically-grouped facade
is needed (e.g., the AST round-trip), promote the facade to a
real submodule file (`translate/ast_codec.rs`) that re-exports
from `encode_ast` / `decode_ast` and explicitly declares its
intent and contract. Alternatively, route the symbol through
`runtime.rs` (the published facade) and remove the back door.

**Detection idea.** grep for `^pub mod \w+ \{` in
`runtime/src/{translate,commands,event_sources}.rs` and
`runtime/src/lib.rs`. Real submodules use `pub mod x;` (no
brace); inline `pub mod x { ... }` in a module-entry file is
the smell. Comment-stripped grep:
`grep -n '^pub mod [a-z_]\+ {' runtime/src/translate.rs
runtime/src/commands.rs runtime/src/lib.rs`. Two-line check.

### Suggested AP-010: re-export-list-matches-documented-allow-list

**Pattern observed:** the `pub use` list in `translate.rs:40-46`
exposes 6 items beyond what `runtime-invariants.md` documents
as the allowed surface for `translate.rs`.

**Why it might be bad.** The invariants doc enumerates the
allowed public items per module-entry file; if the re-export
list silently drifts past that list, the doc becomes
informational fiction. Either the doc is wrong (and should be
amended deliberately, with a justification commit) or the file
is wrong (and the new re-exports indicate an internal that
escaped). Drift in either direction degrades the doc as a
review reference. Flagging the divergence forces an explicit
decision on each new exposure.

**Suggested fix.** When a new symbol needs to be re-exported,
update both the file and the invariants doc in the same commit.
Code review treats a re-export change without a paired
invariants-doc change as a smell.

**Detection idea.** Review-only — too hard to mechanize. Would
need a parser that extracts both the `pub use` list from the
.rs file and the bullet list from the .md file and diffs them.
Possible but high-effort; recommend keeping this as a reviewer
checklist item rather than an automated check.

## Clean

The file is clean of AP-001..AP-008 violations. The findings
above are against the per-file invariant in
`runtime-invariants.md`, not the existing rulebook.
