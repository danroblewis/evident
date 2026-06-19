//! Core shared types: the runtime `Value`, the Z3-side variable/registry types,
//! the `Z3Program` IR the functionizer consumes, the public query result + error,
//! and a couple of pure Seq type-name helpers.

use std::cell::RefCell;
use std::collections::HashMap;
use z3::ast::{Array, Bool, Dynamic, Int, Real, Set, String as Z3Str};
use z3::{DatatypeSort, Solver};

// ───────────────────────────── runtime values ─────────────────────────────

#[derive(Debug, Clone)]
pub struct EvalResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),

    Real(f64),
    Bool(bool),
    Str(String),

    SeqInt(Vec<i64>),
    SeqBool(Vec<bool>),
    SeqStr(Vec<String>),

    Composite(HashMap<String, Value>),

    SeqComposite(Vec<HashMap<String, Value>>),

    SeqEnum(Vec<Value>),

    SetInt(Vec<i64>),
    SetBool(Vec<bool>),
    SetStr(Vec<String>),

    Enum {
        enum_name: String,
        variant: String,
        fields: Vec<Value>,
    },
}

// ─────────────────────────── Z3-side registries & vars ───────────────────────────

pub type DatatypeRegistry =
    RefCell<HashMap<String, (&'static DatatypeSort<'static>, Vec<FieldKind>)>>;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeqElem { Int, Bool, Str }

#[derive(Clone, Debug)]
pub enum FieldKind {
    Primitive {
        name: String,

        prim_type: String,
    },
    Nested {
        name: String,

        #[allow(dead_code)]
        type_name: String,

        dt: &'static DatatypeSort<'static>,

        sub_fields: Vec<FieldKind>,
    },

    SeqField {
        name: String,

        arr_idx: usize,

        len_idx: usize,

        #[allow(dead_code)]
        elem_type_name: String,

        elem: SeqFieldElem,
    },
}

#[derive(Clone, Debug)]
pub enum SeqFieldElem {

    Primitive(SeqElem),

    Enum {
        enum_name: String,
        dt: &'static DatatypeSort<'static>,
    },

    Composite {
        type_name: String,
        dt: &'static DatatypeSort<'static>,
        sub_fields: Vec<FieldKind>,
    },
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

#[derive(Clone)]
pub enum Var<'ctx> {
    IntVar(Int<'ctx>),

    RealVar(Real<'ctx>),
    BoolVar(Bool<'ctx>),
    StrVar(Z3Str<'ctx>),
    SeqVar { arr: Array<'ctx>, len: Int<'ctx>, elem: SeqElem },

    DatatypeSeqVar {
        arr: Array<'ctx>,
        len: Int<'ctx>,
        type_name: String,
        dt: &'static DatatypeSort<'static>,

        fields: Vec<FieldKind>,
    },

    SetVar {
        set: Set<'ctx>,
        elem: SeqElem,
        candidates: std::rc::Rc<std::cell::RefCell<Option<Vec<Value>>>>,
    },

    DatatypeSetVar {
        set: Set<'ctx>,
        type_name: String,
        dt: &'static DatatypeSort<'static>,
        fields: Vec<FieldKind>,
        candidates: std::rc::Rc<std::cell::RefCell<Option<Vec<Value>>>>,
    },

    PinnedInt(i64),

    EnumVar {
        ast: z3::ast::Datatype<'ctx>,
        enum_name: String,
        dt: &'static DatatypeSort<'static>,
    },

    EnumValue {
        ast: z3::ast::Datatype<'ctx>,
    },

    EnumCtor {
        dt: &'static DatatypeSort<'static>,
        variant_idx: usize,

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

pub struct CompiledModel<'ctx> {
    pub env: HashMap<String, Var<'ctx>>,
    pub solver: Solver<'ctx>,

    pub arith_solver: u32,
}

// ───────────────────────── Z3Program IR (functionizer input) ─────────────────────────

#[derive(Debug, Clone)]
pub struct Z3Program<'ctx> {

    pub steps: Vec<Z3Step<'ctx>>,

    pub checks: Vec<(Dynamic<'ctx>, Dynamic<'ctx>)>,

    pub predicates: Vec<Bool<'ctx>>,
}

#[derive(Debug, Clone)]
pub enum Z3Step<'ctx> {

    Scalar { var: String, expr: Dynamic<'ctx> },

    Seq    { var: String, elem_exprs: Vec<Dynamic<'ctx>> },

    Guarded { var: String, branches: Vec<GuardedBranch<'ctx>> },

    PreBaked { var: String, value: Value },
}

#[derive(Debug, Clone)]
pub struct GuardedBranch<'ctx> {
    pub guard: Dynamic<'ctx>,
    pub body:  GuardedBody<'ctx>,
}

#[derive(Debug, Clone)]
pub enum GuardedBody<'ctx> {
    Scalar(Dynamic<'ctx>),
    Seq(Vec<Dynamic<'ctx>>),
}

impl<'ctx> Z3Step<'ctx> {
    pub fn var(&self) -> &str {
        match self {
            Z3Step::Scalar   { var, .. }
            | Z3Step::Seq      { var, .. }
            | Z3Step::Guarded  { var, .. }
            | Z3Step::PreBaked { var, .. } => var,
        }
    }
}

// ───────────────────────── public query result + error ─────────────────────────

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
}

#[derive(Debug)]
pub enum RuntimeError {
    Parse(String),
    UnknownSchema(String),
    Io(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RuntimeError::Parse(s) => write!(f, "{}", s),
            RuntimeError::UnknownSchema(s) => write!(f, "unknown schema {:?}", s),
            RuntimeError::Io(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for RuntimeError {}

// ───────────────────────── pure Seq type-name helpers ─────────────────────────

pub fn parse_seq_type(s: &str) -> Option<&str> {
    if s.starts_with("Seq(") && s.ends_with(')') {
        Some(&s[4..s.len() - 1])
    } else {
        None
    }
}

pub fn internal_cons_helper_name(t: &str) -> String {
    format!("__SeqOf_{}", t)
}
