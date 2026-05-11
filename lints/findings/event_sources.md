# Findings: runtime/src/event_sources.rs

Reviewed against `lints/rules/` as of HEAD (188c682).

The file is 1390 lines, declares 9 `pub struct` bridges + the `EventSource`
trait + `SchedulerEvent` + `WriteQueue` helpers all in one file, and is
documented in `runtime-invariants.md` and `lints/rules/AP-001` as the
"split-pending" file. Each bridge struct is reviewed below as if it were
already in its own future file `runtime/src/event_sources/<name>.rs`.

## Per-bridge invariant compliance

The per-file invariant for `event_sources/<name>.rs` requires: exactly one
`pub struct <Name>Source`, its `impl EventSource`, a constructor, an
optional `Drop`. No imports of `runtime` / `effect_loop` /
`effect_dispatch` / `translate`. No imports of any sibling bridge. May
import `event_sources::mod` (the trait, `SchedulerEvent`, `WriteQueue`)
and `crate::Value`.

### FrameTimer (lines 96-178) — clean per-bridge
- Declares `pub struct FrameTimer`, `impl EventSource`, constructor
  `new`, builder `with_count_field`, `Drop`.
- Uses only `std`, `crate::Value`, and the in-mod `WriteQueue` /
  `SchedulerEvent`.
- No reach into `runtime`, `effect_loop`, `effect_dispatch`, `translate`.
- No reach into a sibling bridge.
- No `unsafe`. Would split out cleanly.

### SigintSource (lines 190-286) — clean per-bridge
- Declares `pub struct SigintSource`, `impl EventSource`, `new`,
  `Default`, `with_count_field`, `Drop`.
- Imports `signal_hook::iterator::Signals` inside the function body —
  fine; per-bridge file would still own this dependency.
- No reach into other bridges or the runtime layers.

### StdinSource (lines 297-396) — clean per-bridge
- Declares `pub struct StdinSource`, constructor `new`,
  `with_seq_field`, `impl EventSource`, `Drop`.
- Comment at `stop()` (lines 372-379) acknowledges it cannot portably
  interrupt the blocking read and intentionally drops the JoinHandle.
  This is a per-bridge concern, not a layering violation.

### FileLineReader (lines 410-528) — clean per-bridge
- Declares `pub struct FileLineReader`, `new`, `with_seq_field`,
  `with_eof_field`, `impl EventSource`, `Drop`.
- Same blocking-read caveat as StdinSource; same handling.

### WallClockSource (lines 539-617) — clean per-bridge
- Declares `pub struct WallClockSource`, `new`, `impl EventSource`,
  `Drop`. Uses `std::time::SystemTime`. Self-contained.

### FileWatcherSource (lines 630-711) — clean per-bridge
- Declares `pub struct FileWatcherSource`, `new`, `impl EventSource`,
  `Drop`. Polls `fs::metadata` mtime. Self-contained.

### OneShotShellSource (lines 722-789) — clean per-bridge
- Declares `pub struct OneShotShellSource`, `new`, `impl EventSource`,
  `Drop`. Uses `std::process::Command`. Self-contained.

### SdlWindowSource (lines 810-1164) — per-bridge clean; carries known AP-001-adjacent content
- Declares `pub struct SdlWindowSource`, `new`,
  `with_gl_context_field`, `with_vao_field`, `start_inline`,
  `impl EventSource`, `Drop`.
- Per-bridge invariant: the `event_sources/<name>.rs` role is the
  ONLY place library-specific Rust code may live. So the dense block
  of `Sdl*` types, `SDL_*` symbol lookups, dlopen paths
  (`/opt/homebrew/lib/libSDL2.dylib`,
  `/System/Library/Frameworks/AppKit.framework/AppKit`,
  `/System/Library/Frameworks/OpenGL.framework/OpenGL`), and the
  `glGenVertexArrays` / `glBindVertexArray` / `glViewport` calls in
  `start_inline` are scope-correct for a future
  `event_sources/sdl_window.rs`. AP-001's scope explicitly excludes
  `event_sources*`. Not flagged.
- BUT — see "Cross-bridge coupling" below: this struct does GL work
  (`glGenVertexArrays`, `glBindVertexArray`, `glViewport`) that
  conceptually belongs to the GL bridge layer. After the split, this
  is the most likely violator of the "no cross-bridge dependency"
  invariant if the GL setup is left in `sdl_window.rs`.

### GlProgramSource (lines 1170-1330) — per-bridge clean; depends on SDL via context-currency
- Declares `pub struct GlProgramSource`, `new`, `start_inline`,
  `impl EventSource`. No `Drop` (the comment at lines 1302-1314 admits
  the GL framework lib is leaked via `Box::leak` because the SDL
  bridge already holds it open; explicitly notes this dependency).
