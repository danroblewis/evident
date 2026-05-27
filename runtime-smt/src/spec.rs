//! The shared vocabulary of the engine: the metadata model and the typed tick
//! result. This module is pure data + parsing — no Z3, no SMT-LIB emission, no
//! IO. It is the frozen interface contract every other module builds against.
//!
//! ## The input model
//!
//! A *problem* is one or more FSMs plus a per-FSM SMT-LIB *transition relation*.
//! A transition relates the previous tick's state to this tick's state, reads
//! some *given* inputs, and produces an *effects* value. The metadata names
//! which SMT-LIB constants play which role — that naming is the whole point of
//! this crate (Z3 sees only anonymous constants; the engine needs to know which
//! is state, which is input, which is output).
//!
//! See `FORMAT.md` for the on-disk fixture format.

use serde::Deserialize;

use crate::z3c::Value;

/// The SMT sort of a named variable. Scalars plus datatypes (enums) and
/// sequences (used for effect lists).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sort {
    Int,
    Bool,
    Real,
    Str,
    /// A user-declared datatype, by its SMT-LIB sort name (e.g. `"Effect"`).
    Datatype(String),
    /// A sequence of the element sort, i.e. SMT-LIB `(Seq T)`.
    Seq(Box<Sort>),
}

impl Sort {
    /// Parse a sort from its metadata spelling: `Int`, `Bool`, `Real`,
    /// `String`, `Seq(T)`, or any other bare word as a datatype name.
    pub fn parse(s: &str) -> Result<Sort, String> {
        let s = s.trim();
        match s {
            "Int" | "Nat" | "Pos" => Ok(Sort::Int),
            "Bool" => Ok(Sort::Bool),
            "Real" => Ok(Sort::Real),
            "String" | "Str" => Ok(Sort::Str),
            _ => {
                if let Some(inner) = s.strip_prefix("Seq(").and_then(|x| x.strip_suffix(')')) {
                    Ok(Sort::Seq(Box::new(Sort::parse(inner)?)))
                } else if s.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
                    && s.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    Ok(Sort::Datatype(s.to_string()))
                } else {
                    Err(format!("unrecognized sort `{s}`"))
                }
            }
        }
    }
}

/// Deserialize a [`Sort`] from a JSON string ("Int", "Seq(Effect)", ...).
impl<'de> Deserialize<'de> for Sort {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Sort, D::Error> {
        let s = String::deserialize(d)?;
        Sort::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// A literal value in metadata (state `init`, pinned `given` value in a fixture
/// scenario). `serde(untagged)` maps JSON naturally: `true`→Bool, `5`→Int,
/// `5.5`→Real, `"x"`→Str, `{"ctor":"Run","args":[5]}`→Ctor.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Lit {
    Bool(bool),
    Int(i64),
    Real(f64),
    Str(String),
    Ctor { ctor: String, args: Vec<Lit> },
}

impl Lit {
    /// Convert to a runtime [`Value`], using the declared sort to disambiguate
    /// (a JSON string under a `Datatype` sort is a nullary constructor, not a
    /// `Str`).
    pub fn to_value(&self, sort: &Sort) -> Value {
        match self {
            Lit::Bool(b) => Value::Bool(*b),
            Lit::Int(i) => Value::Int(*i),
            Lit::Real(r) => Value::Real(*r),
            Lit::Str(s) => match sort {
                Sort::Datatype(_) => Value::nullary(s.clone()),
                _ => Value::Str(s.clone()),
            },
            Lit::Ctor { ctor, args } => {
                let elem = match sort {
                    Sort::Seq(inner) => inner.as_ref(),
                    other => other,
                };
                Value::Enum {
                    ctor: ctor.clone(),
                    args: args.iter().map(|a| a.to_value(elem)).collect(),
                }
            }
        }
    }
}

/// A state variable threaded across ticks: the model's `next` value becomes the
/// `prev` value pinned on the following tick.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StateVar {
    /// SMT-LIB const read as the previous tick's value (e.g. `"_count"`).
    pub prev: String,
    /// SMT-LIB const written as this tick's value (e.g. `"count"`).
    pub next: String,
    pub sort: Sort,
    /// Initial value pinned to `prev` on tick 0. `None` leaves it to Z3.
    #[serde(default)]
    pub init: Option<Lit>,
}

/// A given (input) variable, pinned each tick from outside the solver.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GivenVar {
    pub name: String,
    pub sort: Sort,
}

/// Where the effect list lives in the model: a single SMT-LIB const, expected
/// to be a `(Seq <EffectDatatype>)` whose elements are decoded structurally.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EffectSpec {
    pub var: String,
}

/// How the engine learns this FSM wants to halt, beyond the built-in `Exit`
/// effect: an optional Bool const that signals halt when true.
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct HaltSpec {
    #[serde(default)]
    pub var: Option<String>,
}

