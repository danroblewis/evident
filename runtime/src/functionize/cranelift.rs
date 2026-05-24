//! Cranelift JIT codegen for Z3-AST function-shaped components.
//!
//! Round 26: Seq outputs + payload-bearing constructors. The JIT
//! emits a sequence of `call_indirect` instructions to Rust-side
//! helpers (see `value_builders.rs`) that construct `Value`
//! enums and push them into `Value::SeqEnum` outputs.
//!
//! ## Calling convention
//!
//! ```text
//!   extern "C" fn(inputs: *const i64, outputs: *mut Value)
//! ```
//!
//! - `inputs`: flat array of i64 values packed by the Rust
//!   wrapper from the caller's `given` map. Supports Int,
//!   Bool, and 0-arity enum (variant tag) inputs.
//! - `outputs`: pre-allocated `Vec<Value>` with one slot per
//!   output. The JIT writes into each slot via `ev_*` helpers
//!   (`ev_set_int`, `ev_set_enum_str`, `ev_seq_new`, etc.).
//!
//! For Seq outputs, the JIT calls `ev_seq_new(slot, cap)` then
//! builds each element into a stack-allocated temp `Value` slot
//! and calls `ev_seq_push_clone(slot, temp)`. The runtime owns
//! all heap allocations; the JIT just orchestrates the calls.

use std::collections::HashMap;
use cranelift::prelude::{AbiParam, FunctionBuilder, FunctionBuilderContext,
    InstBuilder, MemFlags, settings, types, StackSlotData, StackSlotKind};
use cranelift::prelude::Value as ClValue;
use cranelift::prelude::settings::Configurable;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};
use z3::ast::{Ast, Dynamic};
use z3::AstKind;
use z3_sys::DeclKind;

use crate::translate::{EnumRegistry, Value};
use crate::z3_eval::{Z3Program, Z3Step, GuardedBody};

pub struct JitProgram {
    _module: JITModule,
    func: unsafe extern "C" fn(*const Value, *mut Value, *const Value),
    pub input_offsets: HashMap<String, usize>,
    pub input_kinds:   HashMap<String, OutputKind>,
    pub output_offsets: HashMap<String, usize>,
    pub output_kinds:   HashMap<String, OutputKind>,
    pub enum_tags:     HashMap<String, HashMap<String, i64>>,
    pub enum_variants: HashMap<String, Vec<String>>,
    /// Interned strings kept alive for the lifetime of the JIT
    /// code (the compiled function holds raw pointers into them).
    _string_pool: Vec<Box<str>>,
    /// Compile-time constant Values for PreBaked steps + other
    /// constant emissions. Kept alive for the JIT module's lifetime;
    /// the JIT emits pointers into this pool.
    value_pool: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputKind {
    Int,
    Bool,
    /// All-nullary enum: encoded as i64 variant tag for inputs.
    /// Outputs use ev_set_enum_nullary.
    Enum(String),
    /// Payload-bearing enum: outputs use ev_set_enum_int/str.
    EnumPayload(String),
    /// Seq output: uses ev_seq_new + ev_seq_push_clone.
    Seq,
    /// String value (only as an output / intermediate; we don't
    /// support String inputs through the i64 ABI in v1).
    Str,
}

impl JitProgram {
    pub fn call(&self, given: &HashMap<String, Value>) -> Option<HashMap<String, Value>> {
        let n_in  = self.input_offsets.len();
        let n_out = self.output_offsets.len();
        // Build input Value array — one slot per input, populated
        // from `given`. Missing inputs use Value::Int(0) sentinel;
        // the JIT only loads inputs it knows about.
        let mut inputs: Vec<Value> = (0..n_in).map(|_| Value::Int(0)).collect();
        for (name, &idx) in &self.input_offsets {
            if let Some(v) = given.get(name) {
                inputs[idx] = v.clone();
            }
        }
        // Initialize each output slot to a default Value::Int(0)
        // before the JIT runs (helpers do `*out = ...` which drops
        // the prior valid value).
        let mut outputs: Vec<Value> = (0..n_out).map(|_| Value::Int(0)).collect();
        let pool_ptr = if self.value_pool.is_empty() {
            std::ptr::null()
        } else {
            self.value_pool.as_ptr()
        };
        // SAFETY: compiled code is alive as long as `_module`;
        // both arrays are valid `Vec<Value>` of the declared size;
        // value_pool outlives the JIT module (stored alongside).
        unsafe {
            (self.func)(inputs.as_ptr(), outputs.as_mut_ptr(), pool_ptr);
        }
        let mut out = HashMap::new();
        for (name, &idx) in &self.output_offsets {
            let v = outputs[idx].clone();
            out.insert(name.clone(), classify_seq(v));
        }
        if std::env::var("EVIDENT_JIT_CALL_TRACE").is_ok() {
            eprintln!("[jit/call] result:");
            for (k, v) in &out {
                eprintln!("    {k} = {v:?}");
            }
        }
        Some(out)
    }
}

/// Reclassify a `Value::SeqEnum` of homogeneous primitives into the
/// matching typed `SeqInt` / `SeqBool` / `SeqStr` variant. The JIT
/// always builds Seq outputs as `SeqEnum` (no static type info on
/// the IR side); callers compare against typed variants per the
/// declared Seq element type.
fn classify_seq(v: Value) -> Value {
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
        _ => Value::SeqEnum(xs),
    }
}

/// FuncIds for the runtime helpers — declared as Linkage::Import.
#[derive(Clone, Copy)]
struct HelperIds {
    init_slot:         FuncId,
    set_int:           FuncId,
    set_bool:          FuncId,
    set_str:           FuncId,
    set_enum_nullary:  FuncId,
    set_enum_int:      FuncId,
    set_enum_str:      FuncId,
    set_enum_multifield: FuncId,
    seq_new:           FuncId,
    seq_push_clone:    FuncId,
    load_int:          FuncId,
    load_bool:         FuncId,
    extract_field:     FuncId,
    seq_select:        FuncId,
    str_concat:        FuncId,
    is_variant:        FuncId,
    clone_from_pool:   FuncId,
}

