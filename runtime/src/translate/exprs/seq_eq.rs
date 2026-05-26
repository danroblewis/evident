//! Seq/Set equality: Cons-chain enum literals, `seq = ⟨…⟩`, `set = {…}`, whole-Seq equality,
//! single-element composite assignment, and composite binding helpers shared with quant/mapping.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Int};
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::{FieldKind, SeqElem, Value, Var};

use super::bool::translate_bool;
use super::enums::{build_cons_chain, resolve_enum_ast};
use super::match_expr::translate_match_arms;
use super::scalar::{translate_int, translate_str};
use super::with_target_enum_hint;

/// Translate `enum_var = ⟨a, b, c⟩` for Cons/Nil-shaped enums (EffectList, LinkedList, etc.)
/// by lowering to nested constructor calls `Cons(a, Cons(b, …, Nil))`.
pub(super) fn translate_cons_chain_eq<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    let items = match rhs { Expr::SeqLit(items) => items, _ => return None };
    let lhs_name = match lhs { Expr::Identifier(n) => n, _ => return None };
    let var = env.get(lhs_name)?;
    let (lhs_ast, enum_name, dt) = match var {
        Var::EnumVar { ast, enum_name, dt } => (ast.clone(), enum_name.clone(), *dt),
        _ => return None,
    };
    let acc = build_cons_chain(items, &enum_name, dt, ctx, env, schemas)?;
    Some(lhs_ast._eq(&acc))
}

/// Translate `seq_var = ⟨e1, e2, …⟩`: pin length and assert per-index equality.
pub(super) fn translate_seq_lit_eq<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    let name = match lhs {
        Expr::Identifier(n) => n,
        _ => return None,
    };
    if env.get(name).is_none() { return None; }
    translate_seq_rhs_eq(name, rhs, ctx, env, schemas)
}

/// Translate `seq_name = rhs` where rhs is a SeqLit, ternary, or match; each branch guarded.
fn translate_seq_rhs_eq<'ctx>(
    name: &str,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    match rhs {
        Expr::SeqLit(items) =>
            translate_seq_lit_for_var(name, items, ctx, env, schemas),
        Expr::Ternary(c, a, b) => {
            let cond = translate_bool(c, ctx, env, schemas)?;
            let then_eq = translate_seq_rhs_eq(name, a, ctx, env, schemas)?;
            let else_eq = translate_seq_rhs_eq(name, b, ctx, env, schemas)?;
            Some(Bool::and(ctx, &[
                &cond.implies(&then_eq),
                &cond.not().implies(&else_eq),
            ]))
        }
        Expr::Match(scr, arms) => {
            let owned_name = name.to_string();
            let compiled = translate_match_arms(scr, arms, ctx, env, |body, e| {
                translate_seq_rhs_eq(&owned_name, body, ctx, e, schemas)
            })?;
            // Wildcard guards = ¬OR(prior testers).
            let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(compiled.len());
            let mut prior_testers: Vec<Bool<'ctx>> = Vec::new();
            for (tester_opt, body_eq) in compiled {
                let guard = match &tester_opt {
                    Some(t) => t.clone(),
                    None => {
                        let nots: Vec<Bool<'ctx>> =
                            prior_testers.iter().map(|p| p.not()).collect();
                        let refs: Vec<&Bool<'ctx>> = nots.iter().collect();
                        Bool::and(ctx, &refs)
                    }
                };
                clauses.push(guard.implies(&body_eq));
                if let Some(t) = tester_opt { prior_testers.push(t); }
            }
            let refs: Vec<&Bool<'ctx>> = clauses.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        _ => None,
    }
}

