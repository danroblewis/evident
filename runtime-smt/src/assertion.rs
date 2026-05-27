//! N1 assertion builder.
//!
//! Turns runtime [`Value`]s into SMT-LIB s-expression terms and assembles
//! per-tick pin assertions that the engine prepends to each tick's solver
//! problem — fixing the previous-tick state and the given inputs before
//! `check-sat`.

use std::collections::BTreeMap;

use crate::z3c::Value;

/// Emit a single [`Value`] as a self-contained SMT-LIB s-expression term.
///
/// The emitted term is valid Z3 input and round-trips through Z3 back to the
/// same value. Rules per variant:
///
/// - `Int(i)`: non-negative → bare decimal; negative → `(- <magnitude>)`.
///   `i64::MIN` is handled by widening to `u128` before formatting.
/// - `Bool(b)`: `"true"` or `"false"` (lowercase, matching SMT-LIB).
/// - `Real(r)`: decimal with at least one `.`; negative → `(- <magnitude>)`.
/// - `Str(s)`: double-quoted SMT-LIB string; internal `"` chars are doubled
///   (SMT-LIB string escape convention).
/// - `Enum { ctor, args }`: nullary → bare constructor name; applied →
///   `(ctor arg₀ arg₁ …)` with args recursively emitted.
/// - `Seq(_)`: returns `Err` — sequences are outputs, not pin-able inputs.
pub fn value_to_smtlib(v: &Value) -> Result<String, String> {
    match v {
        Value::Int(i) => {
            if *i >= 0 {
                Ok(i.to_string())
            } else {
                // Widen to u128 to safely handle i64::MIN without overflow.
                let magnitude = (*i as i128).unsigned_abs();
                Ok(format!("(- {magnitude})"))
            }
        }

        Value::Bool(b) => Ok(if *b { "true".to_string() } else { "false".to_string() }),

        Value::Real(r) => {
            if r.is_nan() || r.is_infinite() {
                return Err(format!("cannot represent non-finite Real ({r}) in SMT-LIB"));
            }
            // Format with enough precision; ensure there is always a `.`.
            let magnitude = r.abs();
            let mag_str = {
                let s = format!("{magnitude}");
                if s.contains('.') {
                    s
                } else {
                    format!("{s}.0")
                }
            };
            if *r < 0.0 {
                Ok(format!("(- {mag_str})"))
            } else {
                Ok(mag_str)
            }
        }

        Value::Str(s) => {
            // SMT-LIB string literals: wrap in double-quotes, double any
            // internal double-quote character.
            let escaped = s.replace('"', "\"\"");
            Ok(format!("\"{escaped}\""))
        }

        Value::Enum { ctor, args } => {
            if args.is_empty() {
                Ok(ctor.clone())
            } else {
                let mut parts = Vec::with_capacity(args.len());
                for arg in args {
                    parts.push(value_to_smtlib(arg)?);
                }
                Ok(format!("({} {})", ctor, parts.join(" ")))
            }
        }

        Value::Seq(_) => Err(
            "Seq values cannot be pinned as SMT-LIB assertions (sequences are outputs, not inputs)"
                .to_string(),
        ),
    }
}