/// FuncRefs after import into the current function's IR.
#[derive(Clone, Copy)]
struct HelperRefs {
    init_slot:         cranelift::codegen::ir::FuncRef,
    set_int:           cranelift::codegen::ir::FuncRef,
    set_bool:          cranelift::codegen::ir::FuncRef,
    set_str:           cranelift::codegen::ir::FuncRef,
    set_enum_nullary:  cranelift::codegen::ir::FuncRef,
    set_enum_int:      cranelift::codegen::ir::FuncRef,
    set_enum_str:      cranelift::codegen::ir::FuncRef,
    set_enum_multifield: cranelift::codegen::ir::FuncRef,
    seq_new:           cranelift::codegen::ir::FuncRef,
    seq_push_clone:    cranelift::codegen::ir::FuncRef,
    load_int:          cranelift::codegen::ir::FuncRef,
    load_bool:         cranelift::codegen::ir::FuncRef,
    extract_field:     cranelift::codegen::ir::FuncRef,
    seq_select:        cranelift::codegen::ir::FuncRef,
    str_concat:        cranelift::codegen::ir::FuncRef,
    is_variant:        cranelift::codegen::ir::FuncRef,
    clone_from_pool:   cranelift::codegen::ir::FuncRef,
}

fn declare_helpers(
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
    let s_seq_new  = mk(&[p, usz], module);
    let s_seq_push = mk(&[p, p], module);
    let s_load_int = mk_ret(&[p], i64t, module);
    let s_load_bool = mk_ret(&[p], i64t, module);
    let s_extract = mk(&[p, p, p, usz], module);
    let s_seq_sel = mk(&[p, p, i64t], module);
    let s_str_cat = mk(&[p, p, usz], module);
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
        seq_new:          module.declare_function("ev_seq_new",          Linkage::Import, &s_seq_new).ok()?,
        seq_push_clone:   module.declare_function("ev_seq_push_clone",   Linkage::Import, &s_seq_push).ok()?,
        load_int:         module.declare_function("ev_load_int",         Linkage::Import, &s_load_int).ok()?,
        load_bool:        module.declare_function("ev_load_bool",        Linkage::Import, &s_load_bool).ok()?,
        extract_field:    module.declare_function("ev_extract_field",    Linkage::Import, &s_extract).ok()?,
        seq_select:       module.declare_function("ev_seq_select",       Linkage::Import, &s_seq_sel).ok()?,
        str_concat:       module.declare_function("ev_str_concat",       Linkage::Import, &s_str_cat).ok()?,
        is_variant:       module.declare_function("ev_is_variant",       Linkage::Import, &s_is_var).ok()?,
        clone_from_pool:  module.declare_function("ev_clone_from_pool",  Linkage::Import, &s_pool).ok()?,
    })
}

fn import_helpers(
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
        seq_new:          module.declare_func_in_func(ids.seq_new,          func),
        seq_push_clone:   module.declare_func_in_func(ids.seq_push_clone,   func),
        load_int:         module.declare_func_in_func(ids.load_int,         func),
        load_bool:        module.declare_func_in_func(ids.load_bool,        func),
        extract_field:    module.declare_func_in_func(ids.extract_field,    func),
        seq_select:       module.declare_func_in_func(ids.seq_select,       func),
        str_concat:       module.declare_func_in_func(ids.str_concat,       func),
        is_variant:       module.declare_func_in_func(ids.is_variant,       func),
        clone_from_pool:  module.declare_func_in_func(ids.clone_from_pool,  func),
    }
}

