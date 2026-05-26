//! Tier-3 blocking interpreter: `run(F, init)` drives a nested FSM to halt, returning the final
//! state. `F` needs one state pair + `halt ∈ Bool`. Effects are captured, not dispatched.

use std::collections::HashMap;

use crate::core::ast::{BodyItem, Effect, Keyword, SchemaDecl};
use crate::core::Value;
use crate::runtime::EvidentRuntime;

use super::collect::collect_dispatchable_effects;

/// Why a `run(F, init)` couldn't be evaluated; every variant is a loud failure.
#[derive(Debug, Clone)]
pub enum RunError {
    /// `run(F, ..)` named a schema that doesn't exist.
    UnknownFsm(String),
    /// `F` isn't declared `fsm`; the keyword is the sole FSM signal (no shape-detection fallback).
    NotFsm { fsm: String, keyword: String },
    /// `F` has no `name, name_next ∈ T` state pair.
    NoStatePair(String),
    /// `F` declares more than one state pair (v1: exactly one required).
    MultipleStatePairs(String, usize),
    /// `F` has no `halt ∈ Bool` declaration.
    NoHaltVar(String),
    /// `F` is `external fsm` — Rust-side body, nothing to drive as a value.
    ExternalNative(String),
    /// `init` couldn't be coerced to `F`'s state type.
    BadInit { fsm: String, type_name: String, got: String },
    /// Per-tick solve came back UNSAT (translator gap or over-constrained body).
    Unsat { fsm: String, step: usize },
    /// Per-tick solve model didn't bind the expected output or `halt`.
    MissingBinding { fsm: String, name: String, step: usize },
    /// `halt` never fired within `max_steps` ticks.
    MaxItersExceeded { fsm: String, max_steps: usize },
    /// Catch-all for runtime invariant violations.
    Internal(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::UnknownFsm(n) =>
                write!(f, "run: first argument `{n}` doesn't name a known schema"),
            RunError::NotFsm { fsm, keyword } =>
                write!(f, "run's target `{fsm}` must be declared `fsm`, not `{keyword}` \
                          — the `fsm` keyword is the sole signal that a schema is an \
                          FSM (no shape-detection). Relabel `{keyword} {fsm}` to \
                          `fsm {fsm}`."),
            RunError::NoStatePair(n) =>
                write!(f, "run({n}, ..): `run`'s first argument must name an \
                          FSM-shaped schema (a `name, name_next ∈ T` state pair \
                          + `halt ∈ Bool`); `{n}` has no state pair"),
            RunError::MultipleStatePairs(n, k) =>
                write!(f, "run({n}, ..): FSM has {k} state pairs; v1 supports \
                          exactly one"),
            RunError::NoHaltVar(n) =>
                write!(f, "run({n}, ..): `run`'s first argument must name an \
                          FSM-shaped schema (state pair + `halt ∈ Bool`); `{n}` \
                          declares no `halt ∈ Bool`"),
            RunError::ExternalNative(n) =>
                write!(f, "run({n}, ..): `run`'s target can't be an `external fsm` \
                          (`{n}`) — its body is implemented in Rust, so there's no \
                          per-tick body to drive as a value. Run it as a top-level \
                          or spawned FSM instead"),
            RunError::BadInit { fsm, type_name, got } =>
                write!(f, "run({fsm}, ..): can't seed state of type `{type_name}` \
                          from init value {got}"),
            RunError::Unsat { fsm, step } =>
                write!(f, "run({fsm}, ..): FSM body returned UNSAT at step {step} \
                          (no model for the pinned input state)"),
            RunError::MissingBinding { fsm, name, step } =>
                write!(f, "run({fsm}, ..): step {step} model has no `{name}` binding"),
            RunError::MaxItersExceeded { fsm, max_steps } =>
                write!(f, "run({fsm}, ..): exceeded the {max_steps}-step max-iteration \
                          guard without `halt` ever firing — non-terminating (or \
                          too-slow) FSM. Raise the guard via LoopOpts.max_steps if \
                          this is a legitimately long run."),
            RunError::Internal(s) =>
                write!(f, "run: internal: {s}"),
        }
    }
}

/// Detected state pair `(input_name, output_name, type_name)`.
struct StatePair {
    input:     String,
    output:    String,
    type_name: String,
}

/// Detect `name, name_next ∈ T` pairs in a schema body. Both halves must share the type name.
fn detect_state_pairs(schema: &SchemaDecl) -> Vec<StatePair> {
    let mut decls: HashMap<String, String> = HashMap::new();
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            decls.insert(name.clone(), type_name.clone());
        }
    }
    let mut pairs = Vec::new();
    for (name, type_name) in &decls {
        if name.ends_with("_next") { continue; }
        let next_name = format!("{name}_next");
        if decls.get(&next_name) == Some(type_name) {
            pairs.push(StatePair {
                input:     name.clone(),
                output:    next_name,
                type_name: type_name.clone(),
            });
        }
    }
    pairs.sort_by(|a, b| a.input.cmp(&b.input));
    pairs
}

