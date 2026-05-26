//! Z3-typed bindings (`Var`), composite-field shapes (`FieldKind`/`SeqFieldElem`),
//! long-lived sort caches (`DatatypeRegistry`/`EnumRegistry`), and `CachedSchema`.

use std::cell::RefCell;
use std::collections::HashMap;
use z3::ast::{Array, Bool, Int, Real, Set, String as Z3Str};
use z3::{DatatypeSort, Solver};

use crate::core::Value;

/// Lazily-built cache of Z3 Datatype sorts for `Seq(UserType)` element types.
/// `'static` mirrors the runtime's leaked Context; each entry pairs the sort with its field list.
pub type DatatypeRegistry =
    RefCell<HashMap<String, (&'static DatatypeSort<'static>, Vec<FieldKind>)>>;

/// Eagerly-built enum sort cache (unlike the lazy `DatatypeRegistry`).
/// `by_name`: enum name → sort + variants; `by_variant`: variant → (enum, index) in O(1).
pub struct EnumRegistry {
    pub by_name: RefCell<HashMap<String,
        (&'static DatatypeSort<'static>, Vec<crate::core::ast::EnumVariant>)>>,
    pub by_variant: RefCell<HashMap<String, (String, usize)>>,
}

impl EnumRegistry {
    pub fn new() -> Self {
        Self {
            by_name: RefCell::new(HashMap::new()),
            by_variant: RefCell::new(HashMap::new()),
        }
    }
}

impl Default for EnumRegistry {
    fn default() -> Self { Self::new() }
}

/// Primitive element type of a homogeneous Seq — routes model extraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeqElem { Int, Bool, Str }

/// One field of a composite element in `Seq(UserType)`. Primitive or recursively nested.
#[derive(Clone, Debug)]
pub enum FieldKind {
    Primitive {
        name: String,
        /// "Int" | "Nat" | "Pos" | "Bool" | "String" — routes extraction to the right accessor.
        prim_type: String,
    },
    Nested {
        name: String,
        #[allow(dead_code)]
        type_name: String,
        /// 'static mirrors the leaked Context — consistent with DatatypeSeqVar.
        dt: &'static DatatypeSort<'static>,
        sub_fields: Vec<FieldKind>,
    },
    /// `Seq(T)` field inside a composite. Parent Datatype has TWO accessors: Array + length.
    /// Enables tree-of-sequences: `Seq(Composite)` can contain a Seq field.
    SeqField {
        name: String,
        arr_idx: usize,
        /// Always `arr_idx + 1` by construction.
        len_idx: usize,
        #[allow(dead_code)]
        elem_type_name: String,
        elem: SeqFieldElem,
    },
}

/// Element metadata for `FieldKind::SeqField`.
#[derive(Clone, Debug)]
pub enum SeqFieldElem {
    Primitive(SeqElem),
    Enum { enum_name: String, dt: &'static DatatypeSort<'static> },
    Composite { type_name: String, dt: &'static DatatypeSort<'static>, sub_fields: Vec<FieldKind> },
}

impl FieldKind {
    pub fn name(&self) -> &str {
        match self {
            FieldKind::Primitive { name, .. } => name,
            FieldKind::Nested { name, .. } => name,
            FieldKind::SeqField { name, .. } => name,
        }
    }
}

