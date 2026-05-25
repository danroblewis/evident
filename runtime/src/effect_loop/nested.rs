//! Tier 3 — blocking-interpret: `run(F, init)` runs a nested FSM to
//! halt and hands its final state back as a `Value`.
//!
//! This is the correctness baseline (and, later, the equivalence
//! oracle — see `docs/design/nested-fsm-strategies.md` §4) of the
//! nested-FSM execution model. It compiles nothing: it drives `F`
//! using the *same per-tick solve* the multi-FSM scheduler uses
//! (`EvidentRuntime::query_with_pins_and_given`), with the scheduler's
//! `LoopOpts.max_steps` cap as its max-iteration guard.
//!
//! ### FSM shape (same as `halts_within`)
//!
//! `F` must declare a single `name, name_next ∈ T` state pair and a
//! `halt ∈ Bool` — the convention CC's `halts_within` reads (see
//! `runtime/src/fsm_unroll/compose.rs`). `halt` is evaluated on each
//! tick's *input* state; the run returns the state at the first tick
//! whose `halt` is true (so `run(decrement, 50)` with
//! `halt = count ≤ 0` returns `0`, the input count at the halting
//! tick).
//!
//! ### Why a dedicated loop instead of `run_scheduler`
//!
//! `run_scheduler` is built for the *enum-state, world-coordinating,
//! effect-emitting* multi-FSM model: it halts implicitly (no FSM
//! scheduled in a tick) and reports a best-effort final state — for a
//! `Done`-variant FSM that final state is the `Done` *variant*, losing
//! the carried value. A value-returning `run(F, init)` needs the exact
//! opposite: a primitive/record/enum state, an explicit `halt` signal,
//! and the *full* final state value (`count = 0`, not "halted"). So
//! tier 3 reuses the scheduler's *primitives* — the per-tick solve, the
//! state encode (`state::encode_state_value`), and the `max_steps` cap
//! — rather than `run_scheduler` wholesale. The execution is
//! synchronous-blocking and isolated: the nested run shares no world
//! with the parent (it is a pure function of `init`, §2/§5).
//!
//! ### Effects: captured, not dispatched (session RR)
//!
//! An `F` *may* declare and solve `effects`. During the nested run those
//! effects are **captured, not dispatched** — `run_nested_capturing`
//! accumulates each advancing tick's effects (in child-tick order) and
//! **returns them to the parent** alongside the final state. The parent
//! (the FSM whose body called `run(F, init)`) dispatches them once, in
//! its own tick — see `runtime/nested.rs`'s percolation thread-local and
//! the scheduler's drain point. This keeps `run(F, init)` a **pure
//! function of `init`**: same `init` → same `(state, effects)`, with no
//! side effects during the child run (§5). It replaces LL's v1
//! reject-effectful-child restriction.
//!
//! ### Remaining v1 restrictions
//!
//!   * **No external natives.** An `external fsm` (a Rust-side bridge /
//!     event source) has no solvable per-tick body to drive as a value;
//!     it is rejected (here and at load).
//!   * **Single state pair.** Multi-pair FSMs are a clean extension but
//!     out of scope for v1.

use std::collections::HashMap;

use crate::core::ast::{BodyItem, Effect, Keyword, SchemaDecl};
use crate::core::Value;
use crate::runtime::EvidentRuntime;

use super::collect::collect_dispatchable_effects;
use super::state::encode_state_value;

