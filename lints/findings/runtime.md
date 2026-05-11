# Findings: runtime/src/runtime.rs

Reviewed against `lints/rules/` as of `baf8078`.

## Violations of existing rules

### AP-001 at runtime/src/runtime.rs:486-490
> ```rust
> const STDLIB_SHIMS: &[&str] = &[
>     "stdlib/sdl.ev",
>     "stdlib/io.ev",
> ];
> if STDLIB_SHIMS.contains(&import_path.as_str()) {
> ```

The runtime facade hardcodes a list of stdlib paths, one of which
names a specific C-library wrapper (`sdl`). AP-001 forbids
library-specific identifiers in the language-core role; runtime.rs
is in scope per AP-001's "Apply to" list. The grep token classes
in AP-001 don't catch lowercase `sdl` in a string literal, but the
pattern's intent — "the language core must compile and work if
every C library binding were deleted" — is squarely violated here:
removing SDL would require editing this list. The neighbouring
comment compounds the issue by referencing Python-only modules
(`executor::load_io_stdlib`, `plugins::sdl::STDLIB_SDL_EV`,
`cmd_execute`) that don't exist in the Rust crate, suggesting this
shim list was carried over without re-grounding it in the Rust
runtime's actual layering.

## Candidate new rules

### Suggested AP-020: facade-method-purpose-must-not-name-execution-layer

**Pattern observed at runtime/src/runtime.rs:1043-1105:**
> ```rust
> /// Like `query_with_program_value` but pins multiple enum-typed
> /// variables in one solve. Used by the effect loop to pin both
> /// `state` and `last_results` per step.
> pub fn query_with_pinned_datatypes(...) { ... }
>
> /// Like `query_with_pinned_datatypes` but also accepts a `given`
> /// map for scalar pins (Int/Bool/String/Real values). Used by the
> /// multi-FSM scheduler to thread the writer's `world_next.*`
> /// values into each reader's `world.*` slots in the same tick.
> pub fn query_with_pins_and_given(...) { ... }
>
> /// Read-only access to the EnumRegistry — used by the effect
> /// loop to look up DatatypeSorts when re-encoding state values
> /// for the next step's pin.
> pub fn enums_registry(&self) -> &crate::translate::EnumRegistry { ... }
>
> /// The 'static Z3 context this runtime allocates against.
> pub fn z3_context(&self) -> &'static z3::Context { ... }
>
> /// Encode a list of EffectResults into a Z3 datatype value
> /// matching stdlib/runtime.ev's `ResultList`. Used by the
> /// effect loop to pin `last_results` for the next step's solve.
> pub fn encode_effect_result_list(
>     &self,
>     items: &[crate::ast::EffectResult],
> ) -> Result<z3::ast::Datatype<'static>, ...>
> ```

**Why it might be bad.** The `runtime-invariants.md` brief for this
file says: "Must NOT know about Effects, multi-FSM scheduler, FFI,
FTI, or library bridges." Five of the 31 public methods exist
explicitly to support `effect_loop.rs` — their doc comments name
"effect loop" / "multi-FSM scheduler" / "next step's solve" as
the calling context. `EffectResult` lives in `ast.rs` (so the
type-import is technically clean), but `encode_effect_result_list`
takes a slice of them and produces a Z3 datatype matching
`stdlib/runtime.ev`'s `ResultList`, which is a runtime-execution
contract the constraint facade shouldn't be aware of. Likewise,
`enums_registry()` and `z3_context()` exist purely so the
scheduler can build Z3 datatype values itself rather than going
through a constraint-shaped facade verb — they leak internals to
let an upper layer reach down. The brief's specific guidance —
"new verbs only if they fit the same shape (operate on loaded
program state, return a result or modify the registry)" — is
violated by methods whose purpose is "expose runtime internals
so a sibling module can construct Z3 values for its own per-tick
plumbing." (See effect_loop.rs:548, :572, :589, :981, :1012,
:1355, :1378, :1390 for the actual call sites.)