pub fn compile_program<'ctx>(
    program: &Z3Program<'ctx>,
    enums: &EnumRegistry,
) -> Option<JitProgram> {
    // ── Phase 1: enum tables ──────────────────────────────
    let mut enum_tags: HashMap<String, HashMap<String, i64>> = HashMap::new();
    let mut enum_variants: HashMap<String, Vec<String>> = HashMap::new();
    let mut variant_arity: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
    {
        let by_name = enums.by_name.borrow();
        for (enum_name, (_dt, variants)) in by_name.iter() {
            let mut tags = HashMap::new();
            let mut names = Vec::with_capacity(variants.len());
            let mut arities = HashMap::new();
            for (idx, v) in variants.iter().enumerate() {
                tags.insert(v.name.clone(), idx as i64);
                names.push(v.name.clone());
                arities.insert(v.name.clone(),
                    v.fields.iter().map(|f| f.type_name.clone()).collect());
            }
            enum_tags.insert(enum_name.clone(), tags);
            enum_variants.insert(enum_name.clone(), names);
            variant_arity.insert(enum_name.clone(), arities);
        }
    }

    // ── Phase 2: output kinds + input collection ──────────
    let mut input_set: std::collections::BTreeSet<(String, OutputKind)> =
        std::collections::BTreeSet::new();
    let mut output_kinds_local: Vec<(String, OutputKind)> = Vec::new();
    for step in &program.steps {
        let (var, kind) = match step {
            Z3Step::Scalar { var, expr } => {
                let k = kind_of_dynamic(expr, &enum_variants, &variant_arity)
                    .unwrap_or(OutputKind::Int);
                (var.clone(), k)
            }
            Z3Step::Seq { var, .. } => (var.clone(), OutputKind::Seq),
            Z3Step::Guarded { var, .. } => {
                // Guarded steps require correctness around "no branch
                // matched" (currently the JIT writes a sentinel int,
                // which propagates as a wrong result to the scheduler).
                // The VM handles this correctly by returning None,
                // letting the function-izer fall through to the slow
                // path. Until the JIT can also produce a None-style
                // bailout, refuse to JIT programs with Guarded steps.
                if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                    eprintln!("[jit] bail: Guarded {var} (would JIT but \
                              falls through to VM for correctness)");
                }
                return None;
            }
            Z3Step::PreBaked { var, .. } => (var.clone(), OutputKind::Seq /* placeholder */),
        };
        output_kinds_local.push((var, kind));
        match step {
            Z3Step::Scalar { expr, .. } =>
                collect_inputs(expr, &mut input_set, &enum_variants, &variant_arity),
            Z3Step::Seq { elem_exprs, .. } =>
                for e in elem_exprs { collect_inputs(e, &mut input_set, &enum_variants, &variant_arity); },
            Z3Step::Guarded { branches, .. } => {
                for b in branches {
                    collect_inputs(&b.guard, &mut input_set, &enum_variants, &variant_arity);
                    match &b.body {
                        GuardedBody::Scalar(e) =>
                            collect_inputs(e, &mut input_set, &enum_variants, &variant_arity),
                        GuardedBody::Seq(es) =>
                            for e in es {
                                collect_inputs(e, &mut input_set, &enum_variants, &variant_arity);
                            },
                    }
                }
            }
            _ => {}
        }
    }
    let output_names: std::collections::HashSet<String> = output_kinds_local.iter()
        .map(|(n, _)| n.clone()).collect();
    // Allow ALL kinds as inputs now (Seq, Composite, etc. handled
    // via ev_load_* / ev_extract_field / ev_seq_select helpers).
    let input_names: Vec<(String, OutputKind)> = input_set.into_iter()
        .filter(|(n, _)| !output_names.contains(n))
        .collect();

    // ── Phase 3: Cranelift IR generation ──────────────────
    let mut flag_builder = settings::builder();
    flag_builder.set("use_colocated_libcalls", "false").ok()?;
    flag_builder.set("is_pic", "false").ok()?;
    let isa_builder = cranelift_native::builder().ok()?;
    let isa = isa_builder.finish(settings::Flags::new(flag_builder)).ok()?;
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    for (name, addr) in crate::value_builders::symbol_table() {
        builder.symbol(name, addr);
    }
    let mut module = JITModule::new(builder);
    let ptr_t = module.target_config().pointer_type();

    let helper_ids = declare_helpers(&mut module, ptr_t)?;

    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(ptr_t));   // *const Value inputs
    sig.params.push(AbiParam::new(ptr_t));   // *mut Value outputs
    sig.params.push(AbiParam::new(ptr_t));   // *const Value value_pool
    let func_id = module.declare_function("compiled_program",
        Linkage::Local, &sig).ok()?;
    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    // Import helpers into this function's IR scope BEFORE we
    // hand ctx.func to FunctionBuilder (since FunctionBuilder
    // takes a mutable borrow of it).
    let helpers = import_helpers(&mut module, helper_ids, &mut ctx.func);

    let input_offsets: HashMap<String, usize> = input_names.iter().enumerate()
        .map(|(i, (n, _))| (n.clone(), i)).collect();
    let input_kinds: HashMap<String, OutputKind> = input_names.iter().cloned().collect();
    let mut output_offsets: HashMap<String, usize> = HashMap::new();
    let mut output_kinds: HashMap<String, OutputKind> = HashMap::new();
    for (i, (name, kind)) in output_kinds_local.iter().enumerate() {
        output_offsets.insert(name.clone(), i);
        output_kinds.insert(name.clone(), kind.clone());
    }
    let size_of_value = std::mem::size_of::<Value>() as i64;

    let mut string_pool: Vec<Box<str>> = Vec::new();
    let mut value_pool: Vec<Value> = Vec::new();
    {
        let mut func_ctx = FunctionBuilderContext::new();
        let mut bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        let entry = bcx.create_block();
        bcx.append_block_params_for_function_params(entry);
        bcx.switch_to_block(entry);
        bcx.seal_block(entry);

        let inputs_ptr  = bcx.block_params(entry)[0];
        let outputs_ptr = bcx.block_params(entry)[1];
        let pool_ptr    = bcx.block_params(entry)[2];

        // Env: var name → ptr-to-Value slot. Inputs are at
        // (inputs_ptr + idx*sizeof(Value)). Outputs are at
        // (outputs_ptr + idx*sizeof(Value)). We compute the slot
        // address once and reuse.
        let mut env: HashMap<String, ClValue> = HashMap::new();
        for (name, idx) in &input_offsets {
            let off = (*idx as i64) * size_of_value;
            let off_v = bcx.ins().iconst(types::I64, off);
            let slot = bcx.ins().iadd(inputs_ptr, off_v);
            env.insert(name.clone(), slot);
        }

        for step in &program.steps {
            let out_idx = output_offsets[step.var()];
            let out_offset = (out_idx as i64) * size_of_value;
            let off_v = bcx.ins().iconst(types::I64, out_offset);
            let out_slot = bcx.ins().iadd(outputs_ptr, off_v);

            match step {
                Z3Step::Scalar { var, expr } => {
                    if emit_write_value(&mut bcx, expr, out_slot, &env,
                        &helpers, &variant_arity, &mut string_pool,
                        ptr_t, size_of_value).is_none() {
                        if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                            eprintln!("[jit] bail: Scalar {var} = {expr}");
                        }
                        return None;
                    }
                    env.insert(var.clone(), out_slot);
                }
                Z3Step::Seq { var, elem_exprs } => {
                    // Detect if all elements are scalar literals or
                    // env refs we can handle. Build via ev_seq_new +
                    // ev_seq_push_clone over per-element temp slots.
                    let cap = bcx.ins().iconst(types::I64, elem_exprs.len() as i64);
                    bcx.ins().call(helpers.seq_new, &[out_slot, cap]);
                    // Use a fresh temp slot per element to be safe
                    // about ev_seq_push_clone's read-and-clone semantics.
                    let temp_slot = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp_slot, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    for (ei, elem) in elem_exprs.iter().enumerate() {
                        if emit_write_value(&mut bcx, elem, temp_ptr, &env,
                            &helpers, &variant_arity, &mut string_pool,
                            ptr_t, size_of_value).is_none() {
                            if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                                eprintln!("[jit] bail: Seq {var}[{ei}] = {elem}");
                            }
                            return None;
                        }
                        bcx.ins().call(helpers.seq_push_clone, &[out_slot, temp_ptr]);
                    }
                    env.insert(var.clone(), out_slot);
                }
                Z3Step::Guarded { var, branches } => {
                    // Compile as a chain of conditional branches:
                    // for each (guard, body) in order, brif on guard.
                    // If guard fires, run body and jump to merge.
                    // Otherwise fall through to the next branch.
                    // If no branch matches, write Value::Int(0) as a
                    // sentinel (matches the VM's behavior on None
                    // body match, which returns None — but here we
                    // need to produce SOMETHING, so use a default).
                    let merge_block = bcx.create_block();
                    for branch in branches {
                        let body_block = bcx.create_block();
                        let next_block = bcx.create_block();
                        let cond_v = match emit_compute_i64(&mut bcx, &branch.guard, &env,
                            &helpers, &variant_arity, &mut string_pool, ptr_t, size_of_value)
                        {
                            Some(v) => v,
                            None => {
                                if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                                    eprintln!("[jit] bail: Guarded {var} guard");
                                }
                                return None;
                            }
                        };
                        bcx.ins().brif(cond_v, body_block, &[], next_block, &[]);
                        bcx.switch_to_block(body_block);
                        bcx.seal_block(body_block);
                        match &branch.body {
                            GuardedBody::Scalar(e) => {
                                if emit_write_value(&mut bcx, e, out_slot, &env,
                                    &helpers, &variant_arity, &mut string_pool,
                                    ptr_t, size_of_value).is_none()
                                {
                                    if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                                        eprintln!("[jit] bail: Guarded {var} scalar body");
                                    }
                                    return None;
                                }
                            }
                            GuardedBody::Seq(es) => {
                                let cap = bcx.ins().iconst(types::I64, es.len() as i64);
                                bcx.ins().call(helpers.seq_new, &[out_slot, cap]);
                                let temp_slot = bcx.create_sized_stack_slot(
                                    StackSlotData::new(StackSlotKind::ExplicitSlot,
                                                       size_of_value as u32));
                                let temp_ptr = bcx.ins().stack_addr(ptr_t, temp_slot, 0);
                                bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                                for e in es {
                                    if emit_write_value(&mut bcx, e, temp_ptr, &env,
                                        &helpers, &variant_arity, &mut string_pool,
                                        ptr_t, size_of_value).is_none()
                                    {
                                        if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                                            eprintln!("[jit] bail: Guarded {var} seq elem");
                                        }
                                        return None;
                                    }
                                    bcx.ins().call(helpers.seq_push_clone,
                                        &[out_slot, temp_ptr]);
                                }
                            }
                        }
                        bcx.ins().jump(merge_block, &[]);
                        bcx.switch_to_block(next_block);
                        bcx.seal_block(next_block);
                    }
                    // Default fallthrough — set Value::Int(0). The
                    // VM would return None here; we must produce a
                    // valid Value because the slot is already alive.
                    let zero = bcx.ins().iconst(types::I64, 0);
                    bcx.ins().call(helpers.set_int, &[out_slot, zero]);
                    bcx.ins().jump(merge_block, &[]);
                    bcx.switch_to_block(merge_block);
                    bcx.seal_block(merge_block);
                    env.insert(var.clone(), out_slot);
                }
                Z3Step::PreBaked { var, value } => {
                    let idx = value_pool.len();
                    value_pool.push(value.clone());
                    let idx_v = bcx.ins().iconst(types::I64, idx as i64);
                    bcx.ins().call(helpers.clone_from_pool,
                        &[out_slot, pool_ptr, idx_v]);
                    env.insert(var.clone(), out_slot);
                }
            }
        }
        bcx.ins().return_(&[]);
        bcx.finalize();
    }

    if std::env::var("EVIDENT_JIT_DUMP").is_ok() {
        eprintln!("[jit] IR for compiled_program:\n{}", ctx.func.display());
    }
    module.define_function(func_id, &mut ctx).ok()?;
    module.clear_context(&mut ctx);
    module.finalize_definitions().ok()?;
    let code_ptr = module.get_finalized_function(func_id);
    // SAFETY: code_ptr points to JIT'd machine code with the
    // ABI we declared above.
    let func: unsafe extern "C" fn(*const Value, *mut Value, *const Value) = unsafe {
        std::mem::transmute(code_ptr)
    };
    Some(JitProgram {
        _module: module,
        func,
        input_offsets,
        input_kinds,
        output_offsets,
        output_kinds,
        enum_tags,
        enum_variants,
        _string_pool: string_pool,
        value_pool,
    })
}

