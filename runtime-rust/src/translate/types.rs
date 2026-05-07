//! Core types for the translate pipeline: `Var` (typed Z3 binding),
//! `Value` (extracted model output), `FieldKind` (composite field
//! metadata), `DatatypeRegistry`, `CachedSchema`, `EvalResult`.
//!
//! Visibility note: items used by other `translate/*.rs` siblings are
//! `pub(super)` — visible inside `translate::` only. The handful that
//! cross the module boundary (`Value`, `EvalResult`, `FieldKind`,
//! `DatatypeRegistry`, `CachedSchema`) are `pub` and re-exported from
//! `translate.rs`.

use std::cell::RefCell;
use std::collections::HashMap;
use z3::ast::{Array, Bool, Int, Set, String as Z3Str};
use z3::{DatatypeSort, Solver};

/// Cache of Z3 Datatype sorts built for user types referenced as the
/// element of `Seq(UserType)`. Built lazily on first reference. The
/// runtime owns this and passes a reference into `evaluate` /
/// `build_cache` the same way `schemas` is passed.
///
/// The `'static` lifetime mirrors the runtime's leaked `Context` —
/// the runtime already leaks its Context, so leaking the per-type
/// `DatatypeSort` (which borrows from that Context) is consistent.
/// See `EvidentRuntime::new` for why the Context is leaked.
///
/// Each entry caches both the DatatypeSort and the parallel
/// `Vec<FieldKind>` that describes how to walk the type's fields
/// (leaf primitives + nested sub-structs). Sharing the field list
/// across siblings (e.g. SDLRect.color and SDLOutput.bg both use
/// Color) avoids re-walking the schema body on every reference.
pub type DatatypeRegistry =
    RefCell<HashMap<String, (&'static DatatypeSort<'static>, Vec<FieldKind>)>>;

/// Result of running one query.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
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
}

/// What primitive a Seq holds. Lets `SeqVar` stay homogeneous while
/// still letting model extraction pick the right path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SeqElem { Int, Bool, Str }

/// One field of a user type stored as the element of `Seq(UserType)`.
/// Two flavors: leaf primitives (Int/Nat/Pos/Bool/String), and nested
/// composite fields whose own type is itself a user struct.
///
/// The accessor in the parent Datatype always returns a `Dynamic` of
/// the field's sort. For primitives that's an Int/Bool/String; for
/// nested composites it's another Datatype value, which has its own
/// list of accessors (the `sub_fields` here, plus the `dt` pointer).
///
/// v1 still rejects fields that are themselves Seqs / Sets — the
/// recursion only handles user-defined struct types.
#[derive(Clone, Debug)]
pub enum FieldKind {
    Primitive {
        name: String,
        /// "Int" | "Nat" | "Pos" | "Bool" | "String" — routes the
        /// extracted Dynamic through the right `as_int` / `as_bool`
        /// / `as_string` extractor and tells callers what sort it
        /// translates to.
        prim_type: String,
    },
    Nested {
        name: String,
        /// User type's name, kept for diagnostics + cache key parity
        /// with what `get_or_build_datatype` registers.
        #[allow(dead_code)]
        type_name: String,
        /// Z3 Datatype for this nested type. Same `'static` lifetime
        /// trick as the outer DatatypeSeqVar's `dt` — the runtime
        /// already leaks its Context, so leaking the per-type sort
        /// is consistent.
        dt: &'static DatatypeSort<'static>,
        /// Recursive: the nested type's own field list. Could itself
        /// contain another `Nested` for two-deep composition (e.g.
        /// SDLOutput.bg.color, if Color had another nested field).
        sub_fields: Vec<FieldKind>,
    },
}

impl FieldKind {
    pub(super) fn name(&self) -> &str {
        match self {
            FieldKind::Primitive { name, .. } => name,
            FieldKind::Nested { name, .. } => name,
        }
    }
}

