//! `desugar_seq_concat` via `stdlib/passes/desugar.ev`; `rewrite` + `FRef`
//! lookup stay in Rust (in-solve string-eq blows up Z3 — #18 cousin).

use std::collections::HashMap;

use super::{run_done_payload, EvidentRunner};
use crate::core::ast::{BinOp, BodyItem, Expr, SchemaDecl};
use crate::core::Value;
use crate::translate::ast_decoder::{decode_expr, decode_list};
use crate::translate::ast_encoder::{body_item_list_to_value, expr_to_value};

guarded_runner!(runner, "passes/desugar.ev", "desugar_gather");

/// Drive `desugar_gather` → `name → items` map (string-keyed lookup in Rust,
/// not in-solve). Empty on any failure.
fn gather(runner: &EvidentRunner, body: &[BodyItem]) -> HashMap<String, Vec<Expr>> {
    let seed = body_item_list_to_value(body);
    let Some(assoc) = run_done_payload(runner, "desugar_gather", seed, "GDone", "desugar/evident")
    else {
        return HashMap::new();
    };
    let mut map = HashMap::new();
    let mut cur = &assoc;
    while let Value::Enum { variant, fields, .. } = cur {
        match (variant.as_str(), fields.as_slice()) {
            ("ANil", _) => break,
            ("ACons", [entry, rest]) => {
                if let Value::Enum { variant: ev, fields: ef, .. } = entry {
                    if ev == "MakeAEntry" && ef.len() == 2 {
                        if let Value::Str(name) = &ef[0] {
                            if let Ok(items) =
                                decode_list(&ef[1], "ExprList", "ELNil", "ELCons", decode_expr)
                            {
                                map.entry(name.clone()).or_insert(items);
                            }
                        }
                    }
                }
                cur = rest;
            }
            _ => break,
        }
    }
    map
}

/// Drive `desugar_flatten` over `e`; reverse the head-first chunk stream and
/// expand: `FLitItem` → itself, `FRef(n)` → `bindings[n]` or `None`.
fn flatten(runner: &EvidentRunner, e: &Expr, bindings: &HashMap<String, Vec<Expr>>) -> Option<Vec<Expr>> {
    let chunks = match runner.run_fsm("desugar_flatten", expr_to_value(e)) {
        Ok(Value::Enum { variant, fields, .. }) if variant == "FDone" && fields.len() == 1 => {
            fields[0].clone()
        }
        Ok(Value::Enum { variant, .. }) if variant == "FFail" => return None,
        other => {
            eprintln!("[desugar/evident] flatten returned an unexpected state: {other:?}");
            return None;
        }
    };
    let mut rev: Vec<&Value> = Vec::new();
    let mut cur = &chunks;
    while let Value::Enum { variant, fields, .. } = cur {
        match (variant.as_str(), fields.as_slice()) {
            ("FCNil", _) => break,
            ("FCCons", [chunk, rest]) => {
                rev.push(chunk);
                cur = rest;
            }
            _ => return None,
        }
    }
    let mut out: Vec<Expr> = Vec::new();
    for chunk in rev.into_iter().rev() {
        let Value::Enum { variant, fields, .. } = chunk else { return None };
        match (variant.as_str(), fields.as_slice()) {
            ("FLitItem", [item]) => out.push(decode_expr(item).ok()?),
            ("FRef", [Value::Str(name)]) => out.extend(bindings.get(name)?.iter().cloned()),
            _ => return None,
        }
    }
    Some(out)
}

/// Pre-order rewrite: replace flattened `Concat` with `SeqLit`; recurse into
/// all other expr children.
fn rewrite(runner: &EvidentRunner, e: &mut Expr, bindings: &HashMap<String, Vec<Expr>>) {
    if let Expr::Binary(BinOp::Concat, ..) = e {
        if let Some(items) = flatten(runner, e, bindings) {
            *e = Expr::SeqLit(items);
            return;
        }
    }
    match e {
        Expr::Binary(_, l, r)
        | Expr::Range(l, r)
        | Expr::InExpr(l, r)
        | Expr::Index(l, r) => {
            rewrite(runner, l, bindings);
            rewrite(runner, r, bindings);
        }
        Expr::Ternary(c, a, b) => {
            rewrite(runner, c, bindings);
            rewrite(runner, a, bindings);
            rewrite(runner, b, bindings);
        }
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) | Expr::Call(_, es) => {
            for x in es {
                rewrite(runner, x, bindings);
            }
        }
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
            rewrite(runner, r, bindings);
            rewrite(runner, b, bindings);
        }
        Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => rewrite(runner, i, bindings),
        Expr::Field(recv, _) => rewrite(runner, recv, bindings),
        Expr::Match(scr, arms) => {
            rewrite(runner, scr, bindings);
            for a in arms {
                rewrite(runner, &mut a.body, bindings);
            }
        }
        _ => {}
    }
}

/// Gather + rewrite one schema's body in place, then recurse into subclaims.
fn rewrite_schema(runner: &EvidentRunner, s: &mut SchemaDecl) {
    if s.external {
        return;
    }
    let bindings = gather(runner, &s.body);
    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => rewrite(runner, e, &bindings),
            BodyItem::ClaimCall { mappings, .. } => {
                for m in mappings.iter_mut() {
                    rewrite(runner, &mut m.value, &bindings);
                }
            }
            _ => {}
        }
    }
    for item in s.body.iter_mut() {
        if let BodyItem::SubclaimDecl(sub) = item {
            rewrite_schema(runner, sub);
        }
    }
}

/// Flatten `Seq(T)` concatenations in `s` in place. No-op (and no engine
/// build) when the schema contains no `++` Concat.
pub fn desugar_seq_concat(s: &mut SchemaDecl) {
    if !schema_has_seq_concat(s) {
        return;
    }
    let Some(runner) = runner() else { return };
    rewrite_schema(&runner, s);
}

/// True if `s` contains a `Concat` anywhere the pass would rewrite it;
/// `false` lets `desugar_seq_concat` short-circuit cheaply.
fn schema_has_seq_concat(s: &SchemaDecl) -> bool {
    s.body.iter().any(|item| match item {
        BodyItem::Constraint(e) => expr_has_concat(e),
        BodyItem::ClaimCall { mappings, .. } => mappings.iter().any(|m| expr_has_concat(&m.value)),
        BodyItem::SubclaimDecl(sub) => schema_has_seq_concat(sub),
        _ => false,
    })
}

/// True if `e` contains a `Concat` anywhere. Visits same nodes as `rewrite`.
fn expr_has_concat(e: &Expr) -> bool {
    match e {
        Expr::Binary(BinOp::Concat, ..) => true,
        Expr::Binary(_, l, r)
        | Expr::Range(l, r)
        | Expr::InExpr(l, r)
        | Expr::Index(l, r) => expr_has_concat(l) || expr_has_concat(r),
        Expr::Ternary(c, a, b) => expr_has_concat(c) || expr_has_concat(a) || expr_has_concat(b),
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) | Expr::Call(_, es) => {
            es.iter().any(expr_has_concat)
        }
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => expr_has_concat(r) || expr_has_concat(b),
        Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => expr_has_concat(i),
        Expr::Field(recv, _) => expr_has_concat(recv),
        Expr::Match(scr, arms) => {
            expr_has_concat(scr) || arms.iter().any(|a| expr_has_concat(&a.body))
        }
        _ => false,
    }
}
