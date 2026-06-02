//! Cranelift JIT backend for scalar Int/Bool `Z3Step`s.
//!
//! Each compiled step becomes a native `extern "C" fn(*const i64) -> i64`:
//! the caller packs the step's referenced input values (Int as itself, Bool
//! as 0/1) into a flat i64 array in `inputs` order, the JIT'd code computes
//! the scalar result and returns it. The tick loop prefers calling this over
//! the AST interpreter (`super::eval`) when a step compiled.
//!
//! Scope: integer arithmetic (`+ - * unary-`), comparisons, boolean
//! connectives, and `ite`. Strings, datatypes, and integer `div`/`mod` are NOT
//! compiled here — `emit` returns `None` for them and the step falls back to
//! the interpreter. This mirrors the legacy backend's "refuse, don't guess"
//! discipline (legacy-rust/functionizer/src/cranelift.rs).
//!
//! Record-Seqs of *fixed, literal* size are supported: `(select rs i)` and
//! `(accessor (mk_T …))` are resolved against the recomposed element ASTs at
//! compile time, so a scalar step like `rs[0].w + rs[1].w` collapses to a pure
//! Int expression over the elements' leaf fields and JITs natively. (Symbolic
//! length/index Seqs are out of scope — they never reach here.)

use std::collections::{HashMap, HashSet};

use cranelift::prelude::*;
use cranelift::prelude::settings::Configurable;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use z3_sys::*;

use crate::tick::Sv;
use super::{children, decl_kind, ast_app_name, accessor_field_index, numeral_i64};

/// Resolve `a` to a datatype-constructor application, transparently indexing
/// fixed-size record-Seqs via `seqs`. Returns `None` if `a` isn't (or doesn't
/// reduce to) a constructor application.
unsafe fn resolve_to_ctor(ctx: Z3_context, a: Z3_ast, seqs: &HashMap<String, Vec<Z3_ast>>) -> Option<Z3_ast> {
    match decl_kind(ctx, a)? {
        DeclKind::DT_CONSTRUCTOR => Some(a),
        DeclKind::SELECT => {
            let ch = children(ctx, a);
            if ch.len() != 2 { return None; }
            let arr = ast_app_name(ctx, ch[0])?;
            let idx = numeral_i64(ctx, ch[1])?;
            let elem = *seqs.get(&arr)?.get(idx as usize)?;
            resolve_to_ctor(ctx, elem, seqs)
        }
        _ => None,
    }
}

/// Walk a candidate scalar expression, collecting the leaf Int/Bool variable
/// names it references (after resolving record-Seq indexing). Returns `false`
/// if any node is un-JITable (string, bare datatype/array const, unsupported
/// op) — the caller then leaves the step to the interpreter.
unsafe fn collect_jit_inputs(
    ctx: Z3_context,
    a: Z3_ast,
    seqs: &HashMap<String, Vec<Z3_ast>>,
    out: &mut HashSet<String>,
) -> bool {
    let kind = Z3_get_ast_kind(ctx, a);
    if kind == AstKind::Numeral {
        return true;
    }
    if kind != AstKind::App || Z3_is_string(ctx, a) {
        return false;
    }
    let Some(dk) = decl_kind(ctx, a) else { return false };
    let ch = children(ctx, a);
    match dk {
        DeclKind::TRUE | DeclKind::FALSE => true,
        DeclKind::UNINTERPRETED if ch.is_empty() => {
            // Only Int/Bool leaves are JIT-packable; a bare Seq/datatype const is not.
            let k = Z3_get_sort_kind(ctx, Z3_get_sort(ctx, a));
            if k == SortKind::Int || k == SortKind::Bool {
                if let Some(n) = ast_app_name(ctx, a) { out.insert(n); }
                true
            } else {
                false
            }
        }
        DeclKind::SELECT => {
            if ch.len() != 2 { return false; }
            let Some(arr) = ast_app_name(ctx, ch[0]) else { return false };
            let Some(idx) = numeral_i64(ctx, ch[1]) else { return false };
            match seqs.get(&arr).and_then(|es| es.get(idx as usize)) {
                Some(&elem) => collect_jit_inputs(ctx, elem, seqs, out),
                None => false,
            }
        }
        DeclKind::DT_ACCESSOR => {
            let Some(ctor) = resolve_to_ctor(ctx, ch[0], seqs) else { return false };
            let Some(fi) = accessor_field_index(ctx, a) else { return false };
            match children(ctx, ctor).get(fi) {
                Some(&field) => collect_jit_inputs(ctx, field, seqs, out),
                None => false,
            }
        }
        DeclKind::ADD | DeclKind::MUL | DeclKind::SUB | DeclKind::UMINUS
        | DeclKind::LE | DeclKind::LT | DeclKind::GE | DeclKind::GT
        | DeclKind::EQ | DeclKind::IFF | DeclKind::NOT | DeclKind::AND
        | DeclKind::OR | DeclKind::IMPLIES | DeclKind::ITE => {
            ch.iter().all(|&c| collect_jit_inputs(ctx, c, seqs, out))
        }
        _ => false,
    }
}

