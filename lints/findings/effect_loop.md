# Findings: runtime/src/effect_loop.rs

Reviewed against `lints/rules/` as of HEAD (53fa1fe).

## Update from prior findings

The previous review's headline findings (AP-001 by-scope violation, the
~160 lines of per-bridge auto-install blocks at lines 305-465, the lifetime-
laundering transmute at lines 129-131, the env-var reads in hot loops) all
remain present and unchanged. The recent edit only touched the
`spawnable_only` marker-detection comment around line 222. The rest of this
file is the same as the prior write-up; the new section is "Spawnable_only
marker check (after edit)" below.

## Spawnable_only marker check (after edit) — CORRECTLY SHAPED

**Location: effect_loop.rs:221-233.**
```rust
if let Some(shape) = detect_fsm_shape(rt, &name) {
    // Skip claims that carry the `spawnable_only` body marker
    // (one of `crate::ast::BODY_MARKERS`) — they should only
    // run when explicitly spawned via Effect::SpawnFsm, not
    // auto-instantiated at startup.
    if let Some(claim) = rt.get_schema(&name) {
        let is_spawn_only = claim.body.iter().any(|item| {
            matches!(item,
                crate::ast::BodyItem::Constraint(crate::ast::Expr::Identifier(s))
                if s == "spawnable_only")
        });
        if is_spawn_only { continue; }
    }
    ...
}
```

The shape is right and matches the layering invariant in `ast.rs`'s
`BODY_MARKERS` doc comment ("scheduler / runtime layers MAY reference
specific entries by looking them up against this list"). The pieces line up:

- `ast.rs:19` (`BODY_MARKERS`) owns the registry of recognized
  bare-identifier markers — the AST layer enumerates which names exist as
  markers, with no knowledge of what any of them mean.
- `translate/inline.rs:509` consumes the registry uniformly:
  `BODY_MARKERS.contains(&s.as_str()) { continue; }` — the translator skips
  every marker without caring which one it is.
- `effect_loop.rs:230` consumes one specific marker by name (`"spawnable_only"`)
  because the scheduler is the layer that knows what spawn-only behavior
  means. This is correct: a marker's *meaning* lives with the layer that
  acts on it; only the *registry* is centralized.

The comment update referencing `crate::ast::BODY_MARKERS` is the right form
of documentation — it tells the reader where the registry lives without
forcing the scheduler to enumerate the registry. Reading from `BODY_MARKERS`
here would be wrong: the scheduler would then have to either (a) act on
every marker uniformly (which is what the translator does, and the wrong
behavior here — `spawnable_only` is the only marker that gates startup
auto-instantiation), or (b) re-derive which entries it cares about, which
is just the same hardcoded string with extra steps. The hardcode IS the
"this layer knows this marker's meaning" signal.

**Minor consistency polish (review-only, not a violation):** a
`debug_assert!(crate::ast::BODY_MARKERS.contains(&"spawnable_only"))` near
the check would catch the drift case where someone removes the entry from
the registry without removing the consumer. Not worth a rule; the test
suite would catch it via failing spawn-only tests anyway.

The spawnable_only edit did NOT make the bigger AP-001-by-scope problem
worse — no new bridge `use`s, no new per-bridge `if has_field(...)` blocks,
no new event_sources references. This is the correctly shaped local fix.

## Pre-existing violations (unchanged from prior findings)

### AP-001 at effect_loop.rs (multiple sites)

**The file's invariant brief explicitly extends AP-001's scope to
`runtime/src/effect_loop.rs`.** AP-001 forbids "library-specific" and
specific-bridge identifiers in the language-core role. The file's own
per-file invariant in `runtime-invariants.md` adds a stronger constraint:
the scheduler must run "without knowing how that collection [of event
sources] was assembled or what each object's specific origin is" and the
"current import of specific bridge types or the registry mechanism is
acceptable only as a transitional shape; the right long-term invariant is
that this file holds no `use` of any specific bridge or registry symbol."

Concrete `use` / path references to specific bridge structs and to the
registry (anything beyond the generic `EventSource` trait + `SchedulerEvent`
+ `WriteQueue` surface):

- effect_loop.rs:155 — `else if type_name == "FrameTimer"` (string literal
  naming a specific bridge type)
- effect_loop.rs:157 — `else if type_name == "Signal"` (string literal
  naming a specific marker type)
- effect_loop.rs:159 — `crate::fti::is_fti_type(type_name)` (registry coupling)
- effect_loop.rs:316 — `has_field("tick_count", "Int")` (FrameTimer-specific
  reserved-field name hardcoded)
- effect_loop.rs:320 — `crate::event_sources::FrameTimer::new(ms, "tick")`
  (specific bridge constructor)
