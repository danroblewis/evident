//! `inline_body_items` — the recursive constraint-translation walker.
//! Handles `Membership` (declare-if-new), `Constraint` (translate +
//! assert), `Passthrough` (`..ClaimName`), and `ClaimCall` (with
//! mappings + per-call fresh Z3 names for the claim's unmapped
//! internals).
//!
//! Bare-identifier-as-passthrough (`Constraint(Identifier(name))`
//! where `name` is a known claim) is handled BEFORE this walker
//! runs by a self-hosted desugar pass —
//! `stdlib/passes/desugar_passthrough.ev` paired with
//! `commands/desugar.rs::auto_apply_desugar`, wired into every CLI
//! subcommand. By the time the AST arrives here, the rewrite has
//! already turned the bare form into an explicit `Passthrough` node.

use std::collections::{HashMap, HashSet};
use z3::{Context, SatResult, Solver};
use z3::ast::{Ast, Bool};

use crate::ast::*;
use crate::pretty;
use super::types::{DatatypeRegistry, EnumRegistry, Var};
use super::declare::{declare_var, declare_var_named, next_call_id};
use super::exprs::{resolve_mapping, translate_bool};

/// Add `b` to the solver. With a tracker, use `assert_and_track` so
/// the constraint joins the unsat-core machinery; otherwise plain
/// `assert`. The tracker stays the same across every assertion derived
/// from one top-level body item, so the entire item shows up as one
/// entry in the core.
fn track_assert(solver: &Solver<'static>, b: &Bool<'static>, tracker: Option<&Bool<'static>>) {
    match tracker {
        Some(t) => solver.assert_and_track(b, t),
        None    => solver.assert(b),
    }
}

/// Rewrite identifiers in `e` so any leading-segment match against the
/// type's `field_set` becomes `<prefix>.<original>`. Used to inherit a
/// type body's Constraint items onto a sub-schema instance:
///
/// ```text
/// type Foo(p ∈ Int)
///     d ∈ Int = p + 1   -- inside Foo's body, `p` and `d` are bare
///
/// claim caller
///     f ∈ Foo (p ↦ 5)
///     -- The body constraint `d = p + 1`, when inherited onto `f`,
///     -- becomes `f.d = f.p + 1`. This function does that rewrite.
/// ```
///
/// Identifiers whose leading segment is NOT a field of the type are
/// left untouched — they're external references (other schemas,
/// quantifier-bound names, constants like `is_first_tick`).
///
/// Both `Identifier("foo")` and `Identifier("foo.bar")` are recognized
/// — the parser folds source-level `foo.bar` into a single dotted
/// `Identifier` (see ast.rs::Field comment). Receiver-side recursion
/// also covers the explicit `Field(receiver, …)` shape used when the
/// receiver is itself an expression (e.g. `seq[i].x`).
fn rewrite_idents_with_prefix(
    e: &Expr,
    prefix: &str,
    field_set: &HashSet<String>,
) -> Expr {
    let r = |x: &Expr| Box::new(rewrite_idents_with_prefix(x, prefix, field_set));
    let rv = |xs: &Vec<Expr>| xs.iter()
        .map(|x| rewrite_idents_with_prefix(x, prefix, field_set))
        .collect();
    match e {
        Expr::Identifier(name) => {
            let first_seg = name.split('.').next().unwrap_or("");
            if field_set.contains(first_seg) {
                Expr::Identifier(format!("{}.{}", prefix, name))
            } else {
                e.clone()
            }
        }
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => e.clone(),
        Expr::SetLit(xs)  => Expr::SetLit(rv(xs)),
        Expr::SeqLit(xs)  => Expr::SeqLit(rv(xs)),
        Expr::Tuple(xs)   => Expr::Tuple(rv(xs)),
        Expr::Range(a, b) => Expr::Range(r(a), r(b)),
        Expr::InExpr(a, b) => Expr::InExpr(r(a), r(b)),
        Expr::Forall(vars, range, body) => {
            // Quantifier bound names shadow field names within the body.
            // If a quantifier introduces `pos` (say) and the type also
            // has a field `pos`, body uses of `pos` inside this forall
            // should NOT get prefixed — they're the bound var, not the
            // field. Build a temporary field_set that excludes the
            // shadowed names.
            let inner_set: HashSet<String> = field_set.iter()
                .filter(|f| !vars.contains(f))
                .cloned()
                .collect();
            Expr::Forall(
                vars.clone(),
                Box::new(rewrite_idents_with_prefix(range, prefix, field_set)),
                Box::new(rewrite_idents_with_prefix(body,  prefix, &inner_set)),
            )
        }
        Expr::Exists(vars, range, body) => {
            let inner_set: HashSet<String> = field_set.iter()
                .filter(|f| !vars.contains(f))
                .cloned()
                .collect();
            Expr::Exists(
                vars.clone(),
                Box::new(rewrite_idents_with_prefix(range, prefix, field_set)),
                Box::new(rewrite_idents_with_prefix(body,  prefix, &inner_set)),
            )
        }
        // Call's first arg is the function/type/constructor NAME — don't
        // touch. Only its args might contain field refs.
        Expr::Call(name, args) => Expr::Call(name.clone(), rv(args)),
        Expr::Cardinality(x) => Expr::Cardinality(r(x)),
        Expr::Index(a, b)    => Expr::Index(r(a), r(b)),
        Expr::Field(recv, f) => Expr::Field(r(recv), f.clone()),
        Expr::Binary(op, a, b) => Expr::Binary(op.clone(), r(a), r(b)),
        Expr::Not(x)           => Expr::Not(r(x)),
        Expr::Ternary(c, a, b) => Expr::Ternary(r(c), r(a), r(b)),
        Expr::Match(scr, arms) => {
            let new_arms: Vec<MatchArm> = arms.iter().map(|arm| {
                // Pattern-bound names shadow field names within this arm.
                let shadowed: HashSet<String> = match &arm.pattern {
                    MatchPattern::Ctor { binds, .. } => binds.iter()
                        .filter_map(|b| b.clone())
                        .collect(),
                    MatchPattern::Wildcard => HashSet::new(),
                };
                let inner: HashSet<String> = field_set.iter()
                    .filter(|n| !shadowed.contains(*n))
                    .cloned()
                    .collect();
                MatchArm {
                    pattern: arm.pattern.clone(),
                    body: Box::new(rewrite_idents_with_prefix(&arm.body, prefix, &inner)),
                }
            }).collect();
            Expr::Match(r(scr), new_arms)
        }
        Expr::Matches(x, p) => Expr::Matches(r(x), p.clone()),
    }
}

/// Recursively translate a list of body items into the solver. Used by
/// the constraint-translation pass of both `evaluate` and `build_cache`,
/// and also called recursively when a `Passthrough`, bare-identifier
/// passthrough, or `ClaimCall` references another claim's body.
///
/// Without this, passthroughs only inlined `Constraint` items — any
/// `ClaimCall` (e.g. `PlayerPhysics(state mapsto state.player, …)`)
/// inside a passthrough was silently dropped. Same problem inside a
/// `ClaimCall`: nested claim calls in the called claim's body were
/// dropped. That broke `..DotCollectGameEngine` (no player, no physics,
/// no background — black screen).
///
/// `visited` is a per-claim depth counter that bounds inlining
/// recursion. Each entry maps a claim name to how many frames of
/// it are currently on the inlining stack. A frame can re-enter the
/// same claim up to `MAX_INLINE_DEPTH` times — enough to walk a
/// recursive AST (transpilers, list emitters, etc.) but bounded so
/// pathological self-passthrough cycles don't OOM. Without unrolling
/// at all, the transpiler-as-recursive-claims pattern doesn't work
/// (Z3 invents arbitrary string values for un-asserted `tail_out`
/// bindings). The depth bound is overridable via
/// `EVIDENT_MAX_INLINE_DEPTH` for ASTs deeper than the default.

