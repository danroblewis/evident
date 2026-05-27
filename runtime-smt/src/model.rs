//! N1 model extractor.
//!
//! Decodes a solved [`Model`] into a typed [`TickModel`] using the FSM's role
//! assignments. Pure — no Z3 calls; it reads only the already-decoded [`Model`].

use crate::spec::{EffectValue, FsmSpec, TickModel};
use crate::z3c::{Model, Value};

/// Extract the typed tick result from a solved model, per the FSM's metadata.
///
/// # Role assignments
///
/// - **next_state**: looks up each `StateVar.next` name; if present, records
///   `(next_name, value)`.
/// - **world_writes**: looks up each name in `FsmSpec.world_writes`; if
///   present, records `(name, value)`.
/// - **effects**: decodes the value named by `FsmSpec.effects.var` (if any)
///   as either a native `(Seq T)` or a cons-list datatype. Unknown shapes
///   produce an empty list rather than a panic.
/// - **halt_flag**: reads the Bool named by `FsmSpec.halt.var` (if any);
///   defaults to `false` when absent or not a Bool.
pub fn extract(model: &Model, fsm: &FsmSpec) -> TickModel {
    // --- next_state -----------------------------------------------------------
    let next_state: Vec<(String, Value)> = fsm
        .state
        .iter()
        .filter_map(|sv| {
            model.get(&sv.next).map(|v| (sv.next.clone(), v.clone()))
        })
        .collect();

    // --- world_writes ---------------------------------------------------------
    let world_writes: Vec<(String, Value)> = fsm
        .world_writes
        .iter()
        .filter_map(|name| {
            model.get(name).map(|v| (name.clone(), v.clone()))
        })
        .collect();

    // --- effects --------------------------------------------------------------
    let effects: Vec<EffectValue> = match &fsm.effects {
        None => Vec::new(),
        Some(spec) => match model.get(&spec.var) {
            None => Vec::new(),
            Some(value) => decode_effects(value),
        },
    };

    // --- halt_flag ------------------------------------------------------------
    let halt_flag = match &fsm.halt {
        None => false,
        Some(h) => match &h.var {
            None => false,
            Some(name) => match model.get(name) {
                Some(Value::Bool(b)) => *b,
                _ => false,
            },
        },
    };

    TickModel { next_state, world_writes, effects, halt_flag }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Decode an effects value into a list of [`EffectValue`]s. Handles:
///
/// 1. `Value::Seq` — native SMT-LIB sequence; each element that is an `Enum`
///    becomes an `EffectValue`. Non-Enum elements are skipped.
/// 2. `Value::Enum` — treated as a cons-list datatype. A cons cell has a ctor
///    of (case-insensitively) `cons` / `insert` with exactly 2 args `[head,
///    tail]`; the terminator is a nullary `Enum` with ctor `nil` / `empty`.
///    The `head` of each cons cell must itself be an `Enum` to become an
///    `EffectValue`; non-Enum heads are skipped.
/// 3. Any other shape — returns an empty list (no panic).
fn decode_effects(value: &Value) -> Vec<EffectValue> {
    match value {
        // Native sequence (the common case).
        Value::Seq(elems) => elems
            .iter()
            .filter_map(|e| {
                if let Value::Enum { ctor, args } = e {
                    Some(EffectValue { ctor: ctor.clone(), args: args.clone() })
                } else {
                    None
                }
            })
            .collect(),

        // Cons-list encoded as a recursive datatype.
        Value::Enum { .. } => {
            let mut out = Vec::new();
            collect_cons_list(value, &mut out);
            out
        }

        // Unknown shape — return empty, no panic.
        _ => Vec::new(),
    }
}

/// Walk a cons-list `Value::Enum`, collecting `EffectValue`s from the heads.
///
/// Terminates when it sees a nullary Enum whose ctor is `nil` / `empty`
/// (case-insensitive) or when the structure no longer matches the expected
/// cons-cell shape (2-arg Enum with a cons-like ctor).
fn collect_cons_list(value: &Value, out: &mut Vec<EffectValue>) {
    let mut current = value;
    loop {
        match current {
            Value::Enum { ctor, args } => {
                let lc = ctor.to_lowercase();
                if lc == "nil" || lc == "empty" {
                    // Terminator — done.
                    break;
                }
                if (lc == "cons" || lc == "insert") && args.len() == 2 {
                    // Cons cell: head = args[0], tail = args[1].
                    let head = &args[0];
                    if let Value::Enum { ctor: hctor, args: hargs } = head {
                        out.push(EffectValue {
                            ctor: hctor.clone(),
                            args: hargs.clone(),
                        });
                    }
                    // Advance to the tail.
                    // SAFETY: we need a reference that outlives the match arm.
                    // We can't move out of `current` (it's a shared ref), so
                    // we use a raw pointer to avoid the borrow-checker complaint
                    // about `args` being borrowed while `current` is reassigned.
                    //
                    // This is safe because the Value tree is immutable and the
                    // reference chain is stable for the lifetime of `value`.
                    current = unsafe {
                        let tail_ptr: *const Value = &args[1];
                        &*tail_ptr
                    };
                } else {
                    // Unrecognised ctor or wrong arity — stop.
                    break;
                }
            }
            // Non-Enum node — stop.
            _ => break,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{EffectSpec, HaltSpec, Sort, StateVar};

    // -------------------------------------------------------------------------
    // Test helpers
    // -------------------------------------------------------------------------

    /// Build a minimal `FsmSpec` with only the fields the caller specifies.
    /// All `Vec` fields default to empty; all `Option` fields default to `None`.
    fn make_fsm(
        state: Vec<StateVar>,
        effects: Option<EffectSpec>,
        halt: Option<HaltSpec>,
        world_writes: Vec<String>,
    ) -> FsmSpec {
        FsmSpec {
            name: "test".into(),
            transition: String::new(),
            state,
            given: Vec::new(),
            effects,
            halt,
            last_results: None,
            world_writes,
            world_reads: Vec::new(),
        }
    }

    fn make_state_var(prev: &str, next: &str) -> StateVar {
        StateVar {
            prev: prev.into(),
            next: next.into(),
            sort: Sort::Int,
            init: None,
        }
    }

    fn make_model(bindings: Vec<(&str, Value)>) -> Model {
        Model {
            bindings: bindings.into_iter().map(|(n, v)| (n.to_string(), v)).collect(),
        }
    }

    // -------------------------------------------------------------------------
    // next_state
    // -------------------------------------------------------------------------

    #[test]
    fn next_state_present_var_is_captured() {
        let fsm = make_fsm(vec![make_state_var("_count", "count")], None, None, vec![]);
        let model = make_model(vec![("count", Value::Int(7))]);
        let tm = extract(&model, &fsm);
        assert_eq!(tm.next_value("count"), Some(&Value::Int(7)));
    }

    #[test]
    fn next_state_absent_var_is_skipped() {
        let fsm = make_fsm(vec![make_state_var("_count", "count")], None, None, vec![]);
        // model has no "count" binding
        let model = make_model(vec![("unrelated", Value::Int(0))]);
        let tm = extract(&model, &fsm);
        assert!(tm.next_state.is_empty(), "expected empty next_state, got {:?}", tm.next_state);
    }

    #[test]
    fn next_state_multiple_vars_partial_presence() {
        let fsm = make_fsm(
            vec![make_state_var("_a", "a"), make_state_var("_b", "b")],
            None,
            None,
            vec![],
        );
        let model = make_model(vec![("a", Value::Int(1))]);
        // "b" absent
        let tm = extract(&model, &fsm);
        assert_eq!(tm.next_state.len(), 1);
        assert_eq!(tm.next_value("a"), Some(&Value::Int(1)));
        assert_eq!(tm.next_value("b"), None);
    }

    // -------------------------------------------------------------------------
    // effects — native Seq
    // -------------------------------------------------------------------------

    #[test]
    fn effects_seq_two_enums_decoded() {
        let fsm = make_fsm(
            vec![],
            Some(EffectSpec { var: "effects".into() }),
            None,
            vec![],
        );
        let seq = Value::Seq(vec![
            Value::Enum { ctor: "Println".into(), args: vec![Value::Str("hi".into())] },
            Value::Enum { ctor: "Exit".into(), args: vec![Value::Int(0)] },
        ]);
        let model = make_model(vec![("effects", seq)]);
        let tm = extract(&model, &fsm);
        assert_eq!(tm.effects.len(), 2);
        assert_eq!(tm.effects[0].ctor, "Println");
        assert_eq!(tm.effects[0].args, vec![Value::Str("hi".into())]);
        assert_eq!(tm.effects[1].ctor, "Exit");
        assert_eq!(tm.effects[1].args, vec![Value::Int(0)]);
    }

    #[test]
    fn effects_seq_skips_non_enum_elements() {
        let fsm = make_fsm(
            vec![],
            Some(EffectSpec { var: "effects".into() }),
            None,
            vec![],
        );
        let seq = Value::Seq(vec![
            Value::Int(42), // not an Enum — should be skipped
            Value::Enum { ctor: "Tick".into(), args: vec![] },
        ]);
        let model = make_model(vec![("effects", seq)]);
        let tm = extract(&model, &fsm);
        assert_eq!(tm.effects.len(), 1);
        assert_eq!(tm.effects[0].ctor, "Tick");
    }

    // -------------------------------------------------------------------------
    // effects — cons-list datatype
    // -------------------------------------------------------------------------

    #[test]
    fn effects_cons_list_single_element() {
        // Cons(Println("hi"), Nil)
        let fsm = make_fsm(
            vec![],
            Some(EffectSpec { var: "effects".into() }),
            None,
            vec![],
        );
        let cons_list = Value::Enum {
            ctor: "cons".into(),
            args: vec![
                Value::Enum { ctor: "Println".into(), args: vec![Value::Str("hi".into())] },
                Value::Enum { ctor: "nil".into(), args: vec![] },
            ],
        };
        let model = make_model(vec![("effects", cons_list)]);
        let tm = extract(&model, &fsm);
        assert_eq!(tm.effects.len(), 1);
        assert_eq!(tm.effects[0].ctor, "Println");
        assert_eq!(tm.effects[0].args, vec![Value::Str("hi".into())]);
    }

    #[test]
    fn effects_cons_list_multiple_elements() {
        // Cons(Println("a"), Cons(Exit(1), Nil))
        let fsm = make_fsm(
            vec![],
            Some(EffectSpec { var: "effects".into() }),
            None,
            vec![],
        );
        let cons_list = Value::Enum {
            ctor: "Cons".into(), // capital C — case-insensitive match
            args: vec![
                Value::Enum { ctor: "Println".into(), args: vec![Value::Str("a".into())] },
                Value::Enum {
                    ctor: "Cons".into(),
                    args: vec![
                        Value::Enum { ctor: "Exit".into(), args: vec![Value::Int(1)] },
                        Value::Enum { ctor: "Nil".into(), args: vec![] }, // capital N
                    ],
                },
            ],
        };
        let model = make_model(vec![("effects", cons_list)]);
        let tm = extract(&model, &fsm);
        assert_eq!(tm.effects.len(), 2);
        assert_eq!(tm.effects[0].ctor, "Println");
        assert_eq!(tm.effects[1].ctor, "Exit");
    }

    #[test]
    fn effects_insert_cons_ctor_is_recognized() {
        // Some fixtures use "insert" instead of "cons"
        let fsm = make_fsm(
            vec![],
            Some(EffectSpec { var: "effects".into() }),
            None,
            vec![],
        );
        let cons_list = Value::Enum {
            ctor: "insert".into(),
            args: vec![
                Value::Enum { ctor: "Tick".into(), args: vec![] },
                Value::Enum { ctor: "empty".into(), args: vec![] },
            ],
        };
        let model = make_model(vec![("effects", cons_list)]);
        let tm = extract(&model, &fsm);
        assert_eq!(tm.effects.len(), 1);
        assert_eq!(tm.effects[0].ctor, "Tick");
    }

    // -------------------------------------------------------------------------
    // effects — absent / None cases
    // -------------------------------------------------------------------------

    #[test]
    fn effects_var_absent_from_model_is_empty() {
        let fsm = make_fsm(
            vec![],
            Some(EffectSpec { var: "effects".into() }),
            None,
            vec![],
        );
        // model has no "effects" binding
        let model = make_model(vec![]);
        let tm = extract(&model, &fsm);
        assert!(tm.effects.is_empty());
    }

    #[test]
    fn effects_none_spec_is_empty() {
        let fsm = make_fsm(vec![], None, None, vec![]);
        let model = make_model(vec![(
            "effects",
            Value::Seq(vec![Value::Enum { ctor: "Tick".into(), args: vec![] }]),
        )]);
        let tm = extract(&model, &fsm);
        assert!(tm.effects.is_empty(), "no effects spec → effects must be empty");
    }

    // -------------------------------------------------------------------------
    // halt_flag
    // -------------------------------------------------------------------------

    #[test]
    fn halt_flag_true_when_bool_var_is_true() {
        let fsm = make_fsm(
            vec![],
            None,
            Some(HaltSpec { var: Some("halt".into()) }),
            vec![],
        );
        let model = make_model(vec![("halt", Value::Bool(true))]);
        let tm = extract(&model, &fsm);
        assert!(tm.halt_flag);
    }

    #[test]
    fn halt_flag_false_when_bool_var_is_false() {
        let fsm = make_fsm(
            vec![],
            None,
            Some(HaltSpec { var: Some("halt".into()) }),
            vec![],
        );
        let model = make_model(vec![("halt", Value::Bool(false))]);
        let tm = extract(&model, &fsm);
        assert!(!tm.halt_flag);
    }

    #[test]
    fn halt_flag_false_when_var_absent() {
        let fsm = make_fsm(
            vec![],
            None,
            Some(HaltSpec { var: Some("halt".into()) }),
            vec![],
        );
        let model = make_model(vec![]);
        let tm = extract(&model, &fsm);
        assert!(!tm.halt_flag);
    }

    #[test]
    fn halt_flag_false_when_no_halt_spec() {
        let fsm = make_fsm(vec![], None, None, vec![]);
        let model = make_model(vec![("halt", Value::Bool(true))]);
        let tm = extract(&model, &fsm);
        assert!(!tm.halt_flag, "no HaltSpec → halt_flag must be false");
    }

    #[test]
    fn halt_flag_false_when_halt_spec_has_no_var() {
        let fsm = make_fsm(vec![], None, Some(HaltSpec { var: None }), vec![]);
        let model = make_model(vec![("halt", Value::Bool(true))]);
        let tm = extract(&model, &fsm);
        assert!(!tm.halt_flag, "HaltSpec with var=None → halt_flag must be false");
    }

    #[test]
    fn halt_flag_false_when_var_is_non_bool() {
        let fsm = make_fsm(
            vec![],
            None,
            Some(HaltSpec { var: Some("halt".into()) }),
            vec![],
        );
        // Provide an Int where a Bool was expected.
        let model = make_model(vec![("halt", Value::Int(1))]);
        let tm = extract(&model, &fsm);
        assert!(!tm.halt_flag, "non-Bool halt var → halt_flag must be false");
    }

    // -------------------------------------------------------------------------
    // world_writes
    // -------------------------------------------------------------------------

    #[test]
    fn world_writes_pulls_named_vars() {
        let fsm = make_fsm(
            vec![],
            None,
            None,
            vec!["world_x".into(), "world_y".into()],
        );
        let model = make_model(vec![
            ("world_x", Value::Int(10)),
            ("world_y", Value::Int(20)),
        ]);
        let tm = extract(&model, &fsm);
        assert_eq!(tm.world_value("world_x"), Some(&Value::Int(10)));
        assert_eq!(tm.world_value("world_y"), Some(&Value::Int(20)));
    }

    #[test]
    fn world_writes_absent_vars_skipped() {
        let fsm = make_fsm(vec![], None, None, vec!["world_x".into()]);
        let model = make_model(vec![]); // nothing in model
        let tm = extract(&model, &fsm);
        assert!(tm.world_writes.is_empty());
    }

    #[test]
    fn world_writes_empty_spec_gives_empty_result() {
        let fsm = make_fsm(vec![], None, None, vec![]);
        let model = make_model(vec![("world_x", Value::Int(99))]);
        let tm = extract(&model, &fsm);
        assert!(tm.world_writes.is_empty());
    }
}