- effect_loop.rs:322 — `timer.with_count_field("tick_count")`
- effect_loop.rs:343-352 — `crate::fti::fti_install_fn`,
  `crate::fti::FtiContext`, the per-FSM FTI installer loop
- effect_loop.rs:359 — `has_field("signal_received", "Int")` (SigintSource-
  specific reserved-field name)
- effect_loop.rs:362 — `crate::event_sources::SigintSource::new()`
- effect_loop.rs:378 — `has_field("stdin_line", "String")` (StdinSource-
  specific reserved-field name)
- effect_loop.rs:384 — `crate::subscriptions::body_references_identifier(claim, "ReadLine")`
  (StdinSource ↔ Effect::ReadLine race-detection logic, hardcoded)
- effect_loop.rs:395-403 — `crate::event_sources::StdinSource::new(...)`,
  `with_seq_field("stdin_seq")`, `ctx.stdin_owned_by_plugin = true`
- effect_loop.rs:413 — `has_field("now_ms", "Int")` (WallClockSource-specific
  field name)
- effect_loop.rs:418 — `crate::event_sources::WallClockSource::new(ms, "now_ms")`
- effect_loop.rs:429 — `has_field("file_changed", "Int")` (FileWatcher-specific)
- effect_loop.rs:435 — `crate::event_sources::FileWatcherSource::new(...)`
- effect_loop.rs:449 — `has_field("file_line", "String")` (FileLineReader-specific)
- effect_loop.rs:451 — `crate::event_sources::FileLineReader::new(...)`
- effect_loop.rs:1238, 1253 — `crate::event_sources::SchedulerEvent::Tick {...}`
  / `Closed {...}` (acceptable — `SchedulerEvent` is the generic trait-
  surface enum, not a specific bridge)

The two acceptable references are `crate::event_sources::EventSource` (the
trait) and `crate::event_sources::SchedulerEvent` (the event enum).
Everything else in the list above makes effect_loop.rs intimately aware of
WHICH bridges exist, what their reserved field names are, and what their
constructors look like.

The current `lints/rules/AP-001-no-library-specific-in-language-core.md`
greps for `Sdl[A-Z]`, `SDL_`, `\bGl[A-Z]`, `Glsl`, `Audio[A-Z]`, dlopen
paths. None of those tokens appear here, so a strict reading of AP-001's
**grep pattern** is clean. But AP-001's **scope clause explicitly lists
`runtime/src/effect_loop.rs`** and the spirit of the rule (no
library-specific knowledge in the language-core role) is broadly violated
by the per-bridge `if has_field(...) { ... bridge::new(...) }` blocks at
lines 305-465. This is the gap between AP-001's grep and AP-001's
intent — see candidate AP-009 below for a generalization.

The runtime-invariants brief states the test plainly: "Adding a new typed
C resource (SDL_Audio, etc.) or removing the FTI mechanism entirely should
not require touching this file." Today, adding a new typed C resource
that uses the world-field plugin pattern (e.g., a `serial_byte: Int`
auto-installed from a `SerialPort` bridge) requires editing this file to
add another `if has_field(..., ...) { ... }` block. That's the failure
mode the invariant warns against.

## Per-file-invariant violations (from `runtime-invariants.md`)

### Direct decode of a Z3 datatype value (`r.bindings`) — borderline

The brief says the file "must NEVER … decode model values directly (uses
ast_decoder)." The file does use `ast_decoder::decode_effect_list` at
lines 609 and 1042 (correct), but also reads `r.bindings.get(&...)` at
lines 604, 606, 1036, 1039, and 1084 to fetch raw `Value` entries
(state_next_val, effects_val, world_next.* writer outputs). Reading
`Value` from the bindings map is reading typed model output, not raw Z3
asts; this is consistent with the brief (decoding happens via
`ast_decoder` for the AST-shaped Effect/Result enums; primitive Values
come straight off the bindings map). Not a violation, just noting that
the line between "decode model values" and "read a Value out of a
results map" is narrow here.

### `unsafe { std::mem::transmute }` lifetime laundering at lines 129-131
> ```rust
> let body: &'a [BodyItem] = unsafe {
>     std::mem::transmute::<&[BodyItem], &'a [BodyItem]>(&sub.body)
> };
> ```

The brief's "must NEVER" list doesn't explicitly forbid `unsafe`, but the
file's role is the multi-FSM scheduler — pure orchestration over Values
and AST data. A transmute to widen a borrow's lifetime, used to allow a
recursive `collect` closure to borrow into another schema's body, is the
kind of footgun that doesn't belong in a scheduler. If the closure can't
express the lifetime relationship safely, the inputs need restructuring
(e.g., resolve the passthrough chain into an owned `Vec<BodyItem>` clone
once, then walk it). Review-only — proposing as candidate AP-010 below.

