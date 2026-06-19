use std::collections::HashMap;
use cranelift::prelude::{AbiParam, types};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use crate::translate::{EnumRegistry, Value};
use crate::z3_eval::Z3Program;

pub struct JitProgram {
    _module: JITModule,
    func: unsafe extern "C" fn(*const Value, *mut Value, *const Value),
    pub input_offsets: HashMap<String, usize>,
    pub input_kinds:   HashMap<String, OutputKind>,
    pub output_offsets: HashMap<String, usize>,
    pub output_kinds:   HashMap<String, OutputKind>,
    pub enum_tags:     HashMap<String, HashMap<String, i64>>,
    pub enum_variants: HashMap<String, Vec<String>>,

    _string_pool: Vec<Box<str>>,

    value_pool: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputKind {
    Int,
    Bool,

    Enum(String),

    EnumPayload(String),

    Seq,

    Str,
}

impl JitProgram {
    pub fn call(&self, given: &HashMap<String, Value>) -> Option<HashMap<String, Value>> {
        let n_in  = self.input_offsets.len();
        let n_out = self.output_offsets.len();

        let mut inputs: Vec<Value> = (0..n_in).map(|_| Value::Int(0)).collect();
        for (name, &idx) in &self.input_offsets {
            if let Some(v) = given.get(name) {
                inputs[idx] = v.clone();
            }
        }

        let mut outputs: Vec<Value> = (0..n_out).map(|_| Value::Int(0)).collect();
        let pool_ptr = if self.value_pool.is_empty() {
            std::ptr::null()
        } else {
            self.value_pool.as_ptr()
        };

        unsafe {
            (self.func)(inputs.as_ptr(), outputs.as_mut_ptr(), pool_ptr);
        }
        let mut out = HashMap::new();
        for (name, &idx) in &self.output_offsets {
            let v = outputs[idx].clone();
            out.insert(name.clone(), classify_seq(v));
        }
        Some(out)
    }
}

pub(super) fn classify_seq(v: Value) -> Value {
    let Value::SeqEnum(xs) = v else { return v };
    match xs.first() {
        None                  => Value::SeqEnum(vec![]),
        Some(Value::Int(_))   => Value::SeqInt(
            xs.into_iter().filter_map(|e|
                if let Value::Int(n) = e { Some(n) } else { None }).collect()),
        Some(Value::Bool(_))  => Value::SeqBool(
            xs.into_iter().filter_map(|e|
                if let Value::Bool(b) = e { Some(b) } else { None }).collect()),
        Some(Value::Str(_))   => Value::SeqStr(
            xs.into_iter().filter_map(|e|
                if let Value::Str(s) = e { Some(s) } else { None }).collect()),

        Some(Value::Composite(_)) => Value::SeqComposite(
            xs.into_iter().filter_map(|e|
                if let Value::Composite(m) = e { Some(m) } else { None }).collect()),
        _ => Value::SeqEnum(xs),
    }
}

#[derive(Clone, Copy)]
pub(super) struct HelperIds {
    init_slot:         FuncId,
    set_int:           FuncId,
    set_bool:          FuncId,
    set_str:           FuncId,
    set_enum_nullary:  FuncId,
    set_enum_int:      FuncId,
    set_enum_str:      FuncId,
    set_enum_multifield: FuncId,
    set_composite:     FuncId,
    seq_new:           FuncId,
    seq_push_clone:    FuncId,
    seq_set:           FuncId,
    load_int:          FuncId,
    load_bool:         FuncId,
    extract_field:     FuncId,
    seq_select:        FuncId,
    is_variant:        FuncId,
    clone_from_pool:   FuncId,
}

#[derive(Clone, Copy)]
pub(super) struct HelperRefs {
    pub(super) init_slot:         cranelift::codegen::ir::FuncRef,
    pub(super) set_int:           cranelift::codegen::ir::FuncRef,
    pub(super) set_bool:          cranelift::codegen::ir::FuncRef,
    pub(super) set_str:           cranelift::codegen::ir::FuncRef,
    pub(super) set_enum_nullary:  cranelift::codegen::ir::FuncRef,
    pub(super) set_enum_int:      cranelift::codegen::ir::FuncRef,
    pub(super) set_enum_str:      cranelift::codegen::ir::FuncRef,
    pub(super) set_enum_multifield: cranelift::codegen::ir::FuncRef,
    pub(super) set_composite:     cranelift::codegen::ir::FuncRef,
    pub(super) seq_new:           cranelift::codegen::ir::FuncRef,
    pub(super) seq_push_clone:    cranelift::codegen::ir::FuncRef,
    pub(super) seq_set:           cranelift::codegen::ir::FuncRef,
    pub(super) load_int:          cranelift::codegen::ir::FuncRef,
    pub(super) load_bool:         cranelift::codegen::ir::FuncRef,
    pub(super) extract_field:     cranelift::codegen::ir::FuncRef,
    pub(super) seq_select:        cranelift::codegen::ir::FuncRef,
    pub(super) is_variant:        cranelift::codegen::ir::FuncRef,
    pub(super) clone_from_pool:   cranelift::codegen::ir::FuncRef,
}

pub(super) fn declare_helpers(
    module: &mut JITModule,
    ptr_t: cranelift::prelude::Type,
) -> Option<HelperIds> {
    let i64t = types::I64;
    let p = ptr_t;
    let usz = ptr_t;
    let mk = |params: &[cranelift::prelude::Type], m: &mut JITModule|
        -> cranelift::codegen::ir::Signature
    {
        let mut s = m.make_signature();
        for &x in params { s.params.push(AbiParam::new(x)); }
        s
    };
    let mk_ret = |params: &[cranelift::prelude::Type],
                  ret: cranelift::prelude::Type,
                  m: &mut JITModule|
        -> cranelift::codegen::ir::Signature
    {
        let mut s = m.make_signature();
        for &x in params { s.params.push(AbiParam::new(x)); }
        s.returns.push(AbiParam::new(ret));
        s
    };
    let s_init     = mk(&[p], module);
    let s_set_int  = mk(&[p, i64t], module);
    let s_set_bool = mk(&[p, i64t], module);
    let s_set_str  = mk(&[p, p, usz], module);
    let s_nullary  = mk(&[p, p, usz, p, usz], module);
    let s_enum_int = mk(&[p, p, usz, p, usz, i64t], module);
    let s_enum_str = mk(&[p, p, usz, p, usz, p, usz], module);
    let s_enum_mf  = mk(&[p, p, usz, p, usz, p, usz], module);
    let s_composite = mk(&[p, p, p, p, usz], module);
    let s_seq_new  = mk(&[p, usz], module);
    let s_seq_push = mk(&[p, p], module);
    let s_seq_set  = mk(&[p, i64t, p], module);
    let s_load_int = mk_ret(&[p], i64t, module);
    let s_load_bool = mk_ret(&[p], i64t, module);
    let s_extract = mk(&[p, p, p, usz], module);
    let s_seq_sel = mk(&[p, p, i64t], module);
    let s_is_var  = mk_ret(&[p, p, usz], i64t, module);
    let s_pool    = mk(&[p, p, usz], module);

    Some(HelperIds {
        init_slot:        module.declare_function("ev_init_slot",        Linkage::Import, &s_init).ok()?,
        set_int:          module.declare_function("ev_set_int",          Linkage::Import, &s_set_int).ok()?,
        set_bool:         module.declare_function("ev_set_bool",         Linkage::Import, &s_set_bool).ok()?,
        set_str:          module.declare_function("ev_set_str",          Linkage::Import, &s_set_str).ok()?,
        set_enum_nullary: module.declare_function("ev_set_enum_nullary", Linkage::Import, &s_nullary).ok()?,
        set_enum_int:     module.declare_function("ev_set_enum_int",     Linkage::Import, &s_enum_int).ok()?,
        set_enum_str:     module.declare_function("ev_set_enum_str",     Linkage::Import, &s_enum_str).ok()?,
        set_enum_multifield: module.declare_function("ev_set_enum_multifield", Linkage::Import, &s_enum_mf).ok()?,
        set_composite:    module.declare_function("ev_set_composite",    Linkage::Import, &s_composite).ok()?,
        seq_new:          module.declare_function("ev_seq_new",          Linkage::Import, &s_seq_new).ok()?,
        seq_push_clone:   module.declare_function("ev_seq_push_clone",   Linkage::Import, &s_seq_push).ok()?,
        seq_set:          module.declare_function("ev_seq_set",          Linkage::Import, &s_seq_set).ok()?,
        load_int:         module.declare_function("ev_load_int",         Linkage::Import, &s_load_int).ok()?,
        load_bool:        module.declare_function("ev_load_bool",        Linkage::Import, &s_load_bool).ok()?,
        extract_field:    module.declare_function("ev_extract_field",    Linkage::Import, &s_extract).ok()?,
        seq_select:       module.declare_function("ev_seq_select",       Linkage::Import, &s_seq_sel).ok()?,
        is_variant:       module.declare_function("ev_is_variant",       Linkage::Import, &s_is_var).ok()?,
        clone_from_pool:  module.declare_function("ev_clone_from_pool",  Linkage::Import, &s_pool).ok()?,
    })
}

pub(super) fn import_helpers(
    module: &mut JITModule,
    ids: HelperIds,
    func: &mut cranelift::codegen::ir::Function,
) -> HelperRefs {
    HelperRefs {
        init_slot:        module.declare_func_in_func(ids.init_slot,        func),
        set_int:          module.declare_func_in_func(ids.set_int,          func),
        set_bool:         module.declare_func_in_func(ids.set_bool,         func),
        set_str:          module.declare_func_in_func(ids.set_str,          func),
        set_enum_nullary: module.declare_func_in_func(ids.set_enum_nullary, func),
        set_enum_int:     module.declare_func_in_func(ids.set_enum_int,     func),
        set_enum_str:     module.declare_func_in_func(ids.set_enum_str,     func),
        set_enum_multifield: module.declare_func_in_func(ids.set_enum_multifield, func),
        set_composite:    module.declare_func_in_func(ids.set_composite,    func),
        seq_new:          module.declare_func_in_func(ids.seq_new,          func),
        seq_push_clone:   module.declare_func_in_func(ids.seq_push_clone,   func),
        seq_set:          module.declare_func_in_func(ids.seq_set,          func),
        load_int:         module.declare_func_in_func(ids.load_int,         func),
        load_bool:        module.declare_func_in_func(ids.load_bool,        func),
        extract_field:    module.declare_func_in_func(ids.extract_field,    func),
        seq_select:       module.declare_func_in_func(ids.seq_select,       func),
        is_variant:       module.declare_func_in_func(ids.is_variant,       func),
        clone_from_pool:  module.declare_func_in_func(ids.clone_from_pool,  func),
    }
}

mod codegen;
pub use codegen::compile_program;

pub struct CraneliftFunctionizer;

impl CraneliftFunctionizer {
    pub fn compile(&self,
                   program:   &Z3Program,
                   enums:     &EnumRegistry,
                   datatypes: &crate::core::DatatypeRegistry)
        -> Option<std::rc::Rc<JitProgram>>
    {
        compile_program(program, enums, datatypes).map(std::rc::Rc::new)
    }
}
