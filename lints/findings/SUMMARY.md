# Findings — current state

This directory holds open code-review findings against `runtime/src/`.
Files are per-`runtime/src/` file. Delete a file once its findings
have been acted on (per `lints/README.md`).

## Status

A 34-agent code-review wave landed in this directory; the first wave
flagged ~12 files with violations and 22 clean. Almost everything
flagged has since been fixed in the commit log between `c6e179e` and
`0ddf6a6` (15 commits). What remains is the one structural item the
agents independently identified — the scheduler / runtime / bridge
layering — broken into three open files:

  * `effect_loop.md` — auto-install hardcoding for FrameTimer /
    SigintSource / StdinSource / WallClockSource / FileWatcherSource /
    FileLineReader (~160 lines at lines 305-465); plus the
    lifetime-laundering `unsafe { mem::transmute }` at lines 129-131
    with no SAFETY comment; plus ~6 `std::env::var` calls in per-FSM
    hot loops that should be cached at startup.

  * `runtime.md` — `STDLIB_SHIMS` const at lines 486-490 hardcodes
    `"stdlib/sdl.ev"` directly in the language-core facade (only the
    surrounding doc-comment was fixed in `c6e179e`); plus 5 facade
    methods (`query_with_pinned_datatypes`, `query_with_pins_and_given`,
    `enums_registry`, `z3_context`, `encode_effect_result_list`) that
    exist solely as scaffolding for the effect loop and read as
    execution-layer concerns leaking into the translate-layer facade.

  * `event_sources.md` — the file is one 1390-line monolith with 9
    bridges; the per-file invariants treat it as already split into
    `event_sources/<name>.rs`. The split is pending. Once split,
    `SdlWindowSource`'s OpenGL.framework dlopen + glGenVertexArrays /
    glBindVertexArray / glViewport calls become a real cross-bridge
    AP-001 violation that the lint will catch.

## Shape of the fix

The agents independently arrived at the same shape: a
`WORLD_PLUGIN_INSTALLERS` registry mirroring `fti::INSTALLERS`. Each
event source declares which world fields it owns; the scheduler walks
the registry instead of hardcoding per-source `if has_field(...)`
blocks. `effect_loop.rs` becomes generic over the registry; `runtime.rs`'s
5 scaffolding methods either move into a separate trait or get
documented as the explicit execution-layer extension surface. The
event_sources.rs split is the natural prerequisite — each bridge
becomes its own file under `event_sources/` and the `mod.rs` exports
the trait + registry.

Rough effort: half-day to a day.

## Note on rule promotion (separate work)

Patterns A/C/D/E from the original wave have been fixed in code but
**not promoted to mechanical lint rules** (would be AP-009..012). New
rule files in `lints/rules/` plus `check_*` functions in
`lints/checks.sh` would catch regression. Estimated ~1-2 hours.
Not in this directory — track separately in the rulebook.
