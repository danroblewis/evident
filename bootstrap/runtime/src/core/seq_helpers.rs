//! Pure string utilities for recognizing and naming the internal Cons-shaped helper enums
//! the runtime synthesizes for `Seq(T)` enum-variant payloads.

/// Parse `Seq(T)` → `Some(T)`; otherwise `None`.
pub fn parse_seq_type(s: &str) -> Option<&str> {
    if s.starts_with("Seq(") && s.ends_with(')') {
        Some(&s[4..s.len() - 1])
    } else {
        None
    }
}

/// Internal Cons-enum name for `Seq(T)`: `__SeqOf_T`. Double-underscore prefix = runtime-internal.
pub fn internal_cons_helper_name(t: &str) -> String {
    format!("__SeqOf_{}", t)
}
