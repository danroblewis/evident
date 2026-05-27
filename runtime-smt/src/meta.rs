//! N1 metadata loader.
//!
//! Parses a fixture file that embeds JSON metadata and one or more named
//! SMT-LIB transition blocks. See `FORMAT.md` for the on-disk format.

use std::path::Path;

use crate::spec::Problem;

// ---------------------------------------------------------------------------
// Public error type
// ---------------------------------------------------------------------------

/// Error from the metadata loader: a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaError(pub String);

impl std::fmt::Display for MetaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "meta: {}", self.0)
    }
}

impl std::error::Error for MetaError {}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a fixture text (in-memory) into a [`Problem`] with `.transition`
/// fields filled in from the `; @transition <name>` blocks.
pub fn load_str(text: &str) -> Result<Problem, MetaError> {
    // --- Phase 1: find @meta / @end bounds and strip ';' prefixes ----------

    let lines: Vec<&str> = text.lines().collect();

    let meta_start = lines.iter().position(|l| l.trim() == "; @meta").ok_or_else(
        || MetaError("missing `; @meta` marker".into()),
    )?;

    let meta_end = lines
        .iter()
        .skip(meta_start + 1)
        .position(|l| l.trim() == "; @end")
        .map(|p| p + meta_start + 1)
        .ok_or_else(|| MetaError("missing `; @end` marker".into()))?;

    // Collect the JSON body lines, stripping leading "; " or ";"
    let json_text: String = lines[meta_start + 1..meta_end]
        .iter()
        .map(|line| {
            let stripped = line.trim_start();
            if let Some(rest) = stripped.strip_prefix(';') {
                // strip the ';' then one optional space
                if let Some(rest2) = rest.strip_prefix(' ') {
                    rest2
                } else {
                    rest
                }
            } else {
                line
            }
        })
        .collect::<Vec<&str>>()
        .join("\n");

    // --- Phase 2: parse JSON ------------------------------------------------

    let mut problem: Problem = serde_json::from_str(&json_text)
        .map_err(|e| MetaError(format!("JSON parse error: {e}")))?;

    if problem.fsms.is_empty() {
        return Err(MetaError("`fsms` must be non-empty".into()));
    }

    // --- Phase 3: validate StateVar prev != next ---------------------------

    for fsm in &problem.fsms {
        for sv in &fsm.state {
            if sv.prev == sv.next {
                return Err(MetaError(format!(
                    "fsm `{}`: state var `{}` has prev == next (they must differ)",
                    fsm.name, sv.prev
                )));
            }
        }
    }

    // --- Phase 4: collect @transition blocks --------------------------------

    // Build a set of valid FSM names (owned Strings, no borrow on problem).
    let fsm_names: std::collections::HashSet<String> =
        problem.fsms.iter().map(|f| f.name.clone()).collect();

    // Parse all lines after @end, collecting name → transition text.
    let after_end = &lines[meta_end + 1..];

    // We'll accumulate: name → vec-of-lines while scanning, then convert.
    let mut transition_map: std::collections::HashMap<String, Vec<&str>> =
        std::collections::HashMap::new();
    let mut current_name: Option<String> = None;

    for &line in after_end {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("; @transition") {
            let name = rest.trim().to_string();
            // Validate immediately — unknown FSM names are an error.
            if !fsm_names.contains(&name) {
                return Err(MetaError(format!(
                    "`; @transition {name}` does not match any declared FSM"
                )));
            }
            current_name = Some(name);
        } else if let Some(ref n) = current_name {
            transition_map.entry(n.clone()).or_default().push(line);
        }
        // Lines before the first @transition (and outside @meta…@end) are ignored.
    }

    // --- Phase 5: validate coverage and assign -----------------------------

    for fsm in &mut problem.fsms {
        let raw_lines = transition_map.remove(&fsm.name).ok_or_else(|| {
            MetaError(format!("no `; @transition {}` block found", fsm.name))
        })?;
        let content_lines = trim_blank_edges(&raw_lines);
        fsm.transition = content_lines.join("\n");
    }

    Ok(problem)
}

/// Load a fixture from disk and call [`load_str`].
pub fn load_file(path: &Path) -> Result<Problem, MetaError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        MetaError(format!("cannot read `{}`: {e}", path.display()))
    })?;
    load_str(&text)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Strip leading and trailing blank (whitespace-only) lines from a slice,