**Suggested fix.** Three options, in increasing order of churn:
(a) move the five methods out of `EvidentRuntime` and into a
small adapter struct in `effect_loop.rs` that holds a `&mut
EvidentRuntime` and offers the scheduler-shaped verbs; (b) keep
the methods but remove the executive-layer references from their
doc comments and reframe each as a generic facade verb (e.g.
"pin one or more enum-typed variables in a query" — no mention
of state/last_results); (c) accept the leak but document it on
the invariants doc as a known relaxation. Today the invariants
doc forbids it without noting an exception.

**Detection idea.** grep `runtime/src/runtime.rs` doc comments
for the strings `effect loop`, `multi-FSM`, `scheduler`, `per
step`, `per tick`, `state machine`, `next step`. If any public-
method doc comment matches, flag for review. Easy to mechanize
with a `grep -B1 'pub fn' runtime/src/runtime.rs` post-filter.

### Suggested AP-021 (review-only): facade method docs reference modules that don't exist in the crate

**Pattern observed at runtime/src/runtime.rs:476-489:**
> ```rust
> // Known-stdlib paths whose types are already provided by the
> // embedded stdlibs we auto-load in `cmd_execute` (Stdin/Stdout
> // via `executor::load_io_stdlib`, SDLInput/SDLOutput/etc. via
> // `plugins::sdl::STDLIB_SDL_EV`). Silently no-op these so
> // programs that import them — which is the convention even
> // though our embedded versions cover the same ground — don't
> // fail just because we don't ship the .ev files at the
> // expected path. Users who DO ship a real `stdlib/sdl.ev`
> // alongside their program (via cwd) will still hit it via
> // verbatim resolution above.
> ```

`cmd_execute`, `executor::load_io_stdlib`, `plugins::sdl::STDLIB_SDL_EV`
are Python-runtime symbols; they don't exist anywhere in
`runtime/src/`. The Rust crate auto-loads stdlib through a
different mechanism (currently via `commands/effect_run.rs` and
fti.rs). A doc comment that justifies a piece of behaviour by
naming non-existent collaborators silently misleads anyone
trying to trace the design.

**Why it might be bad.** The same kind of stale-comment drift
that produced the now-failing claim "we don't ship the .ev
files at the expected path" — except `stdlib/sdl.ev` *does*
exist in the repo today, so the no-op shim is also factually
wrong. Stale doc comments rot the same way stale tests do.

**Suggested fix.** When a Rust file is ported from Python, its
doc comments need to be re-grounded against the Rust call
graph, not preserved verbatim. For this specific block: either
delete the shim (the file exists in cwd and verbatim resolution
would find it) or rewrite the comment to reference the actual
Rust collaborators.

**Detection idea.** Hard to fully mechanize. Possible weak
heuristic: grep doc comments in `runtime/src/*.rs` for tokens
that don't appear as Rust identifiers anywhere in the crate
(`cmd_execute`, `executor::`, `plugins::`, `STDLIB_SDL_EV`).
Review-only — too many false positives at scale.

## Notes (not new candidates)

* The 7-times-repeated `std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok().and_then(|s| s.parse().ok()).unwrap_or(2)`
  pattern at lines 614, 630, 862, 894, 998, 1066, 1115 is a
  clear duplication smell, but it's already covered by the
  pending **AP-010 (env-var-read-in-hot-path)** candidate
  proposed in `lints/findings/translate-inline.md`, whose
  scope explicitly includes `runtime/src/runtime.rs`. Not
  duplicated here.

* The `register_enums` helper at lines 58-178 (with its 100+
  lines of validation + Z3 datatype-builder construction) sits
  uneasily on the boundary between runtime-facade and
  translate-internal work — building Z3 datatype sorts from
  AST enum decls is exactly the kind of thing
  `translate/datatypes.rs` does for `Seq(UserType)` sorts. But
  the invariants doc for `translate/datatypes.rs` says it
  "borrows the registry it writes to" — so moving this work
  there would change that file's role. Worth a design note
  separately; not an existing-rule violation.