/// Build per-tick pin assertions for one FSM tick.
///
/// `prev` maps previous-state const names (e.g. `"_count"`) to their values;
/// `given` maps input const names to their values. Emits one
/// `(assert (= <name> <term>))` line per entry — all `prev` entries first,
/// then `given` entries. Within each map, entries are iterated in sorted
/// (key-alphabetical) order because both maps are `BTreeMap`.
///
/// Returns the lines joined by `'\n'`. Returns an empty string when both maps
/// are empty. Propagates any `value_to_smtlib` error immediately.
pub fn pin_assertions(
    prev: &BTreeMap<String, Value>,
    given: &BTreeMap<String, Value>,
) -> Result<String, String> {
    let mut lines: Vec<String> = Vec::with_capacity(prev.len() + given.len());

    for (name, value) in prev {
        let term = value_to_smtlib(value)?;
        lines.push(format!("(assert (= {name} {term}))"));
    }

    for (name, value) in given {
        let term = value_to_smtlib(value)?;
        lines.push(format!("(assert (= {name} {term}))"));
    }

    Ok(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::z3c::{solve_smtlib, SolveOutcome, Value};

    // -----------------------------------------------------------------------
    // Helper: round-trip a Value through Z3 given a minimal SMT-LIB program.
    // -----------------------------------------------------------------------

    /// Solve a minimal SMT-LIB program and return the model.
    fn sat_model(smt: &str) -> crate::z3c::Model {
        match solve_smtlib(smt).expect("solve_smtlib should not error") {
            SolveOutcome::Sat(m) => m,
            other => panic!("expected Sat, got {other:?}\nSMT:\n{smt}"),
        }
    }

    // -----------------------------------------------------------------------
    // value_to_smtlib — Int
    // -----------------------------------------------------------------------

    #[test]
    fn int_zero_roundtrips() {
        let term = value_to_smtlib(&Value::Int(0)).unwrap();
        assert_eq!(term, "0");
        let smt = format!("(declare-const c Int)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Int(0)));
    }

    #[test]
    fn int_positive_roundtrips() {
        let term = value_to_smtlib(&Value::Int(42)).unwrap();
        assert_eq!(term, "42");
        let smt = format!("(declare-const c Int)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Int(42)));
    }

    #[test]
    fn int_negative_roundtrips() {
        let term = value_to_smtlib(&Value::Int(-5)).unwrap();
        assert_eq!(term, "(- 5)");
        let smt = format!("(declare-const c Int)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Int(-5)));
    }

    #[test]
    fn int_large_negative_roundtrips() {
        // Use a large negative value well within i64 range.
        let term = value_to_smtlib(&Value::Int(-1_000_000_007)).unwrap();
        assert_eq!(term, "(- 1000000007)");
        let smt = format!("(declare-const c Int)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Int(-1_000_000_007)));
    }

    #[test]
    fn int_min_does_not_overflow() {
        // i64::MIN cannot be negated in i64 — must use wider type.
        let result = value_to_smtlib(&Value::Int(i64::MIN));
        assert!(result.is_ok(), "i64::MIN should not return Err: {:?}", result);
        let term = result.unwrap();
        // Should look like "(- 9223372036854775808)"
        assert!(term.starts_with("(- "), "expected negative form, got {term:?}");
        // Z3 round-trip: Int range in Z3 is unbounded, so this is representable.
        let smt = format!("(declare-const c Int)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Int(i64::MIN)));
    }

    // -----------------------------------------------------------------------
    // value_to_smtlib — Bool
    // -----------------------------------------------------------------------

    #[test]
    fn bool_true_roundtrips() {
        let term = value_to_smtlib(&Value::Bool(true)).unwrap();
        assert_eq!(term, "true");
        let smt = format!("(declare-const c Bool)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Bool(true)));
    }

    #[test]
    fn bool_false_roundtrips() {
        let term = value_to_smtlib(&Value::Bool(false)).unwrap();
        assert_eq!(term, "false");
        let smt = format!("(declare-const c Bool)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Bool(false)));
    }

    // -----------------------------------------------------------------------
    // value_to_smtlib — Real
    // -----------------------------------------------------------------------

    #[test]
    fn real_positive_roundtrips() {
        let term = value_to_smtlib(&Value::Real(1.5)).unwrap();
        // Must contain a dot.
        assert!(term.contains('.'), "expected decimal with dot, got {term:?}");
        let smt = format!("(declare-const c Real)\n(assert (= c {term}))");
        match sat_model(&smt).get("c") {
            Some(Value::Real(got)) => assert!(
                (*got - 1.5).abs() < 1e-9,
                "expected 1.5, got {got}"
            ),
            other => panic!("expected Real(1.5), got {other:?}"),
        }
    }

    #[test]
    fn real_negative_roundtrips() {
        let term = value_to_smtlib(&Value::Real(-2.0)).unwrap();
        assert!(term.starts_with("(- "), "expected negative form, got {term:?}");
        assert!(term.contains('.'), "expected decimal with dot in magnitude, got {term:?}");
        let smt = format!("(declare-const c Real)\n(assert (= c {term}))");
        match sat_model(&smt).get("c") {
            Some(Value::Real(got)) => assert!(
                (*got - (-2.0)).abs() < 1e-9,
                "expected -2.0, got {got}"
            ),
            other => panic!("expected Real(-2.0), got {other:?}"),
        }
    }

    #[test]
    fn real_integer_valued_gets_dot() {
        // 3.0 — ensure `.0` suffix is present even for whole-number doubles.
        let term = value_to_smtlib(&Value::Real(3.0)).unwrap();
        assert!(term.contains('.'), "expected dot in {term:?}");
        let smt = format!("(declare-const c Real)\n(assert (= c {term}))");
        match sat_model(&smt).get("c") {
            Some(Value::Real(got)) => assert!((*got - 3.0).abs() < 1e-9, "expected 3.0, got {got}"),
            other => panic!("expected Real(3.0), got {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // value_to_smtlib — Str
    // -----------------------------------------------------------------------

    #[test]
    fn str_plain_roundtrips() {
        let term = value_to_smtlib(&Value::Str("hello".to_string())).unwrap();
        assert_eq!(term, "\"hello\"");
        let smt = format!("(declare-const c String)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Str("hello".to_string())));
    }

    #[test]
    fn str_with_embedded_quote_roundtrips() {
        // he"llo → "he""llo" (SMT-LIB doubles internal quotes)
        let term = value_to_smtlib(&Value::Str("he\"llo".to_string())).unwrap();
        assert_eq!(term, "\"he\"\"llo\"");
        let smt = format!("(declare-const c String)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Str("he\"llo".to_string())));
    }

    #[test]
    fn str_empty_roundtrips() {
        let term = value_to_smtlib(&Value::Str(String::new())).unwrap();
        assert_eq!(term, "\"\"");
        let smt = format!("(declare-const c String)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Str(String::new())));
    }

    #[test]
    fn str_multiple_quotes_roundtrips() {
        // a"b"c → "a""b""c"
        let term = value_to_smtlib(&Value::Str("a\"b\"c".to_string())).unwrap();
        assert_eq!(term, "\"a\"\"b\"\"c\"");
        let smt = format!("(declare-const c String)\n(assert (= c {term}))");
        assert_eq!(sat_model(&smt).get("c"), Some(&Value::Str("a\"b\"c".to_string())));
    }

    // -----------------------------------------------------------------------
    // value_to_smtlib — Enum
    // -----------------------------------------------------------------------

    #[test]
    fn enum_nullary_roundtrips() {
        let v = Value::nullary("Green");
        let term = value_to_smtlib(&v).unwrap();
        assert_eq!(term, "Green");

        let smt = "(declare-datatypes ((Color 0)) (((Red) (Green) (Blue))))\n\
                   (declare-const c Color)\n\
                   (assert (= c Green))";
        // Verify the term is exactly what Z3 expects for the Green constructor.
        match solve_smtlib(smt).unwrap() {
            SolveOutcome::Sat(m) => {
                assert_eq!(m.get("c"), Some(&Value::nullary("Green")));
            }
            other => panic!("expected Sat, got {other:?}"),
        }
    }

    #[test]
    fn enum_nullary_full_roundtrip() {
        // Actually pin using the emitted term.
        let v = Value::nullary("Blue");
        let term = value_to_smtlib(&v).unwrap();

        let smt = format!(
            "(declare-datatypes ((Color 0)) (((Red) (Green) (Blue))))\n\
             (declare-const c Color)\n\
             (assert (= c {term}))"
        );
        assert_eq!(
            sat_model(&smt).get("c"),
            Some(&Value::nullary("Blue"))
        );
    }

    #[test]
    fn enum_applied_roundtrips() {
        // Applied constructor: Println("hi") — needs a datatype with a String arg.
        let v = Value::Enum {
            ctor: "Println".to_string(),
            args: vec![Value::Str("hi".to_string())],
        };
        let term = value_to_smtlib(&v).unwrap();
        // Should be: (Println "hi")
        assert_eq!(term, "(Println \"hi\")");

        let smt = format!(
            "(declare-datatypes ((Effect 0)) (((Println (msg String)) (Exit (code Int)) (Tick))))\n\
             (declare-const c Effect)\n\
             (assert (= c {term}))"
        );
        assert_eq!(
            sat_model(&smt).get("c"),
            Some(&Value::Enum {
                ctor: "Println".to_string(),
                args: vec![Value::Str("hi".to_string())],
            })
        );
    }

    #[test]
    fn enum_applied_int_arg_roundtrips() {
        // Exit(0) — constructor with Int arg.
        let v = Value::Enum {
            ctor: "Exit".to_string(),
            args: vec![Value::Int(0)],
        };
        let term = value_to_smtlib(&v).unwrap();
        assert_eq!(term, "(Exit 0)");

        let smt = format!(
            "(declare-datatypes ((Effect 0)) (((Println (msg String)) (Exit (code Int)) (Tick))))\n\
             (declare-const c Effect)\n\
             (assert (= c {term}))"
        );
        assert_eq!(
            sat_model(&smt).get("c"),
            Some(&Value::Enum {
                ctor: "Exit".to_string(),
                args: vec![Value::Int(0)],
            })
        );
    }

    // -----------------------------------------------------------------------
    // value_to_smtlib — Seq returns Err
    // -----------------------------------------------------------------------

    #[test]
    fn seq_returns_err() {
        let v = Value::Seq(vec![Value::Int(1), Value::Int(2)]);
        assert!(
            value_to_smtlib(&v).is_err(),
            "Seq should return Err from value_to_smtlib"
        );
    }

    #[test]
    fn seq_empty_also_returns_err() {
        let v = Value::Seq(vec![]);
        assert!(
            value_to_smtlib(&v).is_err(),
            "empty Seq should also return Err"
        );
    }

    // -----------------------------------------------------------------------
    // pin_assertions
    // -----------------------------------------------------------------------

    #[test]
    fn pin_assertions_prev_before_given_sorted() {
        let mut prev = BTreeMap::new();
        prev.insert("_count".to_string(), Value::Int(3));

        let mut given = BTreeMap::new();
        given.insert("dt".to_string(), Value::Int(16));

        let result = pin_assertions(&prev, &given).unwrap();
        assert_eq!(
            result,
            "(assert (= _count 3))\n(assert (= dt 16))"
        );
    }

    #[test]
    fn pin_assertions_empty_maps_give_empty_string() {
        let result = pin_assertions(&BTreeMap::new(), &BTreeMap::new()).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn pin_assertions_only_prev() {
        let mut prev = BTreeMap::new();
        prev.insert("_x".to_string(), Value::Bool(true));

        let result = pin_assertions(&prev, &BTreeMap::new()).unwrap();
        assert_eq!(result, "(assert (= _x true))");
    }

    #[test]
    fn pin_assertions_only_given() {
        let mut given = BTreeMap::new();
        given.insert("key".to_string(), Value::Bool(false));

        let result = pin_assertions(&BTreeMap::new(), &given).unwrap();
        assert_eq!(result, "(assert (= key false))");
    }

    #[test]
    fn pin_assertions_multiple_prev_sorted() {
        // BTreeMap sorts alphabetically; _b < _a is impossible (sorted
        // ascending), so _a comes before _b.
        let mut prev = BTreeMap::new();
        prev.insert("_b".to_string(), Value::Int(2));
        prev.insert("_a".to_string(), Value::Int(1));

        let result = pin_assertions(&prev, &BTreeMap::new()).unwrap();
        // BTreeMap iteration is sorted; _a before _b
        assert_eq!(
            result,
            "(assert (= _a 1))\n(assert (= _b 2))"
        );
    }

    #[test]
    fn pin_assertions_seq_value_propagates_err() {
        // A Seq value in prev should cause an Err to bubble up.
        let mut prev = BTreeMap::new();
        prev.insert("_effects".to_string(), Value::Seq(vec![]));

        assert!(
            pin_assertions(&prev, &BTreeMap::new()).is_err(),
            "Seq in prev map should propagate Err"
        );
    }

    #[test]
    fn pin_assertions_negative_int_emits_correct_form() {
        let mut prev = BTreeMap::new();
        prev.insert("_count".to_string(), Value::Int(-7));

        let result = pin_assertions(&prev, &BTreeMap::new()).unwrap();
        assert_eq!(result, "(assert (= _count (- 7)))");
    }

    #[test]
    fn pin_assertions_roundtrip_through_z3() {
        // Build a real SMT problem: declare the consts, pin them, check-sat,
        // verify the model values are exactly what we pinned.
        let mut prev = BTreeMap::new();
        prev.insert("_count".to_string(), Value::Int(3));

        let mut given = BTreeMap::new();
        given.insert("dt".to_string(), Value::Int(16));

        let pins = pin_assertions(&prev, &given).unwrap();

        let smt = format!(
            "(declare-const _count Int)\n\
             (declare-const dt Int)\n\
             {pins}"
        );
        let m = sat_model(&smt);
        assert_eq!(m.get("_count"), Some(&Value::Int(3)));
        assert_eq!(m.get("dt"), Some(&Value::Int(16)));
    }

    #[test]
    fn pin_assertions_enum_roundtrip_through_z3() {
        let mut prev = BTreeMap::new();
        prev.insert("_state".to_string(), Value::nullary("Running"));

        let pins = pin_assertions(&prev, &BTreeMap::new()).unwrap();
        assert_eq!(pins, "(assert (= _state Running))");

        let smt = format!(
            "(declare-datatypes ((Phase 0)) (((Idle) (Running) (Done))))\n\
             (declare-const _state Phase)\n\
             {pins}"
        );
        assert_eq!(
            sat_model(&smt).get("_state"),
            Some(&Value::nullary("Running"))
        );
    }
}
