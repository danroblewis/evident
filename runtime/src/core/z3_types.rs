use std::cell::RefCell;
use std::collections::HashMap;
use z3::ast::{Array, Bool, Int, Real, Set, String as Z3Str};
use z3::{DatatypeSort, Solver};

use crate::core::Value;

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
