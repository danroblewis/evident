//! Pre-solve resolution of `run(F, init)` expressions (tier 3: blocking-interpret).
//! `resolve_runs` drives each `RunFsm` to halt before the outer solve, replacing it
//! with a literal final-state value. `init` must be computable from literals + givens.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::core::ast::{BinOp, BodyItem, Effect, Expr, Mapping, Pins, SchemaDecl};
use crate::core::{RuntimeError, Value};

use super::EvidentRuntime;

thread_local! {
    /// Effects from nested `run(F, init)` not yet dispatched; percolate to the parent FSM's
    /// tick. Non-scheduler query paths (sat_*/unsat_*) append but nobody drains — dropped, correct.
    static PERCOLATED_EFFECTS: RefCell<Vec<Effect>> = const { RefCell::new(Vec::new()) };
}

/// Drain effects captured by nested `run(...)` since the last drain; called by the scheduler.
pub fn take_percolated_effects() -> Vec<Effect> {
    PERCOLATED_EFFECTS.with(|c| std::mem::take(&mut *c.borrow_mut()))
}

fn append_percolated_effects(effects: Vec<Effect>) {
    if effects.is_empty() { return; }
    PERCOLATED_EFFECTS.with(|c| c.borrow_mut().extend(effects));
}

/// Max steps for a nested run; override with `EVIDENT_NESTED_MAX_STEPS`.
fn nested_max_steps() -> usize {
    std::env::var("EVIDENT_NESTED_MAX_STEPS").ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n: &usize| n > 0)
        .unwrap_or(10_000)
}

impl EvidentRuntime {
    /// Validate `run(F,..)` and `halts_within(F,..)` targets at load time.
    /// Unknown F deferred to query time (forward refs). Checks `fsm` keyword + FSM shape.
    pub(super) fn validate_run_targets(&self) -> Result<(), RuntimeError> {
        let names: Vec<String> = self.schema_names().map(|s| s.to_string()).collect();
        for claim_name in &names {
            let Some(schema) = self.get_schema(claim_name) else { continue };
            if body_has_run(&schema.body) {
                let mut targets: Vec<String> = Vec::new();
                collect_run_targets(&schema.body, &mut targets);
                for fsm in targets {
                    if self.get_schema(&fsm).is_none() { continue; } // forward ref → defer
                    if let Err(e) = crate::effect_loop::validate_run_target(self, &fsm) {
                        return Err(RuntimeError::Parse(format!(
                            "in `{claim_name}`: {e}")));
                    }
                }
            }
            // `halts_within(F, ..)` targets — keyword check at load.
            let mut hw_targets: Vec<String> = Vec::new();
            collect_halts_within_targets(&schema.body, &mut hw_targets);
            for fsm in hw_targets {
                let Some(target) = self.get_schema(&fsm) else { continue };
                if !matches!(target.keyword, crate::core::ast::Keyword::Fsm) {
                    return Err(RuntimeError::Parse(format!(
                        "in `{claim_name}`: halts_within's target `{fsm}` must be \
                         declared `fsm`, not `{}` — the `fsm` keyword is the sole \
                         signal that a schema is an FSM (no shape-detection). \
                         Relabel `{} {fsm}` to `fsm {fsm}`.",
                        keyword_word(&target.keyword), keyword_word(&target.keyword))));
                }
            }
        }
        Ok(())
    }

    /// FSM names used as `run(...)` or `halts_within(...)` targets; the scheduler skips
    /// these so an embedded FSM can be declared `fsm` without becoming a standalone FSM.
    pub(crate) fn embedded_fsm_targets(&self) -> std::collections::HashSet<String> {
        let mut out = std::collections::HashSet::new();
        for name in self.schema_names() {
            let Some(schema) = self.get_schema(name) else { continue };
            let mut targets: Vec<String> = Vec::new();
            collect_run_targets(&schema.body, &mut targets);
            collect_halts_within_targets(&schema.body, &mut targets);
            out.extend(targets);
        }
        out
    }

    /// Rewrite all `run(F, init)` nodes to their literal final-state values.
    /// Returns `None` (no clone) when the body has no `run`.
    pub(super) fn resolve_runs(
        &self,
        schema: &SchemaDecl,
        given: &HashMap<String, Value>,
    ) -> Result<Option<SchemaDecl>, RuntimeError> {
        if !body_has_run(&schema.body) {
            return Ok(None);
        }
        let mut out = schema.clone();
        out.body = self.rewrite_body(&schema.body, given)?;
        Ok(Some(out))
    }