/// Z3 binding for a declared variable. Keep a typed handle so we know
/// which AST kind to translate against.
///
/// Seq values are modeled as a Z3 Array(Int → T) plus an explicit
/// length variable. Z3's native Seq sort would work via `Z3_mk_seq_sort`
/// but the safe `z3` crate doesn't expose `Z3_mk_seq_nth` (only
/// `Z3_mk_seq_at` which returns a length-1 sub-sequence with no way
/// to extract the element). The Array+Length encoding is simpler and
/// gives us cardinality + indexing for free; the only downside is the
/// Array has values at all indices, not just 0..len, but we just don't
/// read past `len` during model extraction.
#[derive(Clone)]
pub(super) enum Var<'ctx> {
    IntVar(Int<'ctx>),
    BoolVar(Bool<'ctx>),
    StrVar(Z3Str<'ctx>),
    SeqVar { arr: Array<'ctx>, len: Int<'ctx>, elem: SeqElem },
    /// `Seq(UserType)` — element sort is a Z3 Datatype whose
    /// constructor + accessors live in the shared `DatatypeRegistry`.
    /// Modeled the same as primitive Seqs: `Array(Int → DatatypeSort)
    /// + length`. The `dt` pointer is duplicated here so translators
    /// can resolve `pts[i].field` without threading the registry
    /// through every translate-* call. The `'static` lifetime on
    /// `dt` mirrors the leaked Context; this variant is only ever
    /// constructed from the cached path with `'ctx = 'static`.
    DatatypeSeqVar {
        arr: Array<'ctx>,
        len: Int<'ctx>,
        type_name: String,
        dt: &'static DatatypeSort<'static>,
        /// Per-field metadata in declaration order — the same order as
        /// `dt.variants[0].accessors`. Each entry is a `FieldKind`,
        /// either a leaf primitive (which routes through `as_int` /
        /// `as_bool` / `as_string`) or a `Nested` sub-struct (which
        /// holds its own DatatypeSort + `sub_fields` for further
        /// recursion).
        fields: Vec<FieldKind>,
    },
    /// Z3 Set — characteristic function over an element sort. Supports
    /// `x ∈ s` membership; we don't expose cardinality / iteration
    /// because Z3 sets are functions over infinite domains, not finite
    /// containers. Model extraction returns no binding for SetVars.
    SetVar { set: Set<'ctx>, elem: SeqElem },
    /// Compile-time literal int. Mirrors Python's "value pre-bound in env"
    /// pattern: certain names are known to equal a specific integer
    /// before the solver runs (from `given` + literal-equality body
    /// constraints + length propagation `n = #seq` where #seq is also
    /// pinned). Translating an Identifier bound to PinnedInt yields a
    /// Z3 IntVal, which lets `literal_range` recover the value via
    /// simplify+as_i64. Without this, `∀ i ∈ {0..n - 1}` couldn't unroll.
    PinnedInt(i64),
}

impl<'ctx> Var<'ctx> {
    pub(super) fn as_bool(&self) -> Option<&Bool<'ctx>> {
        match self { Var::BoolVar(b) => Some(b), _ => None }
    }
    pub(super) fn as_str(&self) -> Option<&Z3Str<'ctx>> {
        match self { Var::StrVar(s) => Some(s), _ => None }
    }
    pub(super) fn as_seq(&self) -> Option<(&Array<'ctx>, &Int<'ctx>, SeqElem)> {
        match self { Var::SeqVar { arr, len, elem } => Some((arr, len, *elem)), _ => None }
    }
    pub(super) fn as_set(&self) -> Option<(&Set<'ctx>, SeqElem)> {
        match self { Var::SetVar { set, elem } => Some((set, *elem)), _ => None }
    }
    pub(super) fn as_datatype_seq(&self) -> Option<(&Array<'ctx>, &Int<'ctx>, &str,
                                         &'static DatatypeSort<'static>,
                                         &[FieldKind])> {
        match self {
            Var::DatatypeSeqVar { arr, len, type_name, dt, fields } =>
                Some((arr, len, type_name.as_str(), *dt, fields.as_slice())),
            _ => None,
        }
    }
}

/// Per-schema cache used by `evaluate_cached`. Holds the shared
/// solver (with the schema's body constraints already asserted at
/// the bottom of the assertion stack) and the env mapping used to
/// resolve given-bindings + extract the model.
pub struct CachedSchema<'ctx> {
    pub(super) env: HashMap<String, Var<'ctx>>,
    pub(super) solver: Solver<'ctx>,
    /// The `smt.arith.solver` value this cache was built under (0
    /// means "no explicit setting, use Z3's default"). The runtime's
    /// auto-tuner consults this to decide whether the cache needs
    /// rebuilding under a different config.
    pub arith_solver: u32,
}