/// Surface word for a `Keyword`, for diagnostics.
fn keyword_word(kw: &Keyword) -> &'static str {
    match kw {
        Keyword::Schema   => "schema",
        Keyword::Claim    => "claim",
        Keyword::Type     => "type",
        Keyword::Subclaim => "subclaim",
        Keyword::Fsm      => "fsm",
    }
}

fn has_halt_bool(schema: &SchemaDecl) -> bool {
    schema.body.iter().any(|item| matches!(item,
        BodyItem::Membership { name, type_name, .. }
            if name == "halt" && type_name == "Bool"))
}

/// Returns the name of the `effects ∈ Seq(Effect)` channel if present; None = effect-free.
fn effects_var_name(schema: &SchemaDecl) -> Option<String> {
    schema.body.iter().find_map(|item| match item {
        BodyItem::Membership { name, type_name, .. }
            if name == "effects" || type_name == "Seq(Effect)" => Some(name.clone()),
        _ => None,
    })
}

/// Validate at load time that `fsm_name` is a drivable FSM (single state pair + `halt ∈ Bool`).
pub fn validate_run_target(rt: &EvidentRuntime, fsm_name: &str) -> Result<(), RunError> {
    let schema = rt.get_schema(fsm_name)
        .ok_or_else(|| RunError::UnknownFsm(fsm_name.to_string()))?;
    check_shape(schema, fsm_name).map(|_| ())
}

/// Shared shape check for both `validate_run_target` and `run_nested`.
fn check_shape(schema: &SchemaDecl, fsm_name: &str) -> Result<StatePair, RunError> {
    // `fsm` keyword is the sole FSM signal; `claim`/`type`/`schema` targets rejected.
    if !matches!(schema.keyword, Keyword::Fsm) {
        return Err(RunError::NotFsm {
            fsm: fsm_name.to_string(),
            keyword: keyword_word(&schema.keyword).to_string(),
        });
    }
    // `external fsm` has no solvable body; reject. Effect-declaring bodies are fine —
    // their effects are captured/percolated, not dispatched during the run.
    if schema.external {
        return Err(RunError::ExternalNative(fsm_name.to_string()));
    }
    let mut pairs = detect_state_pairs(schema);
    match pairs.len() {
        0 => return Err(RunError::NoStatePair(fsm_name.to_string())),
        1 => {}
        k => return Err(RunError::MultipleStatePairs(fsm_name.to_string(), k)),
    }
    if !has_halt_bool(schema) {
        return Err(RunError::NoHaltVar(fsm_name.to_string()));
    }
    Ok(pairs.remove(0))
}

fn is_primitive(type_name: &str) -> bool {
    matches!(type_name, "Int" | "Bool" | "Real" | "String")
}

/// Coerce `init` to `F`'s state type. Accepts: matching primitive, same-enum value,
/// or payload wrapped in the state enum's first single-payload variant.
fn coerce_init(
    rt: &EvidentRuntime,
    fsm_name: &str,
    type_name: &str,
    init: &Value,
) -> Result<Value, RunError> {
    let bad = || RunError::BadInit {
        fsm: fsm_name.to_string(),
        type_name: type_name.to_string(),
        got: format!("{init:?}"),
    };
    if is_primitive(type_name) {
        let ok = matches!(
            (type_name, init),
            ("Int", Value::Int(_)) | ("Bool", Value::Bool(_))
                | ("Real", Value::Real(_)) | ("String", Value::Str(_))
        );
        return if ok { Ok(init.clone()) } else { Err(bad()) };
    }
    if let Value::Enum { enum_name, .. } = init {
        if enum_name == type_name { return Ok(init.clone()); }
    }
    if let Some(seeded) = seed_first_variant(rt, type_name, init) {
        return Ok(seeded);
    }
    Err(bad())
}

/// Wrap `init` in the state enum's first single-payload variant if kinds match; else None.
fn seed_first_variant(
    rt: &EvidentRuntime,
    type_name: &str,
    init: &Value,
) -> Option<Value> {
    let enums = rt.enums_registry();
    let by_name = enums.by_name.borrow();
    let (_sort, decl_variants) = by_name.get(type_name)?;
    let first = decl_variants.first()?;
    if first.fields.len() != 1 { return None; }
    if !value_matches_field_type(init, &first.fields[0].type_name) { return None; }
    Some(Value::Enum {
        enum_name: type_name.to_string(),
        variant:   first.name.clone(),
        fields:    vec![init.clone()],
    })
}

