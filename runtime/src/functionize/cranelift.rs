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
    func: unsafe extern "C" fn(*const Value, *mut Value, *const Value, *mut i64),
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
        // Runtime bail flag: a Guarded step whose guards all evaluate
        // false at runtime (no branch matched) sets this to 1. We then
        // return None so the caller falls through to the slow Z3 solve
        // — exactly the None-style bailout the VM did. For an exhaustive
        // match/dispatch this fallthrough is dead code; the flag stays 0.
        let mut bail: i64 = 0;
        // SAFETY: compiled code is alive as long as `_module`;
        // both arrays are valid `Vec<Value>` of the declared size;
        // value_pool outlives the JIT module (stored alongside);
        // `&mut bail` is a valid i64 slot for the call's duration.
        unsafe {
            (self.func)(inputs.as_ptr(), outputs.as_mut_ptr(), pool_ptr, &mut bail);
        }
        if bail != 0 {
            if std::env::var("EVIDENT_JIT_CALL_TRACE").is_ok() {
                eprintln!("[jit/call] guarded no-match bail → slow path");
            }
            return None;
        }
        let mut out = HashMap::new();
        for (name, &idx) in &self.output_offsets {
            // Move the built value out of `outputs` (it's dropped right
            // after) instead of cloning — saves an O(value) deep copy of
            // every output per call, which for a recursive-enum state
            // (the self-hosted walk's `state_next`) is the whole tree
            // (session YY). Offsets are unique per output, so the take
            // leaves a valid sentinel in each slot for the Vec's drop.
            let v = std::mem::replace(&mut outputs[idx], Value::Int(0));
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
        // Record-element Seq — each element is a Value::Composite the
        // JIT built via ev_set_composite. Reclassify to SeqComposite
        // so it matches the slow-path extractor's shape (extract.rs
        // `extract_seq_composite`) and downstream consumers
        // (effect_loop/collect.rs's Seq(Composite) handling).
        Some(Value::Composite(_)) => Value::SeqComposite(
            xs.into_iter().filter_map(|e|
                if let Value::Composite(m) = e { Some(m) } else { None }).collect()),
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
    set_composite:     FuncId,
    seq_new:           FuncId,
    seq_push_clone:    FuncId,
    seq_set:           FuncId,
    load_int:          FuncId,
    load_bool:         FuncId,
    extract_field:     FuncId,
    field_ref:         FuncId,
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
    set_composite:     cranelift::codegen::ir::FuncRef,
    seq_new:           cranelift::codegen::ir::FuncRef,
    seq_push_clone:    cranelift::codegen::ir::FuncRef,
    seq_set:           cranelift::codegen::ir::FuncRef,
    load_int:          cranelift::codegen::ir::FuncRef,
    load_bool:         cranelift::codegen::ir::FuncRef,
    extract_field:     cranelift::codegen::ir::FuncRef,
    field_ref:         cranelift::codegen::ir::FuncRef,
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
    let s_composite = mk(&[p, p, p, p, usz], module);
    let s_seq_new  = mk(&[p, usz], module);
    let s_seq_push = mk(&[p, p], module);
    let s_seq_set  = mk(&[p, i64t, p], module);
    let s_load_int = mk_ret(&[p], i64t, module);
    let s_load_bool = mk_ret(&[p], i64t, module);
    let s_extract = mk(&[p, p, p, usz], module);
    let s_field_ref = mk_ret(&[p, p, usz], p, module);
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
        set_composite:    module.declare_function("ev_set_composite",    Linkage::Import, &s_composite).ok()?,
        seq_new:          module.declare_function("ev_seq_new",          Linkage::Import, &s_seq_new).ok()?,
        seq_push_clone:   module.declare_function("ev_seq_push_clone",   Linkage::Import, &s_seq_push).ok()?,
        seq_set:          module.declare_function("ev_seq_set",          Linkage::Import, &s_seq_set).ok()?,
        load_int:         module.declare_function("ev_load_int",         Linkage::Import, &s_load_int).ok()?,
        load_bool:        module.declare_function("ev_load_bool",        Linkage::Import, &s_load_bool).ok()?,
        extract_field:    module.declare_function("ev_extract_field",    Linkage::Import, &s_extract).ok()?,
        field_ref:        module.declare_function("ev_field_ref",        Linkage::Import, &s_field_ref).ok()?,
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
        set_composite:    module.declare_func_in_func(ids.set_composite,    func),
        seq_new:          module.declare_func_in_func(ids.seq_new,          func),
        seq_push_clone:   module.declare_func_in_func(ids.seq_push_clone,   func),
        seq_set:          module.declare_func_in_func(ids.seq_set,          func),
        load_int:         module.declare_func_in_func(ids.load_int,         func),
        load_bool:        module.declare_func_in_func(ids.load_bool,        func),
        extract_field:    module.declare_func_in_func(ids.extract_field,    func),
        field_ref:        module.declare_func_in_func(ids.field_ref,        func),
        seq_select:       module.declare_func_in_func(ids.seq_select,       func),
        str_concat:       module.declare_func_in_func(ids.str_concat,       func),
        is_variant:       module.declare_func_in_func(ids.is_variant,       func),
        clone_from_pool:  module.declare_func_in_func(ids.clone_from_pool,  func),
    }
}

