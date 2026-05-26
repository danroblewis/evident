//! RAII guard for the `EVIDENT_LENIENT` env var.

/// Sets `EVIDENT_LENIENT=1` while alive, then restores prior state on drop.
/// Lets the functionizer skip untranslatable body items instead of fatal-exiting.
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
