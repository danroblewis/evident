//! Cranelift JIT codegen for Z3-AST function-shaped components.
//!
//! This is the bridge from "interpret the canonical form" to
//! "execute the canonical form as machine code". The input is the
//! same `Z3Program` the AST walker uses; the output is a function
//! pointer to JIT-compiled native code.
//!
//! ## Scope
//!
//! v1 compiles a focused subset of Z3 patterns into native:
//!
//!   * Int arithmetic (ADD, SUB, MUL, IDIV, MOD, UMINUS)
//!   * Int comparisons (EQ, LT, LE, GT, GE) → Bool i8
//!   * Bool ops (AND, OR, NOT)
//!   * ITE (Int/Bool ternary)
//!   * UNINTERPRETED 0-arity vars (load from input slot)
//!   * DT_CONSTRUCTOR for 0-arity enum variants (encoded as i64
//!     constructor index — the Datatype's variant index)
//!   * DT_IS / DT_RECOGNISER on enum vars (compare against
//!     constructor index)
//!
//! Out of scope for v1 (fall back to AST walker):
//!
//!   * String values (need interning / heap)
//!   * Enum payloads with variable fields (Seq(Effect) with
//!     nested LibCall args)
//!   * Seq values in general (Z3 arrays + length)
//!
//! Even with this subset, claims that are pure Int/Bool/enum-tag
//! dispatch — state machines, counters, simple gating — compile
//! to native and run at machine speed.
//!
//! ## Calling convention
//!
//! Each compiled program is a function with signature:
//!
//! ```text
//!   extern "C" fn(inputs: *const i64, outputs: *mut i64)
//! ```
//!
//! Where `inputs` and `outputs` are flat i64 arrays whose offsets
//! map to variable names via `JitProgram::input_offsets` and
//! `JitProgram::output_offsets`. The Rust runtime packs/unpacks
//! Value enums into i64 around the call:
//!
//!   * Value::Int(n)              → n
//!   * Value::Bool(b)             → if b { 1 } else { 0 }
//!   * Value::Enum (0-arity)      → variant index (looked up in
//!                                   the enum's variants list)
//!
//! Non-fitting types abort compilation; the runtime keeps the
//! AST walker as a fallback for those.

use std::collections::HashMap;
use cranelift::prelude::{AbiParam, FunctionBuilder, FunctionBuilderContext,
    InstBuilder, IntCC, MemFlags, settings, types, EntityRef};
use cranelift::prelude::settings::Configurable;
use cranelift::prelude::Value as ClValue;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use z3::ast::{Ast, Dynamic};
use z3::AstKind;
use z3_sys::DeclKind;

use crate::translate::{EnumRegistry, Value};
use crate::z3_eval::{Z3Program, Z3Step, GuardedBody};