/// Default cap — large enough for any realistic shader/transpiler AST,
/// small enough that a self-passthrough loop trips it before the
/// translation context blows out.
const DEFAULT_MAX_INLINE_DEPTH: usize = 64;

fn max_inline_depth() -> usize {
    std::env::var("EVIDENT_MAX_INLINE_DEPTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_INLINE_DEPTH)
}

/// Try to enter a frame of `name` on the inlining stack. Returns
/// `Some(depth)` (the post-increment count) on success, `None` if
/// we'd exceed the depth cap. `depth > 1` ⇒ this is a recursive
/// frame; callers use that to force fresh per-call declarations
/// for body-internal Memberships, otherwise the env-clone would
/// shadow them with outer-scope vars and recursive claims would
/// self-reference (e.g. `out = "x " ++ tail_out` where `tail_out`
/// is the SAME Z3 const as the outer call's `tail_out`).
fn try_enter(visited: &mut HashMap<String, usize>, name: &str) -> Option<usize> {
    let max = max_inline_depth();
    let cnt = visited.entry(name.to_string()).or_insert(0);
    if *cnt >= max {
        None
    } else {
        *cnt += 1;
        Some(*cnt)
    }
}

/// Counterpart to `try_enter` — call after the inlined body has been
/// translated. Removes the entry entirely when its count hits zero
/// so subsequent same-name lookups don't see stale state.
fn exit_frame(visited: &mut HashMap<String, usize>, name: &str) {
    if let Some(cnt) = visited.get_mut(name) {
        *cnt -= 1;
        if *cnt == 0 { visited.remove(name); }
    }
}

/// Pre-isolate helper-local Z3 consts: when entering a ClaimCall, any
/// caller-scope vars whose names match the called claim's body
/// Memberships PAST `param_count` (i.e. the helper's internal locals,
/// not its first-line input/output slots) are removed from the cloned
/// inner env. This prevents recursive helper invocations from
/// accidentally sharing locals via the env-clone chain — without it,
/// nested `emit_ternary(...)` would reuse the OUTER `emit_ternary`'s
/// `cnd`/`thn`/`els` Z3 consts, collapsing distinct AST values to one
/// const and going UNSAT.
///
/// Slot params (the leading body Memberships up to `param_count`) are
/// PRESERVED in the clone so the helper's body can reach the
/// outer-supplied values via names-match composition.
fn isolate_helper_locals(
    body: &[BodyItem],
    inner: &mut HashMap<String, Var<'static>>,
    param_count: usize,
) {
    // When the claim has no first-line params (param_count == 0), we
    // can't tell input slots from helper-locals — fall back to the
    // legacy names-match behavior: keep everything in the cloned env
    // so body Memberships that match outer scope re-use those Z3
    // consts. Helpers that NEED isolation (transpiler-style recursive
    // claims) must use first-line params to declare which body
    // Memberships are inputs.
    if param_count == 0 { return; }
    for (i, item) in body.iter().enumerate() {
        if i < param_count { continue; } // input/output slot — keep.
        if let BodyItem::Membership { name, .. } = item {
            inner.remove(name);
            let prefix = format!("{}.", name);
            let dotted: Vec<String> = inner.keys()
                .filter(|k| k.starts_with(&prefix)).cloned().collect();
            for k in dotted { inner.remove(&k); }
        }
    }
}

/// Returns true if the active inlining guard is satisfiable (or there
/// is no guard). Used to PRUNE recursive ClaimCall expansion when the
/// guard is provably false — the body would generate only dead
/// constraints (Z3 would prove them vacuously true), so skipping the
/// inline saves the translation cost.
///
/// Without this prune, recursive transpiler-style claims (e.g.
/// `e_is_binary ⇒ emit_binary` where `emit_binary` calls `emit_expr`
/// on subexpressions) cascade unconditionally — each level multiplies
/// the inlined body count even though most branches won't fire.
///
/// Implementation: push the guard into the solver, ask Z3 if it's
/// satisfiable in the current scope, pop. Z3 prunes propositional
/// contradictions in microseconds; this lets the depth bound do its
/// real job (cutting genuine cycles) instead of bounding work-per-node.
fn guard_is_satisfiable(
    solver: &Solver<'static>,
    guard: &Option<Bool<'static>>,
) -> bool {
    let g = match guard {
        None => return true,
        Some(g) => g,
    };
    let trace = std::env::var("EVIDENT_INLINE_TRACE").is_ok();
    let t0 = if trace { Some(std::time::Instant::now()) } else { None };
    solver.push();
    solver.assert(g);
    let result = solver.check();
    solver.pop(1);
    if let Some(t0) = t0 {
        eprintln!("[inline] sat-check {:?} in {:?}", result, t0.elapsed());
    }
    !matches!(result, SatResult::Unsat)
}
/// Combine guard + body Bool: `guard ⇒ body` if guarded, else just
/// the body. Operates on already-translated Z3 Bool asts so the guard's
/// resolution is FROZEN at the point a guarded claim was entered —
/// subsequent shadowing in deeper recursive frames can't accidentally
/// rebind the guard's identifiers to fresh per-frame consts of the
/// same name. (That bug used to silently make recursive transpilers
/// emit unconstrained outputs because the depth-1 `e_is_unaryneg`
/// guard, when consumed at depth-2, resolved to depth-2's freshly
/// shadowed `e_is_unaryneg` — which Z3 had constrained to `false`
/// because the inner expression isn't a UnaryNeg.)
fn guarded_bool<'ctx>(b: Bool<'ctx>, guard: &Option<Bool<'ctx>>) -> Bool<'ctx> {
    match guard {
        None => b,
        Some(g) => g.implies(&b),
    }
}

/// Compose two pre-translated guards: `outer ∧ inner`.
fn compose_guards<'ctx>(
    ctx: &'ctx z3::Context,
    outer: &Option<Bool<'ctx>>,
    inner: Bool<'ctx>,
) -> Option<Bool<'ctx>> {
    match outer {
        None => Some(inner),
        Some(o) => Some(Bool::and(ctx, &[o, &inner])),
    }
}

/// Resolution result for a (possibly dotted) call name like
/// `recv.subclaim_name`. Three flavors, tried in priority order:
///
///   * `Subschema { recv, type, subclaim }` — `recv` is a body
///     Membership of record type T and `subclaim` is declared as
///     `subclaim … ` inside T. Dispatch rebinds T's fields onto
///     `recv.field` so the subclaim body's bare references resolve
///     to the receiver's leaves.
///
///   * `ReceiverPrefix { claim_name, recv }` — `recv` is anything
///     (an Int, a dotted field) and the SUFFIX is a known claim.
///     The receiver becomes the first positional arg. Fallback when
///     the subschema path doesn't apply.
///
///   * `Plain { claim_name }` — the whole name is a known schema;
///     no receiver involved.
enum CallDispatch {
    Subschema { recv: String, type_name: String, claim_name: String },
    ReceiverPrefix { claim_name: String, recv: String },
    Plain { claim_name: String },
}

