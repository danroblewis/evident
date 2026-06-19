//! Model-output value types: `Value` (extracted Z3 binding values
//! returned from queries) and `EvalResult` (the result of running
//! one query — satisfiability + bindings).
//!
//! These are part of the runtime's core vocabulary — both the
//! constraint side (translate / runtime / commands) and the
//! execution side (effect_loop) consume them.

use std::collections::HashMap;

/// Result of running one query.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    /// Real-valued binding. Extracted from Z3 via `as_real()` →
    /// `(num: i64, den: i64)` → `num as f64 / den as f64`. Z3
    /// internally stores Real as exact rationals; we lossily project
    /// to f64 at the boundary because that's what consumers use.
    /// For "did the model satisfy x ≈ 3.14" tests, compare with a
    /// tolerance — Z3 gives an exact rational, f64 may round.
    Real(f64),
    Bool(bool),
    Str(String),
    /// Sequence values returned in the model. The variant tracks which
    /// element type was declared so callers don't have to. Length is
    /// implicit in the Vec's len().
    SeqInt(Vec<i64>),
    SeqBool(Vec<bool>),
    SeqStr(Vec<String>),
    /// A single struct value — one entry per declared field, mapping
    /// field name to its primitive Value. Used as the element of
    /// `SeqComposite`. Not currently produced as a top-level binding
    /// (sub-schema field expansion still creates one leaf per field).
    Composite(HashMap<String, Value>),
    /// `Seq(UserType)` — one map per element. Each map keys a flat
    /// field name to the field's primitive Value.
    SeqComposite(Vec<HashMap<String, Value>>),
    /// `Seq(EnumType)` — one Value::Enum per element. Distinct from
    /// SeqComposite because enum elements have a variant tag + payload,
    /// not a flat field map. Populated by `extract_seq_enum` when the
    /// DatatypeSeqVar has empty `fields` (enum case).
    SeqEnum(Vec<Value>),
    /// `Set(Int|Bool|String)` extracted as a Vec for deterministic
    /// iteration. The runtime picks an order at extract time
    /// (currently the order of the SetLit RHS that pinned the Set);
    /// programs must not depend on which order — that's what Set
    /// is for. Future general-extraction work may sort/canonicalize.
    /// Only populated when the Set was constructed via a `S = {…}`
    /// literal assignment; free Sets extract as missing bindings.
    SetInt(Vec<i64>),
    SetBool(Vec<bool>),
    SetStr(Vec<String>),
    /// An enum variant: the enum's name, the chosen variant, and any
    /// payload field values extracted from the Z3 model. Field order
    /// matches the variant's declaration order. For nullary variants
    /// `fields` is empty.
    ///
    /// Recursive payload values nest naturally — a `Cons(5, Cons(7, Nil))`
    /// is `Enum { variant: "Cons", fields: [Int(5),
    /// Enum { variant: "Cons", fields: [Int(7), Enum { variant: "Nil", fields: [] }] }] }`.
    Enum {
        enum_name: String,
        variant: String,
        fields: Vec<Value>,
    },
}