/// returning the trimmed slice. Internal blank lines are preserved.
fn trim_blank_edges<'a>(lines: &'a [&'a str]) -> &'a [&'a str] {
    let start = lines.iter().position(|l| !l.trim().is_empty()).unwrap_or(lines.len());
    let end = lines.iter().rposition(|l| !l.trim().is_empty()).map(|p| p + 1).unwrap_or(start);
    &lines[start..end]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Lit, Sort};

    const COUNTDOWN_FIXTURE: &str = r#"; @meta
; {
;   "fsms": [
;     { "name": "countdown",
;       "state": [{"prev":"_count","next":"count","sort":"Int","init":3}],
;       "effects": {"var":"effects"},
;       "halt": {"var":"halt"} }
;   ]
; }
; @end
; @transition countdown
(declare-datatypes ((Effect 0)) (((Println (msg String)) (Exit (code Int)) (Tick))))
(declare-const _count Int)
(declare-const count Int)
(declare-const effects (Seq Effect))
(declare-const halt Bool)
(assert (= count (- _count 1)))
(assert (= halt (<= count 0)))
(assert (= effects (seq.unit (Tick))))
"#;

    #[test]
    fn countdown_fixture_loads_correctly() {
        let problem = load_str(COUNTDOWN_FIXTURE).expect("should load");
        assert_eq!(problem.fsms.len(), 1);
        let fsm = &problem.fsms[0];
        assert_eq!(fsm.name, "countdown");

        // state var
        assert_eq!(fsm.state.len(), 1);
        let sv = &fsm.state[0];
        assert_eq!(sv.prev, "_count");
        assert_eq!(sv.next, "count");
        assert_eq!(sv.sort, Sort::Int);
        assert_eq!(sv.init, Some(Lit::Int(3)));

        // effects
        assert!(fsm.effects.is_some());
        assert_eq!(fsm.effects.as_ref().unwrap().var, "effects");

        // halt
        assert!(fsm.halt.is_some());
        assert_eq!(fsm.halt.as_ref().unwrap().var, Some("halt".into()));

        // transition text
        assert!(
            fsm.transition.contains("(assert (= count (- _count 1)))"),
            "transition text should contain the assert: {:?}",
            fsm.transition
        );
    }

    #[test]
    fn two_fsm_fixture_assigns_transitions_by_name() {
        let fixture = r#"; @meta
; {
;   "fsms": [
;     { "name": "alpha",
;       "state": [{"prev":"_x","next":"x","sort":"Int"}] },
;     { "name": "beta",
;       "state": [{"prev":"_y","next":"y","sort":"Bool"}] }
;   ]
; }
; @end
; @transition beta
(declare-const _y Bool)
(declare-const y Bool)
(assert (= y (not _y)))
; @transition alpha
(declare-const _x Int)
(declare-const x Int)
(assert (= x (+ _x 1)))
"#;
        let problem = load_str(fixture).expect("should load two-fsm fixture");
        assert_eq!(problem.fsms.len(), 2);

        let alpha = problem.fsms.iter().find(|f| f.name == "alpha").unwrap();
        let beta = problem.fsms.iter().find(|f| f.name == "beta").unwrap();

        assert!(
            alpha.transition.contains("(assert (= x (+ _x 1)))"),
            "alpha got wrong transition: {:?}",
            alpha.transition
        );
        assert!(
            beta.transition.contains("(assert (= y (not _y)))"),
            "beta got wrong transition: {:?}",
            beta.transition
        );
    }

    #[test]
    fn missing_transition_block_is_error() {
        let fixture = r#"; @meta
; { "fsms": [{ "name": "missing" }] }
; @end
"#;
        let err = load_str(fixture).unwrap_err();
        assert!(
            err.0.contains("missing") && err.0.contains("@transition"),
            "expected missing-transition error, got: {:?}",
            err.0
        );
    }

    #[test]
    fn unknown_transition_name_is_error() {
        let fixture = r#"; @meta
; { "fsms": [{ "name": "real_fsm",
;              "state": [{"prev":"_s","next":"s","sort":"Int"}] }] }
; @end
; @transition real_fsm
(declare-const _s Int)
(declare-const s Int)
(assert (= s (+ _s 1)))
; @transition ghost_fsm
(declare-const z Int)
(assert (= z 0))
"#;
        let err = load_str(fixture).unwrap_err();
        assert!(
            err.0.contains("ghost_fsm"),
            "expected error mentioning unknown FSM name, got: {:?}",
            err.0
        );
    }

    #[test]
    fn malformed_json_is_error() {
        let fixture = r#"; @meta
; { "fsms": [ this is not valid JSON ] }
; @end
; @transition x
(assert true)
"#;
        let err = load_str(fixture).unwrap_err();
        assert!(
            err.0.contains("JSON"),
            "expected JSON error, got: {:?}",
            err.0
        );
    }

    #[test]
    fn prev_eq_next_is_error() {
        let fixture = r#"; @meta
; { "fsms": [{ "name": "bad",
;              "state": [{"prev":"count","next":"count","sort":"Int"}] }] }
; @end
; @transition bad
(declare-const count Int)
(assert (= count 0))
"#;
        let err = load_str(fixture).unwrap_err();
        assert!(
            err.0.contains("prev == next"),
            "expected prev==next error, got: {:?}",
            err.0
        );
    }

    #[test]
    fn missing_end_marker_is_error() {
        let fixture = r#"; @meta
; { "fsms": [{ "name": "x" }] }
"#;
        let err = load_str(fixture).unwrap_err();
        assert!(
            err.0.contains("@end"),
            "expected @end error, got: {:?}",
            err.0
        );
    }

    #[test]
    fn missing_meta_marker_is_error() {
        let fixture = r#"(declare-const x Int)
; @end
; @transition something
(assert (= x 0))
"#;
        let err = load_str(fixture).unwrap_err();
        assert!(
            err.0.contains("@meta"),
            "expected @meta error, got: {:?}",
            err.0
        );
    }

    #[test]
    fn lines_before_meta_are_ignored() {
        let fixture = r#"; This file is a countdown fixture
; Author: test
; @meta
; { "fsms": [{ "name": "cd",
;              "state": [{"prev":"_n","next":"n","sort":"Int"}],
;              "halt": {} }] }
; @end
; @transition cd
(declare-const _n Int)
(declare-const n Int)
(assert (= n (- _n 1)))
"#;
        let problem = load_str(fixture).expect("preamble lines should be ignored");
        assert_eq!(problem.fsms[0].name, "cd");
    }

    #[test]
    fn transition_text_preserves_internal_blank_lines() {
        let fixture = r#"; @meta
; { "fsms": [{ "name": "spaced",
;              "state": [{"prev":"_v","next":"v","sort":"Int"}] }] }
; @end
; @transition spaced
(declare-const _v Int)

(declare-const v Int)
(assert (= v (+ _v 1)))
"#;
        let problem = load_str(fixture).expect("should load");
        let t = &problem.fsms[0].transition;
        // There should be a blank line between the two declares
        assert!(t.contains("(declare-const _v Int)\n\n"), "expected blank line preserved: {:?}", t);
    }

    #[test]
    fn empty_fsms_array_is_error() {
        let fixture = r#"; @meta
; { "fsms": [] }
; @end
"#;
        let err = load_str(fixture).unwrap_err();
        assert!(
            err.0.contains("non-empty"),
            "expected non-empty error, got: {:?}",
            err.0
        );
    }

    #[test]
    fn halt_default_has_no_var() {
        // halt: {} → HaltSpec { var: None }
        let fixture = r#"; @meta
; { "fsms": [{ "name": "cd",
;              "state": [{"prev":"_n","next":"n","sort":"Int"}],
;              "halt": {} }] }
; @end
; @transition cd
(declare-const _n Int)
(declare-const n Int)
(assert (= n (- _n 1)))
"#;
        let problem = load_str(fixture).expect("should load");
        let halt = problem.fsms[0].halt.as_ref().unwrap();
        assert_eq!(halt.var, None);
    }

    #[test]
    fn seq_sort_parses_in_state_var() {
        let fixture = r#"; @meta
; { "fsms": [{ "name": "effects_fsm",
;              "state": [{"prev":"_q","next":"q","sort":"Seq(Effect)"}] }] }
; @end
; @transition effects_fsm
(declare-const _q (Seq Effect))
(declare-const q (Seq Effect))
(assert (= q _q))
"#;
        let problem = load_str(fixture).expect("should load");
        assert_eq!(
            problem.fsms[0].state[0].sort,
            Sort::Seq(Box::new(Sort::Datatype("Effect".into())))
        );
    }
}