- The cross-bridge dependency is correctly expressed in user
  Evident code (declaration order: SDL_Window before GL_Program in
  the user's program), per the per-bridge invariant. No Rust import
  of `SdlWindowSource` here. Compliant with the invariant's letter.
- Note: relies on the called thread holding a current GL context —
  set by `SdlWindowSource::start_inline`'s `SDL_GL_MakeCurrent` call.
  This implicit thread-state coupling is the kind of thing the
  invariant says "should be expressed in Evident, not in Rust
  imports" — which is technically what's happening; the user
  declares both, the runtime's install order arranges for SDL to run
  first. OK as long as the runtime preserves install order.

## Cross-bridge coupling (per-bridge invariant violation)

### `SdlWindowSource::start_inline` does GL work (lines 972-1005)
> ```rust
> type GlGenVertexArrays = unsafe extern "C" fn(c_int, *mut c_uint);
> type GlBindVertexArray = unsafe extern "C" fn(c_uint);
> type GlViewport        = unsafe extern "C" fn(c_int, c_int, c_int, c_int);
> let vao_id = if self.vao_field.is_some() {
>     let gl_paths = [
>         "/System/Library/Frameworks/OpenGL.framework/OpenGL",
>         "/usr/lib/x86_64-linux-gnu/libGL.so.1",
>         "/usr/lib/libGL.so",
>     ];
>     // … glGenVertexArrays, glBindVertexArray, glViewport …
> };
> ```

The "SDL_Window" bridge dlopens `OpenGL.framework` / `libGL` and calls
three `gl*` functions. This is GL bridge concern, not SDL bridge
concern. The `vao_field` builder option (line 844) and the entire VAO
block exist because the user's Evident program declares an
`SDL_Window`'s `with_vao_field`, which is treating the SDL bridge as
"SDL+GL setup grab-bag." After the split, this code straddles the
boundary between the future `sdl_window.rs` and `gl_program.rs` files.

The fix at split time is one of:
  * Move the VAO + viewport setup into `GlProgramSource::start_inline`
    (the bridge that already touches `libGL`).
  * Or create a separate `GlContextSource` / `GlVaoSource` bridge whose
    Evident type the user declares between `SDL_Window` and
    `GL_Program`.

Either way, `sdl_window.rs` after split should mention zero `gl[A-Z]`
symbols; today this file's SDL bridge has six (`glGenVertexArrays`,
`glBindVertexArray`, `glViewport` types + their lookups). That's a
cross-bridge violation in the post-split layout.

### `GlProgramSource` comments confirm the implicit dependency
Lines 1302-1311 narrate the dependency:
> ```rust
> // No keepalive thread needed — the lib stays loaded
> // because we leak it (drop suppressed via the
> // Box::leak pattern below would be cleaner, but
> // forgetting the binding works too). For simplicity
> // we just let the borrow extend through `lib` going
> // out of scope; the underlying GL framework remains
> // mapped because SDL_Window's bridge holds it open.
> ```

This is a comment in `gl_program.rs` (post-split) referring by name to
"SDL_Window's bridge." Per the invariant, cross-bridge dependencies
must be expressed in Evident, not in Rust. The user's program already
declares both, so the dependency IS expressed in Evident — but the
comment hard-codes the assumption that "SDL_Window holds GL open."
Better post-split shape: `gl_program.rs` calls `Box::leak(Box::new(lib))`
on its own GL handle (no implicit reliance on a sibling), and the
narrative comment is rewritten to drop the reference to a sibling
bridge by name.

## Helpers that should land in `event_sources/mod.rs` after split

The invariant says: `EventSource` trait, `SchedulerEvent`, `WriteQueue`,
and the helpers (`new_write_queue`, `drain`) live in `mod.rs`. All
bridge-shared utilities go there.

Already-present in this file and slated for `mod.rs`:
- `pub enum SchedulerEvent` (lines 41-50).
- `pub trait EventSource` (lines 53-74).
- `pub type WriteQueue` (line 79).
- `pub fn new_write_queue` (lines 81-83).
- `pub fn drain` (lines 85-88).

These are all clean leaf items; the split move is mechanical.

No bridge-shared HELPER functions beyond the above are present —
each bridge open-codes its `paths.iter().find_map(|p| unsafe {
Library::new(p) }.ok())` SDL/GL path-search. That repetition is a
post-split refactor candidate (a `try_load_library(paths: &[&str]) ->
Option<Library>` in `mod.rs`) but it's not currently a rule violation.

## Violations of existing rules

### AP-001 — not violated (file is OUT of scope)
The file mentions every forbidden token (`SDL_*`, `Sdl*`, `gl[A-Z]`,
`/opt/homebrew/lib/`, `.dylib`, `.framework/`, `/usr/lib/lib*`), but
AP-001's scope explicitly excludes `runtime/src/event_sources*` —
this file is the bridge role, the only role permitted to mention
specific C libraries. `lints/rules/AP-001-no-library-specific-in-language-core.md`
section "Scope" line 47 is explicit: "Do NOT apply to:
`runtime/src/event_sources*`." The current violation pattern
(library-specific tokens in a bridge file) is the EXPECTED shape; the
rule's whole point is to keep this pattern OUT of language-core and
IN this file (or its split successors).

