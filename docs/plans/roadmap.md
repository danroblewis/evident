# Minimal Runtime Roadmap

## Success measure

**Get the Rust runtime to ~11,000 lines** (from today's 17,112).

The architectural target is in `docs/design/minimal-runtime.md`; this
document is the execution plan to reach it.

## LOC accounting

| Source | Lines |
|---|---|
| Today | 17,112 |
| FFI primitive (added in Phase 1) | +700 |
| Plugin removals (Phase 2): SDL/audio/shader/stdio + plugin-abstraction | -2,127 |
| Plugin lifecycle code in executor | -400 |
| GLSL transpiler (Phase 4, after recursive claims) | -1,007 |
| SMT-LIB I/O (Phase 4) | -957 |
| Test reporters (Phase 4) | -400 |
| Inference/desugar Rust glue (Phase 4) | -300 |
| Trace runner slimmed for FFI shim (Phase 1.7) | -283 |
| Runtime API trim (Phase 5) | -400 |
| **Net target** | **~11,938** |

We're aiming for ≤11,000 with the buffer absorbed by aggressive
trimming in Phase 5. If any phase undershoots, the parser bootstrap
remains as a major reduction option (~1,900 lines) but is out of
scope for this roadmap.

## Phases and dependencies

```
Phase 1: FFI + Effects (sequential)
   1.1 ✅ FFI primitive
   1.2  Effect + Result AST types
   1.3  Effect dispatcher in executor (built-ins only)
   1.4  Built-in effects: Print/Println/ReadLine/Time/Exit
   1.5  FFI effects wired into dispatcher
   1.6  First end-to-end Evident → FFI demo
   1.7  stdlib/posix.ev skeleton
   1.8  Trace-test record/replay shim
       │
       ▼
Phase 2: Plugin migrations (parallelizable after Phase 1)
   2.1  Stdin/Stdout → effects
   2.2  SDL → stdlib/sdl/ Evident library
   2.3  Audio → stdlib/audio/
   2.4  Shader → stdlib/shader/
   2.5  Remove plugin abstraction code
       │
       ▼
Phase 3: Language prerequisites (sequential)
   3.1  Recursive claim invocation
   3.2  Unbounded output Seq from passes
   3.3  Enum-typed pattern bindings
       │
       ▼
Phase 4: Codegen + IO library migrations (parallelizable after Phase 3)
   4.1  GLSL transpiler → stdlib/glsl/
   4.2  SMT-LIB export → stdlib/smtlib/export/
   4.3  SMT-LIB import → stdlib/smtlib/import/
   4.4  Test reporters → stdlib/testing/reporters/
   4.5  Inference/desugar consolidation
       │
       ▼
Phase 5: Final trim (sequential, last)
   5.1  Runtime API audit
   5.2  CLI shell trim
   5.3  Dead-code audit
```

## Parallelism map

The Agent tool's `isolation: "worktree"` mode lets multiple agents
work simultaneously on independent branches.

| Phase | Parallelism | Worktree branches |
|---|---|---|
| 1 | Sequential — each task depends on the prior | one branch, in main session |
| 2 | Parallel after 1 done | `phase-2-stdio`, `phase-2-sdl`, `phase-2-audio`, `phase-2-shader`; merge gate at 2.5 |
| 3 | Sequential — each unlocks subsequent ports | one branch |
| 4 | Parallel after 3 done | `phase-4-glsl`, `phase-4-smtlib`, `phase-4-reporters`, `phase-4-passes` |
| 5 | Sequential | one branch |

## Per-task plan files

Each task has a self-contained plan file. The agent executing the
task should be able to read just (a) this roadmap and (b) the task's
plan file, then execute without further context.

- `docs/plans/01-ffi-effects/` — Phase 1 (8 files)
- `docs/plans/02-plugin-migrations/` — Phase 2 (5 files)
- `docs/plans/03-language-prereqs/` — Phase 3 (3 files)
- `docs/plans/04-codegen-libraries/` — Phase 4 (5 files)
- `docs/plans/05-final-trim/` — Phase 5 (3 files)

## Acceptance for each task

Every task plan ends with a checklist:
- [ ] Code change matches spec
- [ ] All existing Rust tests pass (`cargo test`)
- [ ] All conformance tests pass (`for f in tests/lang_tests/*.ev; do evident test "$f"; done`)
- [ ] LOC delta matches expectation (or is documented when it doesn't)
- [ ] Commit lands with co-author footer

## Bookkeeping

Track progress in `docs/plans/PROGRESS.md` (created when the first
task lands). Each phase commit updates it with current LOC count and
which tasks are done.

## Out of scope

These would help but are not on the path to 11K:
- Parser bootstrapping (would cut ~1,900 more)
- Z3 self-hosting (impossible without bootstrapping Z3 itself)
- Replacing the executor's Z3 dispatch with an Evident library
- Rewriting the AST in Evident
