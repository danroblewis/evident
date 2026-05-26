# runtime/src/event_sources/mod.rs — Z3-replaceability

**What it does:** Declares the `EventSource` trait, `SchedulerEvent` enum, shared `WriteQueue` type, `WorldPluginCtx` context struct, and the static `WORLD_PLUGIN_INSTALLERS` registry. Re-exports `FrameTimer` and `DeclarativeInstallSource` for use by the effect loop and FTI.

**Criticality:** critical

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** This is infrastructure/trait definitions and a plugin registry — no computation. The `has_world_field` helper is a one-line HashMap lookup. The `WORLD_PLUGIN_INSTALLERS` array is a static dispatch table, not a relation that Z3 could improve. All logic lives in the individual source files.

**Change made:** none