pub struct JitStep {
    _module: JITModule,
    func: unsafe extern "C" fn(*const i64) -> i64,
    /// Input variable names, in the order the compiled code indexes them.
    pub inputs: Vec<String>,
}

impl JitStep {
    /// Pack `env` into the i64 input array and run the compiled function.
    /// Returns `None` if a referenced input is missing or non-scalar.
    pub fn call(&self, env: &HashMap<String, Sv>) -> Option<i64> {
        let mut args: Vec<i64> = Vec::with_capacity(self.inputs.len());
        for name in &self.inputs {
            let v = match env.get(name)? {
                Sv::Int(n) => *n,
                Sv::Bool(b) => *b as i64,
                _ => return None,
            };
            args.push(v);
        }
        let ptr = if args.is_empty() { std::ptr::null() } else { args.as_ptr() };
        Some(unsafe { (self.func)(ptr) })
    }
}

/// Try to JIT-compile a scalar Int/Bool expression, resolving any fixed-size
/// record-Seq indexing via `seqs` (`var → element ASTs`). The leaf Int/Bool
/// inputs are discovered by `collect_jit_inputs`. Returns `None` if any node is
/// unsupported (string, symbolic Seq, `div`/`mod`, …) ⇒ falls back to interp.
pub unsafe fn compile_step(
    ctx: Z3_context,
    expr: Z3_ast,
    seqs: &HashMap<String, Vec<Z3_ast>>,
) -> Option<JitStep> {
    let mut input_set: HashSet<String> = HashSet::new();
    if !collect_jit_inputs(ctx, expr, seqs, &mut input_set) {
        return None;
    }
    let inputs: Vec<String> = input_set.into_iter().collect();
    let inputs = inputs.as_slice();

    let mut flag_builder = settings::builder();
    flag_builder.set("use_colocated_libcalls", "false").ok()?;
    flag_builder.set("is_pic", "false").ok()?;
    let isa_builder = cranelift_native::builder().ok()?;
    let isa = isa_builder.finish(settings::Flags::new(flag_builder)).ok()?;
    let builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    let mut module = JITModule::new(builder);
    let ptr_t = module.target_config().pointer_type();

    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(ptr_t)); // *const i64 inputs
    sig.returns.push(AbiParam::new(types::I64));
    let func_id = module.declare_function("fz_step", Linkage::Local, &sig).ok()?;
    let mut cctx = module.make_context();
    cctx.func.signature = sig;

    let offsets: HashMap<String, i32> = inputs.iter().enumerate()
        .map(|(i, n)| (n.clone(), (i * 8) as i32)).collect();

    {
        let mut fctx = FunctionBuilderContext::new();
        let mut bcx = FunctionBuilder::new(&mut cctx.func, &mut fctx);
        let entry = bcx.create_block();
        bcx.append_block_params_for_function_params(entry);
        bcx.switch_to_block(entry);
        bcx.seal_block(entry);
        let inputs_ptr = bcx.block_params(entry)[0];

        let v = emit(ctx, &mut bcx, expr, inputs_ptr, &offsets, seqs)?;
        bcx.ins().return_(&[v]);
        bcx.finalize();
    }

    module.define_function(func_id, &mut cctx).ok()?;
    module.clear_context(&mut cctx);
    module.finalize_definitions().ok()?;
    let code = module.get_finalized_function(func_id);
    let func: unsafe extern "C" fn(*const i64) -> i64 = std::mem::transmute(code);
    Some(JitStep { _module: module, func, inputs: inputs.to_vec() })
}

