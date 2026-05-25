//! Pre-solve resolution of `run(F, init)` expressions (tier 3,
//! blocking-interpret).
//!
//! ## The evaluation-timing rule (the crux)
//!
//! `run(F, init)` produces a concrete value the *outer* model then uses
//! in its constraints. So the nested FSM must be **evaluated to a
//! concrete `Value` before the outer solve**, and that value injected
//! as a pinned constant — exactly how a pre-computed `given` enters a
//! model. `resolve_runs` is that hook: before any query translates a
//! schema body, it walks the body, drives each `run(F, init)` to halt
//! via [`crate::effect_loop::run_nested`], and **rewrites the `RunFsm`
//! node to the literal final-state value** (an `Expr::Int` /
//! `Expr::Call(ctor, …)` / …). The translator never sees a `RunFsm`;
//! it sees `final = 0`.
//!
//! Every general query entry point calls `resolve_runs` first:
//! [`EvidentRuntime::query`], `query_with_core`, `query_cached`, and the
//! scheduler's `query_with_pins_and_given`. When the body has no
//! `run(...)` (the overwhelming common case) `resolve_runs` returns
//! `None` and the original schema is used with zero overhead.
//!
//! ## v1 restriction: `init` must be pre-known
//!
//! `init` is evaluated by [`EvidentRuntime::eval_const_init`] from
//! literals + the query's `given` (plus integer arithmetic over those).
//! If `init` names a variable the outer solve hasn't determined yet,
//! that's a **loud error**, not a silent wrong value — there is no
//! "solve, then run" cycle in v1. (`run(decrement, 50)` has a literal
//! init; `run(decrement, seed)` works iff `seed` is a given.)
//!
//! ## Strategy gate
//!
//! `EVIDENT_NESTED_STRATEGY` (`auto` | `blocking` | `loop` | `unroll`,
//! default `auto`) mirrors `EVIDENT_FUNCTIONIZE` et al. Only tier 3
//! (`blocking`) exists this session; `auto` resolves to it. Forcing
//! `loop`/`unroll` errors clearly — those tiers land in later sessions.

use std::collections::HashMap;

use crate::core::ast::{BinOp, BodyItem, Expr, Mapping, Pins, SchemaDecl};
use crate::core::{RuntimeError, Value};

use super::EvidentRuntime;

/// Default max-iteration guard for a nested run. Matches the scheduler's
/// `LoopOpts` default; override with `EVIDENT_NESTED_MAX_STEPS`.
fn nested_max_steps() -> usize {
    std::env::var("EVIDENT_NESTED_MAX_STEPS").ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n: &usize| n > 0)
        .unwrap_or(10_000)
}

impl EvidentRuntime {
    /// Load-time validation of every `run(F, ..)` target across all
    /// loaded schemas. For each `run` whose `F` is already known, check
    /// `F` is FSM-shaped (single state pair + `halt ∈ Bool`) and
    /// effect-free; reject at load otherwise. A `run` whose `F` isn't
    /// yet loaded is left for query-time resolution to surface as an
    /// unknown-FSM error (avoids false positives on cross-file forward
    /// references). See `docs/design/nested-fsm-strategies.md` §1.
    pub(super) fn validate_run_targets(&self) -> Result<(), RuntimeError> {
        let names: Vec<String> = self.schema_names().map(|s| s.to_string()).collect();
        for claim_name in &names {
            let Some(schema) = self.get_schema(claim_name) else { continue };
            if !body_has_run(&schema.body) { continue; }
            let mut targets: Vec<String> = Vec::new();
            collect_run_targets(&schema.body, &mut targets);
            for fsm in targets {
                // Unknown F → defer to query time (forward ref across files).
                if self.get_schema(&fsm).is_none() { continue; }
                if let Err(e) = crate::effect_loop::validate_run_target(self, &fsm) {
                    return Err(RuntimeError::Parse(format!(
                        "in `{claim_name}`: {e}")));
                }
            }
        }
        Ok(())
    }

    /// If `schema`'s body contains any `run(F, init)` expression, return
    /// a rewritten copy with every `run` driven to its final-state value
    /// and replaced by that value's literal expression. Returns `None`
    /// (no clone) when the body has no `run`.
    ///
    /// Called at the top of every query entry point — see the module
    /// doc for the evaluation-timing rationale.
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