/// Why a `run(F, init)` couldn't be evaluated. Surfaced to the caller
/// (`EvidentRuntime::resolve_runs`) which turns it into a load-time or
/// query-time error string. Every variant is a *loud failure*, never a
/// silent wrong value.
#[derive(Debug, Clone)]
pub enum RunError {
    /// `run(F, ..)` named a schema that doesn't exist.
    UnknownFsm(String),
    /// `F` exists but isn't declared with the `fsm` keyword. The keyword
    /// is the sole signal that a schema is an FSM — a `claim`/`type`/
    /// `schema` target is rejected (no shape-detection fallback). Carries
    /// the keyword `F` *was* declared with, for the diagnostic.
    NotFsm { fsm: String, keyword: String },
    /// `F` has no `name, name_next ∈ T` state pair.
    NoStatePair(String),
    /// `F` declares more than one state pair (v1 supports exactly one).
    MultipleStatePairs(String, usize),
    /// `F` has no `halt ∈ Bool` declaration.
    NoHaltVar(String),
    /// `F` is an `external fsm` — its body is implemented in Rust (a
    /// bridge / event source), so there's nothing to drive as a value.
    /// (Effect-*emitting* bodies are now permitted — their effects are
    /// captured and percolated to the parent, session RR.)
    ExternalNative(String),
    /// `init` couldn't be coerced to `F`'s state type.
    BadInit { fsm: String, type_name: String, got: String },
    /// A per-tick solve came back UNSAT — the FSM body has no model for
    /// the pinned input state (a translator gap or an over-constrained
    /// body).
    Unsat { fsm: String, step: usize },
    /// A per-tick solve's model didn't bind the expected output/`halt`.
    MissingBinding { fsm: String, name: String, step: usize },
    /// `halt` never fired within `max_steps` ticks — a non-terminating
    /// (or too-slow) FSM. The scheduler-level analogue of the
    /// loop-functionizer's native `max_iters` overflow.
    MaxItersExceeded { fsm: String, max_steps: usize },
    /// Catch-all for runtime invariant violations (unexpected binding
    /// shape, etc.). Shouldn't fire on well-formed bodies.
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

/// `(input_name, output_name, type_name)` for a detected state pair.
struct StatePair {
    input:     String,
    output:    String,
    type_name: String,
}

/// Detect `name, name_next ∈ T` pairs in a schema body — the same
/// shape `fsm_unroll`'s composer uses, reimplemented here (that module
/// is off-limits, and the logic is small). Both halves must share the
/// type name.
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

/// The surface word for a `Keyword`, for diagnostics ("not `claim`").
fn keyword_word(kw: &Keyword) -> &'static str {
    match kw {
        Keyword::Schema   => "schema",
        Keyword::Claim    => "claim",
        Keyword::Type     => "type",
        Keyword::Subclaim => "subclaim",
        Keyword::Fsm      => "fsm",
    }
}

/// Does the body declare a `halt ∈ Bool`?
fn has_halt_bool(schema: &SchemaDecl) -> bool {
    schema.body.iter().any(|item| matches!(item,
        BodyItem::Membership { name, type_name, .. }
            if name == "halt" && type_name == "Bool"))
}

/// The name of the body's effect channel (`effects ∈ Seq(Effect)`), if
/// any. Used as the `primary_var` for per-tick effect capture — only the
/// elements of THIS Seq dispatch (the legacy ordered shape), mirroring
/// the scheduler's `effects` slot. `None` for an effect-free body, in
/// which case the run captures nothing.
fn effects_var_name(schema: &SchemaDecl) -> Option<String> {
    schema.body.iter().find_map(|item| match item {
        BodyItem::Membership { name, type_name, .. }
            if name == "effects" || type_name == "Seq(Effect)" => Some(name.clone()),
        _ => None,
    })
}

/// Validate that `fsm_name` names an FSM the `run` machinery can drive:
/// a single state pair + `halt ∈ Bool`, effect-free. Used both at load
/// time (so a non-FSM `F` is rejected up front) and as the front of
/// `run_nested`. Returns the state pair on success.
pub fn validate_run_target(rt: &EvidentRuntime, fsm_name: &str) -> Result<(), RunError> {
    let schema = rt.get_schema(fsm_name)
        .ok_or_else(|| RunError::UnknownFsm(fsm_name.to_string()))?;
    check_shape(schema, fsm_name).map(|_| ())
}

/// Shared shape check used by both `validate_run_target` and
/// `run_nested`. Returns the single detected state pair on success.
fn check_shape(schema: &SchemaDecl, fsm_name: &str) -> Result<StatePair, RunError> {
    // The `fsm` keyword is the sole signal that a schema is an FSM. A
    // `run(...)` / `halts_within(...)` target declared `claim`/`type`/
    // `schema` is rejected here — intent declared beats intent inferred,
    // and the old shape-based resolution is gone. This fires at load time
    // (via `validate_run_target`) and defensively at run time (via
    // `run_nested_capturing`).
    if !matches!(schema.keyword, Keyword::Fsm) {
        return Err(RunError::NotFsm {
            fsm: fsm_name.to_string(),
            keyword: keyword_word(&schema.keyword).to_string(),
        });
    }
    // An `external fsm` has no solvable per-tick body — reject. (A body
    // that *declares* `effects` is now fine: its effects are captured and
    // percolated to the parent, not dispatched during the run — see
    // run_nested_capturing and the module doc.)
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

/// Is `type_name` a primitive scalar (state held directly, no Datatype
/// pin)?
fn is_primitive(type_name: &str) -> bool {
    matches!(type_name, "Int" | "Bool" | "Real" | "String")
}

/// Coerce the `init` Value to `F`'s state type.
///
/// Three cases:
///   1. **Primitive state** (`Int`/`Bool`/`Real`/`String`) — the init
///      value's kind must match.
///   2. **Enum state, init already of that enum** — seeds directly
///      (`run(accumulate, Acc(0))`, `run(walk, WSeed(...))`).
///   3. **Enum state, init is the *payload*** — seed the state enum's
///      first single-payload variant when the init value's type matches
///      that payload's field type. This generalizes the bare-Int →
///      first-Int-variant convention (`seed_state_with_arg`) to ANY
///      composite: a tree / recursive-enum / Seq passed straight through
///      `init` (`run(walk, Node(Leaf(1), Leaf(2)))` seeds
///      `WSeed(Node(Leaf(1), Leaf(2)))`). This is the composite-seed
///      half of #19d.
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
    // Enum state: an init value already of this enum type seeds directly.
    if let Value::Enum { enum_name, .. } = init {
        if enum_name == type_name {
            return Ok(init.clone());
        }
    }
    // Otherwise, wrap the init value into the state enum's first
    // single-payload variant when the kinds match.
    if let Some(seeded) = seed_first_variant(rt, type_name, init) {
        return Ok(seeded);
    }
    Err(bad())
}