/// Assert `seq_name = ⟨items…⟩`: pin length and per-index equality for primitive, enum, and composite Seqs.
fn translate_seq_lit_for_var<'ctx>(
    name: &str,
    items: &[Expr],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    let var = env.get(name)?;

    if let Some((arr, len, elem)) = var.as_seq() {
        let n = items.len() as i64;
        let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
        clauses.push(len._eq(&Int::from_i64(ctx, n)));
        for (i, item) in items.iter().enumerate() {
            let idx = Int::from_i64(ctx, i as i64);
            let cell = arr.select(&idx);
            let eq = match elem {
                SeqElem::Int => {
                    let z = cell.as_int()?;
                    let v = translate_int(item, ctx, env)?;
                    z._eq(&v)
                }
                SeqElem::Bool => {
                    let z = cell.as_bool()?;
                    let v = translate_bool(item, ctx, env, schemas)?;
                    z._eq(&v)
                }
                SeqElem::Str => {
                    let z = cell.as_string()?;
                    let v = translate_str(item, ctx, env)?;
                    z._eq(&v)
                }
            };
            clauses.push(eq);
        }
        let refs: Vec<&Bool<'ctx>> = clauses.iter().collect();
        return Some(Bool::and(ctx, &refs));
    }

    // Enum-element Seq (DatatypeSeqVar with empty fields): `last_results = ⟨IntResult(42)⟩`.
    if let Some((arr, len, _, dt, fields)) = var.as_datatype_seq() {
        if fields.is_empty() {
            let enum_name = match var {
                Var::DatatypeSeqVar { type_name, .. } => type_name.clone(),
                _ => unreachable!(),
            };
            let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
            clauses.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            let elems: Option<Vec<Bool<'ctx>>> = with_target_enum_hint(
                Some((enum_name, dt)),
                || {
                    let mut tmp: Vec<Bool<'ctx>> = Vec::with_capacity(items.len());
                    for (i, item) in items.iter().enumerate() {
                        let v = resolve_enum_ast(item, ctx, env, schemas)?;
                        let idx = Int::from_i64(ctx, i as i64);
                        let cell = arr.select(&idx).as_datatype()?;
                        tmp.push(cell._eq(&v));
                    }
                    Some(tmp)
                },
            );
            clauses.extend(elems?);
            let refs: Vec<&Bool<'ctx>> = clauses.iter().collect();
            return Some(Bool::and(ctx, &refs));
        }
        // Composite-element Seq: items must be bare Identifiers with flat-expanded fields in env.
        let n = items.len() as i64;
        let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
        clauses.push(len._eq(&Int::from_i64(ctx, n)));
        for (i, item) in items.iter().enumerate() {
            let ident = match item {
                Expr::Identifier(s) => s,
                _ => return None,
            };
            let elem_dyn = build_composite_dynamic(ident, dt, fields, ctx, env)?;
            let idx = Int::from_i64(ctx, i as i64);
            let cell = arr.select(&idx);
            clauses.push(cell._eq(&elem_dyn));
        }
        let refs: Vec<&Bool<'ctx>> = clauses.iter().collect();
        return Some(Bool::and(ctx, &refs));
    }
    None
}

/// Resolve an expression to a compile-time Value (literal or PinnedInt) for candidate recording.
fn expr_to_const_value(e: &Expr, env: &HashMap<String, Var>) -> Option<Value> {
    match e {
        Expr::Int(n) => Some(Value::Int(*n)),
        Expr::Bool(b) => Some(Value::Bool(*b)),
        Expr::Str(s) => Some(Value::Str(s.clone())),
        Expr::Identifier(name) => match env.get(name)? {
            Var::PinnedInt(v) => Some(Value::Int(*v)),
            _ => None,
        },
        _ => None,
    }
}

/// Detect `∀ x ∈ A : x ∈ B`; returns the Z3 Set handle for B so the quantifier
/// translator can emit native `set_subset` instead of iterating a free Set.
pub(super) fn match_set_subset_body<'a, 'ctx>(
    body: &Expr,
    var: &str,
    env: &'a HashMap<String, Var<'ctx>>,
) -> Option<&'a z3::ast::Set<'ctx>> {
    let Expr::InExpr(lhs, rhs) = body else { return None };
    match lhs.as_ref() {
        Expr::Identifier(n) if n == var => {}
        _ => return None,
    }
    let Expr::Identifier(set_name) = rhs.as_ref() else { return None };
    let v = env.get(set_name)?;
    if let Some((set, _)) = v.as_set() { return Some(set); }
    if let Some((set, _, _, _, _)) = v.as_datatype_set() { return Some(set); }
    None
}

/// Translate `S = {a, b, c}`: builds an exact-membership Z3 literal set and records candidates
/// so `extract_set` can recover members without general Z3-set enumeration.
pub(super) fn translate_set_lit_eq<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    use z3::ast::Set as Z3Set;
    use z3::Sort;

    let items = match rhs {
        Expr::SetLit(items) => items,
        _ => return None,
    };
    let name = match lhs {
        Expr::Identifier(n) => n,
        _ => return None,
    };

    // Composite-element Set: items are bare Identifiers with flat-expanded fields.
    // Placeholder Value::Composite{}s recorded for #s cardinality; field values deferred.
    if let Some((set, _, dt, fields, candidates_cell)) =
        env.get(name).and_then(|v| v.as_datatype_set())
    {
        let mut lit = Z3Set::empty(ctx, &dt.sort);
        for item in items {
            let ident = match item {
                Expr::Identifier(s) => s.as_str(),
                _ => return None,
            };
            let dyn_val = build_composite_dynamic(ident, dt, fields, ctx, env)?;
            lit = lit.add(&dyn_val);
        }
        let placeholders: Vec<Value> = items.iter()
            .map(|_| Value::Composite(HashMap::new()))
            .collect();
        *candidates_cell.borrow_mut() = Some(placeholders);
        return Some(set._eq(&lit));
    }

    let (set_var, elem, candidates_cell) = env.get(name)?.as_set_with_candidates()?;

    let domain = match elem {
        SeqElem::Int  => Sort::int(ctx),
        SeqElem::Bool => Sort::bool(ctx),
        SeqElem::Str  => Sort::string(ctx),
    };
    let mut lit = Z3Set::empty(ctx, &domain);
    for item in items {
        match elem {
            SeqElem::Int  => { let z = translate_int(item, ctx, env)?; lit = lit.add(&z); }
            SeqElem::Bool => {
                let z = translate_bool(item, ctx, env, schemas)?;
                lit = lit.add(&z);
            }
            SeqElem::Str  => { let z = translate_str(item, ctx, env)?; lit = lit.add(&z); }
        }
    }

    // Record static candidates for extraction; leave None if any item is dynamic.
    let mut static_cands: Option<Vec<Value>> = Some(Vec::with_capacity(items.len()));
    for item in items {
        match (&mut static_cands, expr_to_const_value(item, env)) {
            (Some(acc), Some(v)) => acc.push(v),
            _ => { static_cands = None; break; }
        }
    }
    if let Some(cands) = static_cands {
        *candidates_cell.borrow_mut() = Some(cands);
    }

    Some(set_var._eq(&lit))
}

