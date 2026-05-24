//! Z3 datatype type-name helpers for `Seq(T)` payloads.
//!
//! Pure string utilities used by the translator and decoder to
//! recognize and name the internal Cons-shaped helper enums the
//! runtime synthesizes for `Seq(T)` enum-variant payloads.

/// Parse `Seq(T)` → `Some(T)`; otherwise `None`.
pub fn parse_seq_type(s: &str) -> Option<&str> {
    if s.starts_with("Seq(") && s.ends_with(')') {
        Some(&s[4..s.len() - 1])
    } else {
        None
    }
}

/// Helper enum name for internal-Cons backing of `Seq(T)`.
/// Convention: `__SeqOf_T`. The underscores prefix marks it as
/// runtime-internal — never written by users, never appears in
/// error messages outside debug contexts.
pub fn internal_cons_helper_name(t: &str) -> String {
    format!("__SeqOf_{}", t)
}