/// Seed the state enum's first variant when it takes a single payload
/// whose declared type matches `init`'s kind. Returns the wrapped
/// `Value::Enum`, or `None` if the first variant isn't single-payload or
/// the types don't line up.
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

/// Does `v`'s runtime kind match a payload field's declared type name?
/// Used to decide whether a composite init can seed a state enum's first
/// variant. For `Seq(...)` fields the element type is checked against the
/// first element's enum name (empty seqs match any `Seq(enum)`).
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
            (Some(_), None) => true,   // empty seq → any Seq(enum) accepted
            _ => false,
        },
        _ => false,
    }
}

/// Run `F` from `init` to halt, returning its final state `Value`.
///
/// Thin wrapper over [`run_nested_capturing`] that discards the captured
/// effects — the value-only contract the oracle / equivalence harness
/// uses (`runtime/tests/run_fsm.rs`, `tier1_jit.rs`,
/// `composite_tree_walk.rs`).
pub fn run_nested(
    rt: &EvidentRuntime,
    fsm_name: &str,
    init: Value,
    max_steps: usize,
) -> Result<Value, RunError> {
    run_nested_capturing(rt, fsm_name, init, max_steps).map(|(state, _effects)| state)
}

/// Run `F` from `init` to halt, returning `(final_state, captured_effects)`.
///
/// The effects an effect-emitting `F` solves for are **captured, not
/// dispatched** — accumulated across each *advancing* (non-halting) tick
/// in child-tick order and handed back for the parent to dispatch (no
/// side effects during the run). This is what keeps `run(F, init)` a pure
/// function of `init` (§5): the run produces only data. An effect-free
/// `F` returns an empty effect vec.
///
/// The halting tick's body still solves (the per-tick query is whole),
/// but its effects are NOT captured — the run returns the *input* state
/// at the first halting tick (the state before any halting-tick work), so
/// the captured effects are exactly those emitted while advancing toward
/// halt. This mirrors the state semantics: `count` returned is the input
/// at the halting tick, not the post-decrement value.
///
/// `max_steps` is the max-iteration guard: a `halt` that never fires
/// fails loudly at the cap rather than hanging. Pass
/// `LoopOpts::default().max_steps` (10 000) unless the caller has a
/// reason to bound it tighter.
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
    let primitive = is_primitive(&type_name);
    // The body's effect channel, if any. `None` → effect-free F → the
    // run captures nothing.
    let effects_var = effects_var_name(schema);

    let trace = std::env::var("EVIDENT_NESTED_TRACE").is_ok();

    let mut current = coerce_init(rt, fsm_name, &type_name, &init)?;
    let mut captured: Vec<Effect> = Vec::new();
    if trace {
        eprintln!("[run {fsm_name}] seed {input}={current:?} (type {type_name})");
    }

    for step in 0..max_steps {
        // Build the per-tick solve inputs. Primitive state pins via the
        // `given` map only; enum state additionally pins the Datatype
        // (the functionizer reads `given`, the Z3 slow path reads
        // `pins`).
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert(input.clone(), current.clone());
        let pins: Vec<(&str, z3::ast::Datatype<'static>)> = if primitive {
            Vec::new()
        } else {
            match encode_state_value(rt, &current) {
                Some(dt) => vec![(input.as_str(), dt)],
                None => return Err(RunError::Internal(format!(
                    "run({fsm_name}, ..): couldn't encode state value {current:?} \
                     for the next-tick pin"))),
            }
        };

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

        // Advancing (non-halting) tick: capture this tick's effects in
        // child-tick order. Captured, NOT dispatched — they percolate to
        // the parent (session RR). Effect-free F has no `effects` var, so
        // this is skipped. `Some(ev)` selects the legacy ordered-Seq
        // shape (only `effects`'s elements, in their literal order).
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
