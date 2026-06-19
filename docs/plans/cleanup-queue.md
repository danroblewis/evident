# Cleanup queue

The running list of sequenced cleanup/minimization tasks. **Lives on disk so it
survives context compaction.** Items are sequenced because several touch shared
files (the `Effect` enum, `query.rs`, `encode/`) and running them back-to-back
avoids worktree merge conflicts. Mark items done and commit as they land.

## In progress
- _(nothing running)_

## Queued (in order)
- _(empty — cleanup queue clear)_

## Done (recent, newest first)
- **KEYSTONE — removed "world", unified FSM state onto `_var`** (branch
  `unify-fsm-state`) — see [`remove-world-unify-state.md`](remove-world-unify-state.md).
  Deleted `unify_world_syntax`, the `state`/`state_next`·`world`/`world_next` pair
  detection + Datatype-pin + world_snapshot carry + `seed_state`/`encode_state_value`;
  record/enum/scalar state now all carry through the one `_var` mechanism (`_x` reads
  prev `x`). Migrated all demos + integration/JIT tests; purged "world" everywhere;
  rewrote CLAUDE.md's FSM-state guidance. Fixes `Δ`-on-records (verified) and unblocks
  the FTI time-shift. 248 cargo tests + 27 static demos + 3 visual demos green.
- purge stdlib multi-FSM vestiges (5 `external fsm` + FrameTimer/Signal/FrameClock/Timer + dead comments), `runtime.ev` 299→195; `single_fsm` reviewed, already minimal
- move `lower.rs` → `encode/lower.rs`; rename `runtime/` module → `session/` — `6a37276`
- reconcile docs to single-FSM: delete 8 obsolete design docs + multi-fsm cookbook,
  scrub multi-FSM/subscription/event-source machinery from CLAUDE.md and the FTI /
  schema-interface / state-machines-as-relations docs
- _(superseded)_ multi-FSM machinery removed from runtime/src (event-source /
  subscription / scheduler code gone)
- move `z3_eval.rs` → `functionize/extract_program.rs` (IR front-half) — `242ebec`
- merge `dispatch.rs` → `ffi.rs` (the foreign/IO boundary; `ffi` now `pub`) — `e1e2535`
- `effect_loop` → `trampoline`, `effect_dispatch` → `dispatch` — `7fadcfe`
- ParseInt/IntToStr effects → Z3 expression ops (`to_str`/`parse_int`); pruned `start`/`stdin` — `deadc23`
- translate/ → encode/ (+ `translate_*` → `encode_*` helpers) — `91e6c8c`, `f33ad2e`
- delete 6 artifact effects (ShellRun/Time/MonotonicTime/ReadLine/RealToStr/ParseReal) — `2f55ad2`
- consolidate inline tests → `src/tests.rs` — `687041e`
- file restructure 49 → 31 files — `f8da116` … `c1ae048`