/// Build a Datatype Dynamic by applying the constructor to per-field env lookups under `prefix`.
pub(super) fn build_composite_dynamic<'ctx>(
    prefix: &str,
    dt: &'static DatatypeSort<'static>,
    fields: &[FieldKind],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<z3::ast::Dynamic<'ctx>> {
    let mut field_dyns: Vec<z3::ast::Dynamic<'ctx>> = Vec::with_capacity(fields.len());
    for fk in fields.iter() {
        let dynamic = match fk {
            FieldKind::Primitive { name, prim_type } => {
                let key = format!("{}.{}", prefix, name);
                let var = env.get(&key)?;
                match (prim_type.as_str(), var) {
                    ("Int" | "Nat" | "Pos", Var::IntVar(i)) =>
                        z3::ast::Dynamic::from_ast(i),
                    ("Int" | "Nat" | "Pos", Var::PinnedInt(v)) =>
                        z3::ast::Dynamic::from_ast(&Int::from_i64(ctx, *v)),
                    ("Bool", Var::BoolVar(b)) =>
                        z3::ast::Dynamic::from_ast(b),
                    ("String", Var::StrVar(s)) =>
                        z3::ast::Dynamic::from_ast(s),
                    _ => return None,
                }
            }
            FieldKind::Nested { name, dt: nested_dt, sub_fields, .. } => {
                let sub_prefix = format!("{}.{}", prefix, name);
                build_composite_dynamic(&sub_prefix, nested_dt, sub_fields, ctx, env)?
            }
            FieldKind::SeqField { .. } => {
                // Packing flat-expanded (arr, len) Seq fields into a Datatype Dynamic not yet wired.
                return None;
            }
        };
        field_dyns.push(dynamic);
    }
    let dyn_refs: Vec<&dyn Ast> = field_dyns.iter().map(|d| d as &dyn Ast).collect();
    Some(dt.variants[0].constructor.apply(&dyn_refs))
}

/// Translate `seq[i] = composite_var`: asserts `arr.select(idx) == composite` for a Seq(UserType).
pub(super) fn translate_seq_index_assign<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Bool<'ctx>> {
    let (seq_name, idx_expr) = match lhs {
        Expr::Index(seq_expr, idx_expr) => {
            let Expr::Identifier(name) = seq_expr.as_ref() else { return None };
            (name.as_str(), idx_expr.as_ref())
        }
        _ => return None,
    };
    let comp_name = match rhs {
        Expr::Identifier(n) => n.as_str(),
        _ => return None,
    };
    let var = env.get(seq_name)?;
    let (arr, _, _, dt, fields) = var.as_datatype_seq()?;
    // Guard: verify flat-expansion by checking the first leaf exists; prevents silent Bool mismatch.
    let first_field = fields.first().map(|f| f.name())?;
    if !env.contains_key(&format!("{}.{}", comp_name, first_field)) {
        return None;
    }
    let idx = translate_int(idx_expr, ctx, env)?;
    let composite = build_composite_dynamic(comp_name, dt, fields, ctx, env)?;
    let elem = arr.select(&idx);
    Some(elem._eq(&composite))
}