## Other observations

### `MainShape` is a misnamed legacy type
**Observed at effect_loop.rs:54-89:**
> ```rust
> /// For backwards compat the struct is still called `MainShape`. The
> /// new `claim_name` and `world_*` fields default to "main" / None for
> /// single-FSM programs.
> ```

Self-flagged in the doc comment. The struct is now used for every
detected FSM, not specifically for `main`. The comment promises a rename
that hasn't happened. Review-only.

### `eprintln!` for diagnostic output
**Observed at effect_loop.rs:475, 625, 970, 1020, 1117, 1121, 1240, 1308-1326,
and similar sites.**

The file prints diagnostic / timing / trace output via `eprintln!`. The
brief allows this implicitly (no rule forbids it), but a scheduler in a
library (rather than a CLI) ideally surfaces diagnostics through a
caller-supplied logger or a `LoopResult` field. Today every consumer of
`run()` inherits stderr noise gated only by env vars. Review-only.

### `std::env::var` reads inside the hot loop
**Observed at effect_loop.rs:474, 624, 969, 1116, 1170, 1239, plus the
startup ones at 261, 312, 414, 431, 565, 792, 806.**

`EVIDENT_LOOP_TRACE` is queried from inside the per-FSM loop body each
tick (e.g., line 969 inside the `for (idx, fsm) in fsms.iter().enumerate()`
loop). These are ~µs syscalls per tick and add up. The startup-time reads
are fine; the per-tick reads should hoist into bools above the loop.
Review-only — performance, not architecture.

## Candidate new rules

### Suggested AP-009: scheduler-uses-trait-surface-only

**Pattern observed at effect_loop.rs:305-465 (≈160 lines of
per-bridge auto-install blocks):**
> ```rust
> if has_field("tick_count", "Int") {
>     timer = timer.with_count_field("tick_count");
>     plugin_writes.insert("tick_count".to_string());
> }
> ...
> if has_field("signal_received", "Int") {
>     let mut sig = crate::event_sources::SigintSource::new();
>     ...
> }
> if has_field("stdin_line", "String") { ... StdinSource::new(...) ... }
> if has_field("now_ms", "Int")        { ... WallClockSource::new(...) ... }
> if has_field("file_changed", "Int")  { ... FileWatcherSource::new(...) ... }
> if has_field("file_line", "String")  { ... FileLineReader::new(...) ... }
> ```

**Why it might be bad:** The runtime-invariants brief promises that
adding a new typed C resource "should not require touching this file."
Today it does — every new world-field-based bridge needs another
`if has_field(...)` block here, with the bridge's reserved field names,
its specific constructor, and its specific configuration methods all
spelled out in the scheduler. The scheduler has become the registry.

The fix is the same shape as `crate::fti::INSTALLERS`: declare a
`WORLD_PLUGIN_INSTALLERS: &[(field_name, type_name, install_fn)]` table
in `event_sources/mod.rs` (or similar) and have the scheduler iterate
the table once, calling each install function with the world fields it
sees. Then this file goes back to "iterate event sources via the
`EventSource` trait" and stays there. The FTI registry already
demonstrates the shape (`fti::is_fti_type` + `fti::fti_install_fn`); the
world-field plugins should adopt the same registry pattern instead of
each one being a hand-coded `if` block in the scheduler.

This rule generalizes AP-001's scope to a stronger structural property
for the scheduler file specifically: no specific bridge struct
constructor calls, no specific reserved-field-name string literals, no
specific `with_*_field` configurator calls. Permitted: the trait
(`EventSource`), the event type (`SchedulerEvent`), the queue helpers
(`WriteQueue`, `new_write_queue`, `drain`), and registry indirection
(walk a `&'static [...]` table of installers).

The `BODY_MARKERS` design (registry in `ast.rs`, scheduler reads one
specific marker by name) is the working analogue for what this should
look like at the bridge layer — push the names into a centralized
registry, let consumers either iterate it generically or look up the
ones whose meaning they own.

**Suggested fix:** Promote the per-bridge `if has_field(...) { ... }`
blocks into entries in a generic installer registry. The scheduler's job
becomes "for each registered installer, ask it whether it wants to
install given the current world type." See `runtime/src/fti.rs` for the
working pattern.

**Detection idea:** Mechanizable as a grep over `runtime/src/effect_loop.rs`
for any of: `crate::event_sources::[A-Z][a-zA-Z]+` where the suffix is
NOT in {`EventSource`, `SchedulerEvent`, `WriteQueue`}; `crate::fti::`
for anything other than registry-shape calls; string literals that
match known reserved field names (`tick_count`, `stdin_line`,
`now_ms`, `signal_received`, `file_changed`, `file_line`, `file_seq`,
`file_eof`, `stdin_seq`). Allowlist the trait surface; flag everything
else. Doable as `check_scheduler_uses_trait_surface_only` in
`lints/checks.sh`.