/// Lower `a` to an i64-valued Cranelift SSA value (Bool encoded 0/1).
unsafe fn emit(
    ctx: Z3_context,
    bcx: &mut FunctionBuilder,
    a: Z3_ast,
    inputs_ptr: Value,
    offsets: &HashMap<String, i32>,
    seqs: &HashMap<String, Vec<Z3_ast>>,
) -> Option<Value> {
    let kind = Z3_get_ast_kind(ctx, a);
    if kind == AstKind::Numeral {
        let mut n: i64 = 0;
        if Z3_get_numeral_int64(ctx, a, &mut n) {
            return Some(bcx.ins().iconst(types::I64, n));
        }
        return None;
    }
    if kind != AstKind::App {
        return None;
    }
    // Reject string literals (and any non-int/bool 0-arity literal).
    if Z3_is_string(ctx, a) {
        return None;
    }
    let dk = decl_kind(ctx, a)?;
    let ch = children(ctx, a);
    match dk {
        DeclKind::TRUE => Some(bcx.ins().iconst(types::I64, 1)),
        DeclKind::FALSE => Some(bcx.ins().iconst(types::I64, 0)),
        DeclKind::UNINTERPRETED => {
            if !ch.is_empty() { return None; }
            let name = ast_app_name(ctx, a)?;
            let off = *offsets.get(&name)?;
            Some(bcx.ins().load(types::I64, MemFlags::new(), inputs_ptr, off))
        }
        // `(select rs i)` over a fixed-size record-Seq ⇒ emit the i-th element.
        DeclKind::SELECT => {
            if ch.len() != 2 { return None; }
            let arr = ast_app_name(ctx, ch[0])?;
            let idx = numeral_i64(ctx, ch[1])?;
            let elem = *seqs.get(&arr)?.get(idx as usize)?;
            emit(ctx, bcx, elem, inputs_ptr, offsets, seqs)
        }
        // `(w r)` ⇒ resolve `r` to a constructor and emit the accessed field.
        DeclKind::DT_ACCESSOR => {
            let ctor = resolve_to_ctor(ctx, ch[0], seqs)?;
            let fi = accessor_field_index(ctx, a)?;
            let field = *children(ctx, ctor).get(fi)?;
            emit(ctx, bcx, field, inputs_ptr, offsets, seqs)
        }
        DeclKind::ADD | DeclKind::MUL | DeclKind::SUB => {
            let mut it = ch.iter();
            let mut acc = emit(ctx, bcx, *it.next()?, inputs_ptr, offsets, seqs)?;
            for &c in it {
                let v = emit(ctx, bcx, c, inputs_ptr, offsets, seqs)?;
                acc = match dk {
                    DeclKind::ADD => bcx.ins().iadd(acc, v),
                    DeclKind::MUL => bcx.ins().imul(acc, v),
                    DeclKind::SUB => bcx.ins().isub(acc, v),
                    _ => unreachable!(),
                };
            }
            Some(acc)
        }
        DeclKind::UMINUS => {
            let v = emit(ctx, bcx, ch[0], inputs_ptr, offsets, seqs)?;
            Some(bcx.ins().ineg(v))
        }
        DeclKind::LE | DeclKind::LT | DeclKind::GE | DeclKind::GT | DeclKind::EQ | DeclKind::IFF => {
            let l = emit(ctx, bcx, ch[0], inputs_ptr, offsets, seqs)?;
            let r = emit(ctx, bcx, ch[1], inputs_ptr, offsets, seqs)?;
            let cc = match dk {
                DeclKind::LE => IntCC::SignedLessThanOrEqual,
                DeclKind::LT => IntCC::SignedLessThan,
                DeclKind::GE => IntCC::SignedGreaterThanOrEqual,
                DeclKind::GT => IntCC::SignedGreaterThan,
                DeclKind::EQ | DeclKind::IFF => IntCC::Equal,
                _ => unreachable!(),
            };
            let c = bcx.ins().icmp(cc, l, r);
            Some(bcx.ins().uextend(types::I64, c))
        }
        DeclKind::NOT => {
            let v = emit(ctx, bcx, ch[0], inputs_ptr, offsets, seqs)?;
            let one = bcx.ins().iconst(types::I64, 1);
            Some(bcx.ins().bxor(v, one))
        }
        DeclKind::AND => {
            let mut it = ch.iter();
            let mut acc = emit(ctx, bcx, *it.next()?, inputs_ptr, offsets, seqs)?;
            for &c in it {
                let v = emit(ctx, bcx, c, inputs_ptr, offsets, seqs)?;
                acc = bcx.ins().band(acc, v);
            }
            Some(acc)
        }
        DeclKind::OR => {
            let mut it = ch.iter();
            let mut acc = emit(ctx, bcx, *it.next()?, inputs_ptr, offsets, seqs)?;
            for &c in it {
                let v = emit(ctx, bcx, c, inputs_ptr, offsets, seqs)?;
                acc = bcx.ins().bor(acc, v);
            }
            Some(acc)
        }
        DeclKind::IMPLIES => {
            let p = emit(ctx, bcx, ch[0], inputs_ptr, offsets, seqs)?;
            let q = emit(ctx, bcx, ch[1], inputs_ptr, offsets, seqs)?;
            let one = bcx.ins().iconst(types::I64, 1);
            let np = bcx.ins().bxor(p, one);
            Some(bcx.ins().bor(np, q))
        }
        DeclKind::ITE => {
            let c = emit(ctx, bcx, ch[0], inputs_ptr, offsets, seqs)?;
            let t = emit(ctx, bcx, ch[1], inputs_ptr, offsets, seqs)?;
            let e = emit(ctx, bcx, ch[2], inputs_ptr, offsets, seqs)?;
            Some(bcx.ins().select(c, t, e))
        }
        _ => None,
    }
}