/// Bind each field of a composite Seq element into env as `<prefix>.<field>` via Datatype accessors.
/// Recurses for Nested fields; returns false on shape mismatch so the quantifier fails loudly.
pub(super) fn bind_composite_fields<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    elem_dyn: &z3::ast::Dynamic<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'ctx>,
    prefix: &str,
) -> bool {
    use crate::core::SeqFieldElem;
    let Some(elem) = elem_dyn.as_datatype() else { return false };
    // SeqField uses its own arr_idx/len_idx (2 slots); track position for Primitive/Nested.
    let mut acc_pos: usize = 0;
    for fk in fields.iter() {
        match fk {
            FieldKind::Primitive { name, prim_type } => {
                if acc_pos >= dt.variants[0].accessors.len() { return false; }
                let raw = dt.variants[0].accessors[acc_pos].apply(&[&elem]);
                acc_pos += 1;
                let key = format!("{}.{}", prefix, name);
                let var = match prim_type.as_str() {
                    "Int" | "Nat" | "Pos" => raw.as_int().map(Var::IntVar),
                    "Bool"   => raw.as_bool().map(Var::BoolVar),
                    "String" => raw.as_string().map(Var::StrVar),
                    _ => None,
                };
                let Some(v) = var else { return false };
                env.insert(key, v);
            }
            FieldKind::Nested { name, dt: nested_dt, sub_fields, .. } => {
                if acc_pos >= dt.variants[0].accessors.len() { return false; }
                let raw = dt.variants[0].accessors[acc_pos].apply(&[&elem]);
                acc_pos += 1;
                let sub_prefix = format!("{}.{}", prefix, name);
                if !bind_composite_fields(env, &raw, sub_fields, nested_dt, &sub_prefix) {
                    return false;
                }
            }
            FieldKind::SeqField { name, arr_idx, len_idx, elem: seq_elem, .. } => {
                if *len_idx >= dt.variants[0].accessors.len() { return false; }
                let arr_dyn = dt.variants[0].accessors[*arr_idx].apply(&[&elem]);
                let len_dyn = dt.variants[0].accessors[*len_idx].apply(&[&elem]);
                acc_pos = *len_idx + 1;
                let arr = arr_dyn.as_array();
                let len = len_dyn.as_int();
                let (Some(arr), Some(len)) = (arr, len) else { return false; };
                let key = format!("{}.{}", prefix, name);
                match seq_elem {
                    SeqFieldElem::Primitive(elem) => {
                        env.insert(key, Var::SeqVar { arr, len, elem: *elem });
                    }
                    SeqFieldElem::Enum { dt, enum_name } => {
                        env.insert(key, Var::DatatypeSeqVar {
                            arr, len, type_name: enum_name.clone(),
                            dt: *dt, fields: Vec::new(),
                        });
                    }
                    SeqFieldElem::Composite { dt, type_name, sub_fields } => {
                        env.insert(key, Var::DatatypeSeqVar {
                            arr, len, type_name: type_name.clone(),
                            dt: *dt, fields: sub_fields.clone(),
                        });
                    }
                }
            }
        }
    }
    true
}

/// Translate `A = B` for whole-Seq equality: element-wise conjunction plus length match.
/// Requires pinned lengths; returns None if either side isn't a Seq or lengths mismatch.
pub(super) fn translate_seq_eq<'ctx>(
    lhs: &Expr,
    rhs: &Expr,
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
) -> Option<Bool<'ctx>> {
    let Expr::Identifier(l_name) = lhs else { return None };
    let Expr::Identifier(r_name) = rhs else { return None };
    let l = env.get(l_name)?;
    let r = env.get(r_name)?;
    match (l, r) {
        (Var::SeqVar { arr: la, len: ll, elem: le },
         Var::SeqVar { arr: ra, len: lr, elem: re }) => {
            if le != re { return None; }
            let ln = ll.simplify().as_i64()?;
            let rn = lr.simplify().as_i64()?;
            if ln != rn { return None; }
            let mut clauses: Vec<Bool> = Vec::with_capacity(ln as usize);
            for i in 0..ln {
                let idx = Int::from_i64(ctx, i);
                let l_elem = la.select(&idx);
                let r_elem = ra.select(&idx);
                clauses.push(l_elem._eq(&r_elem));
            }
            let refs: Vec<&Bool> = clauses.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        (Var::DatatypeSeqVar { arr: la, len: ll, type_name: lt, .. },
         Var::DatatypeSeqVar { arr: ra, len: lr, type_name: rt, .. }) => {
            if lt != rt { return None; }
            let ln = ll.simplify().as_i64()?;
            let rn = lr.simplify().as_i64()?;
            if ln != rn { return None; }
            let mut clauses: Vec<Bool> = Vec::with_capacity(ln as usize);
            for i in 0..ln {
                let idx = Int::from_i64(ctx, i);
                let l_elem = la.select(&idx);
                let r_elem = ra.select(&idx);
                clauses.push(l_elem._eq(&r_elem));
            }
            let refs: Vec<&Bool> = clauses.iter().collect();
            Some(Bool::and(ctx, &refs))
        }
        _ => None,
    }
}