/// Walk the current body slice for a Membership matching `name`,
/// return its declared type_name. Used to find a receiver's type
/// when dispatching `recv.subclaim(args)`.
/// True if `e` contains a subexpression that's a method-style
/// subclaim call (`recv.subclaim(args)` resolving to a SubschemaDecl
/// on `recv`'s type). Used to decide whether a `∀` body needs to
/// be AST-expanded into per-iteration body items so each subclaim
/// invocation reaches the inline pass (which has solver access)
/// instead of going through translate_bool (which doesn't).
fn body_contains_subschema_call(
    e: &Expr,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> bool {
    match e {
        Expr::Call(name, _) => matches!(
            resolve_call(name, body_items, schemas),
            Some(CallDispatch::Subschema { .. })),
        Expr::Binary(_, l, r) =>
            body_contains_subschema_call(l, body_items, schemas)
                || body_contains_subschema_call(r, body_items, schemas),
        Expr::Not(x) | Expr::Cardinality(x) =>
            body_contains_subschema_call(x, body_items, schemas),
        Expr::Ternary(c, a, b) =>
            body_contains_subschema_call(c, body_items, schemas)
                || body_contains_subschema_call(a, body_items, schemas)
                || body_contains_subschema_call(b, body_items, schemas),
        Expr::SeqLit(items) | Expr::SetLit(items) | Expr::Tuple(items) =>
            items.iter().any(|x| body_contains_subschema_call(x, body_items, schemas)),
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
            body_contains_subschema_call(r, body_items, schemas)
                || body_contains_subschema_call(b, body_items, schemas),
        Expr::Index(a, b) | Expr::InExpr(a, b) | Expr::Range(a, b) =>
            body_contains_subschema_call(a, body_items, schemas)
                || body_contains_subschema_call(b, body_items, schemas),
        Expr::Field(recv, _) => body_contains_subschema_call(recv, body_items, schemas),
        Expr::Match(scr, arms) =>
            body_contains_subschema_call(scr, body_items, schemas)
                || arms.iter().any(|a| body_contains_subschema_call(&a.body, body_items, schemas)),
        Expr::Matches(x, _) => body_contains_subschema_call(x, body_items, schemas),
        _ => false,
    }
}

/// Recursively replace identifier paths that start with `bound_var`
/// with the per-iteration element expression. Handles bare matches
/// (`p`) and dotted suffixes (`p.color`, `p.aabb.pos.x`).
///
/// `elem_expr` is the expression that the bound variable refers to
/// at this iteration (e.g. `Index(Identifier("platforms"), Int(i))`).
/// A dotted suffix like `p.color` becomes
/// `Field(elem_expr, "color")`; deeper paths chain `Field`s.
fn substitute_bound_var(e: &Expr, bound: &str, elem: &Expr) -> Expr {
    let r = |x: &Expr| Box::new(substitute_bound_var(x, bound, elem));
    let rv = |xs: &Vec<Expr>| xs.iter()
        .map(|x| substitute_bound_var(x, bound, elem))
        .collect();
    match e {
        Expr::Identifier(name) => {
            if name == bound { return elem.clone(); }
            let prefix = format!("{}.", bound);
            if let Some(suffix) = name.strip_prefix(&prefix) {
                // Build Field(Field(... Field(elem, seg1), seg2), …, segN).
                let mut out = elem.clone();
                for seg in suffix.split('.') {
                    out = Expr::Field(Box::new(out), seg.to_string());
                }
                return out;
            }
            e.clone()
        }
        Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => e.clone(),
        Expr::SetLit(xs)  => Expr::SetLit(rv(xs)),
        Expr::SeqLit(xs)  => Expr::SeqLit(rv(xs)),
        Expr::Tuple(xs)   => Expr::Tuple(rv(xs)),
        Expr::Range(a, b) => Expr::Range(r(a), r(b)),
        Expr::InExpr(a, b) => Expr::InExpr(r(a), r(b)),
        Expr::Forall(vars, range, body) => {
            // Inner quantifier shadows the substitution if it rebinds
            // the same name.
            if vars.iter().any(|v| v == bound) {
                Expr::Forall(vars.clone(), r(range), body.clone())
            } else {
                Expr::Forall(vars.clone(), r(range), r(body))
            }
        }
        Expr::Exists(vars, range, body) => {
            if vars.iter().any(|v| v == bound) {
                Expr::Exists(vars.clone(), r(range), body.clone())
            } else {
                Expr::Exists(vars.clone(), r(range), r(body))
            }
        }
        Expr::Call(n, args)    => Expr::Call(n.clone(), rv(args)),
        Expr::Cardinality(x)   => Expr::Cardinality(r(x)),
        Expr::Index(a, b)      => Expr::Index(r(a), r(b)),
        Expr::Field(recv, f)   => Expr::Field(r(recv), f.clone()),
        Expr::Binary(op, a, b) => Expr::Binary(op.clone(), r(a), r(b)),
        Expr::Not(x)           => Expr::Not(r(x)),
        Expr::Ternary(c, a, b) => Expr::Ternary(r(c), r(a), r(b)),
        Expr::Match(scr, arms) => {
            let new_arms: Vec<MatchArm> = arms.iter().map(|arm| MatchArm {
                pattern: arm.pattern.clone(),
                body: Box::new(substitute_bound_var(&arm.body, bound, elem)),
            }).collect();
            Expr::Match(r(scr), new_arms)
        }
        Expr::Matches(x, p) => Expr::Matches(r(x), p.clone()),
    }
}

/// Resolve the per-iteration element exprs for each bound variable
/// in a `∀ … : body`. Returns `Some(Vec<(bound_var, element_expr_for_iter_i)>)`
/// per iteration, OR None if the range shape isn't statically
/// unrollable (length unknown / unsupported range form).
///
/// Supported ranges:
///   * `coindexed(seq1, seq2, …)` with tuple binding `(a, b, …)` —
///     element_i for bound k is `Index(Identifier(seq_k), Int(i))`.
///   * Bare `Identifier(seq_name)` with single binding `a` —
///     element_i is `Index(Identifier(seq_name), Int(i))`.
fn resolve_forall_unroll(
    vars: &[String],
    range: &Expr,
    env: &HashMap<String, Var<'static>>,
) -> Option<Vec<Vec<(String, Expr)>>> {
    // coindexed(seq1, …) — tuple binding.
    if let Expr::Call(name, args) = range {
        if name == "coindexed" && args.len() == vars.len() && !args.is_empty() {
            // Collect each seq's pinned length.
            let mut seq_names: Vec<String> = Vec::with_capacity(args.len());
            let mut lens: Vec<i64> = Vec::with_capacity(args.len());
            for arg in args {
                let Expr::Identifier(seq_name) = arg else { return None };
                let var = env.get(seq_name)?;
                let len = if let Some((_, len, _)) = var.as_seq() {
                    len.simplify().as_i64()?
                } else if let Some((_, len, _, _, _)) = var.as_datatype_seq() {
                    len.simplify().as_i64()?
                } else {
                    return None;
                };
                seq_names.push(seq_name.clone());
                lens.push(len);
            }
            let n = *lens.iter().min()?;
            let mut iters: Vec<Vec<(String, Expr)>> = Vec::with_capacity(n as usize);
            for i in 0..n {
                let mut binds: Vec<(String, Expr)> = Vec::with_capacity(vars.len());
                for (v, seq) in vars.iter().zip(seq_names.iter()) {
                    let elem = Expr::Index(
                        Box::new(Expr::Identifier(seq.clone())),
                        Box::new(Expr::Int(i)),
                    );
                    binds.push((v.clone(), elem));
                }
                iters.push(binds);
            }
            return Some(iters);
        }
    }
    // Bare Identifier(seq_name) — single-name binding.
    if let Expr::Identifier(seq_name) = range {
        if vars.len() != 1 { return None; }
        let var = env.get(seq_name)?;
        let n = if let Some((_, len, _)) = var.as_seq() {
            len.simplify().as_i64()?
        } else if let Some((_, len, _, _, _)) = var.as_datatype_seq() {
            len.simplify().as_i64()?
        } else {
            return None;
        };
        let v = &vars[0];
        let iters: Vec<Vec<(String, Expr)>> = (0..n).map(|i| {
            let elem = Expr::Index(
                Box::new(Expr::Identifier(seq_name.clone())),
                Box::new(Expr::Int(i)),
            );
            vec![(v.clone(), elem)]
        }).collect();
        return Some(iters);
    }
    None
}

fn find_membership_type(items: &[BodyItem], name: &str) -> Option<String> {
    for item in items {
        if let BodyItem::Membership { name: n, type_name, .. } = item {
            if n == name { return Some(type_name.clone()); }
        }
    }
    None
}

/// Walk a type's body for a SubclaimDecl matching `name`.
fn type_has_subclaim(type_decl: &SchemaDecl, name: &str) -> bool {
    type_decl.body.iter().any(|item| matches!(item,
        BodyItem::SubclaimDecl(s) if s.name == name))
}

/// Resolve a call name with full receiver awareness. `body_items`
/// is the surrounding body slice (used to look up the receiver's
/// declared type).
fn resolve_call(
    name: &str,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<CallDispatch> {
    // No dots → plain claim invocation.
    if !name.contains('.') {
        if schemas.contains_key(name) {
            return Some(CallDispatch::Plain { claim_name: name.to_string() });
        }
        return None;
    }
    let (prefix, suffix) = name.rsplit_once('.')?;
    // (1) Subschema path: prefix is a bare body var of a record type
    //     T, and T has a SubclaimDecl `suffix`. This is the
    //     "use a field of a schema as a subschema" form.
    if !prefix.contains('.') {
        if let Some(type_name) = find_membership_type(body_items, prefix) {
            if let Some(type_decl) = schemas.get(&type_name) {
                if type_has_subclaim(type_decl, suffix) {
                    return Some(CallDispatch::Subschema {
                        recv: prefix.to_string(),
                        type_name,
                        claim_name: suffix.to_string(),
                    });
                }
            }
        }
    }
    // (2) Receiver-prefix fallback: suffix is a known claim and the
    //     prefix gets prepended as the first positional arg. Works
    //     even with multi-segment prefixes (`win.renderer.foo`).
    if schemas.contains_key(suffix) {
        return Some(CallDispatch::ReceiverPrefix {
            claim_name: suffix.to_string(),
            recv: prefix.to_string(),
        });
    }
    None
}

/// Resolve for the `(args) ∈ rhs` form where rhs is an Identifier.
fn resolve_call_name(
    rhs: &Expr,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<CallDispatch> {
    let Expr::Identifier(n) = rhs else { return None; };
    resolve_call(n, body_items, schemas)
}

/// Back-compat wrappers — the existing Plain / ReceiverPrefix
/// dispatch arms below want the old `(claim_name, Option<recv>)`
/// shape. These collapse Subschema cases out so those arms only
/// see Plain / ReceiverPrefix (the Subschema arm above catches
/// the rest first).
fn method_dispatch_call_compat(
    name: &str,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<(String, Option<String>)> {
    match resolve_call(name, body_items, schemas)? {
        CallDispatch::Plain { claim_name } => Some((claim_name, None)),
        CallDispatch::ReceiverPrefix { claim_name, recv } => Some((claim_name, Some(recv))),
        CallDispatch::Subschema { .. } => None,  // handled by dedicated arm
    }
}

fn method_dispatch_name_compat(
    rhs: &Expr,
    body_items: &[BodyItem],
    schemas: &HashMap<String, SchemaDecl>,
) -> Option<(String, Option<String>)> {
    let Expr::Identifier(n) = rhs else { return None; };
    method_dispatch_call_compat(n, body_items, schemas)
}

/// Inline a subclaim-of-type invocation `recv.subclaim(args)`.
///
/// The "use a field of a schema as a subschema" form: the
/// receiver's record fields get rebound onto T's bare-name
/// fields so the subclaim body's references resolve to the
/// receiver's leaves. Caller has already confirmed that `recv`
/// is a body Membership of `type_name` and that `claim_name`
/// is a subclaim inside `type_name`'s body.
#[allow(clippy::too_many_arguments)]
fn inline_subschema_call(
    recv: &str,
    type_name: &str,
    claim_name: &str,
    args: &[Expr],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    guard: &Option<Bool<'static>>,
    tracker: Option<&Bool<'static>>,
) {
    // Look up the subclaim inside the type's body.
    let Some(type_decl) = schemas.get(type_name) else { return; };
    let mut subclaim: Option<&SchemaDecl> = None;
    for item in &type_decl.body {
        if let BodyItem::SubclaimDecl(s) = item {
            if s.name == claim_name { subclaim = Some(s); break; }
        }
    }
    let Some(subclaim) = subclaim else { return; };

    // Recursion guard via the standard visited map.
    let qualified = format!("{}.{}", type_name, claim_name);
    let Some(_depth) = try_enter(visited, &qualified) else { return; };

    // Build inner env starting from outer. The Z3 vars themselves
    // are shared (same constants); we just add bare-name aliases
    // for each parent-type field so the subclaim's body sees
    // `renderer` and resolves to `recv.renderer`.
    let mut inner = env.clone();
    // Parent-type fields = top-level Memberships inside the type's
    // body (NOT subclaim decls or constraints). For each leaf key
    // in env starting with `recv.`, mirror it without the prefix.
    let prefix = format!("{recv}.");
    let outer_keys: Vec<(String, String)> = env.keys()
        .filter_map(|k| k.strip_prefix(&prefix).map(|rest|
            (k.clone(), rest.to_string())))
        .collect();
    for (full_key, bare) in &outer_keys {
        if let Some(v) = env.get(full_key) {
            inner.insert(bare.clone(), v.clone());
        }
    }

    // Slot info: the subclaim's first-line params (its leading
    // body Memberships up to `args.len()` if needed). Subclaims
    // don't have first-line params today, so this is just the
    // leading body Memberships.
    let slot_info: Vec<(String, String)> = subclaim.body.iter()
        .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
            Some((name.clone(), type_name.clone()))
        } else { None })
        .take(args.len())
        .collect();
    if slot_info.len() != args.len() {
        eprintln!(
            "warning: subschema call `{}.{}` got {} args but the \
             subclaim has only {} param Memberships",
            recv, claim_name, args.len(), slot_info.len()
        );
        exit_frame(visited, &qualified);
        return;
    }
    // Tuple-as-record-literal coercion (same as positional Call arm).
    let mappings: Vec<crate::ast::Mapping> = slot_info.iter()
        .zip(args.iter())
        .map(|((slot, slot_type), value)| {
            let coerced = match value {
                Expr::Tuple(items) if schemas.contains_key(slot_type) =>
                    Expr::Call(slot_type.clone(), items.clone()),
                _ => value.clone(),
            };
            crate::ast::Mapping { slot: slot.clone(), value: coerced }
        })
        .collect();

    isolate_helper_locals(&subclaim.body, &mut inner, subclaim.param_count);
    let slot_set: std::collections::HashSet<String> =
        mappings.iter().map(|m| m.slot.clone()).collect();
    for m in &mappings {
        let bound = resolve_mapping(&m.slot, &m.value, ctx, env, schemas);
        if bound.is_empty() {
            eprintln!("warning: subschema arg didn't resolve: {:?}", m.value);
        }
        for (k, v) in bound { inner.insert(k, v); }
    }
    let call_id = next_call_id();
    for sub in &subclaim.body {
        if let BodyItem::Membership { name: vname, type_name: vty, .. } = sub {
            if slot_set.contains(vname) { continue; }
            if inner.contains_key(vname) { continue; }
            let z3_name = format!("{}__{}__call{}", claim_name, vname, call_id);
            let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                              vty, schemas, Some(registry), enums);
            for c in &post { track_assert(solver, c, tracker); }
        }
    }
    inline_body_items_guarded(
        &subclaim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, tracker,
    );
    exit_frame(visited, &qualified);
}

pub(super) fn inline_body_items(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
) {
    inline_body_items_guarded(items, env, solver, schemas, ctx, registry, enums, visited, &None, None)
}

/// Translate `items` and assert each derived constraint into the
/// solver, additionally tagging every assertion with one of `trackers`
/// so a later `solver.get_unsat_core()` can name the offending
/// top-level body item. `trackers[i]` corresponds to `items[i]` —
/// passing fewer trackers than items means tail items go untracked.
/// Used by `evaluate_with_core` to surface unsat-cores back to the
/// test runner; the regular `evaluate` path passes `None` for
/// zero overhead.
pub(super) fn inline_body_items_tracked(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    trackers: &[Bool<'static>],
) {
    for (idx, item) in items.iter().enumerate() {
        let tracker = trackers.get(idx);
        let slice = std::slice::from_ref(item);
        inline_body_items_guarded(
            slice, env, solver, schemas, ctx, registry, enums, visited, &None, tracker,
        );
    }
}

fn inline_body_items_guarded(
    items: &[BodyItem],
    env: &mut HashMap<String, Var<'static>>,
    solver: &Solver<'static>,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    visited: &mut HashMap<String, usize>,
    guard: &Option<Bool<'static>>,
    tracker: Option<&Bool<'static>>,
) {
    for item in items {
        match item {
            BodyItem::Membership { name, type_name, pins } => {
                // Top-level Memberships are pre-declared by pass 1, so the
                // declare_var call is a no-op there. Useful when the helper
                // recurses into a passthrough's body that introduces
                // variables not yet in env (e.g. a nested claim's locals).
                if !env.contains_key(name) {
                    let post = declare_var(ctx, env, name, type_name, schemas, Some(registry), enums);
                    for c in &post { track_assert(solver, c, tracker); }
                }
                // Resolve `pins` to a list of (field-name, value-expr)
                // pairs. Named is direct; Positional looks up the type's
                // body Membership order to map positions to field names.
                let resolved_pins: Vec<(String, Expr)> = match pins {
                    crate::ast::Pins::None => Vec::new(),
                    crate::ast::Pins::Named(maps) => maps.iter()
                        .map(|m| (m.slot.clone(), m.value.clone())).collect(),
                    crate::ast::Pins::Positional(args) => {
                        // Look up the type's field order from its
                        // SchemaDecl. Strict count match required.
                        let Some(schema) = schemas.get(type_name) else {
                            eprintln!(
                                "error: positional pin on unknown type `{}`",
                                type_name
                            );
                            std::process::exit(1);
                        };
                        let field_order: Vec<String> = schema.body.iter()
                            .filter_map(|item| match item {
                                BodyItem::Membership { name, .. } => Some(name.clone()),
                                _ => None,
                            })
                            .collect();
                        // Partial allowed: too few args pin the leading
                        // fields and leave the rest free. Too many is
                        // a real error — the user is asking to pin
                        // fields that don't exist.
                        if args.len() > field_order.len() {
                            eprintln!(
                                "error: too many positional pins on `{}`: \
                                 type declares {} fields, got {} args",
                                type_name, field_order.len(), args.len()
                            );
                            std::process::exit(1);
                        }
                        field_order.into_iter()
                            .zip(args.iter().cloned())
                            .collect()
                    }
                };
                // Fire each pin as `name.field = value`. Same machinery
                // and same dropped-constraint policy as a regular
                // Constraint — a pin to a non-existent field is the
                // same kind of silent error as a generic dropped
                // translation, so it shares the hard-fail behavior.
                for (slot, value) in resolved_pins {
                    let lhs = Expr::Identifier(format!("{}.{}", name, slot));
                    let eq = Expr::Binary(
                        crate::ast::BinOp::Eq,
                        Box::new(lhs),
                        Box::new(value.clone()),
                    );
                    if let Some(b) = translate_bool(&eq, ctx, env, schemas) {
                        track_assert(solver, &guarded_bool(b, guard), tracker);
                    } else {
                        let lenient = std::env::var("EVIDENT_LENIENT")
                            .map(|v| !v.is_empty() && v != "0")
                            .unwrap_or(false);
                        let pretty = pretty::expr(&eq);
                        if lenient {
                            eprintln!(
                                "warning: type-use pin didn't translate: {}",
                                pretty
                            );
                        } else {
                            eprintln!(
                                "error: type-use pin didn't translate: {}",
                                pretty
                            );
                            eprintln!();
                            eprintln!(
                                "The field `{}` probably doesn't exist on type `{}`,",
                                slot, type_name
                            );
                            eprintln!(
                                "or its type doesn't accept the pinned value's shape."
                            );
                            eprintln!(
                                "Set EVIDENT_LENIENT=1 to demote this to a warning."
                            );
                            std::process::exit(1);
                        }
                    }
                }

                // Inherit the type's body Constraints onto this instance.
                // For each `Constraint(e)` in the type's body, rewrite any
                // identifier whose leading dotted segment names one of the
                // type's own fields by prefixing `name.`. Skip if the type
                // is not a user-defined schema (built-ins like Int / Nat /
                // Seq(...) etc. — they have no body to inherit).
                //
                // This is what makes `mario ∈ MarioSprite (pos ↦ p)` mean
                // "mario satisfies MarioSprite's invariants" rather than
                // "mario has MarioSprite's leaf fields but no constraints
                // between them." Without it, body equalities like
                // `hat = Rect(Color(220, …), pos, …)` in the type body
                // produce no constraint on `mario.hat`, and the instance
                // ends up free.
                if let Some(type_schema) = schemas.get(type_name) {
                    let field_set: std::collections::HashSet<String> = type_schema
                        .body
                        .iter()
                        .filter_map(|item| match item {
                            BodyItem::Membership { name: n, .. } => Some(n.clone()),
                            _ => None,
                        })
                        .collect();
                    for item in &type_schema.body {
                        if let BodyItem::Constraint(e) = item {
                            let rewritten = rewrite_idents_with_prefix(e, name, &field_set);
                            if let Some(b) = translate_bool(&rewritten, ctx, env, schemas) {
                                track_assert(solver, &guarded_bool(b, guard), tracker);
                            }
                            // Silently skip on translation failure — the
                            // type body might contain shapes that only
                            // apply when used with a passthrough (e.g.,
                            // bare claim names that match-by-name). The
                            // hard-fail policy stays on direct body items
                            // of the calling schema.
                        }
                    }
                }

                // Element-level invariant inheritance for `Seq(SomeType)`:
                // when SomeType has body Constraints (e.g. `#effs = 2`),
                // emit per-element substituted versions over the Seq's
                // pinned indices. Without this, a user `plat_effs ∈
                // Seq(EffectPair)` declaration wouldn't auto-pin each
                // bundle's inner length — the user would have to write
                // `∀ i ∈ {0..3} : #plat_effs[i].effs = 2` by hand.
                //
                // The substitution treats each Seq element as a record
                // value reached by `Index(Identifier(name), Int(i))`,
                // and the type's bare field references become
                // `Field(Index(name, i), field_name)` per iteration.
                if let Some(inner) = type_name.strip_prefix("Seq(")
                    .and_then(|s| s.strip_suffix(')'))
                {
                    if let Some(inner_schema) = schemas.get(inner) {
                        let len_opt = env.get(name).and_then(|v| {
                            if let Some((_, len, _, _, _)) = v.as_datatype_seq() {
                                len.simplify().as_i64()
                            } else if let Some((_, len, _)) = v.as_seq() {
                                len.simplify().as_i64()
                            } else { None }
                        });
                        if let Some(n) = len_opt {
                            let field_set: std::collections::HashSet<String> =
                                inner_schema.body.iter()
                                    .filter_map(|item| match item {
                                        BodyItem::Membership { name: n, .. } => Some(n.clone()),
                                        _ => None,
                                    })
                                    .collect();
                            for i in 0..n {
                                for item in &inner_schema.body {
                                    if let BodyItem::Constraint(e) = item {
                                        // Build elem_expr = Index(Identifier(name), Int(i)).
                                        // For each of the inner type's field
                                        // names, substitute bare refs to
                                        // `Field(elem_expr, field_name)`.
                                        let mut substituted = e.clone();
                                        for fname in &field_set {
                                            let elem = Expr::Field(
                                                Box::new(Expr::Index(
                                                    Box::new(Expr::Identifier(name.clone())),
                                                    Box::new(Expr::Int(i)),
                                                )),
                                                fname.clone(),
                                            );
                                            substituted = substitute_bound_var(
                                                &substituted, fname, &elem);
                                        }
                                        if let Some(b) = translate_bool(
                                            &substituted, ctx, env, schemas)
                                        {
                                            track_assert(solver, &guarded_bool(b, guard), tracker);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Bare-identifier-as-passthrough handling moved to the
            // self-hosted desugar pass (`stdlib/passes/desugar_passthrough.ev`
            // + `commands/desugar.rs::auto_apply_desugar`). By the time
            // any constraint arrives here, the rewrite has already turned
            // `Constraint(Identifier(name))` (when `name` is a known
            // claim) into `Passthrough(name)`. Bare-identifier
            // constraints whose name does NOT match a claim fall through
            // to the regular Constraint arm below, same as before.

            // Positional claim invocation: `Constraint(Call(name, args))`
            // whose `name` matches a known claim is treated as a
            // ClaimCall with positional args bound by index to the
            // claim's first-line params (or first N Memberships in
            // declaration order). Encourages the
            //   claim Foo(items ∈ Seq, keys ∈ Seq, asc ∈ Bool)
            //       …body using items/keys/asc…
            //   ⋮
            //   Foo(my_items, my_keys, true)
            // pattern over the longer mapsto form.
            // Tuple-in-claim invocation: `(a, b, c) ∈ claim_name` is
            // the relational reading of a positional claim call —
            // "this tuple is in the set of satisfying assignments."
            // Desugars to the same positional ClaimCall path as
            // `claim_name(a, b, c)`. The dispatch is here (not in
            // exprs.rs's InExpr handler) because it has to fire at
            // BodyItem position, not as a value-producing
            // expression — a claim invocation contributes constraints,
            // not a Bool.
            // Subschema dispatch (priority over receiver-prefix):
            // `(args) ∈ recv.subclaim_name` where `recv` is a body
            // Membership of a record type T AND `subclaim_name` is
            // declared as `subclaim …` inside T. The receiver's
            // fields get rebound onto T's bare-name fields so the
            // subclaim body's references (`renderer`, etc.) resolve
            // to the leaves of `recv`.
            BodyItem::Constraint(Expr::InExpr(lhs, rhs))
                if matches!(resolve_call_name(rhs.as_ref(), items, schemas),
                    Some(CallDispatch::Subschema { .. }))
                && matches!(lhs.as_ref(), Expr::Tuple(_)) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let Some(CallDispatch::Subschema { recv, type_name, claim_name }) =
                    resolve_call_name(rhs.as_ref(), items, schemas) else { unreachable!() };
                let args: Vec<Expr> = match lhs.as_ref() {
                    Expr::Tuple(items) => items.clone(),
                    _ => unreachable!(),
                };
                inline_subschema_call(
                    &recv, &type_name, &claim_name, &args,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            BodyItem::Constraint(Expr::InExpr(lhs, rhs))
                if method_dispatch_name_compat(rhs.as_ref(), items, schemas).is_some()
                && matches!(lhs.as_ref(), Expr::Tuple(_)) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let (name, receiver) = method_dispatch_name_compat(rhs.as_ref(), items, schemas)
                    .expect("guarded above");
                let mut args: Vec<Expr> = match lhs.as_ref() {
                    Expr::Tuple(items) => items.clone(),
                    _ => unreachable!(),
                };
                // Method-style: `(args) ∈ recv.claim_name` prepends
                // `Identifier(recv)` as the first positional arg.
                if let Some(recv) = receiver {
                    args.insert(0, Expr::Identifier(recv));
                }
                let Some(depth) = try_enter(visited, &name) else { continue };
                let Some(claim) = schemas.get(&name) else {
                    exit_frame(visited, &name); continue
                };
                let slot_info: Vec<(String, String)> = claim.body.iter()
                    .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
                        Some((name.clone(), type_name.clone()))
                    } else { None })
                    .take(args.len())
                    .collect();
                if slot_info.len() != args.len() {
                    eprintln!(
                        "warning: tuple-in-claim `(...) ∈ {}` got {} args but \
                         the claim has only {} param Memberships",
                        name, args.len(), slot_info.len()
                    );
                    exit_frame(visited, &name);
                    continue;
                }
                // Tuple-as-record-literal coercion (same rule as the
                // positional-Call arm above) for nested `(a, b, c)`
                // args whose slot is a known record type.
                let mappings: Vec<crate::ast::Mapping> = slot_info.iter()
                    .zip(args.iter())
                    .map(|((slot, slot_type), value)| {
                        let coerced = match value {
                            Expr::Tuple(items) if schemas.contains_key(slot_type) =>
                                Expr::Call(slot_type.clone(), items.clone()),
                            _ => value.clone(),
                        };
                        crate::ast::Mapping { slot: slot.clone(), value: coerced }
                    })
                    .collect();
                let _ = depth;
                let mut inner = env.clone();
                isolate_helper_locals(&claim.body, &mut inner, claim.param_count);
                let slot_set: std::collections::HashSet<String> =
                    mappings.iter().map(|m| m.slot.clone()).collect();
                for m in &mappings {
                    let bound = resolve_mapping(&m.slot, &m.value, ctx, env, schemas);
                    if bound.is_empty() {
                        eprintln!("warning: tuple-in-claim arg didn't resolve: {:?}", m.value);
                    }
                    for (k, v) in bound {
                        inner.insert(k, v);
                    }
                }
                let call_id = next_call_id();
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name, .. } = sub {
                        if slot_set.contains(vname) { continue; }
                        if inner.contains_key(vname) { continue; }
                        let z3_name = format!("{}__{}__call{}", name, vname, call_id);
                        let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                                          type_name, schemas, Some(registry), enums);
                        for c in &post { track_assert(solver, c, tracker); }
                    }
                }
                inline_body_items_guarded(
                    &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, tracker
                );
                exit_frame(visited, &name);
            }
            // Subschema dispatch (priority): `recv.subclaim_name(args)`
            // where `recv` is a body Membership of a record type T
            // AND `subclaim_name` is a SubclaimDecl inside T's body.
            // Field-rebinds T's leaves onto bare names so the
            // subclaim body's references (`renderer`, …) resolve
            // to the receiver's leaves.
            BodyItem::Constraint(Expr::Call(name, args))
                if matches!(resolve_call(name, items, schemas),
                    Some(CallDispatch::Subschema { .. })) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let Some(CallDispatch::Subschema { recv, type_name, claim_name }) =
                    resolve_call(name, items, schemas) else { unreachable!() };
                inline_subschema_call(
                    &recv, &type_name, &claim_name, args,
                    env, solver, schemas, ctx, registry, enums, visited, guard, tracker,
                );
            }
            BodyItem::Constraint(Expr::Call(name, args))
                if method_dispatch_call_compat(name, items, schemas).is_some() =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let (claim_name, receiver) = method_dispatch_call_compat(name, items, schemas)
                    .expect("guarded above");
                // Method-style: `recv.claim_name(args)` prepends
                // `Identifier(recv)` to the positional args.
                let mut owned_args: Vec<Expr> = args.clone();
                if let Some(recv) = receiver {
                    owned_args.insert(0, Expr::Identifier(recv));
                }
                let args = &owned_args;
                let name = &claim_name;
                let Some(depth) = try_enter(visited, name) else { continue };
                let Some(claim) = schemas.get(name) else { exit_frame(visited, name); continue };

                // Pair positional args with the claim's first N Membership
                // body items (which include first-line params, since
                // those desugar to Memberships at the head of the body).
                let slot_info: Vec<(String, String)> = claim.body.iter()
                    .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
                        Some((name.clone(), type_name.clone()))
                    } else { None })
                    .take(args.len())
                    .collect();
                if slot_info.len() != args.len() {
                    eprintln!(
                        "warning: positional ClaimCall to `{}` got {} args but \
                         the claim has only {} param Memberships",
                        name, args.len(), slot_info.len()
                    );
                    exit_frame(visited, name);
                    continue;
                }
                // Tuple-as-record-literal coercion: when an arg is a
                // bare `(a, b, c)` Tuple AND the slot's type names a
                // known record schema, rewrite to `Call(type, items)`
                // — the existing record-literal-in-expression-position
                // path. Lets the user write
                //   set_draw_color((220, 40, 40, 255), out)
                // instead of `Color(220, 40, 40, 255)`.
                let mappings: Vec<crate::ast::Mapping> = slot_info.iter()
                    .zip(args.iter())
                    .map(|((slot, slot_type), value)| {
                        let coerced = match value {
                            Expr::Tuple(items) if schemas.contains_key(slot_type) =>
                                Expr::Call(slot_type.clone(), items.clone()),
                            _ => value.clone(),
                        };
                        crate::ast::Mapping { slot: slot.clone(), value: coerced }
                    })
                    .collect();

                // Pre-isolate: the called claim's body Memberships
                // are LOCAL to this invocation. Any caller-scope vars
                // sharing those names are removed from `inner` so the
                // body either gets the slot-mapped value (if the name
                // is a slot param) or a per-call fresh declaration.
                // Without this, recursive helpers (transpiler-style)
                // that share body-Membership names with the caller's
                // scope collapse Z3 consts across invocations and go
                // UNSAT for inputs that disagree on the shared field.
                let _ = depth;
                let mut inner = env.clone();
                // Slot params (the first param_count body items) are
                // explicit inputs — caller's value goes here. Body
                // Memberships past that index are helper-locals; clear
                // any same-named entries inherited from the caller's
                // scope so they don't collide across nested helper
                // invocations.
                isolate_helper_locals(&claim.body, &mut inner, claim.param_count);
                let slot_set: std::collections::HashSet<String> =
                    mappings.iter().map(|m| m.slot.clone()).collect();
                for m in &mappings {
                    let bound = resolve_mapping(&m.slot, &m.value, ctx, env, schemas);
                    if bound.is_empty() {
                        eprintln!("warning: positional arg didn't resolve: {:?}", m.value);
                    }
                    for (k, v) in bound {
                        inner.insert(k, v);
                    }
                }
                let call_id = next_call_id();
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name, .. } = sub {
                        if slot_set.contains(vname) { continue; }
                        if inner.contains_key(vname) { continue; }
                        let z3_name = format!("{}__{}__call{}", name, vname, call_id);
                        let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                                          type_name, schemas, Some(registry), enums);
                        for c in &post { track_assert(solver, c, tracker); }
                    }
                }
                inline_body_items_guarded(
                    &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, tracker
                );
                exit_frame(visited, name);
            }
            // Guarded claim invocation: `cond ⇒ ClaimName` inlines the
            // claim's body but wraps each constraint in `cond ⇒ …`.
            // Declarations from the claim fire unconditionally; the
            // guard only narrows what the constraints assert. Composes
            // with an outer guard if we're already inside one.
            BodyItem::Constraint(Expr::Binary(crate::ast::BinOp::Implies, ant, cons))
                if matches!(cons.as_ref(),
                    Expr::Identifier(n) if schemas.contains_key(n)) =>
            {
                let claim_name = match cons.as_ref() {
                    Expr::Identifier(n) => n,
                    _ => unreachable!(),
                };
                let Some(ant_bool) = translate_bool(ant, ctx, env, schemas) else {
                    continue;
                };
                let new_guard = compose_guards(ctx, guard, ant_bool);
                if !guard_is_satisfiable(solver, &new_guard) { continue; }
                if try_enter(visited, claim_name).is_none() { continue; }
                let Some(claim) = schemas.get(claim_name) else {
                    exit_frame(visited, claim_name); continue
                };
                // Names-match invocation: clone env, isolate names
                // that clash with the helper's body Memberships,
                // declare those fresh per-call. Each sibling/nested
                // invocation thus gets its own Z3 consts for body
                // locals, rather than collapsing to the caller's
                // same-named entries (see comment in the positional
                // ClaimCall arm above for the recursive transpiler
                // case that drove this design).
                //
                // Note we DON'T cherry-pick "input slot" names from
                // outer — body Memberships that reference outer
                // values via names-match already exist in `env` (the
                // caller's scope), and the constraints they generate
                // assert against the FRESH per-call consts. Those
                // constraints then form an equivalence (the helper
                // sets `out = ...` where `out` is the fresh local;
                // upstream callers explicitly pass their `out` via
                // the surrounding positional ClaimCall's slot
                // mapping, which equates them through the assertion
                // chain). Sharing the Z3 const directly is an
                // optimization that breaks recursion, so we drop it.
                let mut inner = env.clone();
                isolate_helper_locals(&claim.body, &mut inner, claim.param_count);
                let call_id = next_call_id();
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name, .. } = sub {
                        if inner.contains_key(vname) { continue; }
                        let z3_name = format!("{}__{}__call{}", claim_name, vname, call_id);
                        let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                                          type_name, schemas, Some(registry), enums);
                        for c in &post { track_assert(solver, c, tracker); }
                    }
                }
                inline_body_items_guarded(
                    &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, &new_guard, tracker
                );
                exit_frame(visited, claim_name);
            }
            // `∀ vars ∈ range : body` where `body` contains a method-
            // style subclaim invocation (`recv.subclaim(args)`).
            //
            // translate_bool's ∀ translator runs the body through
            // translate_bool, which has no solver access and can't
            // fire the subclaim's per-iteration assertions (the
            // `out = ⟨…⟩` pin inside the subclaim body never lands,
            // leaving outputs free; see COUNTEREXAMPLES #26).
            //
            // Fix: expand the ∀ at AST level for known-length ranges
            // (coindexed of pinned-length Seqs, or a bare pinned Seq).
            // Each iteration becomes a regular BodyItem the inline
            // pass can dispatch — subclaim calls get full solver
            // access via inline_subschema_call as usual.
            BodyItem::Constraint(Expr::Forall(vars, range, body))
                if body_contains_subschema_call(body, items, schemas) =>
            {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let Some(iterations) =
                    resolve_forall_unroll(vars, range, env)
                else {
                    // Range shape not statically unrollable —
                    // fall through to the regular Constraint
                    // translation path. The subclaim assertions
                    // will drop, but the user gets the same
                    // behavior as before this fix.
                    let e = Expr::Forall(
                        vars.clone(), range.clone(), body.clone());
                    if let Some(b) = translate_bool(&e, ctx, env, schemas) {
                        track_assert(solver, &guarded_bool(b, guard), tracker);
                    }
                    continue;
                };
                for binds in iterations {
                    let mut item_body: Expr = (**body).clone();
                    for (bound, elem) in &binds {
                        item_body = substitute_bound_var(&item_body, bound, elem);
                    }
                    // Append the substituted body as the next item of
                    // the OUTER `items` slice and dispatch through
                    // inline_body_items_guarded again. Keeping the
                    // outer items in scope is important: `resolve_call`
                    // walks `items` to find the receiver's type (e.g.
                    // `win ∈ SDL_Window`) for method-style subclaim
                    // dispatch. Passing only the substituted item
                    // would lose those Memberships.
                    let item = BodyItem::Constraint(item_body);
                    let mut expanded = items.to_vec();
                    expanded.push(item);
                    let single_slice = &expanded[expanded.len() - 1 ..];
                    // ↑ a slice of just the new item, but its
                    // `items` siblings (passed via the outer `items`
                    // arg below) include all of the surrounding body.
                    let _ = single_slice;
                    // We can't easily slice the merged vec without
                    // hitting borrow-checker issues; instead, dispatch
                    // the single item directly here without recursion.
                    if let BodyItem::Constraint(ref e) = expanded[expanded.len() - 1] {
                        if let Expr::Call(name, args) = e {
                            if let Some(CallDispatch::Subschema { recv, type_name, claim_name }) =
                                resolve_call(name, items, schemas)
                            {
                                inline_subschema_call(
                                    &recv, &type_name, &claim_name, args,
                                    env, solver, schemas, ctx, registry,
                                    enums, visited, guard, tracker,
                                );
                                continue;
                            }
                        }
                    }
                    // Fall back: regular Constraint translation.
                    if let BodyItem::Constraint(e) = &expanded[expanded.len() - 1] {
                        if let Some(b) = translate_bool(e, ctx, env, schemas) {
                            track_assert(solver, &guarded_bool(b, guard), tracker);
                        }
                    }
                }
            }
            BodyItem::Constraint(e) => {
                // Recognized runtime markers (declared in
                // `crate::ast::BODY_MARKERS`) are bare identifiers
                // that carry metadata for some other runtime layer
                // — they have no Bool translation. Skip silently
                // so they don't trip the dropped-constraint diagnostic.
                if let crate::ast::Expr::Identifier(s) = e {
                    if crate::ast::BODY_MARKERS.contains(&s.as_str()) { continue; }
                }
                if let Some(b) = translate_bool(e, ctx, env, schemas) {
                    track_assert(solver, &guarded_bool(b, guard), tracker);
                } else {
                    let lenient = std::env::var("EVIDENT_LENIENT")
                        .map(|v| !v.is_empty() && v != "0")
                        .unwrap_or(false);
                    let pretty = pretty::expr(e);
                    if lenient {
                        eprintln!("warning: dropped constraint (couldn't translate to Bool): {pretty}");
                    } else {
                        eprintln!("error: dropped constraint (couldn't translate to Bool):");
                        eprintln!("       {pretty}");
                        eprintln!();
                        eprintln!("This constraint can't be expressed as a Z3 Bool with the");
                        eprintln!("current translator — almost certainly a translator gap.");
                        eprintln!("Either rewrite the constraint to a supported shape, or");
                        eprintln!("set EVIDENT_LENIENT=1 to demote this to a warning.");
                        std::process::exit(1);
                    }
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if !guard_is_satisfiable(solver, guard) { continue; }
                if try_enter(visited, claim_name).is_none() { continue; }
                let Some(claim) = schemas.get(claim_name) else {
                    eprintln!("warning: ..{} references unknown claim", claim_name);
                    exit_frame(visited, claim_name);
                    continue;
                };
                inline_body_items_guarded(
                    &claim.body, env, solver, schemas, ctx, registry, enums, visited, guard, tracker
                );
                exit_frame(visited, claim_name);
            }
            BodyItem::ClaimCall { name, mappings } => {
                if !guard_is_satisfiable(solver, guard) { continue; }
                let Some(depth) = try_enter(visited, name) else { continue };
                let Some(claim) = schemas.get(name) else {
                    eprintln!("warning: ClaimCall to unknown claim {}", name);
                    exit_frame(visited, name);
                    continue;
                };
                let mut inner = env.clone();
                let slot_set: std::collections::HashSet<String> =
                    mappings.iter().map(|m| m.slot.clone()).collect();
                for m in mappings {
                    let bound = resolve_mapping(&m.slot, &m.value, ctx, env, schemas);
                    if bound.is_empty() {
                        eprintln!("warning: mapping value didn't resolve: {:?}", m.value);
                    }
                    for (k, v) in bound {
                        inner.insert(k, v);
                    }
                }
                // Declare any of the claim's own variables that weren't
                // bound by a mapping (the claim's "internal" parameters,
                // like AxisPhysics's `intended` / `target`). Each
                // invocation gets a per-call suffix on the Z3 name so
                // two invocations of the same claim get distinct Z3
                // constants — without this, both AxisPhysics calls in
                // PlayerPhysics share one `intended` Z3 var and the
                // x-axis vs. y-axis branches contradict → UNSAT.
                let call_id = next_call_id();
                for sub in &claim.body {
                    if let BodyItem::Membership { name: vname, type_name, .. } = sub {
                        let slot_prefix = format!("{}.", vname);
                        let already_bound = inner.contains_key(vname)
                            || inner.keys().any(|k| k.starts_with(&slot_prefix));
                        // See positional-call arm: recursive frames
                        // shadow body-internal Memberships so each
                        // invocation gets a fresh Z3 const.
                        let force_fresh = depth > 1 && !slot_set.contains(vname);
                        if force_fresh {
                            // declare_var_named's idempotence guard
                            // would skip the re-declaration; pop the
                            // inherited entry first so the fresh decl
                            // sticks.
                            inner.remove(vname);
                            let dotted: Vec<String> = inner.keys()
                                .filter(|k| k.starts_with(&slot_prefix))
                                .cloned().collect();
                            for k in dotted { inner.remove(&k); }
                        }
                        if !already_bound || force_fresh {
                            let z3_name = format!("{}__{}__call{}", name, vname, call_id);
                            let post = declare_var_named(ctx, &mut inner, vname, &z3_name,
                                              type_name, schemas, Some(registry), enums);
                            for c in &post { track_assert(solver, c, tracker); }
                        }
                    }
                }
                inline_body_items_guarded(
                    &claim.body, &mut inner, solver, schemas, ctx, registry, enums, visited, guard, tracker
                );
                exit_frame(visited, name);
            }
            BodyItem::SubclaimDecl(_) => {}
        }
    }
}
