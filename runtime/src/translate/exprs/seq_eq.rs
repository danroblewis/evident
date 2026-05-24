//! Seq / Set-equality translation — the equality shapes that produce a
//! conjunction rather than a single scalar `_eq`. Covers Cons-chain
//! enum literals, `seq = ⟨…⟩` literal assignment, `set = {…}` literal
//! membership, whole-Seq equality, single-element composite assignment,
//! and the composite-element binding plumbing (`bind_composite_fields`,
//! `build_composite_dynamic`) shared with the mapping + quantifier paths.

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

/// Handle `enum_var = ⟨a, b, c⟩` where `enum_var` is a Cons/Nil-shaped
/// enum (one variant with 0 fields = "Nil", one variant with 2 fields
/// where the second field's declared type matches the enum itself =
/// "Cons"). `EffectList` (pending Phase 6.4 migration to Seq),
/// user-defined `LinkedList`, etc. all qualify. The literal is lowered to nested
/// constructor calls: `Cons(a, Cons(b, Cons(c, Nil)))`.
///
/// `⟨⟩` (empty) lowers to just the Nil constructor.
///
/// Returns None if the LHS isn't an enum-typed Identifier, the RHS
/// isn't a SeqLit, or the enum lacks the Nil/Cons shape.
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

/// Handle `seq_var = ⟨e1, e2, …⟩` (sequence-literal assignment).
///
/// Returns the conjunction `len == items.len() ∧ ∀i: arr[i] == translated(e_i)`
/// when `lhs` is an `Identifier(name)` resolving to a `Var::SeqVar` (primitive
/// element) or `Var::DatatypeSeqVar` (composite element), and `rhs` is an
/// `Expr::SeqLit(items)`. Returns `None` otherwise — caller then falls back
/// through the Bool/Int/Str equality paths.
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

/// Translate `seq_name = <rhs>` where `rhs` is a Seq-valued expression
/// — a SeqLit, a `cond ? a : b` ternary whose branches are Seq-valued,
/// or a `match scrutinee | arm ⇒ body` whose arm bodies are Seq-valued.
///
/// The result is a Bool conjunction: each arm/branch contributes a
/// guarded equality `(arm_guard ⇒ seq_name = arm_body)`. For wildcard
/// arms the guard is the negation of all prior arms' guards.
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
            // Body translator: produces `seq_name = arm_body` for each arm.
            let owned_name = name.to_string();
            let compiled = translate_match_arms(scr, arms, ctx, env, |body, e| {
                translate_seq_rhs_eq(&owned_name, body, ctx, e, schemas)
            })?;
            // Fold: each arm contributes a guarded equality. Wildcard
            // arms fire when no prior tester matched (¬OR(priors)).
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

/// Core: assert `seq_name = ⟨items[0], items[1], …⟩` — pins length and
/// per-index equality. Handles primitive, enum-element, and composite-
/// record Seq element kinds. Returns None if `seq_name` doesn't resolve
/// to a Seq-shaped Var or any item doesn't translate.
fn translate_seq_lit_for_var<'ctx>(
    name: &str,
    items: &[Expr],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<Bool<'ctx>> {
    let var = env.get(name)?;

    // Primitive-element Seq: pin length, then per-element equality on the
    // underlying Z3 array.
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

    // Enum-element Seq (`Seq(EnumType)` — DatatypeSeqVar with empty
    // fields). Each item is an enum constructor call like `IntResult(42)`
    // (or a bare nullary variant identifier). Translate to a Datatype
    // value via the existing enum-aware path and assert per-index
    // equality. `last_results = ⟨IntResult(42)⟩` is the headline use.
    if let Some((arr, len, _, dt, fields)) = var.as_datatype_seq() {
        if fields.is_empty() {
            let enum_name = match var {
                Var::DatatypeSeqVar { type_name, .. } => type_name.clone(),
                _ => unreachable!(),
            };
            let mut clauses: Vec<Bool<'ctx>> = Vec::with_capacity(items.len() + 1);
            clauses.push(len._eq(&Int::from_i64(ctx, items.len() as i64)));
            // Hint so that nested ⟨...⟩ items lower against this enum.
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
        // Composite-element Seq: each item must be a bare Identifier referring to
        // flat sub-schema fields (e.g. `ball_rect`). Walk the Datatype's FieldKind
        // list and assemble a constructor application from `env["ident.field"]`
        // lookups, recursing for nested composites (e.g. `ball_rect.color.r`).
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

/// Resolve an expression to a compile-time `Value` if it's a literal
/// (or an identifier bound to a known constant). Used by
/// `translate_set_lit_eq` to record the Set's members for later
/// extraction without needing to model-evaluate at extract time.
///
/// Returns None for expressions whose value depends on the model
/// (free variables, arithmetic over them, etc.). v1 supports this
/// statically-resolvable subset because every Set use site in the
/// FFI surface is expected to be `S = {literal_constants…}`. The
/// dynamic case is a Phase 6.6+ extension.
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

/// Recognize `∀ x ∈ A : x ∈ B` — the subset pattern. Returns the
/// Z3 Set handle for `B` (the superset) if `body` is `Expr::InExpr`
/// whose LHS is exactly the bound name `var` and whose RHS is an
/// Identifier resolving to a SetVar / DatatypeSetVar. Used by the
/// quantifier translator to emit Z3 native `set_subset` instead of
/// trying to unroll a free Set (which has no candidates to iterate).
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

/// Translate `S = {a, b, c}` where S is a SetVar and the RHS is a
/// SetLit. Builds a Z3 literal set by add'ing each element to
/// `Set::empty`, then asserts set-equality against the variable —
/// this gives EXACT membership semantics (S contains a, b, c and
/// nothing else). Also records the literal items in S's `candidates`
/// cell so `extract_set` can recover the members from the model
/// without needing general Z3-set enumeration.
///
/// Returns None when LHS isn't a SetVar or RHS isn't a SetLit, or
/// when the SetLit elements can't all be translated as the Set's
/// element type — caller falls through to the regular Eq path.
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

    // Composite-element Set: items must be bare Identifiers referring
    // to flat-expanded composites (same shape as composite SeqLit).
    // Build each element as a Datatype Dynamic via `build_composite_dynamic`,
    // assemble a literal Z3 Set, and assert set-equality against the var.
    // We record one `Value::Composite{}` placeholder per literal item so
    // `#s` (cardinality) can return the count; per-element field values
    // are left empty in v1 — extracting Set(Composite) into a populated
    // `Value` is deferred until there's a concrete consumer.
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

    // Build the Z3 literal set by add'ing each translated item.
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

    // Best-effort: record statically-resolvable candidates for the
    // extract path. If any item isn't a compile-time constant, leave
    // candidates as None — extraction silently omits the binding,
    // matching the pre-Phase-6.1 behavior for that case.
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

/// Build a single Datatype value (`Dynamic`) by applying `dt.variants[0]
/// .constructor` to one Dynamic per `FieldKind`. Each primitive field is
/// resolved via `env.get(&format!("{prefix}.{field_name}"))`; each nested
/// composite is resolved by recursing with prefix
/// `format!("{prefix}.{field_name}")`.
///
/// Used by `translate_seq_lit_eq` to translate `seq = ⟨ident1, ident2, …⟩`
/// when seq is a `Seq(UserType)` and each `identK` names a flat-expanded
/// sub-schema instance whose fields already exist in env as
/// `identK.field…` Z3 consts.
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
                // Building a Dynamic composite-value when one of its fields
                // is a Seq requires reading the user's flat-expanded
                // (arr, len) pair and packing them as two accessor values.
                // Wire-up TBD; for now signal failure so the literal path
                // drops and the user gets a translator error pointing at
                // their composite literal.
                return None;
            }
        };
        field_dyns.push(dynamic);
    }
    let dyn_refs: Vec<&dyn Ast> = field_dyns.iter().map(|d| d as &dyn Ast).collect();
    Some(dt.variants[0].constructor.apply(&dyn_refs))
}

