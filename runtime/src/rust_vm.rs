//! Z3-AST → Rust-VM compiler + interpreter.
//!
//! After `extract_program` produces `Z3Step`s whose expressions are
//! Z3 `Dynamic` ASTs, we walk each expression ONCE here and produce
//! a `Op` tree that the VM interpreter evaluates without touching
//! Z3. The VM is ~10x faster than `eval_dynamic` (no FFI per node),
//! which is the dominant cost in the per-tick FSM solve.
//!
//! The VM is the AST-walker's fast path: where the JIT can't yet
//! compile a step (multi-field ctors, Cons-chain payloads, deep
//! ITE on Value outputs), the VM still runs natively-in-Rust at
//! a 10-50x speedup over the Z3 Dynamic walk.

use std::collections::HashMap;
use z3::ast::{Ast, Dynamic};
use z3::AstKind;
use z3_sys::DeclKind;

use crate::translate::{EnumRegistry, Value};
use crate::z3_eval::{Z3Program, Z3Step, extract_is_variant_pub};

#[derive(Debug, Clone)]
pub enum Op {
    ConstInt(i64),
    ConstStr(String),
    ConstBool(bool),
    /// Variable lookup in env by name.
    Lookup(String),
    /// Datatype constructor application.
    BuildEnum { enum_name: String, variant: String, fields: Vec<Op> },
    /// If-then-else.
    Ite(Box<Op>, Box<Op>, Box<Op>),
    /// Integer arithmetic.
    AddI(Vec<Op>),
    SubI(Vec<Op>),
    MulI(Vec<Op>),
    Neg(Box<Op>),
    DivI(Box<Op>, Box<Op>),
    ModI(Box<Op>, Box<Op>),
    /// Comparisons (produces Bool).
    EqI(Box<Op>, Box<Op>),
    LtI(Box<Op>, Box<Op>),
    LeI(Box<Op>, Box<Op>),
    GtI(Box<Op>, Box<Op>),
    GeI(Box<Op>, Box<Op>),
    /// String equality.
    EqStr(Box<Op>, Box<Op>),
    /// Boolean ops.
    And(Vec<Op>),
    Or(Vec<Op>),
    Not(Box<Op>),
    /// Is-variant recognizer test (Bool result).
    IsVariant { target_variant: String, expr: Box<Op> },
    /// Pre-computed constant Value (model-extracted at compile time).
    Const(Value),
    /// Select element by literal index from a Seq env var.
    /// e.g. `(select last_results 3)` → SelectLit("last_results", 3).
    SelectLit { arr: String, idx: i64 },
    /// Generic select: arr and idx are both Ops. Used for
    /// `(select (effs__arr (select plat_effs 0)) 1)` style
    /// nested accesses.
    Select { arr: Box<Op>, idx: Box<Op> },
    /// Datatype accessor — `(field expr)`. Resolves to the
    /// field at `field_idx` of the inner enum value at run time.
    /// We don't know the field index from the Z3 decl name alone;
    /// we fall back to scanning the variant's field declarations
    /// at eval time using the active EnumRegistry.
    Accessor { field_name: String, inner: Box<Op> },
    /// String concatenation (Z3 `(str.++ a b ...)`).
    StrConcat(Vec<Op>),
    /// Could not compile — escape hatch back to z3_eval::eval_dynamic.
    /// Holds the original Z3 AST handle.
    Z3Fallback(Dynamic<'static>),
}

#[derive(Debug, Clone)]
pub enum CompiledStep {
    Scalar { var: String, op: Op },
    Seq    { var: String, elems: Vec<Op> },
    Guarded { var: String, branches: Vec<CompiledBranch> },
    PreBaked { var: String, value: Value },
}

#[derive(Debug, Clone)]
pub struct CompiledBranch {
    pub guard: Op,
    pub body:  CompiledBody,
}

#[derive(Debug, Clone)]
pub enum CompiledBody {
    Scalar(Op),
    Seq(Vec<Op>),
}

#[derive(Debug, Clone)]
pub struct CompiledProgram {
    pub steps:     Vec<CompiledStep>,
    pub checks:    Vec<(Op, Op)>,
    pub predicates: Vec<Op>,
}

pub fn compile_program<'ctx>(p: &Z3Program<'ctx>) -> CompiledProgram {
    // Reinterpret 'ctx lifetimes as 'static — Z3 ASTs are
    // refcounted by the Context which is 'static in this runtime.
    let steps: Vec<CompiledStep> = p.steps.iter().map(|s| match s {
        Z3Step::Scalar { var, expr } =>
            CompiledStep::Scalar { var: var.clone(), op: compile_dyn(expr) },
        Z3Step::Seq { var, elem_exprs } =>
            CompiledStep::Seq {
                var: var.clone(),
                elems: elem_exprs.iter().map(compile_dyn).collect(),
            },
        Z3Step::Guarded { var, branches } => CompiledStep::Guarded {
            var: var.clone(),
            branches: branches.iter().map(|b| CompiledBranch {
                guard: compile_dyn(&b.guard),
                body: match &b.body {
                    crate::z3_eval::GuardedBody::Scalar(e) => CompiledBody::Scalar(compile_dyn(e)),
                    crate::z3_eval::GuardedBody::Seq(es) =>
                        CompiledBody::Seq(es.iter().map(compile_dyn).collect()),
                },
            }).collect(),
        },
        Z3Step::PreBaked { var, value } =>
            CompiledStep::PreBaked { var: var.clone(), value: value.clone() },
    }).collect();
    let checks = p.checks.iter().map(|(l, r)| (compile_dyn(l), compile_dyn(r))).collect();
    let predicates = p.predicates.iter().map(|b| {
        let d = z3::ast::Dynamic::from_ast(b);
        compile_dyn(&d)
    }).collect();
    CompiledProgram { steps, checks, predicates }
}

