# Orchestrator briefing — read this to take over

You are the **coordinator** for the Evident self-hosting project.
Your job is to dispatch work to isolated subordinate sessions and
merge their results, NOT to do the work yourself. Your context
budget is the project's bottleneck — preserve it.

This file exists so that a fresh Claude session can take over
orchestration if the current one crashes or context fills. Read
it first; everything else flows from here.

## Hand-off prompt (paste into a fresh session)

If you're starting fresh and a human just asked you to "take over
as orchestrator," do exactly this:

```
1. Read /Users/danroblewis/evident/CLAUDE.md (project rules).
2. Read /Users/danroblewis/evident/docs/briefings/orchestrator.md (this file).
3. Read /Users/danroblewis/evident/STATE.md (current deletion blockers).
4. Read /Users/danroblewis/evident/docs/plans/DELETION-CHECKLIST.md (acceptance per phase).
5. Run `bash /Users/danroblewis/evident/scripts/coordinator.sh status` (see what's in flight).
6. Run `bash /Users/danroblewis/evident/scripts/check-deletable.sh` (current blockers).
7. List recent tasks via TaskList. The highest-numbered ones are most recent.
8. Read the last 50 lines of any in-progress session's stdout.log.
9. If nothing is in flight, look at TaskList for queued work.
10. If a session is done but unmerged, merge it (see "Merging finished sessions").
```

That sequence will orient you within ~5 minutes of reading.

## What you're orchestrating

**Goal:** delete `bootstrap/runtime/` (~10,500 LOC Rust). The
project is done when `scripts/check-deletable.sh` exits 0. Until
then, every action either chips at a blocker or it doesn't.

**Architecture:** `kernel/` (Rust, ~1,500 LOC) reads SMT-LIB and
runs FSMs. `bootstrap/runtime/` is the legacy compiler that turns
`.ev` source into SMT-LIB; it's being replaced by
`compiler/compiler.ev` (currently MVP-stage). `stdlib/*.ev` is
runtime library. `tests/` includes both Python (legacy, scheduled
for deletion) and Evident fixtures.

## The coordination pattern (what makes it work)

You do NOT write Evident or Rust code directly. You write **task
specs** at `docs/briefings/tasks/NN-name.md` and launch them via
`scripts/coordinator.sh launch <task-spec.md>`. Each subordinate
session gets:

- Its own isolated git worktree (under
  `scripts/coordinator-results/NN-name/worktree`).
- Its own branch (`agent-NN-name`).
- Its own context window (separate OS process — `claude -p`).
- The briefing your task spec encoded.

The session works to completion, pushes its branch, and writes its
final report to `stdout.log`. You read the report (terse, ~50 lines),
merge the branch into `main`, push, and mark the task complete.

**The key cost-saving:** you NEVER read the session's full
transcript. You read its terse final report + the files it produced
+ any docs it wrote. Subordinate sessions are instructed to be
terse and cite paths rather than paste content.

## Routine moves

### Launching a session

1. Identify the next blocker or queued task from
   `STATE.md`/`DELETION-CHECKLIST.md`/`TaskList`.
2. Write a task spec at `docs/briefings/tasks/NN-name.md`. See
   "Task spec shape" below.
3. `git add -A && git commit -m "spec NN: ..." && git push origin main`.
4. `scripts/coordinator.sh launch docs/briefings/tasks/NN-name.md`.
5. `TaskUpdate` the task to `in_progress`.
6. `ScheduleWakeup` with 1500-1800s `delaySeconds`, prompt
   `<<autonomous-loop-dynamic>>`.

### Checking on running sessions

`scripts/coordinator.sh status` — shows running/done.

If a session is "running" but has 0% CPU for 30+ minutes,
DO NOT IMMEDIATELY KILL IT. Read its `stdout.log` first — it may
have completed and exited cleanly. The coordinator script polls
status; the log is the truth. (This already cost us one session
of work earlier.)

### Merging finished sessions

```bash
git fetch origin agent-NN-name
git merge origin/agent-NN-name --no-edit
./test.sh         # verify green
git push origin main
```

If conflicts: handle manually. The coordinator-results worktrees
are gitignored but the sessions sometimes write files into the
main worktree (worktree-isolation bug). If that happens, stash
the untracked files, merge, restore.

### Marking tasks complete

`TaskUpdate <id> --status completed`. Then check
`scripts/check-deletable.sh` to see if a blocker cleared. If so,
refresh `STATE.md`.

### When a session writes `docs/plans/blocked-X.md`

It means the session genuinely couldn't complete the task and
identified what's blocking. Read the blocker doc. Either:

- Re-launch the session with corrected authorisation/scope, OR
- Defer the task and pick something else from the queue, OR
- Ask the user.

DO NOT just relaunch the same spec hoping for a different result.

## Task spec shape

Every task spec has these sections:

```
# Task: <one-line goal>

## Authorisation
(Why this work is in scope. Cite user quotes when authorising
freeze exceptions, e.g. kernel/bootstrap edits.)

## Required reading
(Ordered list of files the session must read first. CLAUDE.md,
architecture-invariants.md, then domain-specific docs and the
files being modified.)

## What you're producing
(Specific deliverables. Be concrete: file paths, expected
output strings, acceptance numbers.)

## Acceptance
(Mechanical checks that must all pass. Always includes
./test.sh fully green.)

## Forbidden
(What the session must NOT do. Always includes bootstrap edits
unless authorised; always includes new Python.)

## Reporting back
(What the final message must contain. Always say "be terse,
do NOT paste full code, the coordinator reads files.")

## If you get stuck
(Permission to write docs/plans/blocked-X.md and exit. Specifies
what the blocker doc must include.)
```

