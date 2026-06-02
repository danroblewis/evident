//! Process-wide serialization of Z3 `Context` creation.
//!
//! Z3's first-time global initialization (its memory manager, global symbol
//! tables, GC roots) is **not** thread-safe. When several threads create their
//! first `Context` concurrently — exactly what libtest does when it launches
//! N test threads that each build an `EvidentRuntime` at startup — that init
//! races. The corruption surfaces two ways, both observed in this suite:
//!
//!   * an abnormal abort with **zero** assertion failures (a worker thread's
//!     Z3 calls `abort()`, taking down the whole test binary — historically
//!     seen as `toposort_correctness` "exited abnormally"), or
//!   * worse, a silently **wrong** solver answer (e.g. `basic.rs`'s
//!     `claim_call_unmapped_internal` returning a value outside the asserted
//!     range — a constraint effectively dropped by corrupted Z3 state).
//!
//! Every `Context` in the crate is therefore minted under one global lock so
//! creation is serialized for the process. The lock was already present for
//! the parallel slow-solve workers (`runtime::query`) but `EvidentRuntime::new`
//! created its context outside it — the gap this module closes by making the
//! lock the single shared point both paths use.
//!
//! `runtime::query::build_parallel_slow` additionally holds [`setup_guard`]
//! across the datatype replay that immediately follows creation, so a worker's
//! context is fully populated before another thread starts touching Z3.

use std::sync::{Mutex, MutexGuard};
use z3::{Config, Context};

/// The one global Z3-setup lock. Process-local (a `static` is per-process), so
/// each test binary serializes its own context creation independently.
static SETUP_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the global Z3-setup lock. Hold it across `Context` creation and any
/// immediately-following datatype replay that must observe a quiescent Z3.
/// Poisoning is ignored: the guarded section only constructs Z3 objects, so a
/// panicked predecessor leaves no inconsistent shared state to recover.
pub(crate) fn setup_guard() -> MutexGuard<'static, ()> {
    SETUP_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Mint a fresh `'static` Z3 context, leaked for the process lifetime (the
/// crate's standard one-context-per-runtime pattern), serialized through
/// [`setup_guard`] so concurrent creation never races Z3's global init.
pub(crate) fn leaked_context() -> &'static Context {
    let _guard = setup_guard();
    let cfg = Config::new();
    Box::leak(Box::new(Context::new(&cfg)))
}
