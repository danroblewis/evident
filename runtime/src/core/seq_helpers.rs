pub fn parse_seq_type(s: &str) -> Option<&str> {
    if s.starts_with("Seq(") && s.ends_with(')') {
        Some(&s[4..s.len() - 1])
    } else {
        None
    }
}

pub fn internal_cons_helper_name(t: &str) -> String {
    format!("__SeqOf_{}", t)
}