// NOTE (session I): the un-JIT'd Mario components (display 64/66, etc.)
// are NOT codegen-shape gaps — the codegen here handles every shape they
// use (incl. nested SELECT-through-record-Seq-field). They are correctly
// routed to the scoped slow solve *upstream* of this function, for reasons
// that cannot be fixed inside `functionize/`:
//   * A component may reference an intermediate var (e.g. Mario's
//     `draw_rect__color_eff__callN` libcall) whose *defining* assertion
//     touches no claim output, so `decompose_simplified` files it as a
//     GLOBAL assertion handed to the slow part — `compile_program` never
//     sees it. Forcing such a component to compile reads that var as an
//     absent input (`Value::Int(0)`), silently dropping draws. The fix is
//     in `runtime/runtime/query.rs` (carry a component's intermediate
//     globals into its extracted program).
//   * Content-free crosslink cycles (`phase_chain[k] == hud_effs[i].effs[j]`
//     with the HUD's ground draws dropped in translation) have no value to
//     compute; the topo-bail in `z3_eval` correctly sends them to the slow
//     solve. The fix is in `translate/` (don't drop the `world.lives` read).
// See examples/COUNTEREXAMPLES.md #12 for the full investigation.
pub fn compile_program<'ctx>(
    program: &Z3Program<'ctx>,
    enums: &EnumRegistry,
    datatypes: &crate::core::DatatypeRegistry,
) -> Option<JitProgram> {
    // Dump the IR before codegen if requested. This sits between
    // EVIDENT_FZ_DUMP_BODY (raw simplified Z3 assertions, the
    // extractor input) and EVIDENT_JIT_DUMP (Cranelift CLIF, the
    // codegen output). Fires before any early-return so a refusal
    // can be diagnosed against the exact shape the JIT was handed.
    if std::env::var("EVIDENT_FZ_DUMP_PROGRAM").is_ok() {
        let label = program.label.as_deref().unwrap_or("<anonymous>");
        eprintln!("[fz/program] === claim {} ({} steps, {} checks, {} predicates) ===",
            label, program.steps.len(), program.checks.len(), program.predicates.len());
        eprint!("{program}");
    }
    // Record (user-type) info, keyed by Z3 constructor name (e.g.
    // "mk_IVec2", "mk_EffectPair"). Records aren't in `enums`; their
    // constructor name + field shape (`FieldKind`) come from the
    // DatatypeRegistry. Used by the DT_CONSTRUCTOR codegen path to
    // build `Value::Composite` (vs `Value::Enum` for true enums).
    let record_info: HashMap<String, Vec<crate::core::FieldKind>> = {
        let dts = datatypes.borrow();
        dts.iter().filter_map(|(_type_name, (dt, fields))| {
            let ctor = dt.variants.first()?.constructor.name();
            Some((ctor, fields.clone()))
        }).collect()
    };
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
            Z3Step::Guarded { var, branches } => {
                // We only compile Seq-bodied Guarded steps — the common
                // `effects = match state ⇒ ⟨…⟩` shape (24/27 demos). The
                // "no branch matched" case is handled at runtime via the
                // bail flag (see JitProgram::call): the codegen stores 1
                // in the flag in the fallthrough block and the caller
                // returns None, matching the old slow-path bailout.
                //
                // Scalar-bodied Guarded steps (an enum/String `match`
                // producing a scalar — e.g. extracting `StringResult(s)`
                // from `last_results[1]`) are NOT yet compiled: the
                // payload-extraction codegen miscomputes for some shapes,
                // so we refuse the whole program (→ slow Z3 solve, which
                // is correct). See docs/jit-codegen-gaps.md.
                if branches.iter().any(|b| matches!(b.body, GuardedBody::Scalar(_))) {
                    if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                        eprintln!("[jit] bail: Guarded {var} (scalar body \
                                  — match-to-scalar not yet compiled)");
                    }
                    return None;
                }
                (var.clone(), OutputKind::Seq)
            }
            Z3Step::PreBaked { var, .. } => (var.clone(), OutputKind::Seq /* placeholder */),
            // Sampler steps are the SatisfierFunctionizer's job, not
            // Cranelift's. Refuse the whole program so it either routes
            // to the satisfier (which strips these before delegating
            // back to us) or to the slow Z3 solve.
            Z3Step::SampleRange { .. }
            | Z3Step::SampleEnum { .. }
            | Z3Step::SampleSet { .. } => {
                if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                    eprintln!("[jit] bail: sampler step {} — handled by SatisfierFunctionizer",
                        step.var());
                }
                return None;
            }
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

    // A `<seq>__len` input is the symbolic length of an unpinned Seq
    // (`#seq`, see translate/declare.rs). The runtime supplies the Seq
    // *value* in `given` but never its `__len` symbol, so the JIT would
    // pack it as the `Int(0)` sentinel and silently compute length 0 —
    // wrong (e.g. `#last_results > 0` → false). We can't derive it from
    // the paired Seq value because the symbol is disconnected from it at
    // the ABI, so refuse the whole program (→ correct slow Z3 solve).
    // (Pinned-length seqs fold `#seq` to a numeral and never reach here.)
    if let Some((bad, _)) = input_names.iter().find(|(n, _)| n.ends_with("__len")) {
        if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
            eprintln!("[jit] bail: input {bad} is a Seq-length symbol \
                      (#seq of an unpinned Seq — not supplied in given)");
        }
        return None;
    }

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
    sig.params.push(AbiParam::new(ptr_t));   // *mut i64 bail flag
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
        let bail_ptr    = bcx.block_params(entry)[3];

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
                        &helpers, &variant_arity, &record_info, &mut string_pool,
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
                            &helpers, &variant_arity, &record_info, &mut string_pool,
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
                            &helpers, &variant_arity, &record_info, &mut string_pool, ptr_t, size_of_value)
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
                                    &helpers, &variant_arity, &record_info, &mut string_pool,
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
                                        &helpers, &variant_arity, &record_info, &mut string_pool,
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
                    // Default fallthrough — no guard matched. Set the
                    // runtime bail flag so JitProgram::call returns None
                    // and the caller falls through to the slow Z3 solve
                    // (the correct value comes from the scoped re-solve).
                    // Still write a valid sentinel Int(0) so the slot
                    // stays a well-formed Value for any subsequent step
                    // that reads it before the call unwinds.
                    let one = bcx.ins().iconst(types::I64, 1);
                    bcx.ins().store(MemFlags::new(), one, bail_ptr, 0);
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
                // Unreachable: the Phase 2 walk above already returned
                // None on any sampler step. Kept exhaustive + defensive.
                Z3Step::SampleRange { .. }
                | Z3Step::SampleEnum { .. }
                | Z3Step::SampleSet { .. } => return None,
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
    let func: unsafe extern "C" fn(*const Value, *mut Value, *const Value, *mut i64) = unsafe {
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

/// Return a pointer to a readable `Value` for `expr`, **without
/// cloning** when `expr` is a bare env variable — in that case the
/// env slot is returned directly. Otherwise the value is
/// materialized into a fresh stack temp (which clones) and that
/// temp's pointer is returned.
///
/// Used for *read-only source* positions — the receiver of a
/// recognizer (`(is V x)`) or accessor (`(field x)`). The old code
/// always cloned the whole source into a temp before reading one
/// field / the variant tag; for a deeply-nested `match` over a big
/// cons-list state that meant cloning the entire state per guard and
/// per accessor (the dominant per-tick cost of the self-hosted walk
/// — session YY). Reading the variant tag is O(1) and extracting one
/// field clones only that field, so passing the env slot by
/// reference avoids the wholesale copy. Safe because the helpers
/// (`ev_is_variant`, `ev_extract_field`) only *read* through the
/// pointer (and `ev_extract_field` clones the field before any
/// write, so even out==src would be sound).
fn emit_value_ref<'ctx>(
    bcx: &mut FunctionBuilder,
    expr: &Dynamic<'ctx>,
    env: &HashMap<String, ClValue>,
    helpers: &HelperRefs,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
    record_info: &HashMap<String, Vec<crate::core::FieldKind>>,
    string_pool: &mut Vec<Box<str>>,
    ptr_t: cranelift::prelude::Type,
    size_of_value: i64,
) -> Option<ClValue> {
    if expr.kind() == AstKind::App {
        if let Ok(decl) = expr.safe_decl() {
            let children = expr.children();
            match decl.kind() {
                // Bare env variable → borrow its slot, no clone.
                DeclKind::UNINTERPRETED if children.is_empty() => {
                    if let Some(slot) = env.get(&decl.name()) {
                        return Some(*slot);
                    }
                }
                // Datatype field accessor → walk by reference: get a
                // pointer to the source (recursively, also by ref), then
                // `ev_field_ref` to a pointer INTO it. No clone per link,
                // so a chain like `(head (f0 state))` is a pointer walk.
                // A `__len` accessor isn't a Value field — fall through
                // to materialize.
                DeclKind::DT_ACCESSOR if children.len() == 1 => {
                    let raw = decl.name();
                    if raw.ends_with("__len") {
                        // length: not a borrowable field; materialize below.
                    } else {
                        let fname = raw.strip_suffix("__arr").map(|s| s.to_string()).unwrap_or(raw);
                        let src_ptr = emit_value_ref(bcx, &children[0], env, helpers,
                            variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                        let (np, nl) = intern_str(string_pool, &fname);
                        let np_v = bcx.ins().iconst(types::I64, np);
                        let nl_v = bcx.ins().iconst(types::I64, nl);
                        let call = bcx.ins().call(helpers.field_ref, &[src_ptr, np_v, nl_v]);
                        return Some(bcx.inst_results(call)[0]);
                    }
                }
                // Z3-internal single-child accessor `<field>__arr`.
                DeclKind::UNINTERPRETED if children.len() == 1 => {
                    let raw = decl.name();
                    if let Some(fname) = raw.strip_suffix("__arr") {
                        let fname = fname.to_string();
                        let src_ptr = emit_value_ref(bcx, &children[0], env, helpers,
                            variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                        let (np, nl) = intern_str(string_pool, &fname);
                        let np_v = bcx.ins().iconst(types::I64, np);
                        let nl_v = bcx.ins().iconst(types::I64, nl);
                        let call = bcx.ins().call(helpers.field_ref, &[src_ptr, np_v, nl_v]);
                        return Some(bcx.inst_results(call)[0]);
                    }
                }
                _ => {}
            }
        }
    }
    // Otherwise materialize into a temp (this clones, but only the
    // sub-value `expr` denotes — not necessarily the whole state).
    let temp = bcx.create_sized_stack_slot(
        StackSlotData::new(StackSlotKind::ExplicitSlot, size_of_value as u32));
    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
    emit_write_value(bcx, expr, temp_ptr, env, helpers, variant_arity,
        record_info, string_pool, ptr_t, size_of_value)?;
    Some(temp_ptr)
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
    record_info: &HashMap<String, Vec<crate::core::FieldKind>>,
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
                        let src_ptr = emit_value_ref(bcx, &children[0], env,
                            helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                        let (np, nl) = intern_str(string_pool, &logical);
                        let np_v = bcx.ins().iconst(types::I64, np);
                        let nl_v = bcx.ins().iconst(types::I64, nl);
                        bcx.ins().call(helpers.extract_field,
                            &[out_slot, src_ptr, np_v, nl_v]);
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
                    // Read the field straight off the source value — pass
                    // its slot by reference when it's a bare variable
                    // (no wholesale clone; session YY).
                    let src_ptr = emit_value_ref(bcx, &children[0], env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let (np, nl) = intern_str(string_pool, &accessor_name);
                    let np_v = bcx.ins().iconst(types::I64, np);
                    let nl_v = bcx.ins().iconst(types::I64, nl);
                    bcx.ins().call(helpers.extract_field,
                        &[out_slot, src_ptr, np_v, nl_v]);
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
                    let src_ptr = emit_value_ref(bcx, &children[0], env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let (vp, vl) = intern_str(string_pool, &variant);
                    let vp_v = bcx.ins().iconst(types::I64, vp);
                    let vl_v = bcx.ins().iconst(types::I64, vl);
                    let call = bcx.ins().call(helpers.is_variant,
                        &[src_ptr, vp_v, vl_v]);
                    let r = bcx.inst_results(call)[0];
                    bcx.ins().call(helpers.set_bool, &[out_slot, r]);
                    Some(())
                }
                DeclKind::CONST_ARRAY => {
                    // `(const-array <default>)` — the base of a Z3
                    // array store-chain. We materialize Seq-typed
                    // record fields as a `SeqEnum`, starting empty;
                    // the wrapping STORE chain fills exactly the
                    // indices `0..len`, so the const default (which
                    // would notionally fill all indices) is unused.
                    let cap = bcx.ins().iconst(types::I64, 0);
                    bcx.ins().call(helpers.seq_new, &[out_slot, cap]);
                    Some(())
                }
                DeclKind::STORE => {
                    // `(store <arr> <idx> <val>)` — materialize the
                    // inner array into out_slot (a SeqEnum), then set
                    // element `idx` to `val`. Walking the chain
                    // inner-to-outer (innermost store = lowest index)
                    // builds the Seq(T)-valued field of a record.
                    if children.len() != 3 { return None; }
                    emit_write_value(bcx, &children[0], out_slot, env,
                        helpers, variant_arity, record_info, string_pool,
                        ptr_t, size_of_value)?;
                    let idx_v = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let temp_ptr = bcx.ins().stack_addr(ptr_t, temp, 0);
                    bcx.ins().call(helpers.init_slot, &[temp_ptr]);
                    emit_write_value(bcx, &children[2], temp_ptr, env,
                        helpers, variant_arity, record_info, string_pool,
                        ptr_t, size_of_value)?;
                    bcx.ins().call(helpers.seq_set, &[out_slot, idx_v, temp_ptr]);
                    Some(())
                }
                DeclKind::SELECT => {
                    if children.len() != 2 {
                        if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                            eprintln!("[jit] SELECT children != 2: {expr}");
                        }
                        return None;
                    }
                    let arr_ptr = match emit_value_ref(bcx, &children[0], env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)
                    {
                        Some(p) => p,
                        None => {
                            if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
                                eprintln!("[jit] SELECT arr bail: {}", &children[0]);
                            }
                            return None;
                        }
                    };
                    let idx_v = match emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)
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
                        &[out_slot, arr_ptr, idx_v]);
                    Some(())
                }
                DeclKind::DT_CONSTRUCTOR => {
                    let variant = decl.name();
                    // Record (user-type) constructor → build a
                    // Value::Composite{field → value} rather than a
                    // Value::Enum. Records aren't in the enum
                    // `variant_arity`; they're keyed in `record_info`
                    // by their Z3 ctor name (e.g. "mk_IVec2"). A
                    // Seq(Record) of these becomes Value::SeqComposite
                    // after classify_seq.
                    if let Some(fields) = record_info.get(&variant) {
                        return emit_write_record(bcx, &children, fields, out_slot,
                            env, helpers, variant_arity, record_info,
                            string_pool, ptr_t, size_of_value);
                    }
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
                            helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
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
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let then_block = bcx.create_block();
                    let else_block = bcx.create_block();
                    let merge_block = bcx.create_block();
                    bcx.ins().brif(cond_v, then_block, &[], else_block, &[]);
                    bcx.switch_to_block(then_block);
                    bcx.seal_block(then_block);
                    emit_write_value(bcx, &children[1], out_slot, env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().jump(merge_block, &[]);
                    bcx.switch_to_block(else_block);
                    bcx.seal_block(else_block);
                    emit_write_value(bcx, &children[2], out_slot, env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().jump(merge_block, &[]);
                    bcx.switch_to_block(merge_block);
                    bcx.seal_block(merge_block);
                    Some(())
                }
                DeclKind::ADD | DeclKind::SUB | DeclKind::MUL | DeclKind::UMINUS
                | DeclKind::IDIV | DeclKind::DIV | DeclKind::MOD | DeclKind::REM => {
                    // Int arithmetic → set_int with computed i64.
                    // div/mod were already handled as *operands* by
                    // emit_compute_i64 (sdiv/srem); listing them here lets
                    // a div/mod sit as the top-level expr of a Scalar write
                    // (e.g. `q = seed / 2`) or an ITE branch instead of
                    // falling through to `_ => None`.
                    let v = emit_compute_i64(bcx, expr, env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().call(helpers.set_int, &[out_slot, v]);
                    Some(())
                }
                DeclKind::LT | DeclKind::LE | DeclKind::GT | DeclKind::GE
                | DeclKind::EQ | DeclKind::AND | DeclKind::OR | DeclKind::NOT => {
                    // Bool ops → set_bool with computed i64 (0/1).
                    let v = emit_compute_i64(bcx, expr, env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    bcx.ins().call(helpers.set_bool, &[out_slot, v]);
                    Some(())
                }
                DeclKind::SEQ_CONCAT => {
                    // String concatenation `(str.++ a b …)`. Each operand
                    // is itself a String value (literal or env var); build
                    // each into a temp slot, then call ev_str_concat with an
                    // array of slot pointers (mirrors the multifield ctor
                    // path). String operands reach this via the literal
                    // short-circuit / UNINTERPRETED clone-from-env at the
                    // top of emit_write_value, so nested concats work too.
                    let n = children.len();
                    let arg_slots: Vec<ClValue> = (0..n).map(|_| {
                        let s = bcx.create_sized_stack_slot(
                            StackSlotData::new(StackSlotKind::ExplicitSlot,
                                               size_of_value as u32));
                        bcx.ins().stack_addr(ptr_t, s, 0)
                    }).collect();
                    for s in &arg_slots {
                        bcx.ins().call(helpers.init_slot, &[*s]);
                    }
                    for (i, child) in children.iter().enumerate() {
                        emit_write_value(bcx, child, arg_slots[i], env,
                            helpers, variant_arity, record_info, string_pool,
                            ptr_t, size_of_value)?;
                    }
                    let array_slot = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           (n.max(1) as u32) * 8));
                    let array_ptr = bcx.ins().stack_addr(ptr_t, array_slot, 0);
                    for (i, &s) in arg_slots.iter().enumerate() {
                        bcx.ins().store(MemFlags::new(), s, array_ptr, (i as i32) * 8);
                    }
                    let n_v = bcx.ins().iconst(types::I64, n as i64);
                    bcx.ins().call(helpers.str_concat, &[out_slot, array_ptr, n_v]);
                    Some(())
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// Emit IR that writes a `Value::Composite{field → value}` for a
/// record (user-type) constructor application into `out_slot`.
///
/// `children` are the Z3 constructor args (one per accessor, in
/// declaration order); `fields` is the record's `FieldKind` list
/// (same order). A `Primitive`/`Nested` field consumes one arg; a
/// `SeqField` consumes two (the `Array(Int→T)` and its `Int` length,
/// collapsed into one `SeqEnum` field — the length arg is implicit
/// in the materialized Vec, so it's skipped). Nested record fields
/// recurse back through `emit_write_value` (their arg is itself a
/// record constructor).
#[allow(clippy::too_many_arguments)]
fn emit_write_record<'ctx>(
    bcx: &mut FunctionBuilder,
    children: &[Dynamic<'ctx>],
    fields: &[crate::core::FieldKind],
    out_slot: ClValue,
    env: &HashMap<String, ClValue>,
    helpers: &HelperRefs,
    variant_arity: &HashMap<String, HashMap<String, Vec<String>>>,
    record_info: &HashMap<String, Vec<crate::core::FieldKind>>,
    string_pool: &mut Vec<Box<str>>,
    ptr_t: cranelift::prelude::Type,
    size_of_value: i64,
) -> Option<()> {
    use crate::core::FieldKind;
    let n = fields.len();
    // One value slot per logical field.
    let val_slots: Vec<ClValue> = (0..n).map(|_| {
        let s = bcx.create_sized_stack_slot(
            StackSlotData::new(StackSlotKind::ExplicitSlot, size_of_value as u32));
        let p = bcx.ins().stack_addr(ptr_t, s, 0);
        bcx.ins().call(helpers.init_slot, &[p]);
        p
    }).collect();

    let mut name_pl: Vec<(i64, i64)> = Vec::with_capacity(n);
    let mut arg_idx = 0usize;
    for (fi, fk) in fields.iter().enumerate() {
        name_pl.push(intern_str(string_pool, fk.name()));
        match fk {
            // Seq(T) field: ctor arg `arg_idx` is the Array(Int→T),
            // `arg_idx + 1` is the Int length. The array arg is a Z3
            // const-array/store chain that the CONST_ARRAY/STORE
            // handlers materialize into a SeqEnum.
            FieldKind::SeqField { .. } => {
                let arr_child = children.get(arg_idx)?;
                emit_write_value(bcx, arr_child, val_slots[fi], env,
                    helpers, variant_arity, record_info, string_pool,
                    ptr_t, size_of_value)?;
                arg_idx += 2;
            }
            // Primitive leaf or nested record: one ctor arg.
            _ => {
                let child = children.get(arg_idx)?;
                emit_write_value(bcx, child, val_slots[fi], env,
                    helpers, variant_arity, record_info, string_pool,
                    ptr_t, size_of_value)?;
                arg_idx += 1;
            }
        }
    }

    // Parallel stack arrays: field-name ptrs, field-name lens, value ptrs.
    let mk_arr = |bcx: &mut FunctionBuilder| {
        let slot = bcx.create_sized_stack_slot(
            StackSlotData::new(StackSlotKind::ExplicitSlot, (n.max(1) as u32) * 8));
        bcx.ins().stack_addr(ptr_t, slot, 0)
    };
    let name_ptr_base = mk_arr(bcx);
    let name_len_base = mk_arr(bcx);
    let val_ptr_base  = mk_arr(bcx);
    for (i, (p, l)) in name_pl.iter().enumerate() {
        let pv = bcx.ins().iconst(types::I64, *p);
        bcx.ins().store(MemFlags::new(), pv, name_ptr_base, (i as i32) * 8);
        let lv = bcx.ins().iconst(types::I64, *l);
        bcx.ins().store(MemFlags::new(), lv, name_len_base, (i as i32) * 8);
    }
    for (i, &s) in val_slots.iter().enumerate() {
        bcx.ins().store(MemFlags::new(), s, val_ptr_base, (i as i32) * 8);
    }
    let n_v = bcx.ins().iconst(types::I64, n as i64);
    bcx.ins().call(helpers.set_composite,
        &[out_slot, name_ptr_base, name_len_base, val_ptr_base, n_v]);
    Some(())
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
    record_info: &HashMap<String, Vec<crate::core::FieldKind>>,
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
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    for c in &children[1..] {
                        let v = emit_compute_i64(bcx, c, env, helpers,
                            variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
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
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().ineg(v))
                }
                DeclKind::IDIV | DeclKind::DIV => {
                    if children.len() != 2 { return None; }
                    let l = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let r = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().sdiv(l, r))
                }
                DeclKind::MOD | DeclKind::REM => {
                    if children.len() != 2 { return None; }
                    let l = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let r = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
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
                            let src_ptr = emit_value_ref(bcx, &children[0], env,
                                helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                            let (vp, vl) = intern_str(string_pool, &variant);
                            let vp_v = bcx.ins().iconst(types::I64, vp);
                            let vl_v = bcx.ins().iconst(types::I64, vl);
                            let call = bcx.ins().call(helpers.is_variant,
                                &[src_ptr, vp_v, vl_v]);
                            return Some(bcx.inst_results(call)[0]);
                        }
                        if try_nullary_eq(&children[0], &children[1]).is_some() {
                            let variant = children[0].safe_decl().ok()?.name();
                            let src_ptr = emit_value_ref(bcx, &children[1], env,
                                helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                            let (vp, vl) = intern_str(string_pool, &variant);
                            let vp_v = bcx.ins().iconst(types::I64, vp);
                            let vl_v = bcx.ins().iconst(types::I64, vl);
                            let call = bcx.ins().call(helpers.is_variant,
                                &[src_ptr, vp_v, vl_v]);
                            return Some(bcx.inst_results(call)[0]);
                        }
                    }
                    let l = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let r = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
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
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    for c in &children[1..] {
                        let v = emit_compute_i64(bcx, c, env, helpers,
                            variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                        acc = bcx.ins().band(acc, v);
                    }
                    Some(acc)
                }
                DeclKind::OR => {
                    if children.is_empty() { return Some(bcx.ins().iconst(types::I64, 0)); }
                    let mut acc = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    for c in &children[1..] {
                        let v = emit_compute_i64(bcx, c, env, helpers,
                            variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                        acc = bcx.ins().bor(acc, v);
                    }
                    Some(acc)
                }
                DeclKind::NOT => {
                    if children.len() != 1 { return None; }
                    let v = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let one = bcx.ins().iconst(types::I64, 1);
                    Some(bcx.ins().bxor(v, one))
                }
                DeclKind::ITE => {
                    if children.len() != 3 { return None; }
                    let cond = emit_compute_i64(bcx, &children[0], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let t = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let e = emit_compute_i64(bcx, &children[2], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    Some(bcx.ins().select(cond, t, e))
                }
                DeclKind::DT_IS | DeclKind::DT_RECOGNISER => {
                    if children.len() != 1 { return None; }
                    let app_text = format!("{expr}");
                    let variant = crate::z3_eval::extract_is_variant_pub(&app_text)
                        .or_else(|| decl.name().strip_prefix("is_").map(|s| s.to_string()))?;
                    // Read the variant tag straight off the source (no
                    // wholesale clone when it's a bare variable; session YY).
                    let src_ptr = emit_value_ref(bcx, &children[0], env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let (vp, vl) = intern_str(string_pool, &variant);
                    let vp_v = bcx.ins().iconst(types::I64, vp);
                    let vl_v = bcx.ins().iconst(types::I64, vl);
                    let call = bcx.ins().call(helpers.is_variant,
                        &[src_ptr, vp_v, vl_v]);
                    Some(bcx.inst_results(call)[0])
                }
                DeclKind::DT_ACCESSOR => {
                    if children.len() != 1 { return None; }
                    let raw = decl.name();
                    let accessor_name = raw.strip_suffix("__arr")
                        .or_else(|| raw.strip_suffix("__len"))
                        .map(|s| s.to_string())
                        .unwrap_or(raw);
                    // Read the field straight off the source (no wholesale
                    // clone when it's a bare variable; session YY), then
                    // load as i64. The source is an enum/composite whose
                    // field is Int-typed.
                    let inner_ptr = emit_value_ref(bcx, &children[0], env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
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
                    // Load from the field slot using the loader that
                    // matches the accessor's RESULT sort. A Bool-typed
                    // payload field must read via `load_bool` —
                    // `load_int` returns 0 for a `Value::Bool`, so a
                    // destructured Bool used in a comparison / boolean op
                    // (`Decide(rsn,_) ⇒ rsn ∧ …`) would read false even
                    // when it's true (COUNTEREXAMPLES #18 / #17 keystone).
                    let sort_name = format!("{}", expr.get_sort());
                    let loader = if sort_name == "Bool" {
                        helpers.load_bool
                    } else {
                        helpers.load_int
                    };
                    let call = bcx.ins().call(loader, &[field_ptr]);
                    Some(bcx.inst_results(call)[0])
                }
                DeclKind::SELECT => {
                    if children.len() != 2 { return None; }
                    // Read the array straight off the source (no wholesale
                    // clone when it's a bare variable; session YY), then
                    // seq_select to read elem, then load_int.
                    let arr_ptr = emit_value_ref(bcx, &children[0], env,
                        helpers, variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let idx_v = emit_compute_i64(bcx, &children[1], env, helpers,
                        variant_arity, record_info, string_pool, ptr_t, size_of_value)?;
                    let elem_temp = bcx.create_sized_stack_slot(
                        StackSlotData::new(StackSlotKind::ExplicitSlot,
                                           size_of_value as u32));
                    let elem_ptr = bcx.ins().stack_addr(ptr_t, elem_temp, 0);
                    bcx.ins().call(helpers.init_slot, &[elem_ptr]);
                    bcx.ins().call(helpers.seq_select,
                        &[elem_ptr, arr_ptr, idx_v]);
                    // Same sort-driven loader choice as DT_ACCESSOR: a
                    // Bool element read via `load_int` would read 0.
                    let sort_name = format!("{}", expr.get_sort());
                    let loader = if sort_name == "Bool" {
                        helpers.load_bool
                    } else {
                        helpers.load_int
                    };
                    let call = bcx.ins().call(loader, &[elem_ptr]);
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
               program:   &Z3Program,
               enums:     &EnumRegistry,
               datatypes: &crate::core::DatatypeRegistry)
        -> Option<std::rc::Rc<dyn super::CompiledFunction>>
    {
        let jit = compile_program(program, enums, datatypes)?;
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