fn intern_str(pool: &mut Vec<Box<str>>, s: &str) -> (i64, i64) {
    let boxed: Box<str> = s.to_string().into_boxed_str();
    let ptr = boxed.as_ptr() as usize as i64;
    let len = boxed.len() as i64;
    pool.push(boxed);
    (ptr, len)
}

/// Emit IR that writes a Value derived from `expr` into the
/// memory at `out_slot`. Returns None if the expr uses a
/// pattern we don't yet emit code for.
fn emit_write_value<'ctx>(
    bcx: &mut FunctionBuilder,
    expr: &Dynamic<'ctx>,
    out_slot: ClValue,
    env: &HashMap<String, ClValue>,
    helpers: &HelperRefs,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
    string_pool: &mut Vec<Box<str>>,
    ptr_t: cranelift::prelude::Type,
    size_of_value: i64,
) -> Option<()> {
    // String literal short-circuit — only genuine zero-child literals.
    if expr.kind() == AstKind::App && expr.num_children() == 0 {
        let is_free_var = expr.safe_decl().ok()
            .map(|d| d.kind() == DeclKind::UNINTERPRETED)
            .unwrap_or(false);
        if !is_free_var {
            if let Some(zs) = expr.as_string() {
                if let Some(s) = zs.as_string() {
                    let (p, l) = intern_str(string_pool, &s);
                    let pv = bcx.ins().iconst(types::I64, p);
                    let lv = bcx.ins().iconst(types::I64, l);
                    bcx.ins().call(helpers.set_str, &[out_slot, pv, lv]);
                    return Some(());
                }
            }
        }
    }

    match expr.kind() {
        AstKind::Numeral => {
            let i = expr.as_int().and_then(|x| x.as_i64())?;
            let n = bcx.ins().iconst(types::I64, i);
            bcx.ins().call(helpers.set_int, &[out_slot, n]);
            Some(())
        }
        AstKind::App => {
            let decl = expr.safe_decl().ok()?;
            let kind = decl.kind();
            let children: Vec<Dynamic<'ctx>> = expr.children();
            match kind {
                DeclKind::TRUE => {
                    let n = bcx.ins().iconst(types::I64, 1);
                    bcx.ins().call(helpers.set_bool, &[out_slot, n]);
                    Some(())
                }
                DeclKind::FALSE => {
                    let n = bcx.ins().iconst(types::I64, 0);
                    bcx.ins().call(helpers.set_bool, &[out_slot, n]);
                    Some(())
                }
                DeclKind::UNINTERPRETED => {
                    if children.is_empty() {
                        // Variable lookup: copy from env slot via clone helper.
                        let name = decl.name();
                        let src_slot = *env.get(&name)?;
                        let zero = bcx.ins().iconst(types::I64, 0);
                        bcx.ins().call(helpers.clone_from_pool,
                            &[out_slot, src_slot, zero]);
                        Some(())
                    } else if children.len() == 1 {
                        // Z3 internal accessor: `<field>__arr` /
                        // `<field>__len` — strip suffix to get logical
                        // field name, then extract by name.
                        let name = decl.name();
                        let logical = if let Some(s) = name.strip_suffix("__arr") {
                            s.to_string()
                        } else if let Some(_s) = name.strip_suffix("__len") {
                            return None;  // length extraction not emitted
                        } else { return None; };
                        let temp = bcx.create_sized_stack_slot(
                            StackSlotData::new(StackSlotKind::ExplicitSlot,
                                               size_of_value as u32));
                        let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                        bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                        emit_write_value(bcx, &children[0], temp_ptr, env,
                            helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                        let (np, nl) = intern_str(string_pool, &logical);
                        let np_v = bcx.ins().iconst(types::I64, np);
                        let nl_v = bcx.ins().iconst(types::I64, nl);
                        bcx.ins().call(helpers.extract_field,
                            &[out_slot, temp_ptr, np_v, nl_v]);
                        Some(())
                    } else {
                        None
                    }
                }
                DeclKind::DT_ACCESSOR => {
                    if children.len() != 1 { return None; }
                    let raw = decl.name();
                    // Strip Z3 internal suffixes (`__arr` / `__len`)
                    // — the Value-level field lookup uses the logical
                    // name (e.g. "effs"), not the Z3-internal split
                    // accessor name ("effs__arr").
                    let accessor_name = raw.strip_suffix("__arr")
                        .or_else(|| raw.strip_suffix("__len"))
                        .map(|s| s.to_string())
                        .unwrap_or(raw);
                    let temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    emit_write_value(bcx, &children[0], temp_ptr, env,
                        helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                    let (np, nl) = intern_str(string_pool, &accessor_name);
                    let np_v = bcx.ins().iconst(types::I64, np);
                    let nl_v = bcx.ins().iconst(types::I64, nl);
                    bcx.ins().call(helpers.extract_field,
                        &[out_slot, temp_ptr, np_v, nl_v]);
                    Some(())
                }
                DeclKind::DT_IS | DeclKind::DT_RECOGNISER => {
                    if children.len() != 1 { return None; }
                    // The variant being tested is encoded in the decl
                    // name. Z3 0.12 doesn't expose the constructor
                    // parameter directly; parse from app's text form.
                    let app_text = format!("{expr}");
                    let variant = crate::z3_eval::extract_is_variant_pub(&app_text)
                        .or_else(|| decl.name().strip_prefix("is_").map(|s| s.to_string()))?;
                    let temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    emit_write_value(bcx, &children[0], temp_ptr, env,
                        helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                    let (vp, vl) = intern_str(string_pool, &variant);
                    let vp_v = bcx.ins().iconst(types::I64, vp);
                    let vl_v = bcx.ins().iconst(types::I64, vl);
                    let call = bcx.ins().call(helpers.is_variant,
                        &[temp_ptr, vp_v, vl_v]);
                    let r = bcx.inst_results(call)[0];
                    bcx.ins().call(helpers.set_bool, &[out_slot, r]);
                    Some(())
                }
                DeclKind::SELECT => {
                    if children.len() != 2 {
                        if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                            eprintln!("[jit] SELECT children != 2: {expr}");
                        }
                        return None;
                    }
                    let temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    if emit_write_value(bcx, &children[0], temp_ptr, env,
                        helpers, variant_arity, string_pool, ptr_t, size_of_value).is_none()
                    {
                        if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                            eprintln!("[jit] SELECT arr bail: {}", &children[0]);
                        }
                        return None;
                    }
                    let idx_v = match emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)
                    {
                        Some(v) => v,
                        None => {
                            if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                                eprintln!("[jit] SELECT idx bail: {}", &children[1]);
                            }
                            return None;
                        }
                    };
                    bcx.ins().call(helpers.seq_select,
                        &[out_slot, temp_ptr, idx_v]);
                    Some(())
                }
                DeclKind::DT_CONSTRUCTOR => {
                    let variant = decl.name();
                    // Check Cons-chain pattern at this level — handled
                    // by the caller's Seq build, not here.
                    let (enum_name, field_types) = lookup_variant(&variant, variant_arity)?;
                    let (ep, el) = intern_str(string_pool, &enum_name);
                    let (vp, vl) = intern_str(string_pool, &variant);
                    let ep_v = bcx.ins().iconst(types::I64, ep);
                    let el_v = bcx.ins().iconst(types::I64, el);
                    let vp_v = bcx.ins().iconst(types::I64, vp);
                    let vl_v = bcx.ins().iconst(types::I64, vl);
                    if field_types.is_empty() {
                        bcx.ins().call(helpers.set_enum_nullary,
                            &[out_slot, ep_v, el_v, vp_v, vl_v]);
                        return Some(());
                    }
                    if field_types.len() == 1 {
                        let arg = &children[0];
                        match field_types[0].as_str() {
                            "Int" | "Nat" => {
                                if let Some(n) = arg.as_int().and_then(|x| x.as_i64()) {
                                    let n_v = bcx.ins().iconst(types::I64, n);
                                    bcx.ins().call(helpers.set_enum_int,
                                        &[out_slot, ep_v, el_v, vp_v, vl_v, n_v]);
                                    return Some(());
                                }
                                // Computed Int payload (e.g. enum(_frame + 1)):
                                // build via multifield path.
                            }
                            "String" => {
                                // Only fast-path for literal strings —
                                // free var as_string() returns Some("").
                                let is_literal = arg.kind() == AstKind::App
                                    && arg.num_children() == 0
                                    && arg.safe_decl().ok()
                                        .map(|d| d.kind() != DeclKind::UNINTERPRETED)
                                        .unwrap_or(false);
                                if is_literal {
                                    if let Some(zs) = arg.as_string() {
                                        if let Some(s) = zs.as_string() {
                                            let (p, l) = intern_str(string_pool, &s);
                                            let p_v = bcx.ins().iconst(types::I64, p);
                                            let l_v = bcx.ins().iconst(types::I64, l);
                                            bcx.ins().call(helpers.set_enum_str,
                                                &[out_slot, ep_v, el_v, vp_v, vl_v, p_v, l_v]);
                                            return Some(());
                                        }
                                    }
                                }
                                // Fall through to multifield path for
                                // non-literal String fields.
                            }
                            _ => {}
                        }
                    }
                    // Multi-field (or single computed-field) ctor:
                    // build each field into a stack slot, then call
                    // ev_set_enum_multifield with an array of slot ptrs.
                    let n = children.len();
                    // Allocate stack slots for each arg + an array of
                    // pointers.
                    let arg_slots: Vec<ClValue> = (0..n).map(|_| {
                        let s = bcx.create_sized_stack_slot(
                            StackSlotData::new(StackSlotKind::ExplicitSlot,
                                               size_of_value as u32));
                        bcx.ins().stack_addr(ptr_t, s, 0)
                    }).collect();
                    for s in &arg_slots {
                        bcx.ins().call(helpers.init_slot, &[*s]);
                    }
                    // Recursively emit each field.
                    for (i, child) in children.iter().enumerate() {
                        emit_write_value(bcx, child, arg_slots[i], env,
                            helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                    }
                    // Build the pointer array on the stack.
                    let array_slot = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           (n as u32) * 8));
                    let array_ptr = bcx.ins().stack_addr(ptr_t, array_slot, 0);
                    for (i, &s) in arg_slots.iter().enumerate() {
                        bcx.ins().store(MemFlags::new(),
                            s, array_ptr, (i as i32) * 8);
                    }
                    let n_v = bcx.ins().iconst(types::I64, n as i64);
                    bcx.ins().call(helpers.set_enum_multifield,
                        &[out_slot, ep_v, el_v, vp_v, vl_v, array_ptr, n_v]);
                    Some(())
                }
                DeclKind::ITE => {
                    // ITE: cond is Bool, then/else are Value.
                    if children.len() != 3 { return None; }
                    let cond_v = emit_compute_i64(bcx, &children[0], env,
                        helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                    let then_block = bcx.create_block();
                    let else_block = bcx.create_block();
                    let merge_block = bcx.create_block();
                    bcx.ins().brif(cond_v, then_block, &[], else_block, &[]);
                    bcx.switch_to_block(then_block);
                    bcx.seal_block(then_block);
                    emit_write_value(bcx, &children[1], out_slot, env,
                        helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().jump(merge_block, &[]);
                    bcx.switch_to_block(else_block);
                    bcx.seal_block(else_block);
                    emit_write_value(bcx, &children[2], out_slot, env,
                        helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().jump(merge_block, &[]);
                    bcx.switch_to_block(merge_block);
                    bcx.seal_block(merge_block);
                    Some(())
                }
                DeclKind::ADD | DeclKind::SUB | DeclKind::MUL | DeclKind::UMINUS => {
                    // Int arithmetic → set_int with computed i64.
                    let v = emit_compute_i64(bcx, expr, env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().call(helpers.set_int, &[out_slot, v]);
                    Some(())
                }
                DeclKind::LT | DeclKind::LE | DeclKind::GT | DeclKind::GE
                | DeclKind::EQ | DeclKind::AND | DeclKind::OR | DeclKind::NOT => {
                    // Bool ops → set_bool with computed i64 (0/1).
                    let v = emit_compute_i64(bcx, expr, env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().call(helpers.set_bool, &[out_slot, v]);
                    Some(())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Emit IR that computes an i64 value from `expr`. Used for Int
/// arithmetic operands, Bool conditions, comparison operands.
/// Returns None if the expr can't be reduced to a single i64.
fn emit_compute_i64<'ctx>(
    bcx: &mut FunctionBuilder,
    expr: &Dynamic<'ctx>,
    env: &HashMap<String, ClValue>,
    helpers: &HelperRefs,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
    string_pool: &mut Vec<Box<str>>,
    ptr_t: cranelift::prelude::Type,
    size_of_value: i64,
) -> Option<ClValue> {
    match expr.kind() {
        AstKind::Numeral => {
            let i = expr.as_int().and_then(|x| x.as_i64())?;
            Some(bcx.ins().iconst(types::I64, i))
        }
        AstKind::App => {
            let decl = expr.safe_decl().ok()?;
            let kind = decl.kind();
            let children: Vec<Dynamic<'ctx>> = expr.children();
            match kind {
                DeclKind::TRUE  => Some(bcx.ins().iconst(types::I64, 1)),
                DeclKind::FALSE => Some(bcx.ins().iconst(types::I64, 0)),
                DeclKind::UNINTERPRETED => {
                    if !children.is_empty() { return None; }
                    let name = decl.name();
                    let src_slot = *env.get(&name)?;
                    // Pick loader based on the sort: Bool → load_bool,
                    // else load_int. Sort detection is via the Z3 sort
                    // string on the expr.
                    let sort_name = format!("{}", expr.get_sort());
                    let loader = if sort_name == "Bool" {
                        helpers.load_bool
                    } else {
                        helpers.load_int
                    };
                    let call = bcx.ins().call(loader, &[src_slot]);
                    let result = bcx.inst_results(call)[0];
                    Some(result)
                }
                DeclKind::ADD | DeclKind::SUB | DeclKind::MUL => {
                    if children.is_empty() { return None; }
                    let mut acc = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    for c in &children[1..] {
                        let v = emit_compute_i64(bcx, c, env, helpers,
                            variant_arity, string_pool, ptr_t, size_of_value)?;
                        acc = match kind {
                            DeclKind::ADD => bcx.ins().iadd(acc, v),
                            DeclKind::SUB => bcx.ins().isub(acc, v),
                            DeclKind::MUL => bcx.ins().imul(acc, v),
                            _ => unreachable!(),
                        };
                    }
                    Some(acc)
                }
                DeclKind::UMINUS => {
                    if children.len() != 1 { return None; }
                    let v = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().ineg(v))
                }
                DeclKind::IDIV | DeclKind::DIV => {
                    if children.len() != 2 { return None; }
                    let l = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    let r = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().sdiv(l, r))
                }
                DeclKind::MOD | DeclKind::REM => {
                    if children.len() != 2 { return None; }
                    let l = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    let r = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().srem(l, r))
                }
                DeclKind::LT | DeclKind::LE | DeclKind::GT | DeclKind::GE
                | DeclKind::EQ => {
                    if children.len() != 2 { return None; }
                    // Special case: `(= X NullaryVariant)` — compile
                    // as IsVariant test on X. Lets the JIT handle
                    // enum equality without needing a full Value
                    // equality helper.
                    if matches!(kind, DeclKind::EQ) {
                        let try_nullary_eq = |child: &Dynamic<'ctx>, other: &Dynamic<'ctx>|
                            -> Option<ClValue>
                        {
                            if child.kind() == AstKind::App {
                                let d = child.safe_decl().ok()?;
                                if d.kind() == DeclKind::DT_CONSTRUCTOR
                                    && child.num_children() == 0
                                {
                                    let variant = d.name();
                                    // We need a mutable bcx + helpers here; this
                                    // closure borrows mutably so we inline below.
                                    let _ = variant;
                                    return Some(ClValue::from_u32(0));  // sentinel
                                }
                            }
                            None
                        };
                        if try_nullary_eq(&children[1], &children[0]).is_some() {
                            // r is the nullary variant; test (is variant l).
                            let variant = children[1].safe_decl().ok()?.name();
                            let temp = bcx.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot,
                                                   size_of_value as u32));
                            let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                            bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                            emit_write_value(bcx, &children[0], temp_ptr, env,
                                helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                            let (vp, vl) = intern_str(string_pool, &variant);
                            let vp_v = bcx.ins().iconst(types::I64, vp);
                            let vl_v = bcx.ins().iconst(types::I64, vl);
                            let call = bcx.ins().call(helpers.is_variant,
                                &[temp_ptr, vp_v, vl_v]);
                            return Some(bcx.inst_results(call)[0]);
                        }
                        if try_nullary_eq(&children[0], &children[1]).is_some() {
                            let variant = children[0].safe_decl().ok()?.name();
                            let temp = bcx.create_sized_stack_slot(
                                StackSlotData::new(StackSlotKind::ExplicitSlot,
                                                   size_of_value as u32));
                            let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                            bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                            emit_write_value(bcx, &children[1], temp_ptr, env,
                                helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                            let (vp, vl) = intern_str(string_pool, &variant);
                            let vp_v = bcx.ins().iconst(types::I64, vp);
                            let vl_v = bcx.ins().iconst(types::I64, vl);
                            let call = bcx.ins().call(helpers.is_variant,
                                &[temp_ptr, vp_v, vl_v]);
                            return Some(bcx.inst_results(call)[0]);
                        }
                    }
                    let l = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    let r = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    use cranelift::prelude::IntCC;
                    let cc = match kind {
                        DeclKind::LT => IntCC::SignedLessThan,
                        DeclKind::LE => IntCC::SignedLessThanOrEqual,
                        DeclKind::GT => IntCC::SignedGreaterThan,
                        DeclKind::GE => IntCC::SignedGreaterThanOrEqual,
                        DeclKind::EQ => IntCC::Equal,
                        _ => unreachable!(),
                    };
                    let cmp = bcx.ins().icmp(cc, l, r);
                    // icmp returns i8; widen to i64 for our ABI.
                    Some(bcx.ins().uextend(types::I64, cmp))
                }
                DeclKind::AND => {
                    if children.is_empty() { return Some(bcx.ins().iconst(types::I64, 1)); }
                    let mut acc = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    for c in &children[1..] {
                        let v = emit_compute_i64(bcx, c, env, helpers,
                            variant_arity, string_pool, ptr_t, size_of_value)?;
                        acc = bcx.ins().band(acc, v);
                    }
                    Some(acc)
                }
                DeclKind::OR => {
                    if children.is_empty() { return Some(bcx.ins().iconst(types::I64, 0)); }
                    let mut acc = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    for c in &children[1..] {
                        let v = emit_compute_i64(bcx, c, env, helpers,
                            variant_arity, string_pool, ptr_t, size_of_value)?;
                        acc = bcx.ins().bor(acc, v);
                    }
                    Some(acc)
                }
                DeclKind::NOT => {
                    if children.len() != 1 { return None; }
                    let v = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    let one = bcx.ins().iconst(types::I64, 1);
                    Some(bcx.ins().bxor(v, one))
                }
                DeclKind::ITE => {
                    if children.len() != 3 { return None; }
                    let cond = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    let t = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    let e = emit_compute_i64(bcx, &children[2], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().select(cond, t, e))
                }
                DeclKind::DT_IS | DeclKind::DT_RECOGNISER => {
                    if children.len() != 1 { return None; }
                    let app_text = format!("{expr}");
                    let variant = crate::z3_eval::extract_is_variant_pub(&app_text)
                        .or_else(|| decl.name().strip_prefix("is_").map(|s| s.to_string()))?;
                    // Compile inner into a temp slot.
                    let temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    emit_write_value(bcx, &children[0], temp_ptr, env,
                        helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                    let (vp, vl) = intern_str(string_pool, &variant);
                    let vp_v = bcx.ins().iconst(types::I64, vp);
                    let vl_v = bcx.ins().iconst(types::I64, vl);
                    let call = bcx.ins().call(helpers.is_variant,
                        &[temp_ptr, vp_v, vl_v]);
                    Some(bcx.inst_results(call)[0])
                }
                DeclKind::DT_ACCESSOR => {
                    if children.len() != 1 { return None; }
                    let raw = decl.name();
                    let accessor_name = raw.strip_suffix("__arr")
                        .or_else(|| raw.strip_suffix("__len"))
                        .map(|s| s.to_string())
                        .unwrap_or(raw);
                    // Compile inner into a temp slot, then extract by name,
                    // then load as i64. The inner value is presumably
                    // an enum/composite whose field is Int-typed.
                    let inner_temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let inner_ptr = bcx.ins().stack_addr(ptr_t, inner_temp, 0);
                    bcx.ins().call(helpers.init_slot, &[inner_ptr]);
                    emit_write_value(bcx, &children[0], inner_ptr, env,
                        helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                    let field_temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let field_ptr = bcx.ins().stack_addr(ptr_t, field_temp, 0);
                    bcx.ins().call(helpers.init_slot, &[field_ptr]);
                    let (np, nl) = intern_str(string_pool, &accessor_name);
                    let np_v = bcx.ins().iconst(types::I64, np);
                    let nl_v = bcx.ins().iconst(types::I64, nl);
                    bcx.ins().call(helpers.extract_field,
                        &[field_ptr, inner_ptr, np_v, nl_v]);
                    // Load as int from the field slot.
                    let call = bcx.ins().call(helpers.load_int, &[field_ptr]);
                    Some(bcx.inst_results(call)[0])
                }
                DeclKind::SELECT => {
                    if children.len() != 2 { return None; }
                    // Compile arr into a temp, then seq_select to read elem,
                    // then load_int from the elem slot.
                    let arr_temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let arr_ptr = bcx.ins().stack_addr(ptr_t, arr_temp, 0);
                    bcx.ins().call(helpers.init_slot, &[arr_ptr]);
                    emit_write_value(bcx, &children[0], arr_ptr, env,
                        helpers, variant_arity, string_pool, ptr_t, size_of_value)?;
                    let idx_v = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, string_pool, ptr_t, size_of_value)?;
                    let elem_temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let elem_ptr = bcx.ins().stack_addr(ptr_t, elem_temp, 0);
                    bcx.ins().call(helpers.init_slot, &[elem_ptr]);
                    bcx.ins().call(helpers.seq_select,
                        &[elem_ptr, arr_ptr, idx_v]);
                    let call = bcx.ins().call(helpers.load_int, &[elem_ptr]);
                    Some(bcx.inst_results(call)[0])
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn kind_of_dynamic<'ctx>(
    e: &Dynamic<'ctx>,
    enum_variants: &HashMap<String, Vec<String>>,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
) -> Option<OutputKind> {
    let sort = e.get_sort();
    let sort_name = format!("{sort}");
    if sort_name == "Int" || sort_name == "Real" { return Some(OutputKind::Int); }
    if sort_name == "Bool"   { return Some(OutputKind::Bool); }
    if sort_name == "String" { return Some(OutputKind::Str); }
    for (en, _) in enum_variants {
        if &sort_name == en {
            let all_nullary = variant_arity.get(en).map(|m|
                m.values().all(|v| v.is_empty())).unwrap_or(true);
            return Some(if all_nullary {
                OutputKind::Enum(en.clone())
            } else {
                OutputKind::EnumPayload(en.clone())
            });
        }
    }
    None
}

fn collect_inputs<'ctx>(
    e: &Dynamic<'ctx>,
    out: &mut std::collections::BTreeSet<(String, OutputKind)>,
    enum_variants: &HashMap<String, Vec<String>>,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
) {
    if e.kind() == AstKind::App {
        if let Ok(decl) = e.safe_decl() {
            if decl.kind() == DeclKind::UNINTERPRETED && e.num_children() == 0 {
                let name = decl.name();
                // Always register the input; kind is used for input
                // packing (Int/Bool fast path) but Seq/composite
                // inputs flow through clone_from_pool which doesn't
                // need a kind.
                let k = kind_of_dynamic(e, enum_variants, variant_arity)
                    .unwrap_or(OutputKind::Seq);
                out.insert((name, k));
                return;
            }
        }
        for c in e.children() {
            collect_inputs(&c, out, enum_variants, variant_arity);
        }
    }
}

fn lookup_variant(
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

// ── Functionizer trait wiring ────────────────────────────────────
//
// Zero-sized strategy struct that adapts `compile_program` to the
// `Functionizer` trait, and a `CompiledFunction` impl that wraps
// `JitProgram::call`. The runtime talks to these traits exclusively
// — `JitProgram` and `compile_program` stay private to this module.

/// Cranelift JIT functionizer strategy. Compiles a `Z3Program` to
/// native machine code via Cranelift.
pub struct CraneliftFunctionizer;

impl super::Functionizer for CraneliftFunctionizer {
    fn name(&self) -> &'static str { "cranelift" }

    fn compile(&self,
               program: &Z3Program,
               enums:   &EnumRegistry)
        -> Option<std::rc::Rc<dyn super::CompiledFunction>>
    {
        let jit = compile_program(program, enums)?;
        Some(std::rc::Rc::new(jit))
    }
}

impl super::CompiledFunction for JitProgram {
    fn call(&self, given: &HashMap<String, Value>)
        -> Option<HashMap<String, Value>>
    {
        JitProgram::call(self, given)
    }
}