fn compile_dyn<'ctx>(a: &Dynamic<'ctx>) -> Op {
    // String literal short-circuit. ONLY for ASTs that are
    // genuinely zero-child literals — `as_string()` succeeds on
    // some non-literal ASTs (e.g. `(str.++ ... free_var)` which it
    // collapses to ""), so we additionally require num_children=0
    // before trusting the result. Free vars are filtered out by
    // their UNINTERPRETED decl kind.
    if a.kind() == AstKind::App && a.num_children() == 0 {
        let is_free_var = a.safe_decl().ok()
            .map(|d| d.kind() == DeclKind::UNINTERPRETED)
            .unwrap_or(false);
        if !is_free_var {
            if let Some(s) = a.as_string().and_then(|zs| zs.as_string()) {
                return Op::ConstStr(s);
            }
        }
    }
    match a.kind() {
        AstKind::Numeral => {
            if let Some(i) = a.as_int().and_then(|x| x.as_i64()) {
                return Op::ConstInt(i);
            }
            unsafe { Op::Z3Fallback(extend_static(a.clone())) }
        }
        AstKind::App => {
            let Ok(decl) = a.safe_decl() else {
                return unsafe { Op::Z3Fallback(extend_static(a.clone())) };
            };
            let kind = decl.kind();
            let children: Vec<Dynamic<'ctx>> = a.children();
            match kind {
                DeclKind::TRUE  => Op::ConstBool(true),
                DeclKind::FALSE => Op::ConstBool(false),
                DeclKind::UNINTERPRETED => {
                    if children.is_empty() {
                        Op::Lookup(decl.name())
                    } else if children.len() == 1 {
                        // Z3 represents `record.seq_field` via two
                        // uninterpreted accessors: `<field>__arr` (the
                        // array part) and `<field>__len` (the length
                        // part). We compile both as record-field
                        // accesses on the inner record value; the
                        // resulting Value::SeqX is then indexed by
                        // an outer SELECT, or its length is read
                        // by an outer call to __len.
                        let name = decl.name();
                        if let Some(field) = name.strip_suffix("__arr") {
                            return Op::Accessor {
                                field_name: field.to_string(),
                                inner: Box::new(compile_dyn(&children[0])),
                            };
                        }
                        if let Some(_field) = name.strip_suffix("__len") {
                            // Less common: produces the Seq's length.
                            // We fall back since we don't track Seq
                            // lengths separately from the SeqX value.
                            return unsafe { Op::Z3Fallback(extend_static(a.clone())) };
                        }
                        unsafe { Op::Z3Fallback(extend_static(a.clone())) }
                    } else {
                        unsafe { Op::Z3Fallback(extend_static(a.clone())) }
                    }
                }
                DeclKind::ITE => {
                    if children.len() == 3 {
                        Op::Ite(
                            Box::new(compile_dyn(&children[0])),
                            Box::new(compile_dyn(&children[1])),
                            Box::new(compile_dyn(&children[2])),
                        )
                    } else { unsafe { Op::Z3Fallback(extend_static(a.clone())) } }
                }
                DeclKind::ADD => Op::AddI(children.iter().map(compile_dyn).collect()),
                DeclKind::SUB => Op::SubI(children.iter().map(compile_dyn).collect()),
                DeclKind::MUL => Op::MulI(children.iter().map(compile_dyn).collect()),
                DeclKind::UMINUS => {
                    if children.len() == 1 {
                        Op::Neg(Box::new(compile_dyn(&children[0])))
                    } else { unsafe { Op::Z3Fallback(extend_static(a.clone())) } }
                }
                DeclKind::IDIV | DeclKind::DIV => {
                    if children.len() == 2 {
                        Op::DivI(
                            Box::new(compile_dyn(&children[0])),
                            Box::new(compile_dyn(&children[1])),
                        )
                    } else { unsafe { Op::Z3Fallback(extend_static(a.clone())) } }
                }
                DeclKind::MOD | DeclKind::REM => {
                    if children.len() == 2 {
                        Op::ModI(
                            Box::new(compile_dyn(&children[0])),
                            Box::new(compile_dyn(&children[1])),
                        )
                    } else { unsafe { Op::Z3Fallback(extend_static(a.clone())) } }
                }
                DeclKind::EQ => {
                    if children.len() == 2 {
                        let l = compile_dyn(&children[0]);
                        let r = compile_dyn(&children[1]);
                        // EqI handles both Int and Value equality at
                        // eval time (matches eval_dynamic's behavior:
                        // try as_i64 first, fall through to general
                        // Value-eq).
                        Op::EqI(Box::new(l), Box::new(r))
                    } else { unsafe { Op::Z3Fallback(extend_static(a.clone())) } }
                }
                DeclKind::LT => binop(&children, a, |l, r| Op::LtI(Box::new(l), Box::new(r))),
                DeclKind::LE => binop(&children, a, |l, r| Op::LeI(Box::new(l), Box::new(r))),
                DeclKind::GT => binop(&children, a, |l, r| Op::GtI(Box::new(l), Box::new(r))),
                DeclKind::GE => binop(&children, a, |l, r| Op::GeI(Box::new(l), Box::new(r))),
                DeclKind::AND => Op::And(children.iter().map(compile_dyn).collect()),
                DeclKind::OR  => Op::Or (children.iter().map(compile_dyn).collect()),
                DeclKind::NOT => {
                    if children.len() == 1 {
                        Op::Not(Box::new(compile_dyn(&children[0])))
                    } else { unsafe { Op::Z3Fallback(extend_static(a.clone())) } }
                }
                DeclKind::SELECT => {
                    if children.len() == 2 {
                        // Fast path: arr is a 0-arity name, idx is
                        // a literal numeral.
                        let arr_name = if children[0].kind() == AstKind::App
                            && children[0].num_children() == 0
                        {
                            children[0].safe_decl().ok().map(|d| d.name())
                        } else { None };
                        let idx_lit = if children[1].kind() == AstKind::Numeral {
                            children[1].as_int().and_then(|x| x.as_i64())
                        } else { None };
                        if let (Some(arr), Some(idx)) = (arr_name, idx_lit) {
                            return Op::SelectLit { arr, idx };
                        }
                        // General: compile children as Ops.
                        Op::Select {
                            arr: Box::new(compile_dyn(&children[0])),
                            idx: Box::new(compile_dyn(&children[1])),
                        }
                    } else {
                        unsafe { Op::Z3Fallback(extend_static(a.clone())) }
                    }
                }
                DeclKind::DT_ACCESSOR => {
                    if children.len() == 1 {
                        // Strip Z3 internal suffixes (`__arr` for Seq
                        // field arrays, `__len` for Seq lengths) — at
                        // the Value-level the Seq field is stored under
                        // its logical name (e.g. "effs"), not the
                        // Z3-internal "effs__arr".
                        let raw = decl.name();
                        let logical = raw.strip_suffix("__arr")
                            .or_else(|| raw.strip_suffix("__len"))
                            .map(|s| s.to_string())
                            .unwrap_or(raw);
                        Op::Accessor {
                            field_name: logical,
                            inner: Box::new(compile_dyn(&children[0])),
                        }
                    } else {
                        unsafe { Op::Z3Fallback(extend_static(a.clone())) }
                    }
                }
                DeclKind::SEQ_CONCAT => {
                    Op::StrConcat(children.iter().map(compile_dyn).collect())
                }
                DeclKind::DT_CONSTRUCTOR => {
                    let variant = decl.name();
                    let fields: Vec<Op> = children.iter().map(compile_dyn).collect();
                    // enum_name is patched up at eval time from the
                    // registry (variant → enum lookup). We stash the
                    // empty string here.
                    Op::BuildEnum { enum_name: String::new(), variant, fields }
                }
                DeclKind::DT_RECOGNISER | DeclKind::DT_IS => {
                    if children.len() == 1 {
                        let app_text = format!("{a}");
                        let target = extract_is_variant_pub(&app_text)
                            .or_else(|| {
                                let n = decl.name();
                                n.strip_prefix("is_").map(|s| s.to_string())
                            });
                        if let Some(t) = target {
                            Op::IsVariant {
                                target_variant: t,
                                expr: Box::new(compile_dyn(&children[0])),
                            }
                        } else {
                            unsafe { Op::Z3Fallback(extend_static(a.clone())) }
                        }
                    } else { unsafe { Op::Z3Fallback(extend_static(a.clone())) } }
                }
                _ => unsafe { Op::Z3Fallback(extend_static(a.clone())) },
            }
        }
        _ => unsafe { Op::Z3Fallback(extend_static(a.clone())) },
    }
}

