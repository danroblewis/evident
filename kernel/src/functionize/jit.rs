//! Cranelift JIT backend for scalar Int/Bool `Z3Step`s.
//!
//! Each compiled step becomes a native `extern "C" fn(*const i64) -> i64`:
//! the caller packs the step's referenced input values (Int as itself, Bool
//! as 0/1) into a flat i64 array in `inputs` order, the JIT'd code computes
//! the scalar result and returns it. The tick loop prefers calling this over
//! the AST interpreter (`super::eval`) when a step compiled.
//!
//! Scope: integer arithmetic (`+ - * unary-`), comparisons, boolean
//! connectives, and `ite`. Strings, datatypes, Seqs, and integer `div`/`mod`
//! are NOT compiled here — `emit` returns `None` for them and the step falls
//! back to the interpreter. This mirrors the legacy backend's "refuse, don't
//! guess" discipline (legacy-rust/functionizer/src/cranelift.rs).

use std::collections::HashMap;

use cranelift::prelude::*;
use cranelift::prelude::settings::Configurable;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module};
use z3_sys::*;

use crate::tick::Sv;
use super::{children, decl_kind, ast_app_name};

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

/// Try to JIT-compile a scalar Int/Bool expression. `inputs` is the ordered
/// list of free-variable names the expression references (caller-collected via
/// `super::collect_inputs`). Returns `None` if any node is unsupported.
pub unsafe fn compile_step(ctx: Z3_context, expr: Z3_ast, inputs: &[String]) -> Option<JitStep> {
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

        let v = emit(ctx, &mut bcx, expr, inputs_ptr, &offsets)?;
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
        DeclKind::ADD | DeclKind::MUL | DeclKind::SUB => {
            let mut it = ch.iter();
            let mut acc = emit(ctx, bcx, *it.next()?, inputs_ptr, offsets)?;
            for &c in it {
                let v = emit(ctx, bcx, c, inputs_ptr, offsets)?;
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
            let v = emit(ctx, bcx, ch[0], inputs_ptr, offsets)?;
            Some(bcx.ins().ineg(v))
        }
        DeclKind::LE | DeclKind::LT | DeclKind::GE | DeclKind::GT | DeclKind::EQ | DeclKind::IFF => {
            let l = emit(ctx, bcx, ch[0], inputs_ptr, offsets)?;
            let r = emit(ctx, bcx, ch[1], inputs_ptr, offsets)?;
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
            let v = emit(ctx, bcx, ch[0], inputs_ptr, offsets)?;
            let one = bcx.ins().iconst(types::I64, 1);
            Some(bcx.ins().bxor(v, one))
        }
        DeclKind::AND => {
            let mut it = ch.iter();
            let mut acc = emit(ctx, bcx, *it.next()?, inputs_ptr, offsets)?;
            for &c in it {
                let v = emit(ctx, bcx, c, inputs_ptr, offsets)?;
                acc = bcx.ins().band(acc, v);
            }
            Some(acc)
        }
        DeclKind::OR => {
            let mut it = ch.iter();
            let mut acc = emit(ctx, bcx, *it.next()?, inputs_ptr, offsets)?;
            for &c in it {
                let v = emit(ctx, bcx, c, inputs_ptr, offsets)?;
                acc = bcx.ins().bor(acc, v);
            }
            Some(acc)
        }
        DeclKind::IMPLIES => {
            let p = emit(ctx, bcx, ch[0], inputs_ptr, offsets)?;
            let q = emit(ctx, bcx, ch[1], inputs_ptr, offsets)?;
            let one = bcx.ins().iconst(types::I64, 1);
            let np = bcx.ins().bxor(p, one);
            Some(bcx.ins().bor(np, q))
        }
        DeclKind::ITE => {
            let c = emit(ctx, bcx, ch[0], inputs_ptr, offsets)?;
            let t = emit(ctx, bcx, ch[1], inputs_ptr, offsets)?;
            let e = emit(ctx, bcx, ch[2], inputs_ptr, offsets)?;
            Some(bcx.ins().select(c, t, e))
        }
        _ => None,
    }
}
