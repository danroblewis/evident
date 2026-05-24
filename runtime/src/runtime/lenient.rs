//! RAII guard for the `EVIDENT_LENIENT` env var.

/// RAII guard that turns `EVIDENT_LENIENT=1` on while the guard
/// is alive and restores the prior state on drop. Used to keep
/// the function-izer's `build_cache` call from fatal-exiting on
/// translator gaps in body items we don't depend on — those
/// become silent warnings (still printed to stderr in the
/// existing path), and the function-izer's `extract_program`
/// then sees a partial body. If outputs aren't covered, it
/// returns None and we fall back to the slow path which uses
/// the matching silently-skip path for inherited Constraints.
pub(super) struct LenientGuard {
    prior: Option<String>,
}

impl LenientGuard {
    pub(super) fn enable() -> Self {
        let prior = std::env::var("EVIDENT_LENIENT").ok();
        std::env::set_var("EVIDENT_LENIENT", "1");
        LenientGuard { prior }
    }
}

impl Drop for LenientGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => std::env::set_var("EVIDENT_LENIENT", v),
            None    => std::env::remove_var("EVIDENT_LENIENT"),
        }
    }
}
