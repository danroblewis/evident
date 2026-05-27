//! N3 shared-world plumbing — manages world state across multi-FSM ticks.
//!
//! Three helpers handle the shared-world lifecycle:
//!   * [`init_world`]    — seed the world from declared inits before tick 0.
//!   * [`build_given`]   — build per-FSM inputs from the current+prev world.
//!   * [`record_writes`] — fold a solved tick's world writes into the current world.

use std::collections::BTreeMap;

use crate::spec::{FsmSpec, TickModel, WorldVar};
use crate::z3c::Value;

/// Build the initial shared world from each [`WorldVar`]'s declared `init`,
/// skipping those with no init. Keyed by world var name.
pub fn init_world(world: &[WorldVar]) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    for wv in world {
        if let Some(lit) = &wv.init {
            map.insert(wv.name.clone(), lit.to_value(&wv.sort));
        }
    }
    map
}

/// Build the given-input map for `fsm`: for each world var name in
/// `fsm.world_reads`, take the value from `world_current` if present (a writer
/// already wrote it this tick), else from `world_prev`, else omit it. Keyed by
/// the world var name.
pub fn build_given(
    fsm: &FsmSpec,
    world_current: &BTreeMap<String, Value>,
    world_prev: &BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    let mut map = BTreeMap::new();
    for name in &fsm.world_reads {
        if let Some(val) = world_current.get(name) {
            map.insert(name.clone(), val.clone());
        } else if let Some(val) = world_prev.get(name) {
            map.insert(name.clone(), val.clone());
        }
        // if neither has it, omit
    }
    map
}