/// Does `v`'s runtime kind match a payload field's declared type?
/// For `Seq(...)`, checks element type (empty seqs match any `Seq(enum)`).
fn value_matches_field_type(v: &Value, field_type: &str) -> bool {
    use crate::core::parse_seq_type;
    match v {
        Value::Int(_)  => matches!(field_type, "Int" | "Nat" | "Pos"),
        Value::Bool(_) => field_type == "Bool",
        Value::Real(_) => field_type == "Real",
        Value::Str(_)  => field_type == "String",
        Value::Enum { enum_name, .. } => field_type == enum_name,
        Value::SeqInt(_)  => matches!(parse_seq_type(field_type), Some("Int" | "Nat" | "Pos")),
        Value::SeqBool(_) => parse_seq_type(field_type) == Some("Bool"),
        Value::SeqStr(_)  => parse_seq_type(field_type) == Some("String"),
        Value::SeqEnum(elems) => match (parse_seq_type(field_type), elems.first()) {
            (Some(inner), Some(Value::Enum { enum_name, .. })) => enum_name == inner,
            (Some(_), None) => true, // empty seq → any Seq(enum) accepted
            _ => false,
        },
        _ => false,
    }
}

/// Run `F` from `init` to halt, returning only the final state (discards effects).
pub fn run_nested(
    rt: &EvidentRuntime,
    fsm_name: &str,
    init: Value,
    max_steps: usize,
) -> Result<Value, RunError> {
    run_nested_capturing(rt, fsm_name, init, max_steps).map(|(state, _effects)| state)
}

/// Run `F` from `init` to halt; returns `(final_state, captured_effects)`.
/// Halting tick's effects excluded; `max_steps` guards against non-termination.
pub fn run_nested_capturing(
    rt: &EvidentRuntime,
    fsm_name: &str,
    init: Value,
    max_steps: usize,
) -> Result<(Value, Vec<Effect>), RunError> {
    let schema = rt.get_schema(fsm_name)
        .ok_or_else(|| RunError::UnknownFsm(fsm_name.to_string()))?;
    let pair = check_shape(schema, fsm_name)?;
    let StatePair { input, output, type_name } = pair;
    let effects_var = effects_var_name(schema);

    let trace = std::env::var("EVIDENT_NESTED_TRACE").is_ok();

    let mut current = coerce_init(rt, fsm_name, &type_name, &init)?;
    let mut captured: Vec<Effect> = Vec::new();
    if trace {
        eprintln!("[run {fsm_name}] seed {input}={current:?} (type {type_name})");
    }

    for step in 0..max_steps {
        // No explicit Datatype pins: every Z3 path re-encodes Value::Enum given directly.
        // Building a Datatype pin here was ~37% of per-tick cost and leaked AST per tick.
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert(input.clone(), current.clone());
        let pins: [(&str, z3::ast::Datatype<'static>); 0] = [];

        let r = rt.query_with_pins_and_given(fsm_name, &pins, &given)
            .map_err(|e| RunError::Internal(format!(
                "run({fsm_name}, ..) step {step}: {e}")))?;
        if !r.satisfied {
            return Err(RunError::Unsat { fsm: fsm_name.to_string(), step });
        }

        let halt = match r.bindings.get("halt") {
            Some(Value::Bool(b)) => *b,
            Some(other) => return Err(RunError::Internal(format!(
                "run({fsm_name}, ..) step {step}: `halt` bound to non-Bool {other:?}"))),
            None => return Err(RunError::MissingBinding {
                fsm: fsm_name.to_string(), name: "halt".to_string(), step }),
        };
        if trace {
            eprintln!("[run {fsm_name}]  step {step}: {input}={current:?} halt={halt}");
        }
        if halt {
            return Ok((current, captured));
        }

        // Advancing tick: capture effects (not dispatched) to percolate to parent.
        if let Some(ev) = &effects_var {
            let tick_effects =
                collect_dispatchable_effects(rt, fsm_name, &r.bindings, Some(ev));
            if trace && !tick_effects.is_empty() {
                eprintln!("[run {fsm_name}]  step {step}: captured {} effect(s)",
                    tick_effects.len());
            }
            captured.extend(tick_effects);
        }

        let next = r.bindings.get(&output).cloned().ok_or_else(|| {
            RunError::MissingBinding {
                fsm: fsm_name.to_string(), name: output.clone(), step }
        })?;
        current = next;
    }

    Err(RunError::MaxItersExceeded {
        fsm: fsm_name.to_string(),
        max_steps,
    })
}