    fn rewrite_body(
        &self,
        body: &[BodyItem],
        given: &HashMap<String, Value>,
    ) -> Result<Vec<BodyItem>, RuntimeError> {
        body.iter().map(|item| self.rewrite_body_item(item, given)).collect()
    }

    fn rewrite_body_item(
        &self,
        item: &BodyItem,
        given: &HashMap<String, Value>,
    ) -> Result<BodyItem, RuntimeError> {
        Ok(match item {
            BodyItem::Constraint(e) =>
                BodyItem::Constraint(self.rewrite_expr(e, given)?),
            BodyItem::ClaimCall { name, mappings } =>
                BodyItem::ClaimCall {
                    name: name.clone(),
                    mappings: self.rewrite_mappings(mappings, given)?,
                },
            BodyItem::Membership { name, type_name, pins } =>
                BodyItem::Membership {
                    name: name.clone(),
                    type_name: type_name.clone(),
                    pins: self.rewrite_pins(pins, given)?,
                },
            BodyItem::SubclaimDecl(s) => {
                let mut s2 = s.clone();
                s2.body = self.rewrite_body(&s.body, given)?;
                BodyItem::SubclaimDecl(s2)
            }
            // Passthrough / HaltsWithin carry no embedded expressions.
            other => other.clone(),
        })
    }

    fn rewrite_mappings(
        &self,
        mappings: &[Mapping],
        given: &HashMap<String, Value>,
    ) -> Result<Vec<Mapping>, RuntimeError> {
        mappings.iter().map(|m| Ok(Mapping {
            slot: m.slot.clone(),
            value: self.rewrite_expr(&m.value, given)?,
        })).collect()
    }

    fn rewrite_pins(
        &self,
        pins: &Pins,
        given: &HashMap<String, Value>,
    ) -> Result<Pins, RuntimeError> {
        Ok(match pins {
            Pins::None => Pins::None,
            Pins::Named(maps) => Pins::Named(self.rewrite_mappings(maps, given)?),
            Pins::Positional(args) => Pins::Positional(
                args.iter().map(|a| self.rewrite_expr(a, given)).collect::<Result<_, _>>()?),
        })
    }

    fn rewrite_expr(
        &self,
        e: &Expr,
        given: &HashMap<String, Value>,
    ) -> Result<Expr, RuntimeError> {
        let rb = |x: &Expr| -> Result<Box<Expr>, RuntimeError> {
            Ok(Box::new(self.rewrite_expr(x, given)?))
        };
        let rv = |xs: &[Expr]| -> Result<Vec<Expr>, RuntimeError> {
            xs.iter().map(|x| self.rewrite_expr(x, given)).collect()
        };
        Ok(match e {
            Expr::RunFsm { fsm, init } => {
                let value = self.eval_run(fsm, init, given)?;
                value_to_literal_expr(&value).ok_or_else(|| RuntimeError::Parse(format!(
                    "run({fsm}, ..): final state value {value:?} can't be expressed \
                     as a literal (v1 supports primitive + enum final states)")))?
            }
            Expr::Identifier(_) | Expr::Int(_) | Expr::Real(_)
            | Expr::Bool(_) | Expr::Str(_) => e.clone(),
            Expr::SetLit(items)  => Expr::SetLit(rv(items)?),
            Expr::SeqLit(items)  => Expr::SeqLit(rv(items)?),
            Expr::Tuple(items)   => Expr::Tuple(rv(items)?),
            Expr::Range(lo, hi)  => Expr::Range(rb(lo)?, rb(hi)?),
            Expr::InExpr(l, r)   => Expr::InExpr(rb(l)?, rb(r)?),
            Expr::Forall(vs, range, body) =>
                Expr::Forall(vs.clone(), rb(range)?, rb(body)?),
            Expr::Exists(vs, range, body) =>
                Expr::Exists(vs.clone(), rb(range)?, rb(body)?),
            Expr::Call(name, args) => Expr::Call(name.clone(), rv(args)?),
            Expr::Cardinality(inner) => Expr::Cardinality(rb(inner)?),
            Expr::Index(s, i)    => Expr::Index(rb(s)?, rb(i)?),
            Expr::Field(b, name) => Expr::Field(rb(b)?, name.clone()),
            Expr::Binary(op, l, r) => Expr::Binary(op.clone(), rb(l)?, rb(r)?),
            Expr::Not(inner)     => Expr::Not(rb(inner)?),
            Expr::Ternary(c, a, b) => Expr::Ternary(rb(c)?, rb(a)?, rb(b)?),
            Expr::Match(scr, arms) => {
                let arms2 = arms.iter().map(|a| Ok(crate::core::ast::MatchArm {
                    pattern: a.pattern.clone(),
                    body: rb(&a.body)?,
                })).collect::<Result<Vec<_>, RuntimeError>>()?;
                Expr::Match(rb(scr)?, arms2)
            }
            Expr::Matches(inner, pat) => Expr::Matches(rb(inner)?, pat.clone()),
        })
    }

