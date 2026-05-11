# AP-012: no-specific-bridges-in-scheduler

**Status:** active

**Pattern.** `runtime/src/effect_loop.rs` references a specific
bridge struct as a Rust path: `event_sources::FrameTimer`,
`event_sources::SigintSource`, `event_sources::StdinSource`,
`event_sources::WallClockSource`, `event_sources::FileWatcherSource`,
`event_sources::FileLineReader`, `event_sources::OneShotShellSource`,
`event_sources::SdlWindowSource`, `event_sources::GlProgramSource`,
or `event_sources::GlContext`.

**Why.** After the Pattern B refactor (`63fef74` and friends), the
scheduler walks `WORLD_PLUGIN_INSTALLERS` and is unaware of which
specific bridges exist. Adding a new typed C resource (SDL_Audio,
TouchDevice, …) should require touching `event_sources/<name>.rs`
+ `fti.rs::INSTALLERS` + `stdlib/runtime.ev` — never `effect_loop.rs`.
The scheduler's invariant is that it runs correctly against any
collection of objects implementing the `EventSource` trait, without
knowing the collection's contents. Codify so a future contributor
can't add `if has_field("X") { install_specific_bridge() }` back
in.

**Fix.** Use the registry. Bridges register themselves via
`fti.rs::INSTALLERS` (one entry per Evident type name) and
`WORLD_PLUGIN_INSTALLERS` (one entry per auto-installed world-field
plugin). The scheduler iterates these tables; the names of specific
struct types stay out of `effect_loop.rs`.

**Detection.** grep

**Pattern (grep).** `event_sources::(FrameTimer|SigintSource|StdinSource|WallClockSource|FileWatcherSource|FileLineReader|OneShotShellSource|SdlWindowSource|GlProgramSource|GlContext)`
in `runtime/src/effect_loop.rs` (production code only;
`#[cfg(test)]`-gated blocks are exempt). Comment-only lines exempt.

**Scope.**
  - Apply to: `runtime/src/effect_loop.rs`.
  - Do NOT apply to `runtime/src/fti.rs` (the registry intentionally
    references each bridge by name) or to `runtime/src/event_sources/*`
    (each bridge IS the named struct).

**Exceptions.**
  - String-literal type-name comparisons like
    `type_name == "FrameTimer"` are NOT matched by the regex (which
    requires the `event_sources::` Rust-path prefix). The marker-type
    detector inside `detect_fsm_shape` legitimately uses these
    strings — they're Evident-language type names that happen to
    coincide with bridge struct names.
  - Comments mentioning bridge names are exempt (the regex requires
    the `event_sources::` prefix, which doesn't appear in prose).
  - `#[cfg(test)]`-gated test code is exempt.

**Examples.**
  - Pre-Pattern-B: `effect_loop.rs` had branches like
    `if world_has_tick_count(&rt) { sources.push(Box::new(event_sources::FrameTimer::new(...))) }`,
    once per bridge. Post-`63fef74`: a single loop walks
    `WORLD_PLUGIN_INSTALLERS` and dispatches.