/// Typed Z3 handle for a declared variable. Seqs use Array(Int→T)+length (not native Seq sort:
/// `Z3_mk_seq_nth` unavailable in the safe crate); we don't read past `len` during extraction.
#[derive(Clone)]
pub enum Var<'ctx> {
    IntVar(Int<'ctx>),
    RealVar(Real<'ctx>),
    BoolVar(Bool<'ctx>),
    StrVar(Z3Str<'ctx>),
    SeqVar { arr: Array<'ctx>, len: Int<'ctx>, elem: SeqElem },
    /// `Seq(UserType)` modeled as Array(Int→DatatypeSort)+length.
    /// `dt` duplicated here so translators avoid threading the registry into every call.
    DatatypeSeqVar {
        arr: Array<'ctx>,
        len: Int<'ctx>,
        type_name: String,
        dt: &'static DatatypeSort<'static>,
        fields: Vec<FieldKind>,
    },
    /// `Set(primitive)`. Extraction only works when `S = {…}` is pinned;
    /// `candidates` (None→populated on first literal assignment) is shared across clones.
    SetVar {
        set: Set<'ctx>,
        elem: SeqElem,
        candidates: std::rc::Rc<std::cell::RefCell<Option<Vec<Value>>>>,
    },
    /// `Set(UserType)`. Cardinality via `candidates.len()` (Z3 has no native set cardinality).
    DatatypeSetVar {
        set: Set<'ctx>,
        type_name: String,
        dt: &'static DatatypeSort<'static>,
        fields: Vec<FieldKind>,
        candidates: std::rc::Rc<std::cell::RefCell<Option<Vec<Value>>>>,
    },
    /// Compile-time literal int. Yields a Z3 IntVal so `literal_range` can simplify+as_i64;
    /// needed to unroll `∀ i ∈ {0..n-1}` when `n` is pinned.
    PinnedInt(i64),
    /// Z3 const of an enum's DatatypeSort. `enum_name` drives model-extraction decoding.
    EnumVar {
        ast: z3::ast::Datatype<'ctx>,
        enum_name: String,
        dt: &'static DatatypeSort<'static>,
    },
    /// Pre-applied nullary variant value. Equality `today = Mon` dispatches via `Ast::_eq`.
    EnumValue {
        ast: z3::ast::Datatype<'ctx>,
    },
    /// Unapplied payload-bearing variant constructor. Nullary variants use `EnumValue`.
    EnumCtor {
        dt: &'static DatatypeSort<'static>,
        variant_idx: usize,
        /// Field types in declaration order — used to route each arg through the right translator.
        field_types: Vec<String>,
    },
}

impl<'ctx> Var<'ctx> {
    pub fn as_bool(&self) -> Option<&Bool<'ctx>> {
        match self { Var::BoolVar(b) => Some(b), _ => None }
    }
    pub fn as_str(&self) -> Option<&Z3Str<'ctx>> {
        match self { Var::StrVar(s) => Some(s), _ => None }
    }
    #[allow(dead_code)]
    pub fn as_real(&self) -> Option<&Real<'ctx>> {
        match self { Var::RealVar(r) => Some(r), _ => None }
    }
    pub fn as_seq(&self) -> Option<(&Array<'ctx>, &Int<'ctx>, SeqElem)> {
        match self { Var::SeqVar { arr, len, elem } => Some((arr, len, *elem)), _ => None }
    }
    pub fn as_set(&self) -> Option<(&Set<'ctx>, SeqElem)> {
        match self { Var::SetVar { set, elem, .. } => Some((set, *elem)), _ => None }
    }
    pub fn as_set_with_candidates(&self) -> Option<(&Set<'ctx>, SeqElem,
        &std::rc::Rc<std::cell::RefCell<Option<Vec<Value>>>>)>
    {
        match self {
            Var::SetVar { set, elem, candidates } => Some((set, *elem, candidates)),
            _ => None,
        }
    }
    pub fn as_datatype_set(&self) -> Option<(&Set<'ctx>, &str,
                                         &'static DatatypeSort<'static>,
                                         &[FieldKind],
                                         &std::rc::Rc<std::cell::RefCell<Option<Vec<Value>>>>)>
    {
        match self {
            Var::DatatypeSetVar { set, type_name, dt, fields, candidates } =>
                Some((set, type_name.as_str(), *dt, fields.as_slice(), candidates)),
            _ => None,
        }
    }
    pub fn as_datatype_seq(&self) -> Option<(&Array<'ctx>, &Int<'ctx>, &str,
                                         &'static DatatypeSort<'static>,
                                         &[FieldKind])> {
        match self {
            Var::DatatypeSeqVar { arr, len, type_name, dt, fields } =>
                Some((arr, len, type_name.as_str(), *dt, fields.as_slice())),
            _ => None,
        }
    }
}

/// Per-schema solver cache for `evaluate_cached`. Body constraints pre-asserted.
pub struct CachedSchema<'ctx> {
    pub env: HashMap<String, Var<'ctx>>,
    pub solver: Solver<'ctx>,
    /// `smt.arith.solver` value at build time; 0 = Z3 default. Auto-tuner rebuilds on mismatch.
    pub arith_solver: u32,
}