    /// Drive one `run(fsm, init)` to its final-state value.
    fn eval_run(
        &self,
        fsm: &str,
        init: &Expr,
        given: &HashMap<String, Value>,
    ) -> Result<Value, RuntimeError> {
        match std::env::var("EVIDENT_NESTED_STRATEGY").as_deref() {
            Ok("loop") | Ok("unroll") => {
                let tier = std::env::var("EVIDENT_NESTED_STRATEGY").unwrap();
                return Err(RuntimeError::Parse(format!(
                    "EVIDENT_NESTED_STRATEGY={tier} forces a nested-FSM tier that \
                     isn't implemented yet — only `blocking` (tier 3) exists in \
                     this build. Use `blocking` or `auto`.")));
            }
            _ => {} // blocking | auto | unset → tier 3
        }
        let init_val = self.eval_const_init(fsm, init, given)?;
        let (value, effects) =
            crate::effect_loop::run_nested_capturing(self, fsm, init_val, nested_max_steps())
                .map_err(|e| RuntimeError::Parse(e.to_string()))?;
        append_percolated_effects(effects);
        Ok(value)
    }

    /// Evaluate `init` to a concrete Value using only literals, givens, and arithmetic.
    /// Any undetermined variable is a loud error — no silent wrong values.
    fn eval_const_init(
        &self,
        fsm: &str,
        e: &Expr,
        given: &HashMap<String, Value>,
    ) -> Result<Value, RuntimeError> {
        match e {
            Expr::Int(n)  => Ok(Value::Int(*n)),
            Expr::Real(r) => Ok(Value::Real(*r)),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Str(s)  => Ok(Value::Str(s.clone())),
            Expr::Identifier(name) => {
                if let Some(v) = given.get(name) {
                    return Ok(v.clone());
                }
                if let Some(v) = self.nullary_variant_value(name) {
                    return Ok(v);
                }
                Err(RuntimeError::Parse(format!(
                    "run({fsm}, ..): init references `{name}`, which has no known \
                     value before the solve. v1 requires `run`'s init to be \
                     computable from literals, givens, or enum literals — pin \
                     `{name}` as a given, or pass a literal.")))
            }
            Expr::Binary(op @ (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div), l, r) => {
                let lv = self.eval_const_init(fsm, l, given)?;
                let rv = self.eval_const_init(fsm, r, given)?;
                match (lv, rv) {
                    (Value::Int(a), Value::Int(b)) => match op {
                        BinOp::Add => Ok(Value::Int(a + b)),
                        BinOp::Sub => Ok(Value::Int(a - b)),
                        BinOp::Mul => Ok(Value::Int(a * b)),
                        BinOp::Div if b != 0 => Ok(Value::Int(a / b)),
                        BinOp::Div => Err(RuntimeError::Parse(format!(
                            "run({fsm}, ..): init divides by zero"))),
                        _ => unreachable!(),
                    },
                    _ => Err(RuntimeError::Parse(format!(
                        "run({fsm}, ..): init arithmetic is only supported over \
                         integers in v1"))),
                }
            }
            // Enum constructor literal: look up variant, evaluate payload args recursively.
            Expr::Call(ctor, args) => {
                let enum_name = self.enums_registry().by_variant
                    .borrow().get(ctor).map(|(n, _)| n.clone());
                let Some(enum_name) = enum_name else {
                    return Err(RuntimeError::Parse(format!(
                        "run({fsm}, ..): init constructor `{ctor}` isn't a known \
                         enum variant")));
                };
                let fields = args.iter()
                    .map(|a| self.eval_const_init(fsm, a, given))
                    .collect::<Result<Vec<Value>, _>>()?;
                Ok(Value::Enum { enum_name, variant: ctor.clone(), fields })
            }
            Expr::SeqLit(items) => {
                let vals = items.iter()
                    .map(|x| self.eval_const_init(fsm, x, given))
                    .collect::<Result<Vec<Value>, _>>()?;
                seq_value_from_elems(fsm, vals)
            }
            Expr::RunFsm { fsm: inner, init } => self.eval_run(inner, init, given),
            other => Err(RuntimeError::Parse(format!(
                "run({fsm}, ..): init must be a constant expression computable \
                 before the solve (literal, given, or integer arithmetic over \
                 those); got {}", crate::pretty::expr(other)))),
        }
    }

