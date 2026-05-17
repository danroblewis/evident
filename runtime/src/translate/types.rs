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
use z3::ast::{Array, Bool, Int, Real, Set, String as Z3Str};
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

/// Z3 Datatype + variant list for each declared `enum Name = A | B | C`.
/// Built eagerly at `EvidentRuntime::load_source` time (unlike the
/// lazily-built `DatatypeRegistry`) because enum variants need to be
/// resolvable as identifier expressions everywhere — pre-populating
/// the env with `Mon → EnumValue(Day, 0)` etc. is cheaper than
/// looking up the registry on every Identifier translation.
///
/// `by_name` keys on the enum's name (e.g. `"Day"`); the value's
/// `Vec<String>` lists the variant names in declaration order
/// (the index also matches the underlying Z3 constructor index).
///
/// `by_variant` is the reverse lookup, populated alongside `by_name`,
/// so a bare identifier `Mon` can be classified as "variant 0 of Day"
/// in O(1). Variant names are globally unique (the load-time check in
/// runtime.rs enforces this).
pub struct EnumRegistry {
    pub by_name: RefCell<HashMap<String,
        (&'static DatatypeSort<'static>, Vec<crate::ast::EnumVariant>)>>,
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

/// Result of running one query.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
    /// On UNSAT, optionally populated when `evaluate_with_core` was
    /// used: indices into the schema's top-level body that Z3
    /// identified as the conflicting subset (via `assert_and_track`
    /// + `get_unsat_core`). `None` when the caller didn't request
    /// it; `Some(empty)` when Z3 returned an empty core (rare —
    /// usually means the conflict is encoded entirely outside the
    /// tracked top-level constraints, e.g. in given values).
    pub unsat_core_items: Option<Vec<usize>>,
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
    /// A `Seq(T)` field inside a composite. The parent Datatype has
    /// TWO accessors per Seq field (an Array(Int → element-sort) for
    /// the elements and an Int for the length). They're stored
    /// contiguously in the parent's accessor list at indices
    /// `arr_idx` and `len_idx = arr_idx + 1`.
    ///
    /// Unlocks tree-of-sequences shapes — a composite can contain a
    /// Seq field, and `Seq(Composite)` therefore reaches Seq-of-Seq
    /// via the wrapping composite. Without this variant, fields
    /// typed `Seq(T)` were silently rejected (see COUNTEREXAMPLES.md
    /// #25 before this landed).
    SeqField {
        name: String,
        /// Index of the Array accessor in the parent Datatype's
        /// accessor list.
        arr_idx: usize,
        /// Index of the Int-length accessor (always `arr_idx + 1`
        /// by construction; cached here so callers don't have to
        /// recompute).
        len_idx: usize,
        /// Element type's spelled name — "Int", "Bool", "String",
        /// or a user-defined type / enum name. For diagnostics
        /// and to round-trip the field's declared type in
        /// `extract_seq_composite`'s mirror.
        #[allow(dead_code)]
        elem_type_name: String,
        /// What sort the elements have, mirroring the top-level
        /// Seq encoding (SeqVar for primitives, DatatypeSeqVar
        /// for enums/composites).
        elem: SeqFieldElem,
    },
}

/// Per-element metadata for a `FieldKind::SeqField`. Mirrors the
/// flavors of top-level `Seq(T)` declarations.
#[derive(Clone, Debug)]
pub enum SeqFieldElem {
    /// Int / Bool / String element type.
    Primitive(SeqElem),
    /// Enum element type — the inner Array's range sort is the enum's
    /// DatatypeSort. Stored similarly to `Var::DatatypeSeqVar` with
    /// empty `fields` (the "enum-element seq" marker).
    Enum {
        enum_name: String,
        dt: &'static DatatypeSort<'static>,
    },
    /// Composite element type — the inner Array's range is a
    /// user-defined record's DatatypeSort. `sub_fields` walks the
    /// record's accessors for `seq_field[i].subfield` lookups.
    /// (Recursive `Seq` sub-fields inside this composite are also
    /// supported — `sub_fields` can itself contain `SeqField`.)
    Composite {
        type_name: String,
        dt: &'static DatatypeSort<'static>,
        sub_fields: Vec<FieldKind>,
    },
}