/// One compiled program: a JIT'd native function plus the slot
/// layout for inputs and outputs.
pub struct JitProgram {
    /// JIT module — must stay alive for the duration of any
    /// function calls into it. Cranelift unloads the code when
    /// the module drops.
    _module: JITModule,
    /// Function pointer cast to an extern "C" closure.
    func: unsafe extern "C" fn(*const i64, *mut i64),
    /// `var_name → input slot index`. The caller packs i64 values
    /// into a `Vec<i64>` at these indices before calling.
    pub input_offsets: HashMap<String, usize>,
    /// `var_name → output slot index`. The caller reads i64 values
    /// from these indices after calling.
    pub output_offsets: HashMap<String, usize>,
    /// Per-output, the value kind needed to unpack the i64 back
    /// into a `Value`. We need this because i64 == 1 could mean
    /// `Int(1)`, `Bool(true)`, or `Enum{variant_idx: 1}`
    /// depending on the var's source type.
    pub output_kinds: HashMap<String, OutputKind>,
    /// Per-input, the value kind needed to pack a `Value` into
    /// i64 before calling. Mostly mirrors `output_kinds` for
    /// outputs that loop back as inputs.
    pub input_kinds: HashMap<String, OutputKind>,
    /// Enum variant tables: `enum_name → variant_name → i64
    /// tag`. Used to pack/unpack enum values across the boundary.
    pub enum_tags: HashMap<String, HashMap<String, i64>>,
    /// Reverse lookup: `enum_name → tag → variant_name`. Used to
    /// rebuild Value::Enum from output i64.
    pub enum_variants: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputKind {
    Int,
    Bool,
    Enum(String),  // enum_name
}

impl JitProgram {
    /// Call the compiled function. Packs `given` into the input
    /// buffer, calls the JIT'd code, and unpacks the output
    /// buffer into a `HashMap<String, Value>`.
    pub fn call(&self, given: &HashMap<String, Value>) -> Option<HashMap<String, Value>> {
        let n_in = self.input_offsets.len();
        let n_out = self.output_offsets.len();
        let mut inputs:  Vec<i64> = vec![0; n_in];
        let mut outputs: Vec<i64> = vec![0; n_out];
        for (name, &idx) in &self.input_offsets {
            let value = given.get(name)?;
            let kind  = self.input_kinds.get(name)?;
            inputs[idx] = self.pack(value, kind)?;
        }
        // SAFETY: `func` was built against a JIT module that's
        // alive for the lifetime of self. We pass valid pointers
        // to in-bounds buffers of the declared lengths. The
        // generated code only reads inputs[0..n_in] and writes
        // outputs[0..n_out].
        unsafe {
            (self.func)(inputs.as_ptr(), outputs.as_mut_ptr());
        }
        let mut out = HashMap::new();
        for (name, &idx) in &self.output_offsets {
            let kind = self.output_kinds.get(name)?;
            let v    = self.unpack(outputs[idx], kind)?;
            out.insert(name.clone(), v);
        }
        Some(out)
    }

    fn pack(&self, value: &Value, kind: &OutputKind) -> Option<i64> {
        match (kind, value) {
            (OutputKind::Int,  Value::Int(n))  => Some(*n),
            (OutputKind::Bool, Value::Bool(b)) => Some(if *b { 1 } else { 0 }),
            (OutputKind::Enum(en), Value::Enum { variant, .. }) => {
                self.enum_tags.get(en)?.get(variant).copied()
            }
            _ => None,
        }
    }

