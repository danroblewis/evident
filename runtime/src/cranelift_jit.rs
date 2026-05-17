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
    func: unsafe extern "C" fn(*const i64, *mut Value),
    pub input_offsets: HashMap<String, usize>,
    pub input_kinds:   HashMap<String, OutputKind>,
    pub output_offsets: HashMap<String, usize>,
    pub output_kinds:   HashMap<String, OutputKind>,
    pub enum_tags:     HashMap<String, HashMap<String, i64>>,
    pub enum_variants: HashMap<String, Vec<String>>,
    /// Interned strings kept alive for the lifetime of the JIT
    /// code (the compiled function holds raw pointers into them).
    _string_pool: Vec<Box<str>>,
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
        let mut inputs: Vec<i64> = vec![0; n_in];
        // Initialize each output slot to a default Value::Int(0).
        // The JIT helpers do `*out = ...` which drops the prior
        // value; the default ensures we have a valid Value
        // pre-written before the JIT runs.
        let mut outputs: Vec<Value> = (0..n_out).map(|_| Value::Int(0)).collect();
        for (name, &idx) in &self.input_offsets {
            let value = given.get(name)?;
            let kind  = self.input_kinds.get(name)?;
            inputs[idx] = self.pack_input(value, kind)?;
        }
        // SAFETY: compiled code is alive as long as `_module`;
        // outputs is a properly-aligned `Vec<Value>` of n_out
        // elements; helpers only access elements `0..n_out`.
        unsafe {
            (self.func)(inputs.as_ptr(), outputs.as_mut_ptr());
        }
        let mut out = HashMap::new();
        for (name, &idx) in &self.output_offsets {
            out.insert(name.clone(), outputs[idx].clone());
        }
        Some(out)
    }

    fn pack_input(&self, value: &Value, kind: &OutputKind) -> Option<i64> {
        match (kind, value) {
            (OutputKind::Int,  Value::Int(n))  => Some(*n),
            (OutputKind::Bool, Value::Bool(b)) => Some(if *b { 1 } else { 0 }),
            (OutputKind::Enum(en), Value::Enum { variant, .. }) => {
                self.enum_tags.get(en)?.get(variant).copied()
            }
            _ => None,
        }
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
    let s_extract = mk(&[p, p, usz], module);
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
                let k = kind_of_dynamic(expr, &enum_variants, &variant_arity)?;
                (var.clone(), k)
            }
            Z3Step::Seq { var, .. } => (var.clone(), OutputKind::Seq),
            Z3Step::Guarded { .. } => return None,  // not in v1 codegen
            Z3Step::PreBaked { .. } => return None, // value steps fall back to AST walker
        };
        output_kinds_local.push((var, kind));
        match step {
            Z3Step::Scalar { expr, .. } =>
                collect_inputs(expr, &mut input_set, &enum_variants, &variant_arity),
            Z3Step::Seq { elem_exprs, .. } =>
                for e in elem_exprs { collect_inputs(e, &mut input_set, &enum_variants, &variant_arity); },
            _ => {}
        }
    }
    let output_names: std::collections::HashSet<String> = output_kinds_local.iter()
        .map(|(n, _)| n.clone()).collect();
    let input_names: Vec<(String, OutputKind)> = input_set.into_iter()
        .filter(|(n, _)| !output_names.contains(n))
        .filter(|(_, k)| matches!(k,
            OutputKind::Int | OutputKind::Bool | OutputKind::Enum(_)))
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
    sig.params.push(AbiParam::new(ptr_t));   // *const i64 inputs
    sig.params.push(AbiParam::new(ptr_t));   // *mut Value outputs
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
    {
        let mut func_ctx = FunctionBuilderContext::new();
        let mut bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        let entry = bcx.create_block();
        bcx.append_block_params_for_function_params(entry);
        bcx.switch_to_block(entry);
        bcx.seal_block(entry);

        let inputs_ptr  = bcx.block_params(entry)[0];
        let outputs_ptr = bcx.block_params(entry)[1];

        // Temp slot for building Seq elements.
        let temp_slot = bcx.create_sized_stack_slot(
            StackSlotData::new(StackSlotKind::ExplicitSlot, size_of_value as u32));
        let temp_ptr = bcx.ins().stack_addr(ptr_t, temp_slot, 0);
        // The stack slot is uninitialized memory. We MUST use
        // `ptr::write` (via ev_init_slot) for the first write so
        // we don't try to drop garbage. Subsequent ev_set_* calls
        // on this slot use normal `*out = ...` which drops the
        // prior valid Value correctly.
        bcx.ins().call(helpers.init_slot, &[temp_ptr]);

        // Native env: var name → its computed location.
        let mut env: HashMap<String, EnvVal> = HashMap::new();
        for (name, idx) in &input_offsets {
            let v = bcx.ins().load(types::I64, MemFlags::new(),
                inputs_ptr, (idx * 8) as i32);
            let kind = input_kinds.get(name).cloned().unwrap_or(OutputKind::Int);
            env.insert(name.clone(), EnvVal::I64 { v, kind });
        }

        for step in &program.steps {
            let out_idx = output_offsets[step.var()];
            let out_offset = (out_idx as i64) * size_of_value;
            let off_v = bcx.ins().iconst(types::I64, out_offset);
            let out_slot = bcx.ins().iadd(outputs_ptr, off_v);

            match step {
                Z3Step::Scalar { var, expr } => {
                    emit_write_value(&mut bcx, expr, out_slot, &env,
                        &helpers, &variant_arity, &mut string_pool)?;
                    env.insert(var.clone(), EnvVal::OutSlot { ptr: out_slot });
                }
                Z3Step::Seq { var, elem_exprs } => {
                    let cap = bcx.ins().iconst(types::I64, elem_exprs.len() as i64);
                    bcx.ins().call(helpers.seq_new, &[out_slot, cap]);
                    for elem in elem_exprs {
                        emit_write_value(&mut bcx, elem, temp_ptr, &env,
                            &helpers, &variant_arity, &mut string_pool)?;
                        bcx.ins().call(helpers.seq_push_clone, &[out_slot, temp_ptr]);
                    }
                    env.insert(var.clone(), EnvVal::OutSlot { ptr: out_slot });
                }
                Z3Step::Guarded { .. } => return None,
                Z3Step::PreBaked { .. } => return None,
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
    let func: unsafe extern "C" fn(*const i64, *mut Value) = unsafe {
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
    })
}

#[derive(Clone)]
enum EnvVal {
    /// Primitive (or enum tag) in a register.
    I64 { v: ClValue, kind: OutputKind },
    /// Pointer to an earlier output slot's Value.
    OutSlot { ptr: ClValue },
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
    env: &HashMap<String, EnvVal>,
    helpers: &HelperRefs,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
    string_pool: &mut Vec<Box<str>>,
) -> Option<()> {
    // String literal short-circuit — ONLY for genuine zero-child
    // literals. `as_string()` collapses some non-literal ASTs
    // (e.g. `(ite c "a" "b")`) to empty/garbage; require
    // num_children=0 + non-UNINTERPRETED before trusting.
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
                    if !children.is_empty() { return None; }
                    let name = decl.name();
                    let ev = env.get(&name)?;
                    match ev {
                        EnvVal::I64 { v, kind } => match kind {
                            OutputKind::Int => {
                                bcx.ins().call(helpers.set_int, &[out_slot, *v]);
                                Some(())
                            }
                            OutputKind::Bool => {
                                bcx.ins().call(helpers.set_bool, &[out_slot, *v]);
                                Some(())
                            }
                            _ => None,  // enum-tag inputs not codegen'd yet
                        },
                        EnvVal::OutSlot { ptr: _ } => {
                            // Copy from another slot: not v1.
                            None
                        }
                    }
                }
                DeclKind::DT_CONSTRUCTOR => {
                    let variant = decl.name();
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
                                let n = arg.as_int().and_then(|x| x.as_i64())?;
                                let n_v = bcx.ins().iconst(types::I64, n);
                                bcx.ins().call(helpers.set_enum_int,
                                    &[out_slot, ep_v, el_v, vp_v, vl_v, n_v]);
                                Some(())
                            }
                            "String" => {
                                let s = arg.as_string().and_then(|zs| zs.as_string())?;
                                let (p, l) = intern_str(string_pool, &s);
                                let p_v = bcx.ins().iconst(types::I64, p);
                                let l_v = bcx.ins().iconst(types::I64, l);
                                bcx.ins().call(helpers.set_enum_str,
                                    &[out_slot, ep_v, el_v, vp_v, vl_v, p_v, l_v]);
                                Some(())
                            }
                            _ => None,
                        }
                    } else {
                        // Multi-field payload (e.g. LibCall(String, String,
                        // String, Seq(...))): would need
                        // `ev_set_enum_multifield` with a payload-args
                        // array. Defer to Round 27+ — Mario's display path
                        // doesn't strictly require it because LibCall is
                        // constructed via subschema-dispatch where each
                        // subschema's body has a single-arg ctor pattern.
                        None
                    }
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
                if let Some(k) = kind_of_dynamic(e, enum_variants, variant_arity) {
                    out.insert((name, k));
                }
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
