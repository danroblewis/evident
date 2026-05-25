# Plans Index

Roadmap to a ≤11,000-line Rust runtime, with FSM-driven effect
dispatch and a generic FFI primitive replacing today's plugin
architecture.

Start here: [`roadmap.md`](roadmap.md).

Then per-phase plans (each phase has its own README plus per-task
files):

- [`01-ffi-effects/`](01-ffi-effects/) — FFI primitive + effect dispatcher (sequential)
- [`02-plugin-migrations/`](02-plugin-migrations/) — Move SDL/audio/shader/stdio to Evident libraries (parallel)
- [`03-language-prereqs/`](03-language-prereqs/) — Recursive claims, unbounded output, enum bindings (sequential)
- [`04-codegen-libraries/`](04-codegen-libraries/) — GLSL/SMT-LIB/reporters/passes ports (parallel)
- [`05-final-trim/`](05-final-trim/) — Cut to 11K (sequential)

Progress tracker: [`PROGRESS.md`](PROGRESS.md).

Adjacent design docs (not phased, but on deck):

- [`../design/cegar-scaffolding.md`](../design/cegar-scaffolding.md) — layer CEGAR on the Functionizer trait (oracle + refiner) so FSM verification can route around log-unroll's branching wall.

Cross-cutting inventory: [`../design/self-hosting-inventory.md`](../design/self-hosting-inventory.md) — every `runtime/src/**/*.rs` file classified by tier + prioritized port order.

## Working on a task

1. Read the task's plan file end-to-end.
2. If the plan is unclear or stale, push back: ask for clarification or
   a plan revision before writing code.
3. Execute. Stick to the plan's "Files touched" list. If you need to
   change a file outside the list, update the plan and commit the
   plan change separately.
4. Verify the acceptance checklist.
5. Commit with a referencing footer: `Plan: docs/plans/01-ffi-effects/02-effect-types.md`.
6. Update `PROGRESS.md` with current LOC and which task landed.

## Worktree-based parallel execution

For Phases 2 and 4, multiple tasks can run in parallel via the Agent
tool with `isolation: "worktree"`. The orchestrating session sends a
single message with one Agent call per task; each agent works in an
isolated worktree on its own branch. After completion, branches merge
to main with conflict resolution at the merge gate.

Example (Phase 2 launch):
```
[Agent for 2.1 stdio  → branch phase-2-stdio]
[Agent for 2.2 sdl    → branch phase-2-sdl]
[Agent for 2.3 audio  → branch phase-2-audio]
[Agent for 2.4 shader → branch phase-2-shader]
```

After all four complete, a non-isolated agent runs Phase 2.5 to remove
the plugin abstraction.

## Acceptance discipline

Every task has a checklist in its plan file. Don't mark a task done
until the checklist is met. The 11K target is the only hard success
criterion for the overall roadmap; if a phase undershoots, escalate
rather than fudge.
