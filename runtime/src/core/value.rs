//! Model-output value types: `Value` (extracted Z3 binding) and `EvalResult`
//! (query result — satisfiability + bindings + optional unsat core).

use std::collections::HashMap;

/// Result of running one query.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
    /// On UNSAT, indices of conflicting top-level body items (via `assert_and_track`).
    /// `None` = not requested; `Some([])` = conflict is outside tracked constraints.
    pub unsat_core_items: Option<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    /// Z3 stores reals as exact rationals; we project to f64 at extraction.
    /// Compare with a tolerance — f64 may round from Z3's exact rational.
    Real(f64),
    Bool(bool),
    Str(String),
    SeqInt(Vec<i64>),
    SeqBool(Vec<bool>),
    SeqStr(Vec<String>),
    /// Per-field map for a struct value. Element of `SeqComposite`.
    Composite(HashMap<String, Value>),
    /// `Seq(UserType)` — one flat field-map per element.
    SeqComposite(Vec<HashMap<String, Value>>),
    /// `Seq(EnumType)` — one Value::Enum per element.
    SeqEnum(Vec<Value>),
    /// `Set(Int|Bool|String)` as a Vec. Order reflects the SetLit RHS;
    /// only populated for `S = {…}` literal assignments — free Sets extract as missing.
    SetInt(Vec<i64>),
    SetBool(Vec<bool>),
    SetStr(Vec<String>),
    /// An enum variant value. Fields in declaration order; nullary variants have empty `fields`.
    /// Recursive: `Cons(5, Cons(7, Nil))` nests naturally.
    Enum {
        enum_name: String,
        variant: String,
        fields: Vec<Value>,
    },
}