    /// Build `Value::Enum` for a nullary variant, or `None` for unknowns / payload variants.
    fn nullary_variant_value(&self, name: &str) -> Option<Value> {
        let enums = self.enums_registry();
        let (enum_name, idx) = enums.by_variant.borrow().get(name)?.clone();
        let by_name = enums.by_name.borrow();
        let (_, variants) = by_name.get(&enum_name)?;
        if variants.get(idx)?.fields.is_empty() {
            Some(Value::Enum { enum_name, variant: name.to_string(), fields: vec![] })
        } else {
            None
        }
    }
}

/// Build a Seq Value from homogeneous element values; empty defaults to `SeqEnum([])`.
fn seq_value_from_elems(fsm: &str, vals: Vec<Value>) -> Result<Value, RuntimeError> {
    let mismatch = || RuntimeError::Parse(format!(
        "run({fsm}, ..): sequence-literal init must have homogeneous element \
         types (all Int, all String, all enum, …)"));
    match vals.first() {
        None => Ok(Value::SeqEnum(vec![])),
        Some(Value::Int(_)) => vals.iter().map(|v| match v {
            Value::Int(n) => Some(*n), _ => None }).collect::<Option<Vec<_>>>()
            .map(Value::SeqInt).ok_or_else(mismatch),
        Some(Value::Bool(_)) => vals.iter().map(|v| match v {
            Value::Bool(b) => Some(*b), _ => None }).collect::<Option<Vec<_>>>()
            .map(Value::SeqBool).ok_or_else(mismatch),
        Some(Value::Str(_)) => vals.iter().map(|v| match v {
            Value::Str(s) => Some(s.clone()), _ => None }).collect::<Option<Vec<_>>>()
            .map(Value::SeqStr).ok_or_else(mismatch),
        Some(Value::Enum { .. }) => {
            if vals.iter().all(|v| matches!(v, Value::Enum { .. })) {
                Ok(Value::SeqEnum(vals))
            } else { Err(mismatch()) }
        }
        _ => Err(mismatch()),
    }
}

fn body_has_run(body: &[BodyItem]) -> bool {
    body.iter().any(|item| match item {
        BodyItem::Constraint(e) => expr_has_run(e),
        BodyItem::ClaimCall { mappings, .. } =>
            mappings.iter().any(|m| expr_has_run(&m.value)),
        BodyItem::Membership { pins, .. } => match pins {
            Pins::None => false,
            Pins::Named(ms) => ms.iter().any(|m| expr_has_run(&m.value)),
            Pins::Positional(es) => es.iter().any(expr_has_run),
        },
        BodyItem::SubclaimDecl(s) => body_has_run(&s.body),
        BodyItem::Passthrough(_) | BodyItem::HaltsWithin { .. } => false,
    })
}

fn keyword_word(kw: &crate::core::ast::Keyword) -> &'static str {
    use crate::core::ast::Keyword;
    match kw {
        Keyword::Schema   => "schema",
        Keyword::Claim    => "claim",
        Keyword::Type     => "type",
        Keyword::Subclaim => "subclaim",
        Keyword::Fsm      => "fsm",
    }
}

fn collect_halts_within_targets(body: &[BodyItem], out: &mut Vec<String>) {
    for item in body {
        match item {
            BodyItem::HaltsWithin { fsm_name, .. } => out.push(fsm_name.clone()),
            BodyItem::SubclaimDecl(s) => collect_halts_within_targets(&s.body, out),
            _ => {}
        }
    }
}

