# Where to direct next session's work

After the unified-schema-model arc plus the 2026-05-10 follow-up
work (parse-int, demo rewrites, FileLineReader, SpawnFsm), this
file captures what's left worth doing.

## Recently shipped (2026-05-10)

  * **`Effect::ParseInt` / `Effect::ParseReal`** — runtime-side
    string→int/real parsing. Demos can now do real numeric input.
    `effect_guess_number.ev` rewritten to compare actual numbers.
  * **`effect_echo.ev` / `effect_hello.ev`** — rewritten in modern
    `match` + `⟨…⟩` style. Echo now uses StdinSource auto-install.
  * **`FileLineReader` plugin** (FTI v0) — auto-installs when World
    declares `file_line: String` and `EVIDENT_FILE_INPUT` points
    to a path. Streams lines via world fields. First non-stdin
    file resource demonstrating the lifecycle pattern.
  * **`Effect::SpawnFsm(claim_name)`** — dynamic FSM instantiation.
    A new instance of the named claim joins the scheduler from
    the next tick. Per-instance world / parent-child messaging
    deferred (see fsm-spawning.md for the design space).

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

`Effect::SpawnFsm` returns an instance ID but nothing uses it. To
make spawn useful for connection-per-FSM servers, REPLs, etc.,
we need:
  * **Instance-scoped world**: each spawned FSM gets a private
    world view (or a designated section of the parent's world).
  * **Addressing**: parent writes `world.children[id].request_field`
    to send work; child reads from its private view.
  * **Cleanup**: instances halt via the same mechanisms; runtime
    GCs after all parents have stopped referencing.

Big chunk of work; design is in `fsm-spawning.md`.

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
