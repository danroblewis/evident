use std::collections::HashMap;
use cranelift::prelude::{AbiParam, types};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use crate::encode::{EnumRegistry, Value};
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

/// Find the enum + field-types for an enum variant by name.
pub(super) fn lookup_variant(
    variant: &str,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
) -> Option<(String, Vec<String>)> {
    for (en, vs) in variant_arity {
        if let Some(fields) = vs.get(variant) {
            return Some((en.clone(), fields.clone()));
        }
    }
    None
}

// ===========================================================================
// C-ABI value builders — the helper fns the JIT codegen declares + calls.
// (was value_builders.rs)
// ===========================================================================

unsafe fn str_from_raw<'a>(ptr: *const u8, len: usize) -> &'a str {
    let bytes = std::slice::from_raw_parts(ptr, len);
    std::str::from_utf8_unchecked(bytes)
}

#[no_mangle]
pub unsafe extern "C" fn ev_init_slot(out: *mut Value) {
    std::ptr::write(out, Value::Int(0));
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_int(out: *mut Value, n: i64) {
    *out = Value::Int(n);
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_bool(out: *mut Value, b: i64) {
    *out = Value::Bool(b != 0);
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_str(out: *mut Value, s_ptr: *const u8, s_len: usize) {
    let s = str_from_raw(s_ptr, s_len);
    *out = Value::Str(s.to_string());
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_enum_nullary(
    out: *mut Value,
    enum_ptr: *const u8, enum_len: usize,
    variant_ptr: *const u8, variant_len: usize,
) {
    let enum_name = str_from_raw(enum_ptr, enum_len).to_string();
    let variant   = str_from_raw(variant_ptr, variant_len).to_string();
    *out = Value::Enum { enum_name, variant, fields: vec![] };
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_enum_int(
    out: *mut Value,
    enum_ptr: *const u8, enum_len: usize,
    variant_ptr: *const u8, variant_len: usize,
    payload: i64,
) {
    let enum_name = str_from_raw(enum_ptr, enum_len).to_string();
    let variant   = str_from_raw(variant_ptr, variant_len).to_string();
    *out = Value::Enum {
        enum_name,
        variant,
        fields: vec![Value::Int(payload)],
    };
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_enum_str(
    out: *mut Value,
    enum_ptr: *const u8, enum_len: usize,
    variant_ptr: *const u8, variant_len: usize,
    payload_ptr: *const u8, payload_len: usize,
) {
    let enum_name = str_from_raw(enum_ptr, enum_len).to_string();
    let variant   = str_from_raw(variant_ptr, variant_len).to_string();
    let payload   = str_from_raw(payload_ptr, payload_len).to_string();
    *out = Value::Enum {
        enum_name,
        variant,
        fields: vec![Value::Str(payload)],
    };
}

#[no_mangle]
pub unsafe extern "C" fn ev_seq_new(out: *mut Value, cap: usize) {
    *out = Value::SeqEnum(Vec::with_capacity(cap));
}

#[no_mangle]
pub unsafe extern "C" fn ev_seq_set(seq: *mut Value, idx: i64, elem: *const Value) {
    let elem = (*elem).clone();
    if let Value::SeqEnum(v) = &mut *seq {
        let i = idx.max(0) as usize;
        if i >= v.len() { v.resize(i + 1, Value::Int(0)); }
        v[i] = elem;
    } else {
        eprintln!("ev_seq_set: target is not a SeqEnum: {:?}", *seq);
    }
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_composite(
    out: *mut Value,
    names_ptr: *const *const u8, name_lens_ptr: *const usize,
    vals_ptr: *const *const Value, n: usize,
) {
    let name_ptrs = std::slice::from_raw_parts(names_ptr, n);
    let name_lens = std::slice::from_raw_parts(name_lens_ptr, n);
    let val_ptrs  = std::slice::from_raw_parts(vals_ptr, n);
    let mut map: HashMap<String, Value> = HashMap::with_capacity(n);
    for i in 0..n {
        let name = str_from_raw(name_ptrs[i], name_lens[i]).to_string();
        map.insert(name, (*val_ptrs[i]).clone());
    }
    *out = Value::Composite(map);
}

#[no_mangle]
pub unsafe extern "C" fn ev_seq_push_clone(seq: *mut Value, elem: *const Value) {
    let elem = (*elem).clone();
    if let Value::SeqEnum(v) = &mut *seq {
        v.push(elem);
    } else {
        eprintln!("ev_seq_push_clone: target is not a SeqEnum: {:?}", *seq);
    }
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_enum_multifield(
    out: *mut Value,
    enum_ptr: *const u8, enum_len: usize,
    variant_ptr: *const u8, variant_len: usize,
    args_ptr: *const *const Value, args_len: usize,
) {
    let enum_name = str_from_raw(enum_ptr, enum_len).to_string();
    let variant   = str_from_raw(variant_ptr, variant_len).to_string();
    let slice = std::slice::from_raw_parts(args_ptr, args_len);
    let mut fields: Vec<Value> = Vec::with_capacity(args_len);
    for p in slice {
        fields.push((**p).clone());
    }

    let is_cell = variant.starts_with("__Cell_") || variant.starts_with("__Empty_");
    if !is_cell {
        for f in fields.iter_mut() {
            if let Some(flat) = flatten_seq_of_chain(f) { *f = flat; }
        }
    }
    *out = Value::Enum { enum_name, variant, fields };
}

fn flatten_seq_of_chain(v: &Value) -> Option<Value> {
    let Value::Enum { enum_name, .. } = v else { return None };
    if !enum_name.starts_with("__SeqOf_") { return None; }
    let mut out: Vec<Value> = Vec::new();
    let mut cur = v;
    loop {
        let Value::Enum { variant, fields, .. } = cur else { return None };
        if variant.starts_with("__Empty_") { break; }
        if !variant.starts_with("__Cell_") { return None; }
        if fields.len() != 2 { return None; }
        let mut head = fields[0].clone();
        if let Value::Enum { variant: hv, fields: hf, .. } = &mut head {
            if !hv.starts_with("__Cell_") && !hv.starts_with("__Empty_") {
                for f in hf.iter_mut() {
                    if let Some(flat) = flatten_seq_of_chain(f) { *f = flat; }
                }
            }
        }
        out.push(head);
        cur = &fields[1];
    }

    Some(match out.first() {
        None => Value::SeqEnum(vec![]),
        Some(Value::Int(_)) => Value::SeqInt(out.into_iter().filter_map(|v|
            if let Value::Int(n) = v { Some(n) } else { None }).collect()),
        Some(Value::Bool(_)) => Value::SeqBool(out.into_iter().filter_map(|v|
            if let Value::Bool(b) = v { Some(b) } else { None }).collect()),
        Some(Value::Str(_)) => Value::SeqStr(out.into_iter().filter_map(|v|
            if let Value::Str(s) = v { Some(s) } else { None }).collect()),
        _ => Value::SeqEnum(out),
    })
}

#[no_mangle]
pub unsafe extern "C" fn ev_clone_from_pool(
    out: *mut Value,
    pool_ptr: *const Value,
    index: usize,
) {
    let src = &*pool_ptr.add(index);
    *out = src.clone();
}

#[no_mangle]
pub unsafe extern "C" fn ev_load_int(slot: *const Value) -> i64 {
    match &*slot {
        Value::Int(n) => *n,
        _ => 0,
    }
}

pub fn symbol_table() -> Vec<(&'static str, *const u8)> {
    vec![
        ("ev_init_slot",        ev_init_slot        as *const u8),
        ("ev_set_int",          ev_set_int          as *const u8),
        ("ev_set_bool",         ev_set_bool         as *const u8),
        ("ev_set_str",          ev_set_str          as *const u8),
        ("ev_set_enum_nullary", ev_set_enum_nullary as *const u8),
        ("ev_set_enum_int",     ev_set_enum_int     as *const u8),
        ("ev_set_enum_str",     ev_set_enum_str     as *const u8),
        ("ev_seq_new",          ev_seq_new          as *const u8),
        ("ev_seq_push_clone",   ev_seq_push_clone   as *const u8),
        ("ev_seq_set",          ev_seq_set          as *const u8),
        ("ev_set_composite",    ev_set_composite    as *const u8),
        ("ev_load_int",         ev_load_int         as *const u8),
        ("ev_set_enum_multifield", ev_set_enum_multifield as *const u8),
        ("ev_clone_from_pool",  ev_clone_from_pool  as *const u8),
        ("ev_seq_extract_field", ev_seq_extract_field as *const u8),
        ("ev_extract_field",    ev_extract_field    as *const u8),
        ("ev_seq_select",       ev_seq_select       as *const u8),
        ("ev_load_bool",        ev_load_bool        as *const u8),
        ("ev_is_variant",       ev_is_variant       as *const u8),
    ]
}

#[no_mangle]
pub unsafe extern "C" fn ev_load_bool(slot: *const Value) -> i64 {
    match &*slot {
        Value::Bool(b) => if *b { 1 } else { 0 },
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn ev_extract_field(
    out: *mut Value,
    src_slot: *const Value,
    name_ptr: *const u8, name_len: usize,
) {
    let name = str_from_raw(name_ptr, name_len);
    match &*src_slot {
        Value::Enum { fields, .. } => {

            if let Some(idx_str) = name.strip_prefix('f') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let Some(v) = fields.get(idx) {
                        *out = v.clone();
                        return;
                    }
                }
            }

            *out = Value::Int(0);
        }
        Value::Composite(map) => {
            if let Some(v) = map.get(name) {
                *out = v.clone();
            } else {
                *out = Value::Int(0);
            }
        }
        _ => { *out = Value::Int(0); }
    }
}

#[no_mangle]
pub unsafe extern "C" fn ev_seq_extract_field(
    out: *mut Value,
    src_slot: *const Value,
    name_ptr: *const u8, name_len: usize,
) {
    ev_extract_field(out, src_slot, name_ptr, name_len);
}

#[no_mangle]
pub unsafe extern "C" fn ev_seq_select(
    out: *mut Value,
    arr_slot: *const Value,
    idx: i64,
) {
    let i = idx as usize;
    let v = match &*arr_slot {
        Value::SeqEnum(xs) => xs.get(i).cloned(),
        Value::SeqInt(xs)  => xs.get(i).map(|n| Value::Int(*n)),
        Value::SeqBool(xs) => xs.get(i).map(|b| Value::Bool(*b)),
        Value::SeqStr(xs)  => xs.get(i).map(|s| Value::Str(s.clone())),
        Value::SeqComposite(xs) => xs.get(i).map(|m| Value::Composite(m.clone())),
        _ => None,
    }.unwrap_or(Value::Int(0));
    *out = v;
}

#[no_mangle]
pub unsafe extern "C" fn ev_is_variant(
    src_slot: *const Value,
    target_ptr: *const u8, target_len: usize,
) -> i64 {
    let target = str_from_raw(target_ptr, target_len);
    if let Value::Enum { variant, .. } = &*src_slot {
        if variant == target { 1 } else { 0 }
    } else { 0 }
}