/// Where the FSM reads the PREVIOUS tick's effect results. The engine dispatches
/// tick K's effects, maps each to a `Result`-enum value (see `effect.rs`), and
/// pins the ordered `(Seq Result)` of those as tick K+1's `given[var]`. On tick
/// 0 the pin is the empty sequence. `elem_sort` is the SMT-LIB element sort name
/// (default `"Result"`), needed to emit the empty-seq `(as seq.empty …)` form.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LastResultsSpec {
    /// SMT-LIB const the FSM reads as the prior tick's results (default
    /// `"last_results"`).
    #[serde(default = "default_last_results_var")]
    pub var: String,
    /// Element sort name of the `(Seq T)` (default `"Result"`).
    #[serde(default = "default_result_sort")]
    pub elem_sort: String,
}

fn default_last_results_var() -> String {
    "last_results".to_string()
}
fn default_result_sort() -> String {
    "Result".to_string()
}

impl Default for LastResultsSpec {
    fn default() -> Self {
        LastResultsSpec {
            var: default_last_results_var(),
            elem_sort: default_result_sort(),
        }
    }
}

/// One finite state machine: its transition relation plus the role assignments.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FsmSpec {
    pub name: String,
    /// The SMT-LIB transition text. NOT in the JSON — filled by the loader from
    /// the matching `; @transition <name>` block.
    #[serde(skip)]
    pub transition: String,
    #[serde(default)]
    pub state: Vec<StateVar>,
    #[serde(default)]
    pub given: Vec<GivenVar>,
    #[serde(default)]
    pub effects: Option<EffectSpec>,
    #[serde(default)]
    pub halt: Option<HaltSpec>,
    /// Where this FSM reads the previous tick's effect results. When present,
    /// the engine threads dispatched `Result`s into the next tick's `given`.
    #[serde(default)]
    pub last_results: Option<LastResultsSpec>,
    /// N3: world var names this FSM writes this tick (its model exposes a `next`
    /// const for each). Empty for single-FSM problems.
    #[serde(default)]
    pub world_writes: Vec<String>,
    /// N3: world var names this FSM reads (pinned as given from shared world).
    #[serde(default)]
    pub world_reads: Vec<String>,
}

/// A shared-world variable (N3). Multiple FSMs read it; at most one writes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WorldVar {
    pub name: String,
    pub sort: Sort,
    /// Initial value before tick 0.
    #[serde(default)]
    pub init: Option<Lit>,
}

/// A whole problem: the FSMs and any shared world.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Problem {
    pub fsms: Vec<FsmSpec>,
    #[serde(default)]
    pub world: Vec<WorldVar>,
}

// ---------------------------------------------------------------------------
// Typed tick result (output of the model extractor)
// ---------------------------------------------------------------------------

/// A decoded effect: constructor name + decoded argument values. The effect
/// dispatcher (Phase 2) interprets these (`Println` → stdout, `Exit` → halt).
#[derive(Debug, Clone, PartialEq)]
pub struct EffectValue {
    pub ctor: String,
    pub args: Vec<Value>,
}

/// The typed result of solving one tick: the next state, emitted effects, and
/// the halt flag (if the FSM declared one).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TickModel {
    /// `next`-var name → decoded value. Keyed by the StateVar `next` name.
    pub next_state: Vec<(String, Value)>,
    /// World writes this tick: world-var name → decoded value (N3).
    pub world_writes: Vec<(String, Value)>,
    pub effects: Vec<EffectValue>,
    pub halt_flag: bool,
}

impl TickModel {
    pub fn next_value(&self, next_name: &str) -> Option<&Value> {
        self.next_state.iter().find(|(n, _)| n == next_name).map(|(_, v)| v)
    }
    pub fn world_value(&self, name: &str) -> Option<&Value> {
        self.world_writes.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scalar_and_composite_sorts() {
        assert_eq!(Sort::parse("Int").unwrap(), Sort::Int);
        assert_eq!(Sort::parse("Nat").unwrap(), Sort::Int);
        assert_eq!(Sort::parse("String").unwrap(), Sort::Str);
        assert_eq!(Sort::parse("Effect").unwrap(), Sort::Datatype("Effect".into()));
        assert_eq!(
            Sort::parse("Seq(Effect)").unwrap(),
            Sort::Seq(Box::new(Sort::Datatype("Effect".into())))
        );
    }

    #[test]
    fn lit_to_value_respects_sort() {
        assert_eq!(Lit::Int(3).to_value(&Sort::Int), Value::Int(3));
        // A JSON string under a datatype sort is a nullary ctor, not a Str.
        assert_eq!(
            Lit::Str("Start".into()).to_value(&Sort::Datatype("S".into())),
            Value::nullary("Start")
        );
        assert_eq!(Lit::Str("hi".into()).to_value(&Sort::Str), Value::Str("hi".into()));
        assert_eq!(
            Lit::Ctor { ctor: "Run".into(), args: vec![Lit::Int(5)] }
                .to_value(&Sort::Datatype("S".into())),
            Value::Enum { ctor: "Run".into(), args: vec![Value::Int(5)] }
        );
    }
}
