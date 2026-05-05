//! AST → Z3 expressions. v0.1: Int/Bool only, flat declarations,
//! arithmetic + boolean + comparisons.

use crate::ast::*;
use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int};
use z3::{Context, SatResult, Solver};

/// Result of running one query.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Bool(bool),
}

/// Z3 binding for a declared variable. Keep a typed handle so we know
/// which AST kind to translate against.
enum Var<'ctx> {
    IntVar(Int<'ctx>),
    BoolVar(Bool<'ctx>),
}

impl<'ctx> Var<'ctx> {
    fn as_int(&self) -> Option<&Int<'ctx>> {
        match self { Var::IntVar(i) => Some(i), _ => None }
    }
    fn as_bool(&self) -> Option<&Bool<'ctx>> {
        match self { Var::BoolVar(b) => Some(b), _ => None }
    }
}

/// Evaluate a single schema. Builds a fresh solver, declares variables,
/// translates body constraints, calls check, extracts the model.
pub fn evaluate(schema: &SchemaDecl) -> EvalResult {
    let cfg = z3::Config::new();
    let ctx = Context::new(&cfg);
    let solver = Solver::new(&ctx);
    let mut env: HashMap<String, Var> = HashMap::new();

    // Pass 1: declare variables and add per-type constraints.
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name } = item {
            match type_name.as_str() {
                "Int" => {
                    env.insert(name.clone(), Var::IntVar(Int::new_const(&ctx, name.as_str())));
                }
                "Nat" => {
                    let v = Int::new_const(&ctx, name.as_str());
                    solver.assert(&v.ge(&Int::from_i64(&ctx, 0)));
                    env.insert(name.clone(), Var::IntVar(v));
                }
                "Pos" => {
                    let v = Int::new_const(&ctx, name.as_str());
                    solver.assert(&v.gt(&Int::from_i64(&ctx, 0)));
                    env.insert(name.clone(), Var::IntVar(v));
                }
                "Bool" => {
                    env.insert(name.clone(), Var::BoolVar(Bool::new_const(&ctx, name.as_str())));
                }
                other => {
                    // Unsupported type for v0.1 — skip with no constraints.
                    eprintln!("warning: unsupported type {} for {}", other, name);
                }
            }
        }
    }

    // Pass 2: translate body constraints and assert.
    for item in &schema.body {
        if let BodyItem::Constraint(e) = item {
            let z3_bool = match translate_bool(e, &ctx, &env) {
                Some(b) => b,
                None => {
                    eprintln!("warning: dropped constraint that didn't translate to Bool");
                    continue;
                }
            };
            solver.assert(&z3_bool);
        }
    }

    let satisfied = matches!(solver.check(), SatResult::Sat);
    let mut bindings = HashMap::new();
    if satisfied {
        if let Some(model) = solver.get_model() {
            for (name, var) in env.iter() {
                match var {
                    Var::IntVar(i) => {
                        if let Some(val) = model.eval(i, true) {
                            if let Some(n) = val.as_i64() {
                                bindings.insert(name.clone(), Value::Int(n));
                            }
                        }
                    }
                    Var::BoolVar(b) => {
                        if let Some(val) = model.eval(b, true) {
                            if let Some(bv) = val.as_bool() {
                                bindings.insert(name.clone(), Value::Bool(bv));
                            }
                        }
                    }
                }
            }
        }
    }
    EvalResult { satisfied, bindings }
}

fn translate_int<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Int<'ctx>> {
    match e {
        Expr::Int(n) => Some(Int::from_i64(ctx, *n)),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_int().cloned()),
        Expr::Binary(op, lhs, rhs) => {
            let l = translate_int(lhs, ctx, env)?;
            let r = translate_int(rhs, ctx, env)?;
            Some(match op {
                BinOp::Add => Int::add(ctx, &[&l, &r]),
                BinOp::Sub => Int::sub(ctx, &[&l, &r]),
                BinOp::Mul => Int::mul(ctx, &[&l, &r]),
                BinOp::Div => l.div(&r),
                _ => return None,
            })
        }
        _ => None,
    }
}

fn translate_bool<'ctx>(e: &Expr, ctx: &'ctx Context, env: &HashMap<String, Var<'ctx>>) -> Option<Bool<'ctx>> {
    match e {
        Expr::Bool(b) => Some(Bool::from_bool(ctx, *b)),
        Expr::Identifier(name) => env.get(name).and_then(|v| v.as_bool().cloned()),
        Expr::Not(inner) => Some(translate_bool(inner, ctx, env)?.not()),
        Expr::Binary(op, lhs, rhs) => match op {
            // Boolean combinators
            BinOp::And => {
                let l = translate_bool(lhs, ctx, env)?;
                let r = translate_bool(rhs, ctx, env)?;
                Some(Bool::and(ctx, &[&l, &r]))
            }
            BinOp::Or => {
                let l = translate_bool(lhs, ctx, env)?;
                let r = translate_bool(rhs, ctx, env)?;
                Some(Bool::or(ctx, &[&l, &r]))
            }
            BinOp::Implies => {
                let l = translate_bool(lhs, ctx, env)?;
                let r = translate_bool(rhs, ctx, env)?;
                Some(l.implies(&r))
            }
            // Eq/Neq work over either Bool or Int operands. Try Bool first;
            // if either side isn't a Bool, retry as Int.
            BinOp::Eq | BinOp::Neq => {
                if let (Some(l), Some(r)) =
                    (translate_bool(lhs, ctx, env), translate_bool(rhs, ctx, env))
                {
                    return Some(match op {
                        BinOp::Eq  => l._eq(&r),
                        BinOp::Neq => l._eq(&r).not(),
                        _ => unreachable!(),
                    });
                }
                let l = translate_int(lhs, ctx, env)?;
                let r = translate_int(rhs, ctx, env)?;
                Some(match op {
                    BinOp::Eq  => l._eq(&r),
                    BinOp::Neq => l._eq(&r).not(),
                    _ => unreachable!(),
                })
            }
            // Numeric-only comparisons.
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let l = translate_int(lhs, ctx, env)?;
                let r = translate_int(rhs, ctx, env)?;
                Some(match op {
                    BinOp::Lt => l.lt(&r),
                    BinOp::Le => l.le(&r),
                    BinOp::Gt => l.gt(&r),
                    BinOp::Ge => l.ge(&r),
                    _ => unreachable!(),
                })
            }
            _ => None,
        }
        _ => None,
    }
}