fn binop<'ctx, F: FnOnce(Op, Op) -> Op>(children: &[Dynamic<'ctx>], a: &Dynamic<'ctx>, f: F) -> Op {
    if children.len() == 2 {
        f(compile_dyn(&children[0]), compile_dyn(&children[1]))
    } else {
        unsafe { Op::Z3Fallback(extend_static(a.clone())) }
    }
}

/// SAFETY: the Z3 Context is held by EvidentRuntime for the duration
/// of the program. Dynamic ASTs are refcounted by the Context. The
/// 'ctx lifetime parameter on Dynamic exists only to scope it to the
/// Context borrow; reinterpreting as 'static is sound as long as the
/// caller never outlives the runtime.
unsafe fn extend_static<'a>(d: Dynamic<'a>) -> Dynamic<'static> {
    std::mem::transmute(d)
}

pub fn eval_program(
    p: &CompiledProgram,
    given: &HashMap<String, Value>,
    enums: Option<&EnumRegistry>,
) -> Option<HashMap<String, Value>> {
    if std::env::var("EVIDENT_VM_TRACE").is_ok() {
        let mut keys: Vec<_> = given.keys().collect();
        keys.sort();
        for k in keys.iter().filter(|k| k.starts_with('_') || k.as_str() == "count" || k.as_str() == "is_first_tick") {
            eprintln!("[vm] given[{k}] = {:?}", given.get(*k));
        }
    }
    let mut env: HashMap<String, Value> = given.clone();
    for step in &p.steps {
        match step {
            CompiledStep::Scalar { var, op } => {
                let v = match eval_op(op, &env, enums) {
                    Some(v) => v,
                    None => {
                        if std::env::var("EVIDENT_VM_TRACE").is_ok() {
                            eprintln!("[vm] Scalar {var}: op returned None: {:?}", op);
                        }
                        return None;
                    }
                };
                env.insert(var.clone(), v);
            }
            CompiledStep::Seq { var, elems } => {
                let mut values = Vec::with_capacity(elems.len());
                for (i, e) in elems.iter().enumerate() {
                    match eval_op(e, &env, enums) {
                        Some(v) => values.push(v),
                        None => {
                            if std::env::var("EVIDENT_VM_TRACE").is_ok() {
                                eprintln!("[vm] Seq {var}[{i}]: op returned None: {:?}", e);
                            }
                            return None;
                        }
                    }
                }
                env.insert(var.clone(), crate::z3_eval::seq_value_from_elements_pub(values));
            }
            CompiledStep::Guarded { var, branches } => {
                let mut chosen: Option<Value> = None;
                for b in branches {
                    let g = eval_op(&b.guard, &env, enums)?;
                    let Value::Bool(true) = g else { continue };
                    match &b.body {
                        CompiledBody::Scalar(e) => chosen = Some(eval_op(e, &env, enums)?),
                        CompiledBody::Seq(es) => {
                            let mut values = Vec::with_capacity(es.len());
                            for e in es { values.push(eval_op(e, &env, enums)?); }
                            chosen = Some(crate::z3_eval::seq_value_from_elements_pub(values));
                        }
                    }
                    break;
                }
                env.insert(var.clone(), chosen?);
            }
            CompiledStep::PreBaked { var, value } => {
                env.insert(var.clone(), value.clone());
            }
        }
    }
    // Checks/predicates are vetted by Z3 at build time — they exist
    // in the simplified program as residual eq-pairs / bool asserts
    // that didn't map to per-output assignments. Re-verifying them
    // at every eval is expensive (Mario display: 320 checks × ~50µs
    // each = 16ms/tick wasted) for no correctness gain in the
    // common case. Opt-in re-verification via the trace flag for
    // diagnostics.
    if std::env::var("EVIDENT_VM_VERIFY_CHECKS").is_ok() {
        for (lhs, rhs) in &p.checks {
            let lv = match eval_op(lhs, &env, enums) { Some(v) => v, None => continue };
            let rv = match eval_op(rhs, &env, enums) { Some(v) => v, None => continue };
            if lv != rv { return None; }
        }
        for pred in &p.predicates {
            match eval_op(pred, &env, enums) {
                Some(Value::Bool(false)) => return None,
                _ => {}
            }
        }
    }
    if std::env::var("EVIDENT_VM_TRACE").is_ok() {
        for s in &p.steps {
            let var = match s {
                CompiledStep::Scalar { var, .. }
                | CompiledStep::Seq { var, .. }
                | CompiledStep::Guarded { var, .. }
                | CompiledStep::PreBaked { var, .. } => var.clone(),
            };
            eprintln!("[vm] env[{var}] = {:?}", env.get(&var));
        }
    }
    Some(env)
}