    fn unpack(&self, raw: i64, kind: &OutputKind) -> Option<Value> {
        match kind {
            OutputKind::Int  => Some(Value::Int(raw)),
            OutputKind::Bool => Some(Value::Bool(raw != 0)),
            OutputKind::Enum(en) => {
                let variants = self.enum_variants.get(en)?;
                let idx = raw as usize;
                let variant = variants.get(idx)?.clone();
                Some(Value::Enum {
                    enum_name: en.clone(),
                    variant,
                    fields: vec![],
                })
            }
        }
    }
}

/// Try to compile a `Z3Program` into native code. Returns None
/// when any step contains an operation v1 doesn't yet emit IR for
/// — the caller falls back to the AST walker for those programs.
pub fn compile_program<'ctx>(
    program: &Z3Program<'ctx>,
    enums: &EnumRegistry,
) -> Option<JitProgram> {
    // Phase 1: determine input names + types from the program
    // (every UNINTERPRETED var referenced by any expression) and
    // output names + types from the steps.
    let mut input_names_set: std::collections::BTreeSet<(String, OutputKind)> =
        std::collections::BTreeSet::new();
    let mut output_kinds_local: Vec<(String, OutputKind)> = Vec::new();
    let mut enum_tags: HashMap<String, HashMap<String, i64>> = HashMap::new();
    let mut enum_variants: HashMap<String, Vec<String>> = HashMap::new();
    // Pre-populate enum tables from the registry so we can pack
    // any enum value the program might reference.
    {
        let by_name = enums.by_name.borrow();
        for (enum_name, (_dt, variants)) in by_name.iter() {
            // Skip enums that have ANY payload-bearing variant —
            // the JIT can only handle 0-arity variants in v1.
            // Programs that use them fall back to the AST walker.
            if variants.iter().any(|v| !v.fields.is_empty()) { continue; }
            let mut tags: HashMap<String, i64> = HashMap::new();
            let mut names: Vec<String> = Vec::with_capacity(variants.len());
            for (idx, v) in variants.iter().enumerate() {
                tags.insert(v.name.clone(), idx as i64);
                names.push(v.name.clone());
            }
            enum_tags.insert(enum_name.clone(), tags);
            enum_variants.insert(enum_name.clone(), names);
        }
    }

    for step in &program.steps {
        let var = match step {
            Z3Step::Scalar  { var, .. }
            | Z3Step::Seq      { var, .. }
            | Z3Step::Guarded  { var, .. } => var.clone(),
        };
        // For now only Scalar and Guarded scalar-bodied steps
        // compile. Seq outputs aren't supported in v1.
        let kind = match step {
            Z3Step::Scalar { expr, .. } => kind_of_dynamic(expr, &enum_variants)?,
            Z3Step::Seq { .. } => return None,
            Z3Step::Guarded { branches, .. } => {
                let mut bk: Option<OutputKind> = None;
                for b in branches {
                    let k = match &b.body {
                        GuardedBody::Scalar(e) => kind_of_dynamic(e, &enum_variants)?,
                        GuardedBody::Seq(_)    => return None,
                    };
                    bk = Some(k);
                }
                bk?
            }
        };
        output_kinds_local.push((var.clone(), kind));
        // Also collect Identifiers referenced by all steps as
        // inputs.
        match step {
            Z3Step::Scalar { expr, .. } => collect_inputs(expr, &mut input_names_set, &enum_variants),
            Z3Step::Seq    { elem_exprs, .. } => {
                for e in elem_exprs { collect_inputs(e, &mut input_names_set, &enum_variants); }
            }
            Z3Step::Guarded { branches, .. } => {
                for b in branches {
                    collect_inputs(&b.guard, &mut input_names_set, &enum_variants);
                    match &b.body {
                        GuardedBody::Scalar(e) => collect_inputs(e, &mut input_names_set, &enum_variants),
                        GuardedBody::Seq(es)   =>
                            for e in es { collect_inputs(e, &mut input_names_set, &enum_variants); },
                    }
                }
            }
        }
    }
    // Filter inputs: anything that's ALSO an output is computed,
    // not provided externally.
    let output_set: std::collections::HashSet<String> = output_kinds_local.iter()
        .map(|(n, _)| n.clone()).collect();
    let input_names: Vec<(String, OutputKind)> = input_names_set.into_iter()
        .filter(|(n, _)| !output_set.contains(n)).collect();

    // Phase 2: Cranelift IR generation.
    let mut flag_builder = settings::builder();
    flag_builder.set("use_colocated_libcalls", "false").ok()?;
    flag_builder.set("is_pic", "false").ok()?;
    let isa_builder = cranelift_native::builder().ok()?;
    let isa = isa_builder.finish(settings::Flags::new(flag_builder)).ok()?;
    let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    let mut module = JITModule::new(builder);

    let pointer_type = module.target_config().pointer_type();
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(pointer_type));   // *const i64 (inputs)
    sig.params.push(AbiParam::new(pointer_type));   // *mut i64  (outputs)

    let func_id = module.declare_function("compiled_program", Linkage::Local, &sig).ok()?;
    let mut ctx = module.make_context();
    ctx.func.signature = sig;

    // Precompute the output and input layouts so they survive
    // the FunctionBuilder's borrow scope below.
    let input_offsets: HashMap<String, usize> = input_names.iter().enumerate()
        .map(|(i, (n, _))| (n.clone(), i)).collect();
    let mut output_offsets: HashMap<String, usize> = HashMap::new();
    let mut output_kinds: HashMap<String, OutputKind> = HashMap::new();
    for (i, (name, kind)) in output_kinds_local.iter().enumerate() {
        output_offsets.insert(name.clone(), i);
        output_kinds.insert(name.clone(), kind.clone());
    }

    let mut func_ctx = FunctionBuilderContext::new();
    {
        let mut bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
        let entry = bcx.create_block();
        bcx.append_block_params_for_function_params(entry);
        bcx.switch_to_block(entry);
        bcx.seal_block(entry);

        let inputs_ptr  = bcx.block_params(entry)[0];
        let outputs_ptr = bcx.block_params(entry)[1];

        // Pre-load inputs from the inputs_ptr buffer.
        let mut env: HashMap<String, Value_> = HashMap::new();
        for (name, idx) in &input_offsets {
            let v = bcx.ins().load(types::I64, MemFlags::new(),
                inputs_ptr, (idx * 8) as i32);
            env.insert(name.clone(), Value_ { v, kind: input_names.iter()
                .find(|(n,_)| n == name).map(|(_,k)| k.clone()).unwrap_or(OutputKind::Int) });
        }

        for step in &program.steps {
            match step {
                Z3Step::Scalar { var, expr } => {
                    let v = emit_dynamic(&mut bcx, expr, &env, &enum_tags, &enum_variants)?;
                    let idx = output_offsets[var];
                    bcx.ins().store(MemFlags::new(), v.v, outputs_ptr, (idx * 8) as i32);
                    env.insert(var.clone(), v);
                }
                Z3Step::Guarded { var, branches } => {
                    // Cascade: pick the first true guard.
                    // Synthesize nested SELECTs. Default value
                    // (when all guards false) is 0 — the caller's
                    // eval would have returned None, but compiled
                    // code can't gracefully fail mid-execution.
                    let mut result = bcx.ins().iconst(types::I64, 0);
                    for b in branches.iter().rev() {
                        let guard = emit_dynamic(&mut bcx, &b.guard, &env, &enum_tags, &enum_variants)?;
                        let body_v = match &b.body {
                            GuardedBody::Scalar(e) =>
                                emit_dynamic(&mut bcx, e, &env, &enum_tags, &enum_variants)?,
                            GuardedBody::Seq(_) => return None,
                        };
                        result = bcx.ins().select(guard.v, body_v.v, result);
                    }
                    let idx = output_offsets[var];
                    bcx.ins().store(MemFlags::new(), result, outputs_ptr, (idx * 8) as i32);
                    // Synthetic kind from output_kinds_local for env.
                    let kind = output_kinds.get(var).cloned().unwrap_or(OutputKind::Int);
                    env.insert(var.clone(), Value_ { v: result, kind });
                }
                Z3Step::Seq { .. } => return None,
            }
        }

        bcx.ins().return_(&[]);
        bcx.finalize();
    }

    module.define_function(func_id, &mut ctx).ok()?;
    module.clear_context(&mut ctx);
    module.finalize_definitions().ok()?;
    let code_ptr = module.get_finalized_function(func_id);
    let func: unsafe extern "C" fn(*const i64, *mut i64) = unsafe {
        std::mem::transmute(code_ptr)
    };
    let input_kinds: HashMap<String, OutputKind> = input_names.into_iter().collect();
    Some(JitProgram {
        _module: module,
        func,
        input_offsets,
        output_offsets,
        output_kinds,
        input_kinds,
        enum_tags,
        enum_variants,
    })
}

