//! Lenient-mode flag: process-wide via the `EVIDENT_LENIENT` env var.
//! Demotes the otherwise-fatal "dropped constraint" error to a warning.

pub(crate) fn lenient_enabled() -> bool {
    std::env::var("EVIDENT_LENIENT")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}