    /// Recursively rewrite every `RunFsm` node in `e` to its literal
    /// final-state value. Non-`run` subtrees are reconstructed as-is.
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
            // Leaves — no embedded expressions.
            Expr::Identifier(_) | Expr::Int(_) | Expr::Real(_)
            | Expr::Bool(_) | Expr::Str(_) => e.clone(),
            // Recurse structurally everywhere a sub-expression can hide.
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
        // Strategy gate. Only tier 3 (`blocking`) exists this session;
        // `auto` resolves to it. Forcing an unbuilt tier errors clearly.
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
        crate::effect_loop::run_nested(self, fsm, init_val, nested_max_steps())
            .map_err(|e| RuntimeError::Parse(e.to_string()))
    }

    /// Evaluate a `run`'s `init` expression to a concrete `Value` using
    /// only values known before the outer solve: literals, the query's
    /// `given`, integer arithmetic over those, and (recursively) nested
    /// `run`s. Anything else is the "init depends on an undetermined
    /// variable" error — loud, never silent.
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
                // A bare nullary enum-variant literal (`Empty`, `NLNil`)
                // — part of a composite init like `Node("a", NLNil)`.
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
            // A composite enum-constructor literal — `Leaf(7)`,
            // `Node(Leaf(1), Leaf(2))`, `WSeed(...)`. Look the variant up
            // to recover its enum name, then recursively evaluate each
            // payload arg. This is the composite seed (#19d): a tree /
            // recursive-enum value passed straight through `init`.
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
            // A sequence-literal seed — `⟨root⟩`, `⟨"a", "b"⟩`, or an
            // empty `⟨⟩` children list inside a composite. Evaluate each
            // element and pick the Seq Value variant from the element
            // kinds (#19d composite seed, Seq case).
            Expr::SeqLit(items) => {
                let vals = items.iter()
                    .map(|x| self.eval_const_init(fsm, x, given))
                    .collect::<Result<Vec<Value>, _>>()?;
                seq_value_from_elems(fsm, vals)
            }
            // A run nested in an init expression: recurse.
            Expr::RunFsm { fsm: inner, init } => self.eval_run(inner, init, given),
            other => Err(RuntimeError::Parse(format!(
                "run({fsm}, ..): init must be a constant expression computable \
                 before the solve (literal, given, or integer arithmetic over \
                 those); got {}", crate::pretty::expr(other)))),
        }
    }

    /// If `name` is a registered nullary enum variant (`Empty`, `NLNil`,
    /// `Nil`), build its `Value::Enum`. Used by `eval_const_init` so a
    /// composite init literal can carry bare nullary variants
    /// (`Node("a", NLNil)`). Returns `None` for unknown names or
    /// payload-bearing variants.
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

/// Build a `Seq` `Value` from already-evaluated element values, picking
/// the variant from the (homogeneous) element kinds. An empty literal
/// defaults to `SeqEnum([])` — the common shape for an empty
/// recursive-enum children list. A mixed-kind literal is an error.
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

/// Does any body item carry a `run(...)` expression?
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

/// Collect every `run(F, ..)` target FSM name reachable from `body`
/// (including inside subclaims).
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

/// Convert a nested-run final-state `Value` to the literal `Expr` that
/// pins it into the outer model. The outer translator's existing
/// equality paths then lower that literal:
///   * Primitive → its literal (`Int`/`Bool`/`Real`/`Str`).
///   * Nullary enum variant → a bare `Identifier` (`Empty` → `Empty`),
///     NOT a zero-arg `Call`: `resolve_enum_ast`'s Identifier path
///     resolves the `EnumValue`, whereas a `Call("Empty", [])` would
///     look for an `EnumCtor` and the equality would silently drop.
///   * Payload enum variant → a constructor `Call`, recursing into each
///     field (so a nested-enum payload like `Done(Push(Leaf(7), Empty))`
///     round-trips).
///   * Seq value → a `SeqLit` of element literals; the outer
///     `translate_seq_lit_eq` / `translate_seq_arg_for_ctor` paths pin
///     length + per-element values. This is the composite final-state
///     return — a `Seq`/`Set`-accumulator FSM can now hand its result
///     back as a value.
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
        // Set values and flat composite records aren't expressible as a
        // single outer literal yet (a `Set`-accumulator return is the
        // honest remaining gap — Set literals need bare-identifier
        // elements, not nested literals).
        _ => None,
    }
}