/// Fold an FSM's world writes (from its solved tick) into `world_current`.
/// Overwrites any previous value for the same name.
pub fn record_writes(model: &TickModel, world_current: &mut BTreeMap<String, Value>) {
    for (name, value) in &model.world_writes {
        world_current.insert(name.clone(), value.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{Sort, Lit};

    /// Build a minimal [`FsmSpec`] with the given `world_reads`. All other fields
    /// are empty / None so tests only need to specify what they care about.
    fn make_fsm(world_reads: Vec<&str>) -> FsmSpec {
        FsmSpec {
            name: "test_fsm".into(),
            transition: String::new(),
            state: Vec::new(),
            given: Vec::new(),
            effects: None,
            halt: None,
            world_writes: Vec::new(),
            world_reads: world_reads.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Build a minimal [`TickModel`] with the given `world_writes`.
    fn make_tick_model(world_writes: Vec<(&str, Value)>) -> TickModel {
        TickModel {
            next_state: Vec::new(),
            world_writes: world_writes.into_iter().map(|(n, v)| (n.to_string(), v)).collect(),
            effects: Vec::new(),
            halt_flag: false,
        }
    }

    // --- init_world tests ---

    #[test]
    fn init_world_includes_vars_with_init_omits_without() {
        let world = vec![
            WorldVar { name: "n".into(), sort: Sort::Int, init: Some(Lit::Int(0)) },
            WorldVar { name: "m".into(), sort: Sort::Int, init: None },
        ];
        let result = init_world(&world);
        assert_eq!(result.len(), 1, "only 'n' should be present");
        assert_eq!(result.get("n"), Some(&Value::Int(0)));
        assert!(result.get("m").is_none(), "'m' has no init, must be absent");
    }

    #[test]
    fn init_world_empty_slice() {
        let result = init_world(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn init_world_all_have_inits() {
        let world = vec![
            WorldVar { name: "a".into(), sort: Sort::Bool, init: Some(Lit::Bool(true)) },
            WorldVar { name: "b".into(), sort: Sort::Int, init: Some(Lit::Int(42)) },
        ];
        let result = init_world(&world);
        assert_eq!(result.get("a"), Some(&Value::Bool(true)));
        assert_eq!(result.get("b"), Some(&Value::Int(42)));
    }

    // --- build_given tests ---

    #[test]
    fn build_given_prefers_current_over_prev() {
        let fsm = make_fsm(vec!["n"]);
        let mut current = BTreeMap::new();
        current.insert("n".to_string(), Value::Int(3));
        let mut prev = BTreeMap::new();
        prev.insert("n".to_string(), Value::Int(0));

        let given = build_given(&fsm, &current, &prev);
        assert_eq!(given.get("n"), Some(&Value::Int(3)), "current (3) should win over prev (0)");
    }

    #[test]
    fn build_given_falls_back_to_prev_when_current_missing() {
        let fsm = make_fsm(vec!["n"]);
        let current: BTreeMap<String, Value> = BTreeMap::new();
        let mut prev = BTreeMap::new();
        prev.insert("n".to_string(), Value::Int(0));

        let given = build_given(&fsm, &current, &prev);
        assert_eq!(given.get("n"), Some(&Value::Int(0)), "should fall back to prev (0)");
    }

    #[test]
    fn build_given_omits_when_neither_has_value() {
        let fsm = make_fsm(vec!["n"]);
        let current: BTreeMap<String, Value> = BTreeMap::new();
        let prev: BTreeMap<String, Value> = BTreeMap::new();

        let given = build_given(&fsm, &current, &prev);
        assert!(given.is_empty(), "should be empty when neither world has 'n'");
    }

    #[test]
    fn build_given_ignores_vars_not_in_world_reads() {
        let fsm = make_fsm(vec!["n"]); // only reads "n", not "x"
        let mut current = BTreeMap::new();
        current.insert("n".to_string(), Value::Int(1));
        current.insert("x".to_string(), Value::Int(99)); // not in world_reads
        let prev: BTreeMap<String, Value> = BTreeMap::new();

        let given = build_given(&fsm, &current, &prev);
        assert_eq!(given.len(), 1);
        assert_eq!(given.get("n"), Some(&Value::Int(1)));
        assert!(given.get("x").is_none(), "'x' not in world_reads, must be absent");
    }

    #[test]
    fn build_given_empty_world_reads() {
        let fsm = make_fsm(vec![]);
        let mut current = BTreeMap::new();
        current.insert("n".to_string(), Value::Int(7));
        let prev: BTreeMap<String, Value> = BTreeMap::new();

        let given = build_given(&fsm, &current, &prev);
        assert!(given.is_empty());
    }

    // --- record_writes tests ---

    #[test]
    fn record_writes_inserts_into_empty_world() {
        let model = make_tick_model(vec![("n", Value::Int(5))]);
        let mut world_current: BTreeMap<String, Value> = BTreeMap::new();
        record_writes(&model, &mut world_current);
        assert_eq!(world_current.get("n"), Some(&Value::Int(5)));
    }

    #[test]
    fn record_writes_overwrites_existing_value() {
        let model = make_tick_model(vec![("n", Value::Int(5))]);
        let mut world_current = BTreeMap::new();
        world_current.insert("n".to_string(), Value::Int(2));
        record_writes(&model, &mut world_current);
        assert_eq!(world_current.get("n"), Some(&Value::Int(5)), "should overwrite 2 with 5");
    }

    #[test]
    fn record_writes_no_writes_leaves_world_unchanged() {
        let model = make_tick_model(vec![]);
        let mut world_current = BTreeMap::new();
        world_current.insert("n".to_string(), Value::Int(7));
        record_writes(&model, &mut world_current);
        assert_eq!(world_current.get("n"), Some(&Value::Int(7)));
    }

    #[test]
    fn record_writes_multiple_vars() {
        let model = make_tick_model(vec![
            ("a", Value::Int(1)),
            ("b", Value::Bool(true)),
        ]);
        let mut world_current: BTreeMap<String, Value> = BTreeMap::new();
        record_writes(&model, &mut world_current);
        assert_eq!(world_current.get("a"), Some(&Value::Int(1)));
        assert_eq!(world_current.get("b"), Some(&Value::Bool(true)));
    }
}