### Suggested AP-010: no-lifetime-laundering-transmute

**Pattern observed at effect_loop.rs:129-131:**
> ```rust
> let body: &'a [BodyItem] = unsafe {
>     std::mem::transmute::<&[BodyItem], &'a [BodyItem]>(&sub.body)
> };
> ```

**Why it might be bad:** `std::mem::transmute` between two `&[T]`
references with different lifetimes is exclusively a tool to bypass the
borrow checker — the data shape is identical; only the lifetime changes.
This works ONLY because the larger function (`detect_fsm_shape`) holds
the runtime by `&EvidentRuntime` and the schemas it returns live as long
as the runtime, but the borrow checker can't prove that across the
nested closure boundary. A future refactor that changes how schemas are
stored (e.g., to RefCell-wrapped storage so a passthrough resolution can
mutate the registry) silently invalidates this transmute and turns it
into use-after-free.

This is the kind of "I needed to make rustc shut up" transmute that
should never appear outside `unsafe { ... } // SAFETY: …` with a real
proof. There is no SAFETY comment here.

**Suggested fix:** One of:
1. Change the closure into an explicit recursive `fn` that takes
   `&EvidentRuntime` and returns `Vec<BodyItem>` (owned clones).
   `BodyItem` already derives `Clone`; the cost is one allocation per
   passthrough chain at startup, not in the hot loop.
2. Pre-resolve passthrough chains in a separate pass that yields a
   single owned `Vec<BodyItem>` per FSM, before lifetime-sensitive
   borrow patterns enter the picture.

**Detection idea:** grep for `mem::transmute` in `runtime/src/**/*.rs`
where the source and destination types are both `&[...]` or
both `&...` (different lifetime, same shape). Easy regex; add to
`lints/checks.sh`. There's only one instance in the runtime today,
making this a preventive rule.

### Suggested AP-011: hoist-env-var-reads-out-of-hot-loops (review-only)

**Pattern observed at effect_loop.rs:474, 624, 969, 1116, 1170, 1239:**
> ```rust
> if std::env::var("EVIDENT_LOOP_TRACE").is_ok() {
>     eprintln!("[loop] tick {step_count} fsm={}: ...", ...);
> }
> ```

Six `std::env::var(...)` calls inside per-tick, per-FSM loops. Each is
a syscall. For a FrameTimer running at 60Hz with 4 FSMs, that's ~1440
env-var lookups per second purely to gate diagnostic output that nobody
is reading.

**Why it might be bad:** Diagnostic gates should compile down to a
single Bool comparison. The current pattern multiplies cheap runtime
overhead by the number of times the gate is checked.

**Suggested fix:** Read each env var ONCE near the top of `run_with_ctx`
into a local `let trace_loop: bool = std::env::var(...).is_ok();` and
gate the eprintln on the local Bool.

**Detection idea:** Hard to mechanize cleanly without false positives —
distinguishing "called once at startup" from "called every tick" needs
control-flow analysis. Review-only.

## Clean

Not clean. The headline finding is unchanged: effect_loop.rs lines 305-465
encode specific knowledge of six different bridge types
(`FrameTimer`, `SigintSource`, `StdinSource`, `WallClockSource`,
`FileWatcherSource`, `FileLineReader`) plus the FTI registry, directly
contradicting the file's invariant that "adding a new typed C resource
… should not require touching this file." The bridges could be hidden
behind a small registry analogous to `fti::INSTALLERS`, after which
this file would hold zero specific-bridge `use` paths and the per-bridge
`if has_field(...)` blocks would collapse into a single iterator over
the registry.

The recent edit (the `spawnable_only` marker comment update around line
222) is correctly shaped: the `BODY_MARKERS` registry lives in `ast.rs`
where it owns the language-level enumeration of marker names; the
translator (`translate/inline.rs:509`) consumes the registry generically
via `.contains()`; the scheduler consumes ONE specific entry by string
literal because the scheduler is the layer that knows what
`spawnable_only` means. This is the right layering — meanings live with
the layer that acts on them; only the registry is centralized — and the
edit did not introduce any new bridge-specific coupling.

AP-001 technically fires by scope (the file is in AP-001's scope clause)
but its grep patterns don't match these identifiers — proposed AP-009
generalizes the structural rule. Two additional candidate rules: AP-010
(`unsafe transmute` lifetime laundering, mechanizable, preventive) and
AP-011 (env var reads in hot loops, review-only).