/// IR-level temporary: the Cranelift Value plus its kind, so we
/// can compare/branch correctly.
#[derive(Clone)]
struct Value_ {
    v: ClValue,
    kind: OutputKind,
}

fn emit_dynamic<'ctx>(
    bcx: &mut FunctionBuilder,
    e: &Dynamic<'ctx>,
    env: &HashMap<String, Value_>,
    enum_tags: &HashMap<String, HashMap<String, i64>>,
    enum_variants: &HashMap<String, Vec<String>>,
) -> Option<Value_> {
    match e.kind() {
        AstKind::Numeral => {
            let i = e.as_int().and_then(|x| x.as_i64())?;
            Some(Value_ { v: bcx.ins().iconst(types::I64, i), kind: OutputKind::Int })
        }
        AstKind::App => {
            let decl = e.safe_decl().ok()?;
            let kind = decl.kind();
            let children: Vec<Dynamic<'ctx>> = e.children();
            match kind {
                DeclKind::TRUE  => Some(Value_ { v: bcx.ins().iconst(types::I64, 1), kind: OutputKind::Bool }),
                DeclKind::FALSE => Some(Value_ { v: bcx.ins().iconst(types::I64, 0), kind: OutputKind::Bool }),
                DeclKind::UNINTERPRETED => {
                    if !children.is_empty() { return None; }
                    let name = decl.name();
                    env.get(&name).cloned()
                }
                DeclKind::ITE => {
                    let c = emit_dynamic(bcx, &children[0], env, enum_tags, enum_variants)?;
                    let a = emit_dynamic(bcx, &children[1], env, enum_tags, enum_variants)?;
                    let b = emit_dynamic(bcx, &children[2], env, enum_tags, enum_variants)?;
                    let r = bcx.ins().select(c.v, a.v, b.v);
                    Some(Value_ { v: r, kind: a.kind })
                }
                DeclKind::EQ => {
                    let a = emit_dynamic(bcx, &children[0], env, enum_tags, enum_variants)?;
                    let b = emit_dynamic(bcx, &children[1], env, enum_tags, enum_variants)?;
                    let r = bcx.ins().icmp(IntCC::Equal, a.v, b.v);
                    let r = bcx.ins().uextend(types::I64, r);
                    Some(Value_ { v: r, kind: OutputKind::Bool })
                }
                DeclKind::ADD => {
                    let mut iter = children.iter();
                    let first = emit_dynamic(bcx, iter.next()?, env, enum_tags, enum_variants)?;
                    let mut acc = first.v;
                    for c in iter {
                        let x = emit_dynamic(bcx, c, env, enum_tags, enum_variants)?;
                        acc = bcx.ins().iadd(acc, x.v);
                    }
                    Some(Value_ { v: acc, kind: OutputKind::Int })
                }
                DeclKind::SUB => {
                    let mut iter = children.iter();
                    let first = emit_dynamic(bcx, iter.next()?, env, enum_tags, enum_variants)?;
                    let mut acc = first.v;
                    for c in iter {
                        let x = emit_dynamic(bcx, c, env, enum_tags, enum_variants)?;
                        acc = bcx.ins().isub(acc, x.v);
                    }
                    Some(Value_ { v: acc, kind: OutputKind::Int })
                }
                DeclKind::UMINUS => {
                    let x = emit_dynamic(bcx, &children[0], env, enum_tags, enum_variants)?;
                    let neg = bcx.ins().ineg(x.v);
                    Some(Value_ { v: neg, kind: OutputKind::Int })
                }
                DeclKind::MUL => {
                    let mut iter = children.iter();
                    let first = emit_dynamic(bcx, iter.next()?, env, enum_tags, enum_variants)?;
                    let mut acc = first.v;
                    for c in iter {
                        let x = emit_dynamic(bcx, c, env, enum_tags, enum_variants)?;
                        acc = bcx.ins().imul(acc, x.v);
                    }
                    Some(Value_ { v: acc, kind: OutputKind::Int })
                }
                DeclKind::IDIV | DeclKind::DIV => {
                    let a = emit_dynamic(bcx, &children[0], env, enum_tags, enum_variants)?;
                    let b = emit_dynamic(bcx, &children[1], env, enum_tags, enum_variants)?;
                    Some(Value_ { v: bcx.ins().sdiv(a.v, b.v), kind: OutputKind::Int })
                }
                DeclKind::LT => emit_icmp(bcx, &children, env, enum_tags, enum_variants, IntCC::SignedLessThan),
                DeclKind::LE => emit_icmp(bcx, &children, env, enum_tags, enum_variants, IntCC::SignedLessThanOrEqual),
                DeclKind::GT => emit_icmp(bcx, &children, env, enum_tags, enum_variants, IntCC::SignedGreaterThan),
                DeclKind::GE => emit_icmp(bcx, &children, env, enum_tags, enum_variants, IntCC::SignedGreaterThanOrEqual),
                DeclKind::AND => {
                    let mut iter = children.iter();
                    let first = emit_dynamic(bcx, iter.next()?, env, enum_tags, enum_variants)?;
                    let mut acc = first.v;
                    for c in iter {
                        let x = emit_dynamic(bcx, c, env, enum_tags, enum_variants)?;
                        acc = bcx.ins().band(acc, x.v);
                    }
                    Some(Value_ { v: acc, kind: OutputKind::Bool })
                }
                DeclKind::OR => {
                    let mut iter = children.iter();
                    let first = emit_dynamic(bcx, iter.next()?, env, enum_tags, enum_variants)?;
                    let mut acc = first.v;
                    for c in iter {
                        let x = emit_dynamic(bcx, c, env, enum_tags, enum_variants)?;
                        acc = bcx.ins().bor(acc, x.v);
                    }
                    Some(Value_ { v: acc, kind: OutputKind::Bool })
                }
                DeclKind::NOT => {
                    let x = emit_dynamic(bcx, &children[0], env, enum_tags, enum_variants)?;
                    let one = bcx.ins().iconst(types::I64, 1);
                    Some(Value_ { v: bcx.ins().bxor(x.v, one), kind: OutputKind::Bool })
                }
                DeclKind::DT_CONSTRUCTOR => {
                    // 0-arity constructor → look up tag.
                    if !children.is_empty() { return None; }
                    let variant = decl.name();
                    // Find the enum by scanning enum_tags.
                    for (en, tags) in enum_tags {
                        if let Some(&tag) = tags.get(&variant) {
                            return Some(Value_ {
                                v: bcx.ins().iconst(types::I64, tag),
                                kind: OutputKind::Enum(en.clone()),
                            });
                        }
                    }
                    None
                }
                DeclKind::DT_RECOGNISER | DeclKind::DT_IS => {
                    // ((_ is Variant) val) — compare tag.
                    let val = emit_dynamic(bcx, &children[0], env, enum_tags, enum_variants)?;
                    let OutputKind::Enum(en) = &val.kind else { return None };
                    let target = crate::z3_eval::extract_is_variant_pub(&format!("{e}"))?;
                    let tag = enum_tags.get(en)?.get(&target).copied()?;
                    let tag_v = bcx.ins().iconst(types::I64, tag);
                    let r = bcx.ins().icmp(IntCC::Equal, val.v, tag_v);
                    let r = bcx.ins().uextend(types::I64, r);
                    Some(Value_ { v: r, kind: OutputKind::Bool })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn emit_icmp<'ctx>(
    bcx: &mut FunctionBuilder,
    children: &[Dynamic<'ctx>],
    env: &HashMap<String, Value_>,
    enum_tags: &HashMap<String, HashMap<String, i64>>,
    enum_variants: &HashMap<String, Vec<String>>,
    cc: IntCC,
) -> Option<Value_> {
    let a = emit_dynamic(bcx, &children[0], env, enum_tags, enum_variants)?;
    let b = emit_dynamic(bcx, &children[1], env, enum_tags, enum_variants)?;
    let r = bcx.ins().icmp(cc, a.v, b.v);
    let r = bcx.ins().uextend(types::I64, r);
    Some(Value_ { v: r, kind: OutputKind::Bool })
}

fn kind_of_dynamic<'ctx>(
    e: &Dynamic<'ctx>,
    enum_variants: &HashMap<String, Vec<String>>,
) -> Option<OutputKind> {
    let sort = e.get_sort();
    let sort_name = format!("{sort}");
    if sort_name == "Int" || sort_name == "Real" {
        return Some(OutputKind::Int);
    }
    if sort_name == "Bool" {
        return Some(OutputKind::Bool);
    }
    // Datatype sort: name matches an enum.
    for (en, _) in enum_variants {
        if &sort_name == en {
            return Some(OutputKind::Enum(en.clone()));
        }
    }
    None
}

fn collect_inputs<'ctx>(
    e: &Dynamic<'ctx>,
    out: &mut std::collections::BTreeSet<(String, OutputKind)>,
    enum_variants: &HashMap<String, Vec<String>>,
) {
    match e.kind() {
        AstKind::App => {
            if let Ok(decl) = e.safe_decl() {
                if decl.kind() == DeclKind::UNINTERPRETED && e.num_children() == 0 {
                    let name = decl.name();
                    if let Some(k) = kind_of_dynamic(e, enum_variants) {
                        out.insert((name, k));
                    }
                    return;
                }
            }
            for c in e.children() {
                collect_inputs(&c, out, enum_variants);
            }
        }
        _ => {}
    }
}