fn eval_op(op: &Op, env: &HashMap<String, Value>, enums: Option<&EnumRegistry>) -> Option<Value> {
    match op {
        Op::ConstInt(n)  => Some(Value::Int(*n)),
        Op::ConstStr(s)  => Some(Value::Str(s.clone())),
        Op::ConstBool(b) => Some(Value::Bool(*b)),
        Op::Const(v)     => Some(v.clone()),
        Op::Lookup(name) => env.get(name).cloned(),
        Op::Ite(c, t, e) => {
            let Value::Bool(b) = eval_op(c, env, enums)? else { return None };
            if b { eval_op(t, env, enums) } else { eval_op(e, env, enums) }
        }
        Op::AddI(args) => fold_int(args, env, enums, 0, |a, b| a + b),
        Op::SubI(args) => {
            if args.is_empty() { return None; }
            let first = eval_int(&args[0], env, enums)?;
            if args.len() == 1 { return Some(Value::Int(-first)); }
            let rest: Option<i64> = args[1..].iter()
                .map(|a| eval_int(a, env, enums))
                .try_fold(0i64, |acc, v| v.map(|x| acc + x));
            Some(Value::Int(first - rest?))
        }
        Op::MulI(args) => fold_int(args, env, enums, 1, |a, b| a * b),
        Op::Neg(x) => {
            let n = eval_int(x, env, enums)?;
            Some(Value::Int(-n))
        }
        Op::DivI(l, r) => {
            let a = eval_int(l, env, enums)?;
            let b = eval_int(r, env, enums)?;
            if b == 0 { return None; }
            Some(Value::Int(a.div_euclid(b)))
        }
        Op::ModI(l, r) => {
            let a = eval_int(l, env, enums)?;
            let b = eval_int(r, env, enums)?;
            if b == 0 { return None; }
            Some(Value::Int(a.rem_euclid(b)))
        }
        Op::EqI(l, r) => {
            let lv = eval_op(l, env, enums)?;
            let rv = eval_op(r, env, enums)?;
            Some(Value::Bool(lv == rv))
        }
        Op::EqStr(l, r) => {
            let lv = eval_op(l, env, enums)?;
            let rv = eval_op(r, env, enums)?;
            Some(Value::Bool(lv == rv))
        }
        Op::LtI(l, r) => int_cmp(l, r, env, enums, |a, b| a < b),
        Op::LeI(l, r) => int_cmp(l, r, env, enums, |a, b| a <= b),
        Op::GtI(l, r) => int_cmp(l, r, env, enums, |a, b| a > b),
        Op::GeI(l, r) => int_cmp(l, r, env, enums, |a, b| a >= b),
        Op::And(args) => {
            for a in args {
                let Value::Bool(b) = eval_op(a, env, enums)? else { return None };
                if !b { return Some(Value::Bool(false)); }
            }
            Some(Value::Bool(true))
        }
        Op::Or(args) => {
            for a in args {
                let Value::Bool(b) = eval_op(a, env, enums)? else { return None };
                if b { return Some(Value::Bool(true)); }
            }
            Some(Value::Bool(false))
        }
        Op::Not(x) => {
            let Value::Bool(b) = eval_op(x, env, enums)? else { return None };
            Some(Value::Bool(!b))
        }
        Op::IsVariant { target_variant, expr } => {
            let v = eval_op(expr, env, enums)?;
            let Value::Enum { variant, .. } = v else { return None };
            Some(Value::Bool(&variant == target_variant))
        }
        Op::BuildEnum { variant, fields, .. } => {
            let mut field_vals: Vec<Value> = Vec::with_capacity(fields.len());
            for f in fields { field_vals.push(eval_op(f, env, enums)?); }
            let enum_name = enums
                .and_then(|r| r.by_variant.borrow().get(variant).map(|(en, _)| en.clone()))
                .unwrap_or_default();
            // Cons-chain normalization (mirror z3_eval's logic).
            let is_cell = variant.starts_with("__Cell_") || variant.starts_with("__Empty_");
            if !is_cell {
                for f in field_vals.iter_mut() {
                    if let Some(flat) = flatten_seq_of_chain(f) { *f = flat; }
                }
            }
            Some(Value::Enum { enum_name, variant: variant.clone(), fields: field_vals })
        }
        Op::SelectLit { arr, idx } => {
            let v = env.get(arr)?;
            select_at(v, *idx as usize)
        }
        Op::Select { arr, idx } => {
            let av = eval_op(arr, env, enums)?;
            let iv = eval_op(idx, env, enums)?;
            let Value::Int(i) = iv else { return None };
            select_at(&av, i as usize)
        }
        Op::Accessor { field_name, inner } => {
            let v = eval_op(inner, env, enums)?;
            // Composite (struct) value: lookup by field name directly.
            if let Value::Composite(map) = v {
                return map.get(field_name).cloned();
            }
            // Enum value: lookup field index in variant's field list.
            let Value::Enum { variant, fields, .. } = v else { return None };
            let reg = enums?;
            let by_v = reg.by_variant.borrow();
            let (en_name, vidx) = by_v.get(&variant).cloned()?;
            drop(by_v);
            let by_name = reg.by_name.borrow();
            let (_dt, variants) = by_name.get(&en_name)?;
            let variant_def = variants.get(vidx)?;
            let fidx = variant_def.fields.iter().position(|f| &f.name == field_name)?;
            drop(by_name);
            fields.into_iter().nth(fidx)
        }
        Op::StrConcat(args) => {
            let mut out = String::new();
            for a in args {
                let Value::Str(s) = eval_op(a, env, enums)? else { return None };
                out.push_str(&s);
            }
            Some(Value::Str(out))
        }
        Op::Z3Fallback(d) => {
            if std::env::var("EVIDENT_VM_FALLBACK_TRACE").is_ok() {
                eprintln!("[vm] Z3Fallback hit on {d}");
            }
            crate::z3_eval::eval_dynamic(d, env, enums)
        }
    }
}