impl FieldKind {
    pub(super) fn name(&self) -> &str {
        match self {
            FieldKind::Primitive { name, .. } => name,
            FieldKind::Nested { name, .. } => name,
            FieldKind::SeqField { name, .. } => name,
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
pub enum Var<'ctx> {
    IntVar(Int<'ctx>),
    /// Real-valued Z3 const. Supports add/sub/mul/div via Z3 LRA;
    /// comparison via lt/le/gt/ge; equality via Ast::_eq.
    RealVar(Real<'ctx>),
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
    /// `x ∈ s` membership directly. Z3 sets are functions over an
    /// (often infinite) element domain so general model extraction
    /// would need to enumerate the domain.
    ///
    /// For v1 we support extraction *only* when the Set was pinned to
    /// a literal `S = {a, b, c}`. The translate path then records the
    /// literal items in `candidates`, and `extract_set` reads them.
    /// `candidates` is None at declaration; the first `S = SetLit(...)`
    /// against this var populates it. The `Rc<RefCell<…>>` shape lets
    /// the field survive `Var: Clone` — all clones see the same cell.
    SetVar {
        set: Set<'ctx>,
        elem: SeqElem,
        candidates: std::rc::Rc<std::cell::RefCell<Option<Vec<Value>>>>,
    },
    /// `Set(UserType)` — element sort is a Z3 Datatype. Mirrors
    /// `DatatypeSeqVar` for composite-element collections, but uses
    /// Z3's native Set sort (characteristic function over the
    /// DatatypeSort) instead of an Array+length encoding. Membership
    /// `x ∈ s` routes to `set.member(build_composite_dynamic(x))`;
    /// `S = {a, b, c}` builds a literal set by add'ing each composite
    /// Dynamic to `Set::empty`.
    ///
    /// `candidates` lifecycle matches `SetVar`: None at declaration,
    /// populated by the first `S = {…}` literal assignment. Cardinality
    /// `#s` uses `candidates.len()` (Z3 has no native set cardinality).
    DatatypeSetVar {
        set: Set<'ctx>,
        type_name: String,
        dt: &'static DatatypeSort<'static>,
        fields: Vec<FieldKind>,
        candidates: std::rc::Rc<std::cell::RefCell<Option<Vec<Value>>>>,
    },
    /// Compile-time literal int. Mirrors Python's "value pre-bound in env"
    /// pattern: certain names are known to equal a specific integer
    /// before the solver runs (from `given` + literal-equality body
    /// constraints + length propagation `n = #seq` where #seq is also
    /// pinned). Translating an Identifier bound to PinnedInt yields a
    /// Z3 IntVal, which lets `literal_range` recover the value via
    /// simplify+as_i64. Without this, `∀ i ∈ {0..n - 1}` couldn't unroll.
    PinnedInt(i64),
    /// Z3 const of an enum's DatatypeSort (one of N nullary
    /// constructors). `enum_name` lets model extraction look up the
    /// variant list to decode the returned constructor index back to
    /// a variant name.
    EnumVar {
        ast: z3::ast::Datatype<'ctx>,
        enum_name: String,
        dt: &'static DatatypeSort<'static>,
    },
    /// A specific variant value of an enum (e.g. the bare identifier
    /// `Mon` after lookup). Holds the constructor's Datatype value
    /// directly so equality `today = Mon` can dispatch via Ast::_eq.
    EnumValue {
        ast: z3::ast::Datatype<'ctx>,
    },
    /// A reference to a payload-bearing variant's constructor — not
    /// yet applied. Nullary variants stay as `EnumValue` (pre-applied);
    /// only variants whose `fields` are non-empty land here.
    EnumCtor {
        dt: &'static DatatypeSort<'static>,
        variant_idx: usize,
        /// Declared field types, in order. Used to type-check args at
        /// the call site and to decide which translate_* function to
        /// route each arg through.
        field_types: Vec<String>,
    },
}

impl<'ctx> Var<'ctx> {
    pub(super) fn as_bool(&self) -> Option<&Bool<'ctx>> {
        match self { Var::BoolVar(b) => Some(b), _ => None }
    }
    pub(super) fn as_str(&self) -> Option<&Z3Str<'ctx>> {
        match self { Var::StrVar(s) => Some(s), _ => None }
    }
    #[allow(dead_code)]   // symmetry with as_bool/as_str; reserved for future use
    pub(super) fn as_real(&self) -> Option<&Real<'ctx>> {
        match self { Var::RealVar(r) => Some(r), _ => None }
    }
    pub(super) fn as_seq(&self) -> Option<(&Array<'ctx>, &Int<'ctx>, SeqElem)> {
        match self { Var::SeqVar { arr, len, elem } => Some((arr, len, *elem)), _ => None }
    }
    pub(super) fn as_set(&self) -> Option<(&Set<'ctx>, SeqElem)> {
        match self { Var::SetVar { set, elem, .. } => Some((set, *elem)), _ => None }
    }
    pub(super) fn as_set_with_candidates(&self) -> Option<(&Set<'ctx>, SeqElem,
        &std::rc::Rc<std::cell::RefCell<Option<Vec<Value>>>>)>
    {
        match self {
            Var::SetVar { set, elem, candidates } => Some((set, *elem, candidates)),
            _ => None,
        }
    }
    pub(super) fn as_datatype_set(&self) -> Option<(&Set<'ctx>, &str,
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
    pub env: HashMap<String, Var<'ctx>>,
    pub solver: Solver<'ctx>,
    /// The `smt.arith.solver` value this cache was built under (0
    /// means "no explicit setting, use Z3's default"). The runtime's
    /// auto-tuner consults this to decide whether the cache needs
    /// rebuilding under a different config.
    pub arith_solver: u32,
}

/// Clone an env. `Var` derives `Clone` (Z3 ast types are reference-
/// counted), so this is a shallow copy — both envs continue to refer
/// to the same Z3 constants. Used by quantifier unrollers that need
/// to shadow the bound variable per iteration without disturbing the
/// outer env.
///
/// Lives here because it's a pure data utility — no Z3 expression
/// building, no Solver use — and other translate siblings need it
/// from the leaf layer to keep the dependency graph acyclic.
pub(super) fn env_clone<'ctx>(env: &HashMap<String, Var<'ctx>>) -> HashMap<String, Var<'ctx>> {
    env.clone()
}
