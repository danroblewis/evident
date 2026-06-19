//! Expression translation: Evident `Expr` → Z3 ASTs, split by result category.
//! All submodules share one flat namespace via `use super::*` + the `pub(super)`
//! re-exports below, so the mutual recursion between the translators (bool ↔ int
//! ↔ enum ↔ seq) works exactly as it did in the single-file form.
//!
//!   - resolve    — mapping/enum/seq-handle resolution from env
//!   - values     — scalar translators (str/int/real) + record-vector lifting
//!   - equations  — seq/set equality + composite binding/building
//!   - bool       — encode_bool (the dispatcher) + literal_range

use std::collections::HashMap;
use z3::ast::{Ast, Bool};
use z3::{Context, DatatypeSort};

use crate::core::ast::*;
use crate::core::{EnumRegistry, Var};

mod resolve;
mod values;
mod equations;
mod bool;

// resolve + bool export resolve_mapping / encode_bool to encode::inline,
// so those two re-exports widen to pub(in crate::encode); the rest stay
// exprs-internal (mutual visibility comes from `use super::*` in each submodule).
pub(super) use resolve::*;
pub(super) use self::bool::*;
use values::*;
use equations::*;

// ───────────────────────── enum registry guard + target hint (thread-local) ─────────────────────────

thread_local! {

    static ACTIVE_ENUMS: std::cell::Cell<Option<*const EnumRegistry>> =
        const { std::cell::Cell::new(None) };
}

pub struct EnumRegistryGuard {
    prev: Option<*const EnumRegistry>,
}

impl EnumRegistryGuard {
    pub fn new(enums: Option<&EnumRegistry>) -> Self {
        let new_ptr = enums.map(|r| r as *const EnumRegistry);
        let prev = ACTIVE_ENUMS.with(|c| {
            let was = c.get();
            c.set(new_ptr);
            was
        });
        Self { prev }
    }
}

impl Drop for EnumRegistryGuard {
    fn drop(&mut self) {
        ACTIVE_ENUMS.with(|c| c.set(self.prev));
    }
}

pub(super) fn with_active_enums<R>(f: impl FnOnce(Option<&EnumRegistry>) -> R) -> R {
    let ptr = ACTIVE_ENUMS.with(|c| c.get());

    let opt = ptr.map(|p| unsafe { &*p });
    f(opt)
}

thread_local! {

    static TARGET_ENUM_HINT: std::cell::RefCell<Option<(String, &'static DatatypeSort<'static>)>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) fn with_target_enum_hint<R>(
    target: Option<(String, &'static DatatypeSort<'static>)>,
    f: impl FnOnce() -> R,
) -> R {
    let prev = TARGET_ENUM_HINT.with(|c| c.replace(target));
    let r = f();
    TARGET_ENUM_HINT.with(|c| { *c.borrow_mut() = prev; });
    r
}

pub(super) fn current_target_enum() -> Option<(String, &'static DatatypeSort<'static>)> {
    TARGET_ENUM_HINT.with(|c| c.borrow().clone())
}

// ───────────────────────── shared match-arm compilation ─────────────────────────

pub(super) type CompiledArm<'ctx, T> = (Option<Bool<'ctx>>, T);