/// Handle `seq[i] = composite_var` (single-element composite assignment
/// against a `Seq(UserType)`). Used by `output.rects[#state.dots] = player_rect`
/// in the dot-collect engine: assign one composite value into one slot of a
/// composite-element seq.
///
/// LHS must be `Index(Identifier(seq_name), idx_expr)` where `seq_name`
/// resolves to a `Var::DatatypeSeqVar`. RHS must be `Identifier(comp_name)`
/// where `comp_name.*` keys exist in env (flat-expanded composite from a
/// sub-schema membership). Builds the per-element Datatype value via
/// `build_composite_dynamic` and asserts `arr.select(idx) == composite`.
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
    // The composite must be flat-expanded — verify by checking at least one
    // expected leaf exists in env. Without this, `output.rects[i] = player_rect`
    // would silently match `player_rect ∈ Bool` and translate wrong.
    let first_field = fields.first().map(|f| f.name())?;
    if !env.contains_key(&format!("{}.{}", comp_name, first_field)) {
        return None;
    }
    let idx = translate_int(idx_expr, ctx, env)?;
    let composite = build_composite_dynamic(comp_name, dt, fields, ctx, env)?;
    let elem = arr.select(&idx);
    Some(elem._eq(&composite))
}

/// Walk a composite seq element and bind each declared field as
/// `<prefix>.<field_name>` in env, with the field's Z3 expression
/// extracted via the Datatype's accessor. Used by `∀ var ∈ <seq>`
/// composite iteration: for each iteration index i, the body
/// references `var.field1`, `var.field2`, etc. — those resolve via
/// env-key lookup, so we populate env with the right per-iteration
/// values before translating the body.
///
/// Recurses for `FieldKind::Nested` (e.g. `dot.color.r` where
/// `color ∈ Color`). Returns false on shape mismatch (caller
/// should fail the whole quantifier rather than silently produce
/// a wrong model).
pub(super) fn bind_composite_fields<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    elem_dyn: &z3::ast::Dynamic<'ctx>,
    fields: &[FieldKind],
    dt: &DatatypeSort<'ctx>,
    prefix: &str,
) -> bool {
    use crate::core::SeqFieldElem;
    let Some(elem) = elem_dyn.as_datatype() else { return false };
    // Track linear accessor position for Primitive / Nested fields.
    // SeqField uses its own arr_idx / len_idx, not the loop counter,
    // since each SeqField consumes TWO accessor slots.
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

/// Whole-Seq equality: `A = B` where both `A` and `B` resolve to Seq
/// vars (primitive `SeqVar` or `DatatypeSeqVar`). Desugars to
/// element-wise `∀ i ∈ {0..n-1} : A[i] = B[i]` plus a length match.
///
/// Returns None if either side isn't a Seq, the element kinds don't
/// match, or either length isn't a literal int (we need a pinned
/// length to unroll the element-wise conjunction).
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