AP-002 through AP-008 do not target this file (their scopes are
`examples/`, `tests/conformance/`, `runtime/tests/`).

## The split itself (well-known issue)

The file is the canonical "split-pending" file called out in
`runtime-invariants.md` (Group 5 introduction: "currently in the
single 1390-line `event_sources.rs` pending split") and in AP-001's
"Fix" section ("Library-specific code goes in the bridge role
(`runtime/src/event_sources/<library>.rs`, currently in the single
1390-line `event_sources.rs` pending split)"). Per the invariant
each bridge IS the unit of review, and the absence of that physical
split is the framing-level violation:

  - 9 `pub struct *Source` declarations in one file (FrameTimer,
    SigintSource, StdinSource, FileLineReader, WallClockSource,
    FileWatcherSource, OneShotShellSource, SdlWindowSource,
    GlProgramSource).
  - File length: 1390 lines.
  - Per-bridge file should average ~150 lines (range ~80-350). The
    current file is ~10× the per-bridge target.
  - The trait + queue helpers (lines 41-88) are the future
    `mod.rs`; the 9 bridges are the future leaf files.

Until split, the only way to reason about per-bridge invariants is
to mentally bracket the file. Today that bracketing is correct for
8 of 9 bridges (no cross-bridge Rust imports exist, because there's
nothing to import from); the SdlWindowSource ↔ gl_*-symbols
coupling above is the latent cross-bridge violation that the split
will surface.

## Candidate new rules

### Suggested AP-009: no-cross-bridge-coupling
**Pattern observed at event_sources.rs:972-1005:**
> SdlWindowSource's `start_inline` opens
> `OpenGL.framework`/`libGL` and calls `glGenVertexArrays`,
> `glBindVertexArray`, `glViewport`. These are the GL bridge's
> concerns surfacing inside the SDL bridge.

**Why it might be bad:** The per-bridge invariant is "each file owns
ONE bridge (one `pub struct *Source`)" and "bridges may NOT import
each other; if a bridge needs another bridge's resource, the user's
Evident program declares both and the scheduler arranges install
order." Today nothing ENFORCES this — even after the file is split,
nothing prevents `event_sources/sdl_window.rs` from `use
crate::event_sources::gl_program::*` or open-coding GL symbol
lookups itself. The latter is what's happening here. If "SDL bridge
sets up GL VAO and viewport" gets entrenched, every future GL-using
bridge has to either reach back into SDL or duplicate the setup.
The invariant calls this out narratively; mechanizing it would
prevent regressions.

**Suggested fix (post-split):** For each
`runtime/src/event_sources/<name>.rs`, the file may dlopen libraries
matching the file's name's prefix only. `sdl_window.rs` may touch
`SDL_*` and `libSDL2`; `gl_program.rs` may touch `gl*` and `libGL` /
`OpenGL.framework`. Cross-mention is a violation. The check is
per-file: extract the bridge name from filename, then grep for
forbidden token classes from the OTHER bridge's namespace.

**Detection idea:** post-split grep. For each
`event_sources/<name>.rs`, derive a "permitted prefix" from the
filename (e.g., `sdl_window` → SDL; `gl_program` → GL); flag any
hit on the OTHER known prefixes. Pre-split this is moot (no
per-bridge files exist yet). Recommend FILING the rule (this entry)
but NOT adding to `checks.sh` until after the split, since today
there are no files to check.

(Looked at `lints/rules/` — highest active number is AP-008. AP-009
is the next available number. This entry uses AP-009.)

### Suggested AP-010: bridge-file-cap (review-only)
**Pattern observed at event_sources.rs (entire file):**
> One file declares 9 `pub struct *Source` types.

**Why it might be bad:** The per-file invariant says "Exactly one
`pub struct <Name>Source`" per `event_sources/<name>.rs`. Mechanizing
this prevents the next "I'll just add my new bridge here" temptation
that produced the current 1390-line file. The invariant exists; what's
missing is a check.

**Suggested fix (post-split):** count `^pub struct [A-Z][A-Za-z]+Source\b`
hits in each `event_sources/*.rs` file (excluding `mod.rs`); fail if
any file has `> 1`. Pre-split this whole file is one giant violation;
post-split the check enforces the invariant.

**Detection idea:** trivial grep, but only after the split happens.
Add to `checks.sh` post-split. Review-only until then.

(AP-010, the next available after AP-009.)

## Clean

Not clean. The file is the documented split-pending special case —
that's not new. Per-bridge: 8 of 9 bridges are clean against the
per-file invariant for their post-split role; the 9th
(`SdlWindowSource`) carries GL-bridge concerns that will surface as
a cross-bridge coupling violation at split time. Two new rules
proposed (AP-009 cross-bridge coupling; AP-010 bridge-file cap),
both review-only / "file post-split" — not added to `checks.sh`
yet because they have nothing to check until the file is split.