pub(super) fn encode_match_arms<'ctx, T>(
    scr: &Expr,
    arms: &[crate::core::ast::MatchArm],
    ctx: &'ctx Context,
    env: &HashMap<String, Var<'ctx>>,
    body_translator: impl Fn(&Expr, &HashMap<String, Var<'ctx>>) -> Option<T>,
) -> Option<Vec<CompiledArm<'ctx, T>>> {
    use crate::core::ast::MatchPattern;

    let (scr_dt, dt, scr_enum_name) = match scr {
        Expr::Identifier(n) if !n.contains('.') => {
            match env.get(n)? {
                Var::EnumVar { ast, dt, enum_name } =>
                    (ast.clone(), *dt, enum_name.clone()),
                Var::EnumValue { .. } => return None,
                _ => return None,
            }
        }
        Expr::Index(seq_expr, idx_expr) => {
            let Expr::Identifier(seq_name) = seq_expr.as_ref() else { return None };
            if seq_name.contains('.') { return None; }
            let (arr, dt, type_name) = match env.get(seq_name)? {
                Var::DatatypeSeqVar { arr, dt, type_name, fields, .. }
                    if fields.is_empty() =>
                        (arr.clone(), *dt, type_name.clone()),
                _ => return None,
            };
            let idx = encode_int(idx_expr, ctx, env)?;
            let elem_dt = arr.select(&idx).as_datatype()?;
            (elem_dt, dt, type_name)
        }
        _ => return None,
    };
    let mut compiled: Vec<CompiledArm<T>> = Vec::new();
    for arm in arms {
        match &arm.pattern {
            MatchPattern::Wildcard => {
                let body = body_translator(&arm.body, env)?;
                compiled.push((None, body));
            }
            MatchPattern::Ctor { name, binds } => {
                let var_idx = dt.variants.iter()
                    .position(|v| v.constructor.name() == *name)?;
                let z3_var = &dt.variants[var_idx];
                if binds.len() != z3_var.accessors.len() { return None; }
                let tester = z3_var.tester.apply(&[&scr_dt]).as_bool()?;
                let mut env2 = env.clone();
                let scr_enum_name = scr_enum_name.clone();
                let field_decls: Vec<crate::core::ast::EnumField> = with_active_enums(|enums| {
                    enums.and_then(|er| {
                        er.by_name.borrow().get(&scr_enum_name)
                            .and_then(|(_, variants)| {
                                variants.iter()
                                    .find(|v| v.name == *name)
                                    .map(|v| v.fields.clone())
                            })
                    }).unwrap_or_default()
                });
                for (j, bind_opt) in binds.iter().enumerate() {
                    let Some(bind_name) = bind_opt else { continue };
                    let acc = &z3_var.accessors[j];
                    let raw = acc.apply(&[&scr_dt]);

                    let var = if let Some(i) = raw.as_int() { Var::IntVar(i) }
                        else if let Some(b) = raw.as_bool() { Var::BoolVar(b) }
                        else if let Some(s) = raw.as_string() { Var::StrVar(s) }
                        else if let Some(r) = raw.as_real() { Var::RealVar(r) }
                        else if let Some(payload_dt) = raw.as_datatype() {

                            let field_type = field_decls.get(j)
                                .map(|f| f.type_name.clone())
                                .unwrap_or_else(|| scr_enum_name.clone());
                            let payload_dt_sort: &'static DatatypeSort<'static> =
                                with_active_enums(|enums| {
                                    enums.and_then(|er| {
                                        er.by_name.borrow().get(&field_type)
                                            .map(|(d, _)| *d)
                                    })
                                }).unwrap_or(dt);
                            Var::EnumVar {
                                ast: payload_dt,
                                enum_name: field_type,
                                dt: payload_dt_sort,
                            }
                        }
                        else { return None; };
                    env2.insert(bind_name.clone(), var);
                }
                let body = body_translator(&arm.body, &env2)?;
                compiled.push((Some(tester), body));
            }
        }
    }
    Some(compiled)
}

pub(super) fn fold_arms_to_ite<'ctx, T>(
    mut compiled: Vec<CompiledArm<'ctx, T>>,
) -> Option<T>
where
    T: z3::ast::Ast<'ctx>,
{
    if compiled.is_empty() { return None; }
    let (_, last_body) = compiled.pop()?;
    let mut acc = last_body;
    for (tester_opt, body) in compiled.into_iter().rev() {
        match tester_opt {
            None       => { acc = body; }
            Some(tester) => { acc = tester.ite(&body, &acc); }
        }
    }
    Some(acc)
}