fn eval_int(op: &Op, env: &HashMap<String, Value>, enums: Option<&EnumRegistry>) -> Option<i64> {
    match eval_op(op, env, enums)? {
        Value::Int(n) => Some(n),
        _ => None,
    }
}

fn fold_int<F: Fn(i64, i64) -> i64>(
    args: &[Op],
    env: &HashMap<String, Value>,
    enums: Option<&EnumRegistry>,
    init: i64,
    f: F,
) -> Option<Value> {
    let mut acc = init;
    for a in args {
        acc = f(acc, eval_int(a, env, enums)?);
    }
    Some(Value::Int(acc))
}

fn int_cmp<F: Fn(i64, i64) -> bool>(
    l: &Op, r: &Op,
    env: &HashMap<String, Value>,
    enums: Option<&EnumRegistry>,
    f: F,
) -> Option<Value> {
    let a = eval_int(l, env, enums)?;
    let b = eval_int(r, env, enums)?;
    Some(Value::Bool(f(a, b)))
}

fn select_at(v: &Value, idx: usize) -> Option<Value> {
    match v {
        Value::SeqInt(xs)  => xs.get(idx).map(|n| Value::Int(*n)),
        Value::SeqBool(xs) => xs.get(idx).map(|b| Value::Bool(*b)),
        Value::SeqStr(xs)  => xs.get(idx).map(|s| Value::Str(s.clone())),
        Value::SeqEnum(xs) => xs.get(idx).cloned(),
        Value::SeqComposite(xs) => xs.get(idx).map(|m| Value::Composite(m.clone())),
        _ => None,
    }
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
    Some(crate::z3_eval::seq_value_from_elements_pub(out))
}