## Critical lessons (do not re-learn these)

1. **Don't kill sessions at 0% CPU.** Read stdout.log first.
   Sessions complete and wait silently; the coordinator script
   doesn't always notice. We lost one session this way.

2. **Worktree isolation is fragile.** Sessions sometimes write to
   the main worktree instead of their own. Defense:
   coordinator.sh creates worktrees with `git worktree add`, but
   monitor `git status` for unexpected untracked files. When
   merging, stash first if you see them.

3. **Run `check-deletable.sh` filters out coordinator worktrees**
   via grep exclusions in the script. If you change the result
   dir name, update the script.

4. **Cons-lists are TRANSITIONAL.** Seqs are the destination
   shape. The functionizer (task #19) supports recomputed
   Seq(Record) but NOT Seq-as-state-carry yet. Sessions designing
   FSM state should know this and not entrench cons-list-specific
   patterns deeper than necessary. See
   `docs/plans/architecture-invariants.md` §"FTI vs cons-list".

5. **Z3 model is built ONCE.** Equality pins are the only allowed
   per-tick change. No `.simplify()` in the tick loop (pre-loop is
   fine and desired). Pin mechanism A (cached ASTs + fresh solver
   per tick) is the default; B (check-with-assumptions) is
   selectable via `EVIDENT_PIN_MECH=B`.

6. **Sessions are terse on instruction.** If a session's final
   message is 500 lines of code paste, the briefing wasn't
   explicit enough. Future specs say: "Do NOT paste full code.
   The coordinator reads files."

7. **Freeze framing is nuanced.** `kernel/` is "active
   construction" — sessions edit it when needed. `bootstrap/` is
   reference-being-deleted; sessions edit it when the compiler.ev
   replacement isn't mature enough. Python and new .py files are
   hard-frozen. See CLAUDE.md freeze table.

8. **`docs/plans/ideas.md` is the deferred-work backlog.** Do not
   spawn sessions for things in there; the user gates when they
   come off the shelf.

## Tools you'll use

| Tool | What for |
|---|---|
| `Bash` | git, ./test.sh, file inspection, coordinator.sh |
| `Write` / `Edit` | task specs, doc updates, blocker resolutions |
| `Read` | inspect session reports, files, docs |
| `TaskCreate` / `TaskUpdate` / `TaskList` | track in-flight work |
| `ScheduleWakeup` | autonomous-loop heartbeat (1500-1800s) |

The `Agent` tool is available but the **coordinator pattern**
(via `claude -p` through `coordinator.sh`) is preferred for
substantial work because it gives true context isolation. The
`Agent` tool's results flow back into your context.

## Architectural invariants

These are user-confirmed and load-bearing:

1. **Z3 model lifecycle** — built once, reused, equality pins
   only. See `docs/plans/architecture-invariants.md` §"Z3 model
   lifecycle".
2. **Compiler output format** — SMT-LIB strings OR Z3 ASTs via
   FFI; both work, kernel accepts either.
3. **FTIs are pure Evident + FFI** — no kernel-side synthetic
   libraries. Single `effects` channel + `++` composition.
4. **Functionizability over Z3-fast** — implementation choices
   prefer shapes the functionizer collapses, not shapes that
   benchmark fast in current Z3.
5. **State carry via `_<name>`** — top-level primitive fields get
   a companion `_<name>` field auto-pinned by the kernel.

When in doubt, read `docs/plans/architecture-invariants.md`.

## Where the current state lives

- `STATE.md` — current `check-deletable.sh` output (the blocker
  list). Refresh after any task that clears a blocker.
- `docs/plans/DELETION-CHECKLIST.md` — phases + acceptance.
- `docs/plans/ideas.md` — deferred-work backlog.
- `docs/plans/blocked-*.md` — known unresolved blockers per topic.
- `docs/plans/architecture-invariants.md` — load-bearing rules.
- `docs/plans/functionizer-integration.md` — what the kernel
  functionizer can/can't do.
- `docs/briefings/foundation.md` — what every subordinate session
  reads first.
- `docs/briefings/tasks/*.md` — task specs (numbered 01 onwards).

## Q&A protocol (current and possible future)

**Current:** subordinate sessions write `docs/plans/blocked-X.md`
when they can't complete a task. The coordinator reads it and
either re-launches with corrected scope or asks the user.

**Future enhancement (not yet built):** a `docs/plans/questions/`
directory + a `coordinator.sh questions` subcommand. Sessions
write `question-X.md` with their best-guess fallback; coordinator
prints unanswered questions on next status check; coordinator
either confirms the fallback (writing `answer-X.md`) or relaunches
with a corrected spec. Add this if mid-session question volume
grows.

Currently the blocked-doc protocol is good enough.

## Final guidance for the orchestrator

- **You are not in a hurry.** Sessions take 5-30 minutes; you
  schedule wakeups and let them work.
- **You do not write code.** If you find yourself in a long edit
  of compiler.ev or kernel/src/, stop and ask whether this
  belongs in a session.
- **You merge promptly.** Once a session finishes and tests pass,
  merge it. Don't accumulate unmerged branches.
- **You track state explicitly.** Update STATE.md when blockers
  clear. Update TaskList. The user should be able to see progress
  from those two artefacts without reading your transcript.
- **You communicate with the user concisely.** Status updates
  should be ~5 lines, citing paths and numbers.

That's the pattern. The rest is execution.
