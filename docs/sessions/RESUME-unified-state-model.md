# RESUME — unified state model migration

This file is the handoff for the next session. Read it first.
The goal of the arc is to land the unified state model and then
build a Mario game. We're partway through.

## What's already committed and pushed

| Commit | What |
|---|---|
| `3436d79` | `(args) ∈ claim_name` tuple-in-claim membership syntax |
| `41af2f8` | LTS design doc + `external fsm` legalization |
| `6a90ed6` | `_var` time-shift convention + `is_first_tick` auto-inject |

All three are on `origin/main`. `./test.sh` passes.

## The conceptual frame (read this carefully)

The whole design is anchored in
`docs/design/state-machines-as-relations.md`. Key points:

1. **A `claim` denotes a set of tuples** — the set of all
   parameter-assignments that satisfy its body. `∈` is set
   membership against that set.
2. **An `fsm` is a scheduled `claim`** with the `_var` time-shift
   convention. Inside the body, `count` is the current tick's
   value being computed; `_count` is the previous tick's value.
3. **Runtime bridges are `external fsm`s** — StdinSource,
   FrameTimer, EffectDispatcher, etc. Their Evident declaration
   is the contract for which shared-state slots they read/write.
4. **Coordination is by name** — two fsms sharing a variable
   name share that slot in global state.
5. **No special variable types or implicit categories.** A user
   fsm declares what it needs; the runtime treats every variable
   uniformly.

## What's working right now

- `_var` reads work end-to-end. A user fsm body that references
  `_count` gets `_count` auto-declared (typed to match `count`).
  The runtime pins `_count` from the previous tick's `count`
  value. `is_first_tick ∈ Bool` is auto-injected and pinned
  true on tick 0, false thereafter.
- `external fsm` parses and loads. The scheduler skips
  auto-instantiating external fsms (their Rust implementations
  do the work).
- `examples/test_19_prev_tick.ev` is the worked example. Run:
  `./runtime/target/release/evident effect-run examples/test_19_prev_tick.ev`
  — should print `count = 0`, `count = 1`, `count = 2`, `done`.

## What we just started but DIDN'T finish

**Task #31 — smart-inject of implicit fsm params.** This was
moved to "in_progress" in the task list but not implemented.

The plan agreed on:

Currently `inject_fsm_params` in `runtime/src/runtime.rs:74` does
two things:
  1. **Errors** if `state ∈ <Type>` is not declared.
  2. **Unconditionally injects** `state_next`, `last_results`,
     `effects` (skipping any already declared).

This means every fsm declares — implicitly or explicitly — all
four of `state, state_next, last_results, effects`, even pure
counter fsms that just want `count ∈ Int`. The unified model
says authors should declare only what they use.

**The plan**: change `inject_fsm_params` to be smart:
  - Drop the "requires `state`" error. A pure-counter fsm with no
    state enum is valid.
  - Only inject `state_next` if `state` is declared AND
    `state_next` is referenced in the body (not just always).
  - Only inject `effects` if it's referenced in the body
    (anywhere — LHS of `=`, `#effects`, etc.) AND not declared.
  - Only inject `last_results` if it's referenced AND not
    declared.

The reference-detection is the same pattern as
`inject_prev_tick_decls`'s walker — scan body Constraint /
ClaimCall expressions for `Identifier(name)`. Use the existing
walker pattern (it's right there in the same file).

**Why "smart" and not "drop entirely"**: dropping unconditionally
would break every existing demo (they all reference
`state_next` / `effects` / `last_results` without declaring
them, relying on injection). Smart-inject keeps them working
without manual migration of 38 .ev files.

Also need to:
  - **Make scheduler tolerant of missing slots** in
    `effect_loop.rs:detect_fsm_shape`. Currently
    `state_pair?`, `last_results_var?`, `effects_var?` are
    REQUIRED — `None` means the fsm isn't detected. After
    smart-inject, a pure counter fsm has none of these. The
    scheduler should run it anyway (just skip state-pin,
    effects-decode, last_results-encode when the slot doesn't
    exist).

**What remains after task #31:**
  - Task #32 — declare runtime bridges as `external fsm` in
    stdlib/runtime.ev (StdinSource, FrameTimer,
    EffectDispatcher, SigintSource, WallClock, FileWatcher).
    These are documentation contracts; Rust implementations
    already exist in `runtime/src/event_sources/`.
  - Task #33 — verify demos still pass after #31 (mostly a
    no-op given smart-inject; just run `./test.sh`).
  - Task #34 — rewrite `packages/sdl/*.ev` as multi-FSM
    coordination. Each SDL bridge (window, renderer, event-pump)
    becomes an `external fsm` whose state is shared with user
    FSMs via name-matched slots. This is the SUBSTANTIAL one —
    requires re-architecting how SDL is presented to users.
  - Task #35 — Mario game using rectangle sprites. Multi-FSM
    design: PlayerFSM, PhysicsFSM, InputFSM, LevelFSM,
    RenderFSM. Built on top of #34.

## File-level pointers

| File | Role |
|---|---|
| `docs/design/state-machines-as-relations.md` | Conceptual anchor |
| `runtime/src/runtime.rs:74` | `inject_fsm_params` (target of #31) |
| `runtime/src/runtime.rs:~135` | `inject_prev_tick_decls` (pattern to copy for smart-inject scan) |
| `runtime/src/effect_loop.rs:162` | `detect_fsm_shape` (needs to be tolerant) |
| `runtime/src/effect_loop.rs:709` | `FsmRt` struct (has `prev_values` already) |
| `runtime/src/effect_loop.rs:~1003` | Per-tick `fsm_view` build (already pins `_var` and `is_first_tick`) |
| `runtime/src/parser.rs:302` | Schema decl parser (already accepts `external fsm`) |
| `examples/test_19_prev_tick.ev` | Worked example of `_var` |

## Tests

`./test.sh` — 415 cargo + 91 conformance + lints all green.
`./test.sh --examples-only` — 16/18 demos pass + 2 visual.

Note: there's an **untracked `examples/a.ev`** in the user's
working tree (their draft of the unified-model example). It
triggers AP-002/003 lint failures because it has raw `LibCall`
with a dylib path. The user knows about it. Move it aside before
running `./test.sh` if needed (or commit/delete it).

## Resumption checklist

The user is at 99% context and asked for a recovery anchor before
compaction. When picking this up, the next Claude should:

1. Read this file (`docs/sessions/RESUME-unified-state-model.md`).
2. Read `docs/design/state-machines-as-relations.md`.
3. Read the three relevant commits' diffs:
   `git show 3436d79 41af2f8 6a90ed6`
4. Pick up at **task #31 — smart-inject**, following the plan
   above.
5. Then #32, #33, #34, #35 in order.

## Conventions to preserve

- All commits include `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
- `./test.sh` MUST pass before committing.
- `examples/a.ev` is the user's untracked draft — do not commit
  it without asking. Move it aside when running tests if it
  trips the lint.
- The lint rules forbid `LibCall` / dylib paths in `examples/*.ev`;
  only `external claim` bodies in `stdlib/` and `packages/` may
  contain them.
- FSMs use `keyword: Keyword::Fsm`, `external` is a `bool` flag on
  `SchemaDecl`.