fn collect_run_targets(body: &[BodyItem], out: &mut Vec<String>) {
    for item in body {
        match item {
            BodyItem::Constraint(e) => collect_run_targets_expr(e, out),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { collect_run_targets_expr(&m.value, out); },
            BodyItem::Membership { pins, .. } => match pins {
                Pins::None => {}
                Pins::Named(ms) => for m in ms { collect_run_targets_expr(&m.value, out); },
                Pins::Positional(es) => for e in es { collect_run_targets_expr(e, out); },
            },
            BodyItem::SubclaimDecl(s) => collect_run_targets(&s.body, out),
            BodyItem::Passthrough(_) | BodyItem::HaltsWithin { .. } => {}
        }
    }
}

fn collect_run_targets_expr(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::RunFsm { fsm, init } => {
            out.push(fsm.clone());
            collect_run_targets_expr(init, out);
        }
        Expr::Identifier(_) | Expr::Int(_) | Expr::Real(_)
        | Expr::Bool(_) | Expr::Str(_) => {}
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
            for x in es { collect_run_targets_expr(x, out); },
        Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b)
        | Expr::Binary(_, a, b) => { collect_run_targets_expr(a, out); collect_run_targets_expr(b, out); }
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
            { collect_run_targets_expr(r, out); collect_run_targets_expr(b, out); }
        Expr::Call(_, args) => for a in args { collect_run_targets_expr(a, out); },
        Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => collect_run_targets_expr(i, out),
        Expr::Field(recv, _) => collect_run_targets_expr(recv, out),
        Expr::Ternary(c, a, b) =>
            { collect_run_targets_expr(c, out); collect_run_targets_expr(a, out); collect_run_targets_expr(b, out); }
        Expr::Match(scr, arms) => {
            collect_run_targets_expr(scr, out);
            for a in arms { collect_run_targets_expr(&a.body, out); }
        }
    }
}

fn expr_has_run(e: &Expr) -> bool {
    match e {
        Expr::RunFsm { .. } => true,
        Expr::Identifier(_) | Expr::Int(_) | Expr::Real(_)
        | Expr::Bool(_) | Expr::Str(_) => false,
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
            es.iter().any(expr_has_run),
        Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b)
        | Expr::Binary(_, a, b) => expr_has_run(a) || expr_has_run(b),
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
            expr_has_run(r) || expr_has_run(b),
        Expr::Call(_, args) => args.iter().any(expr_has_run),
        Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => expr_has_run(i),
        Expr::Field(recv, _) => expr_has_run(recv),
        Expr::Ternary(c, a, b) =>
            expr_has_run(c) || expr_has_run(a) || expr_has_run(b),
        Expr::Match(scr, arms) =>
            expr_has_run(scr) || arms.iter().any(|a| expr_has_run(&a.body)),
    }
}

/// Convert a final-state Value to the literal Expr pinned into the outer model.
/// Nullary enum variants → bare Identifier (NOT zero-arg Call; the latter silently drops).
fn value_to_literal_expr(v: &Value) -> Option<Expr> {
    let seq_lit = |items: Vec<Expr>| Some(Expr::SeqLit(items));
    match v {
        Value::Int(n)  => Some(Expr::Int(*n)),
        Value::Bool(b) => Some(Expr::Bool(*b)),
        Value::Real(r) => Some(Expr::Real(*r)),
        Value::Str(s)  => Some(Expr::Str(s.clone())),
        Value::Enum { variant, fields, .. } => {
            if fields.is_empty() {
                Some(Expr::Identifier(variant.clone()))
            } else {
                let args: Option<Vec<Expr>> =
                    fields.iter().map(value_to_literal_expr).collect();
                Some(Expr::Call(variant.clone(), args?))
            }
        }
        Value::SeqInt(xs)  => seq_lit(xs.iter().map(|n| Expr::Int(*n)).collect()),
        Value::SeqBool(xs) => seq_lit(xs.iter().map(|b| Expr::Bool(*b)).collect()),
        Value::SeqStr(xs)  => seq_lit(xs.iter().map(|s| Expr::Str(s.clone())).collect()),
        Value::SeqEnum(xs) => {
            let items: Option<Vec<Expr>> =
                xs.iter().map(value_to_literal_expr).collect();
            seq_lit(items?)
        }
        _ => None, // Set / composite records not yet expressible as outer literal
    }
}
