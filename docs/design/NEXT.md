# Where to direct next session's work

After the unified-schema-model arc plus the 2026-05-10 follow-up
work (parse-int, demo rewrites, FileLineReader, SpawnFsm), this
file captures what's left worth doing.

## Recently shipped (2026-05-10)

Effect surface (numeric ↔ string + shell):
  * **`Effect::ParseInt` / `Effect::ParseReal`** — string→int/real.
  * **`Effect::IntToStr` / `Effect::RealToStr`** — int/real→string.
  * **`Effect::ShellRun`** — synchronous `sh -c …` capturing stdout.

Plugins-as-writers:
  * **`FileLineReader`** — auto-installs when World has `file_line:
    String` and `EVIDENT_FILE_INPUT` env. Streams lines.
  * **`WallClock`** — auto-installs when World has `now_ms: Int`.
    Updates current Unix time at `EVIDENT_CLOCK_MS` interval.
  * **`FileWatcher`** — auto-installs when World has `file_changed:
    Int` and `EVIDENT_FILE_WATCH` env. Polls mtime, increments
    counter on each detected change.

Dynamic FSMs:
  * **`Effect::SpawnFsm(claim, Int)`** — spawns a new instance of
    `claim`, pinning the Int into the new FSM's first state-variant
    payload. Lets parent pass an instance ID; spawned FSM reads it
    via `match state`. v1 of parent-child communication.

Demo rewrites:
  * `effect_echo.ev` / `effect_hello.ev` — modern `match` +
    `⟨…⟩` style, plugin-as-writer Stdin.
  * `effect_guess_number.ev` — uses ParseInt for real numeric
    comparison (higher/lower/correct).

## Explicitly NOT planned

### Bounded write-queues for plugin sources

User framing (2026-05-10): "in the future we want to expose
memory more readily to Evident models. The boundedness of queues
will emerge naturally, and the constraints will be different than
what we would implement now."

## High value, bounded scope

### 1. SDL/GL demo migrations to modern patterns

The biggest bag of "old patterns" is in `programs/demos/effect_sdl_*`
and `effect_gl_*`. They use legacy effect-list shapes (LibCall +
ArgCons + state-payload-threaded handles). Migrating to the
plugin-as-writer + Foreign-Type-Interface model is the dominant
remaining cleanup.

Order: do these AFTER FTI v1 lands (next item) so the migration
has a target shape to translate to.

### 2. Foreign Type Interface v1 — per-instance typed resources

FileLineReader (just landed) demonstrates the lifecycle pattern
but is hard-coded to one instance via env var. The full FTI:
  * `t ∈ Timer (interval_ms ↦ 50)` declares a per-instance
    configurable resource.
  * Bridge plugin materializes one instance per declaration.
  * Type fields (e.g. `t.tick_count`) translate to per-instance
    world-like state the user FSM reads.

Real architectural change: needs read-set/write-set extended to
type-instance-prefixed fields, and per-instance plugin lifecycle.
First target: convert FileLineReader to use the typed shape (no
env var), giving a per-file-instance pattern.

After FTI v1: migrate one SDL or socket binding as proof.

### 3. Parent-child communication for spawned FSMs

v1 done — `Effect::SpawnFsm(claim, Int)` lets the parent pass
an Int into the spawned FSM's first state-variant payload. The
spawned FSM reads it via `match state` and uses it for
self-identification. Sufficient for worker-pool patterns
(siblings indexed by ID).

Remaining for full parent-child:
  * **Instance-scoped world**: each spawned FSM gets a private
    world view (or a designated section of the parent's world).
  * **Richer initial parameters**: not just one Int — pass
    Strings, records, etc.
  * **Addressing**: parent writes `world.children[id].request`
    to send work; child reads its slot.
  * **Cleanup**: instance halt is automatic via subscription
    silence; explicit Effect::Kill(id) for forced termination.

Big chunk of work; design in `fsm-spawning.md`.

### 4. Additional plugins

The plugin model is well-validated. Adding new sources is
implementing `EventSource` + wiring auto-install. Possibilities:
  * **WallClock** — exposes current Unix time as `world.now_ms`.
  * **TcpListener** — bind/listen/accept; spawned FSM per
    connection (needs #3).
  * **FileWatcher** — inotify/fsevents → world delta.
  * **HttpClient** — request/response style; one connection per
    spawned FSM.

Each unlocks a class of programs.

## Patterns to keep using

  * **One commit per coherent change** with full body explaining
    motivation + trade-offs.
  * **Test through the binary** for plugin behavior (subprocess
    in cargo test) — the runtime's `DispatchContext::stdin`
    doesn't reach the plugin's `std::io::stdin()`.
  * **Lang tests as documentation**. Each new pattern gets an
    `programs/lang_tests/multi_fsm/0N_*.ev` + a Rust integration
    test.
  * **Cross-link docs**. Every design doc → its siblings; every
    guide → the design.

## Should NOT touch

  * **Constraint solving substrate** (Z3 + claim translation) —
    works, performant, well-tested.
  * **`Effect::FFICall`** — keep until FTI v1 covers the same
    surface case-by-case.
  * **Spec docs** in `spec/` — describe the constraint language.
  * **Legacy mode** (`EVIDENT_SCHEDULER=legacy`) — keep until the
    new model has a year of bake time.
