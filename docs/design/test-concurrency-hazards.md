# Test-suite concurrency hazards (why `./test.sh` is reliably green)

`cargo test` runs each integration-test target as a **separate process,
one at a time** (verified: max 1 `deps/*` binary alive at once), but
**within** each binary libtest runs tests on up to `ncpus` threads. The
runtime is heavily multi-threaded inside a process too (the parallel
slow-solve workers, the self-hosted-pass engines). So every test binary is
a concurrency stress test of the runtime, and two process-global hazards
made the suite flake — most visibly `toposort_correctness` "exiting
abnormally" (exit 1, **zero** assertion failures) and, less often,
`basic.rs::claim_call_unmapped_internal` returning a wrong answer.

Both were diagnosed and fixed in session `test-infra-fix`. They are
**root-cause** concurrency fixes in `runtime/`, not a test-harness
workaround — the suite intentionally still runs at full parallelism so it
keeps exercising these paths and catches regressions.

## Hazard 1 — Z3 `Context` creation raced Z3's global init

Z3's first-time global initialization (memory manager, symbol tables) is
not thread-safe. The crate already serialized worker-context creation in
`runtime::query` behind a `z3_setup_lock`, but **`EvidentRuntime::new`
created its context outside that lock**. When libtest launched N
runtime-building threads at binary startup, their concurrent `Context::new`
calls raced — surfacing as either an abnormal abort or a silently **wrong**
solver answer (a constraint effectively dropped by corrupted Z3 state, e.g.
`claim_call_unmapped_internal` returning a value outside its asserted
range).

**Fix:** `runtime/src/z3_ctx.rs` — one crate-shared `SETUP_LOCK` with
`leaked_context()` / `setup_guard()`. *Every* `Context::new` site (notably
`EvidentRuntime::with_functionizer`, the parallel-slow workers, and the
unit-test context helpers) now mints its context under it.
Measured: `basic.rs` 4/30 → 0/30 failures under 6-core CPU saturation.

## Hazard 2 — lenient mode was a process-global env var

`LenientGuard` (used around each query's functionize attempt, so an
untranslatable body item is *skipped* with a warning and handed to the slow
Z3 path instead of being fatal) toggled the **process-global** env var
`EVIDENT_LENIENT` with `set_var`/`remove_var` per query. With N concurrent
query threads, one thread's guard-drop cleared the flag *mid-translation* on
another thread. `ToposortRanks`' `distinct(pos)` / range constraints
routinely take the lenient-skip path, so `toposort_correctness` toggled the
flag constantly → maximal exposure. A thread that read `lenient = off` at
the wrong instant took the `std::process::exit(1)` branch in
`translate/inline/{walk,membership}.rs` → silent abnormal exit. (Concurrent
`setenv`/`getenv` is also undefined behavior in Rust.)

**Fix:** `runtime/src/runtime/lenient.rs` — a **thread-local** depth counter.
`lenient_enabled()` ORs the thread-local state with the still-honored
read-only `EVIDENT_LENIENT` user/CLI preference. Per-query lenient state is
now per-thread.
Measured: `toposort_correctness` 6/12 → 0/15 aborts under 6-core CPU
saturation.

## Reproducing / guarding against regressions

The flakes are timing-sensitive; the reliable repro is CPU contention. To
stress a single binary:

```sh
for c in 1 2 3 4 5 6; do yes >/dev/null & done   # saturate cores
BIN=runtime/target/release/deps/toposort_correctness-*   # or basic-*
for i in $(seq 1 15); do "$BIN" || echo "iter $i FAILED"; done
kill %1 %2 %3 %4 %5 %6
```

Pre-fix this aborts/wrong-answers within a handful of iterations; post-fix
it is clean. If either hazard regresses (e.g. a new per-operation
`set_var`, or a `Context::new` that bypasses `z3_ctx`), `toposort_correctness`
/ `basic.rs` start flaking under load again — the suite at full parallelism
is the guard.
