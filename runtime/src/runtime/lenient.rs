//! Thread-local lenient-mode flag + RAII guard.
//!
//! Lenient mode demotes the otherwise-fatal "dropped constraint (couldn't
//! translate to Bool)" error to a warning, letting the functionizer skip an
//! untranslatable body item during a per-component compile attempt (the slow
//! Z3 path then owns that constraint). A query enables it around its
//! gap-fill/functionize attempt via [`LenientGuard`].
//!
//! It MUST be thread-local. The runtime runs independent queries on multiple
//! threads (libtest, the parallel slow-solve workers, the self-hosted-pass
//! engines). The previous implementation toggled the **process-global** env
//! var `EVIDENT_LENIENT` with `std::env::set_var`/`remove_var` per query, so
//! one thread's guard-drop cleared the flag *mid-translation* on another
//! thread — turning a should-be-lenient skip into a `std::process::exit(1)`.
//! That surfaced as `toposort_correctness` "exiting abnormally" under load
//! (its `distinct(pos)` / range constraints routinely hit the lenient-skip
//! path, so it toggled the flag constantly). Concurrent `setenv`/`getenv` is
//! also undefined behavior in Rust. A thread-local depth counter removes both
//! hazards while still honoring the read-only `EVIDENT_LENIENT` env var as a
//! process-wide *user* preference.

use std::cell::Cell;

thread_local! {
    /// Nesting depth of active [`LenientGuard`]s on this thread (`>0` ⇒ lenient).
    static LENIENT_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// True if lenient mode is active for the current thread — either an enclosing
/// [`LenientGuard`] or the process-wide `EVIDENT_LENIENT` env-var preference
/// (truthy = set, non-empty, not `"0"`).
pub(crate) fn lenient_enabled() -> bool {
    LENIENT_DEPTH.with(|d| d.get() > 0) || env_pref()
}

/// The user's process-wide `EVIDENT_LENIENT` preference. Read-only here — the
/// CLI sets it once at startup (single-threaded); per-query toggling is the
/// thread-local guard's job.
fn env_pref() -> bool {
    std::env::var("EVIDENT_LENIENT")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

/// Marks lenient mode active on the current thread while alive; restores the
/// prior depth on drop. Reentrant (depth counter), so nested guards compose.
pub(crate) struct LenientGuard;

impl LenientGuard {
    pub(crate) fn enable() -> Self {
        LENIENT_DEPTH.with(|d| d.set(d.get() + 1));
        LenientGuard
    }
}

impl Drop for LenientGuard {
    fn drop(&mut self) {
        LENIENT_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}
