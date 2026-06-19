# Cleanup queue

The running list of sequenced cleanup/minimization tasks. **Lives on disk so it
survives context compaction.** Items are sequenced because several touch shared
files (the `Effect` enum, `query.rs`, `encode/`) and running them back-to-back
avoids worktree merge conflicts. Mark items done and commit as they land.

## In progress
- _(nothing running)_

## Queued (in order)
1. **Audit leftover multi-FSM machinery** — `single_fsm` enumerates FSM candidates
   and rejects >1; the runtime is single-FSM now (core.md stage 4), but CLAUDE.md
   still documents a multi-FSM subscription scheduler (world read-sets,
   plugin-as-writer, event sources). Check whether the event-source / subscription /
   multi-FSM code is vestigial, simplify `single_fsm`, and reconcile the docs.

## Done (recent, newest first)
- move `z3_eval.rs` → `functionize/extract_program.rs` (IR front-half) — `242ebec`
- merge `dispatch.rs` → `ffi.rs` (the foreign/IO boundary; `ffi` now `pub`) — `e1e2535`
- `effect_loop` → `trampoline`, `effect_dispatch` → `dispatch` — `7fadcfe`
- ParseInt/IntToStr effects → Z3 expression ops (`to_str`/`parse_int`); pruned `start`/`stdin` — `deadc23`
- translate/ → encode/ (+ `translate_*` → `encode_*` helpers) — `91e6c8c`, `f33ad2e`
- delete 6 artifact effects (ShellRun/Time/MonotonicTime/ReadLine/RealToStr/ParseReal) — `2f55ad2`
- consolidate inline tests → `src/tests.rs` — `687041e`
- file restructure 49 → 31 files — `f8da116` … `c1ae048`
