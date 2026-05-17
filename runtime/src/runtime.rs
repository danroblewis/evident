//! Top-level API. Mirrors the Python `EvidentRuntime` for the v0.1 subset.
//!
//! ## Public verbs
//!
//! Most callers (commands/, embedders, tests) use the
//! constraint-solver verbs: `load_file` / `load_source` to load
//! programs, `query` / `query_cached` / `sample` to ask whether
//! claims are satisfiable, `get_schema` / `schema_names` to
//! introspect what's loaded.
//!
//! ## Execution-layer extension surface
//!
//! A small handful of verbs exist explicitly to support the
//! multi-FSM scheduler (`effect_loop.rs`):
//!
//!   * `query_with_pinned_datatypes` / `query_with_pins_and_given`
//!     — pin enum-valued variables (`state`, `last_results`)
//!     across a query so the scheduler can advance an FSM one
//!     tick under known-state.
//!   * `enums_registry` / `z3_context` — read-only access to the
//!     EnumRegistry and `'static` Z3 Context so the scheduler can
//!     re-encode `state_next` as a Datatype value for the next
//!     tick's pin.
//!   * `effect_results_to_value` — build a `Value::SeqEnum` of
//!     Result enums for pinning `last_results ∈ Seq(Result)` via
//!     the multi-FSM scheduler's `given` map.
//!
//! These methods are part of the facade rather than a separate
//! trait because the per-tick query path needs read access to
//! state (registries, context, schemas, cache) that lives behind
//! `&self` and would otherwise need parallel exposure. They
//! intentionally do NOT widen the constraint-side facade — they
//! expose the read-handles necessary for execution-layer
//! callers, nothing more. Callers outside the execution layer
//! should use `query` / `query_cached`; if you find yourself
//! reaching for one of these methods from elsewhere, reconsider
//! whether the verb you need exists on the constraint-side
//! facade or whether your concern belongs in the execution
//! layer alongside `effect_loop.rs`.

use crate::ast::{BodyItem, Program, SchemaDecl};
use crate::parser;
use crate::translate::{
    build_cache, run_cached, sample_cached_inner, structural_signature,
    CachedSchema, DatatypeRegistry, StructuralSignature,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use z3::{Config, Context};

pub use crate::translate::Value;

/// Parse "Edge<Rect>" into ("Edge", "Rect"). Returns None for
/// type-name strings that aren't a generic instantiation (no `<`,
/// or unbalanced angle brackets).
///
/// Handles nested generic args by counting depth: "Edge<Pair<Int,
/// String>>" parses to ("Edge", "Pair<Int, String>").
fn split_generic_head(type_name: &str) -> Option<(String, String)> {
    let bytes = type_name.as_bytes();
    let lt = bytes.iter().position(|&b| b == b'<')?;
    if !type_name.ends_with('>') { return None; }
    // Verify balanced.
    let mut depth: i32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 && i != bytes.len() - 1 { return None; }
            }
            _ => {}
        }
    }
    if depth != 0 { return None; }
    let name = type_name[..lt].to_string();
    let inner = type_name[lt + 1..bytes.len() - 1].to_string();
    Some((name, inner))
}

/// Split a comma-separated arg list at the TOP level — commas
/// inside nested `<...>` are not splits. "Pair<Int, String>, Bool"
/// → ["Pair<Int, String>", "Bool"].
fn split_top_level_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let bytes = args.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' | b'(' => depth += 1,
            b'>' | b')' => depth -= 1,
            b',' if depth == 0 => {
                out.push(args[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = args[start..].trim();
    if !tail.is_empty() { out.push(tail.to_string()); }
    out
}

/// Replace every token in `s` matching a key in `subst` with its
/// value. Tokens are maximal runs of identifier-char (ASCII
/// alphanumeric + `_`); other characters are passed through. Used
/// to substitute type parameters in a type-name string —
/// "Seq(T)" with `T → Rect` becomes "Seq(Rect)", "Pair<T, U>"
/// with `T → A, U → B` becomes "Pair<A, B>", `T_total` (an
/// unrelated identifier) is left alone.
fn substitute_idents(s: &str, subst: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut cur = String::new();
    fn is_ident_char(c: char) -> bool { c.is_ascii_alphanumeric() || c == '_' }
    for c in s.chars() {
        if is_ident_char(c) {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                if let Some(rep) = subst.get(&cur) { out.push_str(rep); }
                else { out.push_str(&cur); }
                cur.clear();
            }
            out.push(c);
        }
    }
    if !cur.is_empty() {
        if let Some(rep) = subst.get(&cur) { out.push_str(rep); }
        else { out.push_str(&cur); }
    }
    out
}

/// Apply a type-param substitution to every `type_name` in a
/// SchemaDecl's body. Recurses into subclaim bodies.
fn substitute_type_params_in_body(body: &mut Vec<BodyItem>, subst: &HashMap<String, String>) {
    use crate::ast::BodyItem;
    for item in body.iter_mut() {
        match item {
            BodyItem::Membership { type_name, .. } => {
                *type_name = substitute_idents(type_name, subst);
            }
            BodyItem::SubclaimDecl(sub) => {
                substitute_type_params_in_body(&mut sub.body, subst);
            }
            _ => {}
        }
    }
}

/// Collect every (composite_name, generic_head, args_str) tuple
/// referenced anywhere in the schemas map. Used by
/// `monomorphize_generics` to find work to do.
fn collect_generic_uses(schemas: &HashMap<String, SchemaDecl>) -> Vec<(String, String, String)> {
    use crate::ast::BodyItem;
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    fn walk_expr(e: &crate::ast::Expr, out: &mut Vec<(String, String, String)>, seen: &mut HashSet<String>) {
        use crate::ast::Expr;
        match e {
            // `Foo<Bar>(args, …)` — positional generic invocation
            // (e.g. `Permutation<Int>(a, b)` as a body constraint).
            Expr::Call(name, args) => {
                collect_from_type_name(name, out, seen);
                for a in args { walk_expr(a, out, seen); }
            }
            Expr::Binary(_, l, r) | Expr::Range(l, r)
            | Expr::InExpr(l, r) | Expr::Index(l, r) => {
                walk_expr(l, out, seen); walk_expr(r, out, seen);
            }
            Expr::Ternary(c, a, b) => {
                walk_expr(c, out, seen); walk_expr(a, out, seen); walk_expr(b, out, seen);
            }
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) => {
                for x in es { walk_expr(x, out, seen); }
            }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
                walk_expr(r, out, seen); walk_expr(b, out, seen);
            }
            Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => {
                walk_expr(i, out, seen);
            }
            Expr::Field(recv, _) => walk_expr(recv, out, seen),
            Expr::Match(scr, arms) => {
                walk_expr(scr, out, seen);
                for arm in arms { walk_expr(&arm.body, out, seen); }
            }
            _ => {}
        }
    }
    fn walk(body: &[BodyItem], out: &mut Vec<(String, String, String)>, seen: &mut HashSet<String>) {
        for item in body {
            match item {
                BodyItem::Membership { type_name, .. } => {
                    collect_from_type_name(type_name, out, seen);
                }
                BodyItem::SubclaimDecl(sub) => walk(&sub.body, out, seen),
                // Generic claim invocations: `FirstEqual<Rect>(a ↦ …)`.
                BodyItem::ClaimCall { name, mappings } => {
                    collect_from_type_name(name, out, seen);
                    for m in mappings { walk_expr(&m.value, out, seen); }
                }
                // Generic passthrough: `..Edge<Rect>`.
                BodyItem::Passthrough(name) => {
                    collect_from_type_name(name, out, seen);
                }
                // Body constraints can contain `Foo<Bar>(args)` positional
                // invocations or `Foo<Bar>` constructor calls inline.
                BodyItem::Constraint(e) => walk_expr(e, out, seen),
            }
        }
    }
    fn collect_from_type_name(t: &str, out: &mut Vec<(String, String, String)>, seen: &mut HashSet<String>) {
        // Handle the simple generic form "Edge<Rect>".
        if let Some((head, args)) = split_generic_head(t) {
            if seen.insert(t.to_string()) {
                out.push((t.to_string(), head.clone(), args.clone()));
            }
            // Each top-level arg may itself be a generic.
            for arg in split_top_level_args(&args) {
                collect_from_type_name(&arg, out, seen);
            }
            return;
        }
        // Handle "Seq(Edge<Rect>)" — recurse into the inner.
        if let Some(inner) = strip_seq_wrapper(t) {
            collect_from_type_name(inner, out, seen);
        }
    }
    for s in schemas.values() {
        walk(&s.body, &mut out, &mut seen);
    }
    out
}

/// If `t` is `"Seq(X)"`, `"Set(X)"`, `"Bag(X)"`, or `"Map(X)"`,
/// return Some(X). Otherwise None.
fn strip_seq_wrapper(t: &str) -> Option<&str> {
    for prefix in &["Seq(", "Set(", "Bag(", "Map("] {
        if let Some(rest) = t.strip_prefix(prefix) {
            if let Some(inner) = rest.strip_suffix(')') {
                return Some(inner);
            }
        }
    }
    None
}

/// Monomorphize: produce concrete SchemaDecls for every generic
/// instantiation referenced in the program. After this pass, every
/// type_name containing `<` resolves to a real schema in the map.
///
/// Iterates to a fixed point: monomorphized schemas can themselves
/// reference generic types (`Toposort<T>`'s body has
/// `edges ∈ Seq(Edge<T>)`, which after substitution becomes
/// `edges ∈ Seq(Edge<Rect>)` — that's a new instantiation to expand).
fn monomorphize_generics(
    schemas: &mut HashMap<String, SchemaDecl>,
    schema_order: &mut Vec<String>,
) -> Result<(), RuntimeError> {
    for _iteration in 0..50 {
        let needed = collect_generic_uses(schemas);
        let mut produced = 0;
        for (composite_name, generic_head, args_str) in needed {
            if schemas.contains_key(&composite_name) { continue; }
            let generic = match schemas.get(&generic_head) {
                Some(g) => g,
                None => continue,  // not a generic we know about; leave it
            };
            if generic.type_params.is_empty() {
                // Referenced like `Foo<Bar>` but Foo isn't generic.
                return Err(RuntimeError::Parse(format!(
                    "type `{}` referenced with type arguments `<{}>` but \
                     isn't declared as generic",
                    generic_head, args_str)));
            }
            let args = split_top_level_args(&args_str);
            if args.len() != generic.type_params.len() {
                return Err(RuntimeError::Parse(format!(
                    "type `{}` expects {} type argument(s), got {}: `{}`",
                    generic_head, generic.type_params.len(), args.len(),
                    composite_name)));
            }
            let mut subst: HashMap<String, String> = HashMap::new();
            for (p, a) in generic.type_params.iter().zip(args.iter()) {
                subst.insert(p.clone(), a.clone());
            }
            let mut mono = generic.clone();
            mono.name = composite_name.clone();
            mono.type_params = Vec::new();
            substitute_type_params_in_body(&mut mono.body, &subst);
            schemas.insert(composite_name.clone(), mono);
            schema_order.push(composite_name);
            produced += 1;
        }
        if produced == 0 { return Ok(()); }
    }
    Err(RuntimeError::Parse(
        "monomorphize_generics: didn't converge after 50 iterations (cycle?)".to_string()))
}

/// Snapshot of "everything loaded so far is the system layer."
/// Schemas / enums loaded after a `mark_system_loads_complete()` call
/// are treated as the user's program for the purposes of AST encoding.
#[derive(Default, Clone)]
pub struct SystemBoundary {
    pub schemas: HashSet<String>,
    pub enums:   HashSet<String>,
}

/// Desugar `Seq(T)` concatenation. The user writes
///
/// ```text
/// effects = sky_effs ++ rect_effs ++ ⟨present_eff⟩ ++ input_effs
/// ```
///
/// This pass walks the body twice: first to gather every
/// `name = ⟨items⟩` binding into a name→items map, then to walk
/// every body expression and rewrite each `Concat` subtree into a
/// flat `SeqLit`. The flattener resolves operands by:
///   * `SeqLit(items)` → use `items`.
///   * `Identifier(name)` → look up `seq_lits[name]`.
///   * `Concat(a, b)` → recurse.
///
/// If a `Concat` subtree fully resolves, it's replaced by a single
/// `SeqLit` of the flattened items. Concat nested inside a `Ternary`,
/// `Match` arm, claim-call argument, or further `Binary` ops is
/// rewritten too. If any operand can't be resolved (an opaque Seq
/// var coming from a claim invocation, for example), that subtree
/// is left alone and the translator will fail with the usual
/// "couldn't translate to Bool" error pointing at it.
fn desugar_seq_concat(s: &mut SchemaDecl) {
    use crate::ast::{BinOp, BodyItem, Expr};
    if s.external { return; }

    // Pass 1: gather SeqLit bindings.
    let mut seq_lits: HashMap<String, Vec<Expr>> = HashMap::new();
    for item in &s.body {
        let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item else { continue };
        if let (Expr::Identifier(name), Expr::SeqLit(items)) =
            (lhs.as_ref(), rhs.as_ref())
        {
            seq_lits.insert(name.clone(), items.clone());
        }
    }

    fn flatten(
        e: &Expr,
        seq_lits: &HashMap<String, Vec<Expr>>,
    ) -> Option<Vec<Expr>> {
        match e {
            Expr::Binary(BinOp::Concat, l, r) => {
                let mut left = flatten(l, seq_lits)?;
                let right = flatten(r, seq_lits)?;
                left.extend(right);
                Some(left)
            }
            Expr::SeqLit(items) => Some(items.clone()),
            Expr::Identifier(name) => seq_lits.get(name).cloned(),
            _ => None,
        }
    }

    // Replace any Concat subexpression that fully flattens into a
    // SeqLit. Walks the entire tree so Concat inside Ternary,
    // Match arms, Call args, etc. all get rewritten.
    fn rewrite(
        e: &mut Expr,
        seq_lits: &HashMap<String, Vec<Expr>>,
    ) {
        if let Expr::Binary(BinOp::Concat, ..) = e {
            if let Some(items) = flatten(e, seq_lits) {
                *e = Expr::SeqLit(items);
                return;
            }
        }
        match e {
            Expr::Binary(_, l, r)
            | Expr::Range(l, r)
            | Expr::InExpr(l, r)
            | Expr::Index(l, r) => { rewrite(l, seq_lits); rewrite(r, seq_lits); }
            Expr::Ternary(c, a, b) => {
                rewrite(c, seq_lits); rewrite(a, seq_lits); rewrite(b, seq_lits);
            }
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es)
            | Expr::Call(_, es) => {
                for x in es { rewrite(x, seq_lits); }
            }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
                rewrite(r, seq_lits); rewrite(b, seq_lits);
            }
            Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => {
                rewrite(i, seq_lits);
            }
            Expr::Field(recv, _) => rewrite(recv, seq_lits),
            Expr::Match(scr, arms) => {
                rewrite(scr, seq_lits);
                for a in arms { rewrite(&mut a.body, seq_lits); }
            }
            _ => {}
        }
    }

    // Pass 2: walk every body item's expressions and rewrite Concat in place.
    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => rewrite(e, &seq_lits),
            BodyItem::ClaimCall { mappings, .. } => {
                for m in mappings.iter_mut() {
                    rewrite(&mut m.value, &seq_lits);
                }
            }
            _ => {}
        }
    }

    // Recurse into subclaims.
    for item in s.body.iter_mut() {
        if let BodyItem::SubclaimDecl(sub) = item {
            desugar_seq_concat(sub);
        }
    }
}

/// Unified-state world syntax. When an fsm declares
/// `world ∈ World` but NOT `world_next ∈ World`, the user is
/// using the `_var` time-shift convention for shared state:
///   * `world.X` reads/writes the current tick's value.
///   * `_world.X` reads the previous tick's value.
///
/// The multi-FSM scheduler still expects the legacy writer
/// pattern (`world` read-only + `world_next` write-only), so
/// this pass rewrites the body in-place to that shape:
///   * Every `world.X` reference (read or write) → `world_next.X`.
///     That makes it one Z3 var that's both constrained and
///     read within the same body — same semantics as the new
///     model's "this-tick value."
///   * Every `_world.X` reference → `world.X`. That's the
///     scheduler's "read of previous snapshot" path.
///   * Auto-inject `world_next ∈ World` so downstream
///     translation sees the legacy shape.
///
/// External fsms are skipped (they don't carry user logic).
fn unify_world_syntax(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
    use crate::ast::{BodyItem, Expr, Keyword, Pins};
    if !matches!(s.keyword, Keyword::Fsm) { return Ok(()); }
    if s.external { return Ok(()); }

    // Find `world` membership type (if any) and whether
    // `world_next` is already declared.
    let mut world_type: Option<String> = None;
    let mut has_world_next = false;
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            if name == "world" { world_type = Some(type_name.clone()); }
            if name == "world_next" { has_world_next = true; }
        }
    }
    let Some(world_ty) = world_type else { return Ok(()); };
    if has_world_next { return Ok(()); }   // legacy pattern; leave alone.

    // Only trigger the rewrite when the body actually uses
    // `_world.X` references — that's the unambiguous signal that
    // the user is in the unified-syntax world. Without this check,
    // legacy read-only fsms (declare `world ∈ World`, no `world_next`,
    // never write to world) would have their reads of `world.X`
    // wrongly promoted to writes, and the scheduler's single-owner-
    // per-field check would reject the program.
    fn uses_underscore_world(e: &Expr) -> bool {
        match e {
            Expr::Identifier(n) => n.starts_with("_world."),
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => false,
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                es.iter().any(uses_underscore_world),
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
                uses_underscore_world(a) || uses_underscore_world(b),
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                uses_underscore_world(r) || uses_underscore_world(b),
            Expr::Call(_, args) => args.iter().any(uses_underscore_world),
            Expr::Cardinality(i) | Expr::Not(i) => uses_underscore_world(i),
            Expr::Field(recv, _) => uses_underscore_world(recv),
            Expr::Binary(_, l, r) =>
                uses_underscore_world(l) || uses_underscore_world(r),
            Expr::Ternary(c, a, b) =>
                uses_underscore_world(c) || uses_underscore_world(a)
                    || uses_underscore_world(b),
            Expr::Match(scr, arms) =>
                uses_underscore_world(scr)
                    || arms.iter().any(|a| uses_underscore_world(&a.body)),
            Expr::Matches(e, _) => uses_underscore_world(e),
        }
    }
    let uses_new_syntax = s.body.iter().any(|item| match item {
        BodyItem::Constraint(e) => uses_underscore_world(e),
        BodyItem::ClaimCall { mappings, .. } =>
            mappings.iter().any(|m| uses_underscore_world(&m.value)),
        _ => false,
    });
    if !uses_new_syntax { return Ok(()); }

    // Rewrite Identifier strings in the body.
    //   "_world.X" → "world.X"
    //   "world.X"  → "world_next.X"
    // Same walk so both happen in one pass without re-matching.
    fn rewrite_ident(name: &str) -> Option<String> {
        if let Some(rest) = name.strip_prefix("_world.") {
            return Some(format!("world.{rest}"));
        }
        if let Some(rest) = name.strip_prefix("world.") {
            return Some(format!("world_next.{rest}"));
        }
        None
    }
    fn walk(e: &mut Expr) {
        match e {
            Expr::Identifier(n) => {
                if let Some(new_n) = rewrite_ident(n) { *n = new_n; }
            }
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                for x in es { walk(x); },
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
                { walk(a); walk(b); }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                { walk(r); walk(b); }
            Expr::Call(_, args) => for a in args { walk(a); },
            Expr::Cardinality(i) | Expr::Not(i) => walk(i),
            Expr::Field(recv, _) => walk(recv),
            Expr::Binary(_, l, r) => { walk(l); walk(r); }
            Expr::Ternary(c, a, b) => { walk(c); walk(a); walk(b); }
            Expr::Match(scr, arms) => {
                walk(scr);
                for arm in arms { walk(arm.body.as_mut()); }
            }
            Expr::Matches(e, _) => walk(e),
        }
    }
    for item in s.body.iter_mut() {
        match item {
            BodyItem::Constraint(e) => walk(e),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { walk(&mut m.value); },
            // Pin values inside type-use Memberships also need
            // rewriting — `mario ∈ MarioSprite (pos ↦ _world.player.pos)`
            // desugars at translate time to `mario.pos =
            // _world.player.pos`, which only resolves if the RHS has
            // been promoted to `world.player.pos` like the rest of the
            // body's `_world` reads.
            BodyItem::Membership { pins, .. } => match pins {
                Pins::Named(named) => for m in named { walk(&mut m.value); },
                Pins::Positional(vals) => for v in vals { walk(v); },
                Pins::None => {}
            },
            _ => {}
        }
    }

    // Inject `world_next ∈ World` so the scheduler's writer-shape
    // detection finds it.
    let insert_pos = s.param_count;
    s.body.insert(insert_pos, BodyItem::Membership {
        name: "world_next".to_string(),
        type_name: world_ty,
        pins: Pins::None,
    });
    Ok(())
}

/// Smart-inject implicit fsm machinery. For each canonical slot
/// (`state_next`, `last_results`, `effects`), inject the membership
/// only when (a) the body actually references the name AND (b) the
/// user didn't already declare it. A "pure" fsm that just maintains
/// internal state via `_var` time-shift gets nothing injected.
///
/// `state_next` additionally requires `state ∈ <Type>` to be
/// declared (it mirrors that type). If `state` isn't declared, we
/// skip — the fsm is free to have no state-pair.
///
/// `external fsm` declarations are CONTRACTS for runtime-side
/// bridge FSMs; they get no injection at all.
fn inject_fsm_params(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
    use crate::ast::{BodyItem, Expr, Keyword, Pins};
    if !matches!(s.keyword, Keyword::Fsm) {
        return Ok(());
    }
    if s.external {
        return Ok(());
    }
    // Scan declared names + find the state type (for state_next mirror).
    let mut state_type: Option<String> = None;
    let mut have_state_next = false;
    let mut have_last_results = false;
    let mut have_effects = false;
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            match name.as_str() {
                "state" if state_type.is_none() => state_type = Some(type_name.clone()),
                "state_next"   => have_state_next   = true,
                "last_results" => have_last_results = true,
                "effects"      => have_effects      = true,
                _ => {}
            }
        }
    }

    // Scan body expressions for references to the canonical slots.
    fn walk(e: &Expr, targets: &mut [(&str, &mut bool)]) {
        match e {
            Expr::Identifier(n) => {
                for (name, hit) in targets.iter_mut() {
                    if n == *name { **hit = true; }
                }
            }
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                for x in es { walk(x, targets); },
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
                { walk(a, targets); walk(b, targets); }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                { walk(r, targets); walk(b, targets); }
            Expr::Call(_, args) =>
                for a in args { walk(a, targets); },
            Expr::Cardinality(i) | Expr::Not(i) => walk(i, targets),
            Expr::Field(recv, _) => walk(recv, targets),
            Expr::Binary(_, l, r) => { walk(l, targets); walk(r, targets); }
            Expr::Ternary(c, a, b) =>
                { walk(c, targets); walk(a, targets); walk(b, targets); }
            Expr::Match(scr, arms) => {
                walk(scr, targets);
                for arm in arms { walk(&arm.body, targets); }
            }
            Expr::Matches(e, _) => walk(e, targets),
        }
    }
    let mut ref_state_next = false;
    let mut ref_last_results = false;
    let mut ref_effects = false;
    {
        let mut targets: Vec<(&str, &mut bool)> = vec![
            ("state_next",   &mut ref_state_next),
            ("last_results", &mut ref_last_results),
            ("effects",      &mut ref_effects),
        ];
        for item in &s.body {
            match item {
                BodyItem::Constraint(e) => walk(e, &mut targets),
                BodyItem::ClaimCall { mappings, .. } =>
                    for m in mappings { walk(&m.value, &mut targets); },
                _ => {}
            }
        }
    }

    let mut injected: Vec<BodyItem> = Vec::new();
    if !have_state_next && ref_state_next {
        if let Some(st) = &state_type {
            injected.push(BodyItem::Membership {
                name: "state_next".to_string(),
                type_name: st.clone(),
                pins: Pins::None,
            });
        }
    }
    if !have_last_results && ref_last_results {
        injected.push(BodyItem::Membership {
            name: "last_results".to_string(),
            type_name: "Seq(Result)".to_string(),
            pins: Pins::None,
        });
    }
    if !have_effects && ref_effects {
        injected.push(BodyItem::Membership {
            name: "effects".to_string(),
            type_name: "Seq(Effect)".to_string(),
            pins: Pins::None,
        });
    }
    let insert_pos = s.param_count;
    for (i, item) in injected.into_iter().enumerate() {
        s.body.insert(insert_pos + i, item);
    }
    Ok(())
}

/// Auto-declare `_name` memberships for any underscore-prefix
/// identifier referenced in an `fsm` body where the corresponding
/// `name` is declared. Adds `is_first_tick ∈ Bool` once if any
/// `_name` was referenced.
///
/// This is the syntactic half of the time-shift convention: in an
/// fsm body, writing `_count` reads the previous tick's `count`.
/// The runtime half (pinning `_count = prev_values["count"]` per
/// tick) lives in the scheduler — see `effect_loop.rs`.
///
/// External fsm declarations are CONTRACTS for runtime-side bridges
/// and don't get this treatment — their slots are written by Rust.
fn inject_prev_tick_decls(s: &mut SchemaDecl) -> Result<(), RuntimeError> {
    use crate::ast::{BodyItem, Keyword, Pins, Expr};
    if !matches!(s.keyword, Keyword::Fsm) { return Ok(()); }
    if s.external { return Ok(()); }

    // Step 1: Collect every name → type from this body's
    // memberships. Determines whether a `_name` reference has a
    // matching `name` to mirror.
    let mut declared: HashMap<String, String> = HashMap::new();
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            declared.insert(name.clone(), type_name.clone());
        }
    }

    // Step 2: Walk the body's expressions for any
    // `Identifier(_name)` where `name` (without the underscore) is
    // declared. Collect their target types.
    let mut prev_refs: HashMap<String, String> = HashMap::new();
    fn walk(e: &Expr, declared: &HashMap<String, String>,
            prev_refs: &mut HashMap<String, String>) {
        match e {
            Expr::Identifier(n) => {
                // Two shapes:
                //   * `_count`         → strip → `count`
                //   * `_pos.x`         → strip first segment → `pos`
                // The parser collapses dotted chains in `parse_atom`,
                // so `_pos.x` arrives as Identifier("_pos.x"); we
                // only want to register the prev-tick parent (`pos`).
                let Some(after_underscore) = n.strip_prefix('_') else { return; };
                let first_seg = after_underscore
                    .split('.').next().unwrap_or(after_underscore);
                if let Some(ty) = declared.get(first_seg) {
                    // Key by the bare `_first_seg`, value is its type.
                    // We inject `_first_seg ∈ ty` once; per-field
                    // expansion (`_pos.x`, `_pos.y`) happens at
                    // translation via declare_var_named.
                    let key = format!("_{first_seg}");
                    prev_refs.insert(key, ty.clone());
                }
            }
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                for x in es { walk(x, declared, prev_refs); },
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
                { walk(a, declared, prev_refs); walk(b, declared, prev_refs); }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                { walk(r, declared, prev_refs); walk(b, declared, prev_refs); }
            Expr::Call(_, args) =>
                for a in args { walk(a, declared, prev_refs); },
            Expr::Cardinality(i) | Expr::Not(i) =>
                walk(i, declared, prev_refs),
            Expr::Field(recv, _) => walk(recv, declared, prev_refs),
            Expr::Binary(_, l, r) =>
                { walk(l, declared, prev_refs); walk(r, declared, prev_refs); }
            Expr::Ternary(c, a, b) => {
                walk(c, declared, prev_refs);
                walk(a, declared, prev_refs);
                walk(b, declared, prev_refs);
            }
            Expr::Match(scr, arms) => {
                walk(scr, declared, prev_refs);
                for arm in arms { walk(&arm.body, declared, prev_refs); }
            }
            Expr::Matches(e, _) => walk(e, declared, prev_refs),
        }
    }
    for item in &s.body {
        match item {
            BodyItem::Constraint(e) => walk(e, &declared, &mut prev_refs),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { walk(&m.value, &declared, &mut prev_refs); },
            _ => {}
        }
    }

    if prev_refs.is_empty() { return Ok(()); }

    // Step 3: For each `_name` referenced, add a Membership for
    // it (typed to match `name`) unless the user already declared
    // it themselves. Also add `is_first_tick ∈ Bool` for tick-0
    // dispatch.
    let mut to_inject: Vec<BodyItem> = Vec::new();
    for (prev_name, ty) in &prev_refs {
        if !declared.contains_key(prev_name) {
            to_inject.push(BodyItem::Membership {
                name: prev_name.clone(),
                type_name: ty.clone(),
                pins: Pins::None,
            });
        }
    }
    if !declared.contains_key("is_first_tick") {
        to_inject.push(BodyItem::Membership {
            name: "is_first_tick".to_string(),
            type_name: "Bool".to_string(),
            pins: Pins::None,
        });
    }
    let insert_pos = s.param_count;
    for (i, item) in to_inject.into_iter().enumerate() {
        s.body.insert(insert_pos + i, item);
    }
    Ok(())
}

/// Infer types for fresh names used as positional args in claim
/// calls. When the user writes
///
///   set_draw_color(win.renderer, Color(...), sky_eff)
///   effects = ⟨sky_eff, ...⟩
///
/// `sky_eff` is not declared as a Membership but its type is recoverable
/// from `set_draw_color`'s third param (`out ∈ Effect`). This pass
/// auto-injects `sky_eff ∈ Effect` so the user can drop the manual
/// decl line.
///
/// **Typo defense**: we only infer when the name appears in ≥ 2
/// expression positions across the body. If a name shows up exactly
/// once (just the call site), it might be a typo of an intended
/// reference — leave it alone so translation fails loudly. The common
/// case (claim-call output threaded into the effects list) hits ≥ 2
/// uses naturally.
///
/// Handles method-style `recv.claim(args)` too: the receiver counts
/// as a positional arg, shifting the arg-to-param mapping by 1.
fn inject_claim_arg_types(
    s: &mut SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
) -> Result<(), RuntimeError> {
    use crate::ast::{BodyItem, Expr, Keyword, Pins};
    if s.external { return Ok(()); }
    // Apply to fsm bodies and ordinary claim bodies alike — the
    // pattern is the same wherever a positional claim call has a
    // fresh output arg.
    let _ = Keyword::Fsm;

    // Step 1: declared names (memberships).
    let mut declared: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &s.body {
        if let BodyItem::Membership { name, .. } = item {
            declared.insert(name.clone());
        }
    }

    // Step 2: count Identifier references across body expressions.
    let mut uses: HashMap<String, usize> = HashMap::new();
    fn walk(e: &Expr, uses: &mut HashMap<String, usize>) {
        match e {
            Expr::Identifier(n) => { *uses.entry(n.clone()).or_default() += 1; }
            Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => {}
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) =>
                for x in es { walk(x, uses); },
            Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
                { walk(a, uses); walk(b, uses); }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                { walk(r, uses); walk(b, uses); }
            Expr::Call(_, args) => for a in args { walk(a, uses); },
            Expr::Cardinality(i) | Expr::Not(i) => walk(i, uses),
            Expr::Field(recv, _) => walk(recv, uses),
            Expr::Binary(_, l, r) => { walk(l, uses); walk(r, uses); }
            Expr::Ternary(c, a, b) =>
                { walk(c, uses); walk(a, uses); walk(b, uses); }
            Expr::Match(scr, arms) => {
                walk(scr, uses);
                for arm in arms { walk(&arm.body, uses); }
            }
            Expr::Matches(e, _) => walk(e, uses),
        }
    }
    for item in &s.body {
        match item {
            BodyItem::Constraint(e) => walk(e, &mut uses),
            BodyItem::ClaimCall { mappings, .. } =>
                for m in mappings { walk(&m.value, &mut uses); },
            _ => {}
        }
    }

    // Step 3: scan body for positional claim calls. For each
    // Identifier arg that's fresh + multi-use, look up the
    // corresponding param's type from the called claim and queue
    // a Membership injection.
    let mut to_inject_map: HashMap<String, String> = HashMap::new();

    // Dispatch resolver, subschema-aware. Three flavors:
    //   * Subschema: `recv.subclaim` where recv is a body Membership
    //     of record T AND T's body has SubclaimDecl `subclaim`.
    //     Args bind to subclaim's leading Memberships starting at
    //     position 0 (the receiver is NOT a positional arg — it
    //     provides the parent-type field scope at invocation time).
    //   * Receiver-prefix: `recv.claim` where claim is just a plain
    //     known schema. Receiver becomes the first positional arg,
    //     so args bind starting at slot 1.
    //   * Plain: bare `claim(args)`. Args bind from slot 0.
    let resolve = |name: &str| -> Option<(String, /*arg_offset:*/ usize)> {
        if schemas.contains_key(name) {
            return Some((name.to_string(), 0));
        }
        let (prefix, suffix) = name.rsplit_once('.')?;
        // Subschema check: prefix is a single-segment Membership of
        // a record type with `suffix` as a SubclaimDecl.
        if !prefix.contains('.') {
            for item in &s.body {
                if let BodyItem::Membership { name: mname, type_name, .. } = item {
                    if mname == prefix {
                        if let Some(type_decl) = schemas.get(type_name) {
                            let has_sub = type_decl.body.iter().any(|i|
                                matches!(i, BodyItem::SubclaimDecl(sub) if sub.name == suffix));
                            if has_sub {
                                return Some((suffix.to_string(), 0));
                            }
                        }
                    }
                }
            }
        }
        // Receiver-prefix fallback.
        if schemas.contains_key(suffix) {
            return Some((suffix.to_string(), 1));
        }
        None
    };

    let process_call = |claim_name: &str, arg_offset: usize, args: &[Expr],
                        declared: &std::collections::HashSet<String>,
                        uses: &HashMap<String, usize>,
                        to_inject_map: &mut HashMap<String, String>| {
        let Some(claim) = schemas.get(claim_name) else { return; };
        // Leading Memberships (first-line params + body Memberships).
        let claim_params: Vec<(String, String)> = claim.body.iter()
            .filter_map(|i| if let BodyItem::Membership { name, type_name, .. } = i {
                Some((name.clone(), type_name.clone()))
            } else { None })
            .take(claim.param_count.max(args.len() + arg_offset))
            .collect();
        for (i, arg) in args.iter().enumerate() {
            let Expr::Identifier(arg_name) = arg else { continue; };
            if arg_name.contains('.') { continue; }   // field-access, not fresh
            if declared.contains(arg_name) { continue; }
            if schemas.contains_key(arg_name) { continue; }   // claim/type name
            let Some((_, param_type)) = claim_params.get(i + arg_offset) else { continue; };
            let count = uses.get(arg_name).copied().unwrap_or(0);
            if count < 2 { continue; }                // typo defense
            // First call wins if multiple sites disagree on type.
            to_inject_map.entry(arg_name.clone()).or_insert_with(|| param_type.clone());
        }
    };

    for item in &s.body {
        match item {
            BodyItem::Constraint(Expr::Call(name, args)) => {
                if let Some((cn, off)) = resolve(name) {
                    process_call(&cn, off, args, &declared, &uses, &mut to_inject_map);
                }
            }
            BodyItem::Constraint(Expr::InExpr(lhs, rhs)) => {
                if let (Expr::Tuple(items), Expr::Identifier(rname)) =
                    (lhs.as_ref(), rhs.as_ref())
                {
                    if let Some((cn, off)) = resolve(rname) {
                        process_call(&cn, off, items, &declared, &uses, &mut to_inject_map);
                    }
                }
            }
            _ => {}
        }
    }

    if to_inject_map.is_empty() { return Ok(()); }
    let insert_pos = s.param_count;
    // Stable order for diagnostics.
    let mut entries: Vec<(String, String)> = to_inject_map.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (i, (name, type_name)) in entries.into_iter().enumerate() {
        s.body.insert(insert_pos + i, BodyItem::Membership {
            name, type_name, pins: Pins::None,
        });
    }
    Ok(())
}

/// Infer Memberships for body-level `Identifier = Expr` constraints
/// whose LHS is undeclared and whose RHS has a recoverable type.
///
/// Lets a subclaim or fsm body write
///
///   out = LibCall("...", "...", "...", ⟨…⟩)
///
/// without first declaring `out ∈ Effect` — the RHS's `LibCall` is
/// an Effect-constructor variant, so we inject `out ∈ Effect` at
/// the head of the body. Same idea for record constructors
/// (`pos = IVec2(3, 4)` infers `pos ∈ IVec2`).
///
/// Recursive: also processes SubclaimDecls inside the body, so the
/// inference fires inside types' rendering subclaims.
fn inject_lhs_eq_types(
    s: &mut SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
    enums: &crate::translate::EnumRegistry,
) {
    use crate::ast::{BinOp, BodyItem, Expr, Pins};

    // Collect names already declared in this body, with their types
    // (for field-access inference via dotted identifiers).
    let mut declared_types: HashMap<String, String> = HashMap::new();
    for item in &s.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            declared_types.insert(name.clone(), type_name.clone());
        }
    }
    let declared: std::collections::HashSet<String> =
        declared_types.keys().cloned().collect();

    // Walk a dotted identifier path (`world.tick`, `win.pos.x`) and
    // return the leaf field's declared type. Looks up `head` in the
    // current body's memberships, then chains through schema bodies
    // for each subsequent segment.
    fn lookup_field_type(
        dotted: &str,
        declared_types: &HashMap<String, String>,
        schemas: &HashMap<String, SchemaDecl>,
    ) -> Option<String> {
        let mut parts = dotted.split('.');
        let head = parts.next()?;
        let mut current_type = declared_types.get(head).cloned()?;
        for field in parts {
            let type_decl = schemas.get(&current_type)?;
            let mut next_type: Option<String> = None;
            for item in &type_decl.body {
                if let BodyItem::Membership { name, type_name, .. } = item {
                    if name == field { next_type = Some(type_name.clone()); break; }
                }
            }
            current_type = next_type?;
        }
        Some(current_type)
    }

    // Top-level inference: looks at an Expr and returns its type if
    // determinable. Leaves bare primitive literals (`Int(_)`, etc.)
    // to query-time inference. Recursive helpers DO match literals
    // because they're inside compound shapes (ternary arms, binary
    // operands) where the chained-membership inference is the only
    // way to know the result type.
    //
    // Recursive: ternaries / matches descend into arms; binary ops
    // either yield Bool (comparisons / logical) or recurse into
    // operands (arithmetic).
    fn infer_recursive(
        e: &Expr,
        declared_types: &HashMap<String, String>,
        schemas: &HashMap<String, SchemaDecl>,
        enums: &crate::translate::EnumRegistry,
    ) -> Option<String> {
        match e {
            Expr::Int(_)  => Some("Int".to_string()),
            Expr::Bool(_) => Some("Bool".to_string()),
            Expr::Str(_)  => Some("String".to_string()),
            Expr::Real(_) => Some("Real".to_string()),
            Expr::Identifier(n) => {
                if let Some(t) = declared_types.get(n) { return Some(t.clone()); }
                if n.contains('.') {
                    return lookup_field_type(n, declared_types, schemas);
                }
                None
            }
            Expr::Field(recv, field) => {
                // Compose the dotted path from receiver if it's an Identifier.
                if let Expr::Identifier(head) = recv.as_ref() {
                    return lookup_field_type(
                        &format!("{head}.{field}"), declared_types, schemas);
                }
                None
            }
            Expr::Call(name, _) => {
                if let Some((enum_name, _)) = enums.by_variant.borrow().get(name) {
                    return Some(enum_name.clone());
                }
                if schemas.contains_key(name) {
                    return Some(name.clone());
                }
                None
            }
            Expr::Ternary(_, a, b) =>
                infer_recursive(a, declared_types, schemas, enums)
                    .or_else(|| infer_recursive(b, declared_types, schemas, enums)),
            Expr::Match(_, arms) => arms.iter().find_map(|arm|
                infer_recursive(&arm.body, declared_types, schemas, enums)),
            Expr::Binary(op, l, r) => match op {
                BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
                | BinOp::Eq | BinOp::Neq
                | BinOp::And | BinOp::Or | BinOp::Implies =>
                    Some("Bool".to_string()),
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div =>
                    infer_recursive(l, declared_types, schemas, enums)
                        .or_else(|| infer_recursive(r, declared_types, schemas, enums)),
                BinOp::Concat => Some("String".to_string()),
            },
            Expr::Not(_) => Some("Bool".to_string()),
            Expr::Cardinality(_) => Some("Int".to_string()),
            _ => None,
        }
    }

    // Top-level wrapper: skip bare primitive literals so the
    // query-time pass picks them up (preserves the `--strict`
    // contract — see `cli_query_without_infer_types_fails…`).
    fn infer_type(
        e: &Expr,
        declared_types: &HashMap<String, String>,
        schemas: &HashMap<String, SchemaDecl>,
        enums: &crate::translate::EnumRegistry,
    ) -> Option<String> {
        if matches!(e, Expr::Int(_) | Expr::Bool(_) | Expr::Str(_) | Expr::Real(_)) {
            return None;
        }
        infer_recursive(e, declared_types, schemas, enums)
    }

    // Walk body constraints; queue inferrable Memberships.
    let mut to_inject: Vec<(String, String)> = Vec::new();
    let mut already_queued: std::collections::HashSet<String> = std::collections::HashSet::new();
    for item in &s.body {
        let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item else { continue };
        let Expr::Identifier(name) = lhs.as_ref() else { continue };
        if name.contains('.') { continue; }                  // field access on existing record
        if declared.contains(name) { continue; }              // already declared
        if already_queued.contains(name) { continue; }        // first eq wins
        if schemas.contains_key(name) { continue; }           // not a fresh local — claim/type name
        let Some(ty) = infer_type(rhs, &declared_types, schemas, enums) else { continue };
        to_inject.push((name.clone(), ty));
        already_queued.insert(name.clone());
    }

    // Inject at body head (after first-line params).
    let insert_pos = s.param_count;
    for (i, (name, type_name)) in to_inject.into_iter().enumerate() {
        s.body.insert(insert_pos + i, BodyItem::Membership {
            name, type_name, pins: Pins::None,
        });
    }

    // Recurse into nested subclaims.
    for item in s.body.iter_mut() {
        if let BodyItem::SubclaimDecl(sub) = item {
            inject_lhs_eq_types(sub, schemas, enums);
        }
    }
}

/// Reject non-`external` schemas that try to construct FFI effects
/// (`FFICall` / `LibCall` / `FFIOpen` / `FFILookup`). The rule:
/// only `external` schemas (`external type` / `external claim` /
/// `external fsm`) may produce those effect values. Demos and
/// ordinary library code reach C through the `external claim`
/// wrappers in `packages/` and `stdlib/posix.ev`.
///
/// The check walks every `Constraint` body item's expression tree.
/// SubclaimDecl bodies are skipped — their own load pass handles them.
fn enforce_external_only(s: &SchemaDecl) -> Result<(), RuntimeError> {
    use crate::ast::{BodyItem, Expr};
    if s.external { return Ok(()); }
    fn find_ffi_call(e: &Expr) -> Option<&'static str> {
        match e {
            Expr::Call(name, args) => {
                let banned = match name.as_str() {
                    "FFICall"   => Some("FFICall"),
                    "FFIOpen"   => Some("FFIOpen"),
                    "FFILookup" => Some("FFILookup"),
                    "LibCall"   => Some("LibCall"),
                    _ => None,
                };
                if banned.is_some() { return banned; }
                args.iter().filter_map(find_ffi_call).next()
            }
            Expr::Binary(_, l, r) =>
                find_ffi_call(l).or_else(|| find_ffi_call(r)),
            Expr::Not(i) | Expr::Cardinality(i) => find_ffi_call(i),
            Expr::Ternary(c, a, b) =>
                find_ffi_call(c).or_else(|| find_ffi_call(a))
                                .or_else(|| find_ffi_call(b)),
            Expr::Index(s, i) | Expr::Range(s, i) | Expr::InExpr(s, i) =>
                find_ffi_call(s).or_else(|| find_ffi_call(i)),
            Expr::Field(b, _) => find_ffi_call(b),
            Expr::Matches(e, _) => find_ffi_call(e),
            Expr::SeqLit(items) | Expr::SetLit(items) =>
                items.iter().filter_map(find_ffi_call).next(),
            Expr::Match(scr, arms) =>
                find_ffi_call(scr).or_else(|| arms.iter()
                    .filter_map(|a| find_ffi_call(&a.body)).next()),
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) =>
                find_ffi_call(r).or_else(|| find_ffi_call(b)),
            _ => None,
        }
    }
    for item in &s.body {
        if let BodyItem::Constraint(e) = item {
            if let Some(call) = find_ffi_call(e) {
                let kind = match s.keyword {
                    crate::ast::Keyword::Fsm => "fsm",
                    crate::ast::Keyword::Type => "type",
                    crate::ast::Keyword::Claim => "claim",
                    crate::ast::Keyword::Schema => "schema",
                    crate::ast::Keyword::Subclaim => "subclaim",
                };
                return Err(RuntimeError::Parse(format!(
                    "{kind} `{}` constructs `{call}(...)` but isn't \
                     declared `external`. Either mark this declaration \
                     `external claim` / `external type`, or move the \
                     FFI into an `external claim` helper and call it \
                     from here.",
                    s.name
                )));
            }
        }
    }
    Ok(())
}

/// Walk a schema body and register any nested `subclaim` declarations
/// into `schemas` (recursively, so a subclaim of a subclaim is also
/// reachable).
fn register_subclaims(body: &[BodyItem], schemas: &mut HashMap<String, SchemaDecl>) {
    for item in body {
        if let BodyItem::SubclaimDecl(s) = item {
            schemas.insert(s.name.clone(), s.clone());
            register_subclaims(&s.body, schemas);
        }
    }
}

/// Batched build of Z3 DatatypeSorts for every enum declared in
/// `decls`, using `z3::datatype_builder::create_datatypes` so that
/// enums can forward-reference each other or be mutually recursive.
///
/// Three kinds of payload-field references are resolved per variant:
///
///   * Primitive (`Int`/`Nat`/`Pos`/`Real`/`Bool`/`String`) →
///     `DatatypeAccessor::Sort(...)`.
///   * Self-reference or forward-reference to another enum *in this
///     batch* → `DatatypeAccessor::Datatype(name)`. The Z3 multi-
///     builder resolves these during `create_datatypes`.
///   * Reference to an enum already registered in a previous load →
///     `DatatypeAccessor::Sort(prev.sort.clone())`.
///
/// Anything else (unknown type name) errors with a user-readable
/// message naming the offending variant + field.
///
/// Variant names are globally unique across all enums; load fails
/// on collision, both within `decls` and against previously-loaded
/// enums in the registry.
fn register_enums(
    decls: &[crate::ast::EnumDecl],
    ctx: &'static Context,
    registry: &crate::translate::EnumRegistry,
) -> Result<(), RuntimeError> {
    use z3::{DatatypeAccessor, DatatypeBuilder, DatatypeSort, Sort};
    if decls.is_empty() { return Ok(()); }

    // Pre-flight checks: variant uniqueness (across this batch and
    // previously-loaded enums), and enum-name uniqueness (same).
    let batch_names: std::collections::HashSet<&str> =
        decls.iter().map(|d| d.name.as_str()).collect();
    {
        // Same-batch enum-name uniqueness: walk decls once and bail on
        // the first repeat. If batch_names.len() != decls.len() then
        // some name collided; locate it for a useful message.
        if batch_names.len() != decls.len() {
            let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for d in decls {
                if !seen.insert(d.name.as_str()) {
                    return Err(RuntimeError::Parse(format!(
                        "enum `{}` declared more than once in the same load",
                        d.name)));
                }
            }
        }
        let existing_by_name = registry.by_name.borrow();
        for d in decls {
            if existing_by_name.contains_key(&d.name) {
                return Err(RuntimeError::Parse(format!(
                    "enum `{}` declared more than once", d.name)));
            }
            if d.variants.is_empty() {
                return Err(RuntimeError::Parse(
                    format!("enum {} has no variants", d.name)));
            }
        }
        let by_variant = registry.by_variant.borrow();
        let mut batch_seen: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for d in decls {
            for v in &d.variants {
                if let Some((existing_enum, _)) = by_variant.get(&v.name) {
                    return Err(RuntimeError::Parse(format!(
                        "enum variant `{}` is declared twice — once in `{}` and once in `{}`",
                        v.name, existing_enum, d.name,
                    )));
                }
                if let Some(prev_in_batch) = batch_seen.get(&v.name) {
                    return Err(RuntimeError::Parse(format!(
                        "enum variant `{}` is declared twice — once in `{}` and once in `{}`",
                        v.name, prev_in_batch, d.name,
                    )));
                }
                batch_seen.insert(v.name.clone(), d.name.clone());
            }
        }
    }

    // Phase 6.5: when a variant has a `Seq(T)` field where T is in
    // THIS batch (so its sort isn't available yet), the two-accessor
    // Array(Int → T) expansion fails — Z3's array sort needs a
    // concrete element sort, and there's no forward-ref mechanism
    // for sorts wrapping a batch-local datatype. Generate an
    // internal Cons-shaped helper datatype `__SeqOf_T` with
    // `__Empty_T` + `__Cell_T(T, __SeqOf_T)`, add it to the batch.
    // The original `Seq(T)` field's accessor becomes a single
    // Datatype ref to `__SeqOf_T`, which Z3 *can* forward-ref via
    // the existing in-batch resolver.
    //
    // From the user's POV nothing changes: source still says
    // `Seq(T)`, the `⟨a, b, c⟩` literal works (build_cons_chain
    // already handles Cons/Nil-shaped enums). The helper enum
    // names start with `__` and are not visible in error
    // messages or self-hosted-pass code.
    let decls_owned: Vec<crate::ast::EnumDecl>;
    let internal_cons_set: std::collections::HashSet<String>;
    let decls: &[crate::ast::EnumDecl] = {
        let (rewritten, set) = generate_internal_cons_helpers(decls);
        if set.is_empty() {
            internal_cons_set = set;
            decls
        } else {
            decls_owned = rewritten;
            internal_cons_set = set;
            &decls_owned
        }
    };
    // batch_names recomputed after possible rewrite (so helper enums
    // are part of the in-batch set for forward-ref resolution).
    let batch_names: std::collections::HashSet<&str> =
        decls.iter().map(|d| d.name.as_str()).collect();

    // Stage decls by Seq-in-payload dependency: an enum X depends on
    // an enum Y if X has any variant field typed `Seq(Y)` and Y is
    // also in this batch. The Array(Int → Y) sort needed to declare
    // the Seq field requires Y's concrete sort to exist already, so
    // X must go in a later stage than Y. Regular Datatype references
    // (`Variant(Y)` without Seq) are still resolved via Z3's in-batch
    // forward-ref machinery.
    let stages = topo_stage_enums(decls, &batch_names, &internal_cons_set)?;

    for stage in stages {
        // Names of enums declared in this stage (for in-stage forward
        // refs via DatatypeAccessor::Datatype).
        let stage_names: std::collections::HashSet<&str> =
            stage.iter().map(|&i| decls[i].name.as_str()).collect();

        let mut builders: Vec<DatatypeBuilder<'static>> = Vec::with_capacity(stage.len());
        for &i in &stage {
            let d = &decls[i];
            let mut builder = DatatypeBuilder::new(ctx, d.name.as_str());
            for v in &d.variants {
                let mut accessors: Vec<(&str, DatatypeAccessor)> = Vec::new();
                // Owned names for two-accessor expansion (`f_arr`,
                // `f_len`) — kept alive via this Vec so the &str
                // pushed into `accessors` outlives the variant build.
                let mut owned_names: Vec<String> = Vec::new();
                for f in &v.fields {
                    if let Some(inner) = parse_seq_type(&f.type_name) {
                        // Internal-Cons backing: `Seq(T)` where T is a
                        // batch-local enum — use a single accessor
                        // pointing to the generated `__SeqOf_T`
                        // helper enum (added to the batch by
                        // `generate_internal_cons_helpers`).
                        if internal_cons_set.contains(inner) {
                            let helper = internal_cons_helper_name(inner);
                            // Helper is in this same stage (we order
                            // it together with T's group), use
                            // forward-ref by name.
                            owned_names.push(helper);
                            let nm_idx = owned_names.len() - 1;
                            let nm: &str = unsafe {
                                &*(owned_names[nm_idx].as_str() as *const str)
                            };
                            accessors.push((f.name.as_str(),
                                DatatypeAccessor::Datatype(nm.into())));
                            continue;
                        }
                        // Two-accessor expansion: Seq(T) becomes
                        // (arr: Array(Int → T), len: Int). Only for
                        // primitives + previously-loaded enums.
                        let elem_sort = resolve_concrete_sort(
                            inner, ctx, &stage_names, registry, &d.name, &v.name)?;
                        if elem_sort.is_none() {
                            return Err(RuntimeError::Parse(format!(
                                "internal: Seq({}) field in `{}::{}` references \
                                 an in-stage enum without an internal-Cons helper",
                                inner, d.name, v.name)));
                        }
                        let arr_sort = Sort::array(ctx, &Sort::int(ctx), &elem_sort.unwrap());
                        owned_names.push(format!("{}_arr", f.name));
                        let arr_name_idx = owned_names.len() - 1;
                        owned_names.push(format!("{}_len", f.name));
                        let len_name_idx = owned_names.len() - 1;
                        let arr_name: &str = unsafe {
                            &*(owned_names[arr_name_idx].as_str() as *const str)
                        };
                        let len_name: &str = unsafe {
                            &*(owned_names[len_name_idx].as_str() as *const str)
                        };
                        accessors.push((arr_name, DatatypeAccessor::Sort(arr_sort)));
                        accessors.push((len_name, DatatypeAccessor::Sort(Sort::int(ctx))));
                        continue;
                    }
                    let acc = match f.type_name.as_str() {
                        "Int" | "Nat" | "Pos" =>
                            DatatypeAccessor::Sort(Sort::int(ctx)),
                        "Bool"   => DatatypeAccessor::Sort(Sort::bool(ctx)),
                        "Real"   => DatatypeAccessor::Sort(Sort::real(ctx)),
                        "String" => DatatypeAccessor::Sort(Sort::string(ctx)),
                        other if stage_names.contains(other) => {
                            // In-stage forward-ref via Z3's resolver.
                            DatatypeAccessor::Datatype(other.into())
                        }
                        other => {
                            // Previously-loaded enum (earlier stage or
                            // earlier load batch). Resolve to concrete.
                            if let Some((prev, _)) = registry.by_name.borrow().get(other) {
                                DatatypeAccessor::Sort(prev.sort.clone())
                            } else {
                                return Err(RuntimeError::Parse(format!(
                                    "unknown payload type `{}` in variant `{}::{}` \
                                     (must be a primitive or a declared enum)",
                                    other, d.name, v.name,
                                )));
                            }
                        }
                    };
                    accessors.push((f.name.as_str(), acc));
                }
                builder = builder.variant(v.name.as_str(), accessors);
                // Drop owned_names at end of variant — the builder
                // has copied its contents (datatype_builder.rs:21
                // does `accessor_name.to_string()`).
                drop(owned_names);
            }
            builders.push(builder);
        }

        let sorts: Vec<DatatypeSort<'static>> =
            z3::datatype_builder::create_datatypes(builders);
        assert_eq!(sorts.len(), stage.len());

        // Stash each built sort + its variant decl list.
        {
            let mut by_name = registry.by_name.borrow_mut();
            let mut by_variant = registry.by_variant.borrow_mut();
            for (&i, dt) in stage.iter().zip(sorts.into_iter()) {
                let d = &decls[i];
                let leaked: &'static DatatypeSort<'static> = Box::leak(Box::new(dt));
                by_name.insert(d.name.clone(), (leaked, d.variants.clone()));
                for (idx, v) in d.variants.iter().enumerate() {
                    by_variant.insert(v.name.clone(), (d.name.clone(), idx));
                }
            }
        }
    }
    Ok(())
}

/// Parse `Seq(T)` → `Some(T)`; otherwise `None`. Used by the enum
/// loader to detect Seq-typed payload fields.
pub(crate) fn parse_seq_type(s: &str) -> Option<&str> {
    if s.starts_with("Seq(") && s.ends_with(')') {
        Some(&s[4..s.len() - 1])
    } else {
        None
    }
}

/// Helper enum name for internal-Cons backing of `Seq(T)`.
/// Convention: `__SeqOf_T`. The underscores prefix marks it as
/// runtime-internal — never written by users, never appears in
/// error messages outside debug contexts.
pub(crate) fn internal_cons_helper_name(t: &str) -> String {
    format!("__SeqOf_{}", t)
}

/// Walk `decls` for `Seq(T)` enum-variant fields where T is also in
/// `decls` (batch-local). For each such T, generate a Cons-shaped
/// helper enum:
///
/// ```text
/// enum __SeqOf_T =
///     __Empty_T
///     __Cell_T(T, __SeqOf_T)
/// ```
///
/// Returns the augmented decl list (original + helpers) and the set
/// of T-names that got helpers. Caller uses the set to route Seq
/// fields through the Cons helper in register_enums.
///
/// When no Seq-of-batch-local fields exist, returns (empty vec,
/// empty set) and the caller uses the original `decls` unchanged.
fn generate_internal_cons_helpers(
    decls: &[crate::ast::EnumDecl],
) -> (Vec<crate::ast::EnumDecl>, std::collections::HashSet<String>) {
    use crate::ast::{EnumDecl, EnumField, EnumVariant};
    let batch_names: std::collections::HashSet<&str> =
        decls.iter().map(|d| d.name.as_str()).collect();
    let mut needs_helper: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for d in decls {
        for v in &d.variants {
            for f in &v.fields {
                if let Some(inner) = parse_seq_type(&f.type_name) {
                    if batch_names.contains(inner) {
                        needs_helper.insert(inner.to_string());
                    }
                }
            }
        }
    }
    if needs_helper.is_empty() {
        return (Vec::new(), needs_helper);
    }
    let mut out: Vec<EnumDecl> = decls.to_vec();
    for t in &needs_helper {
        let helper_name = internal_cons_helper_name(t);
        let empty = EnumVariant {
            name: format!("__Empty_{}", t),
            fields: Vec::new(),
        };
        let cell = EnumVariant {
            name: format!("__Cell_{}", t),
            fields: vec![
                EnumField {
                    name: "head".to_string(),
                    type_name: t.clone(),
                },
                EnumField {
                    name: "tail".to_string(),
                    type_name: helper_name.clone(),
                },
            ],
        };
        out.push(EnumDecl {
            name: helper_name,
            variants: vec![empty, cell],
        });
    }
    (out, needs_helper)
}

/// Resolve a payload element type to a concrete Z3 Sort. Returns
/// `Ok(Some(sort))` for primitives + previously-loaded enums,
/// `Ok(None)` when the type is in the current stage (caller decides
/// how to handle — Seq fields error out, plain Datatype refs use
/// forward-ref). Returns `Err` on unknown types.
fn resolve_concrete_sort<'ctx>(
    type_name: &str,
    ctx: &'ctx z3::Context,
    stage_names: &std::collections::HashSet<&str>,
    registry: &crate::translate::EnumRegistry,
    enclosing_enum: &str,
    enclosing_variant: &str,
) -> Result<Option<z3::Sort<'ctx>>, RuntimeError> {
    use z3::Sort;
    match type_name {
        "Int" | "Nat" | "Pos" => Ok(Some(Sort::int(ctx))),
        "Bool"   => Ok(Some(Sort::bool(ctx))),
        "Real"   => Ok(Some(Sort::real(ctx))),
        "String" => Ok(Some(Sort::string(ctx))),
        other if stage_names.contains(other) => Ok(None),
        other => {
            if let Some((prev, _)) = registry.by_name.borrow().get(other) {
                Ok(Some(prev.sort.clone()))
            } else {
                Err(RuntimeError::Parse(format!(
                    "unknown element type `{}` in Seq payload of `{}::{}` \
                     (must be a primitive or a declared enum)",
                    other, enclosing_enum, enclosing_variant,
                )))
            }
        }
    }
}

/// Partition `decls` into stages. Two kinds of dependencies:
///
///   * **Hard** (regular Datatype payload ref like `EffCons(Effect,
///     EffectList)`): the referenced enum must be in the SAME stage
///     as the referencer, so Z3's batch forward-ref machinery can
///     resolve it. Hard edges are transitive — they merge enums
///     into one stage via union-find.
///   * **Soft** (Seq-in-payload like `FFICall(Int, String, Seq(FFIArg))`):
///     the Seq element's sort must be concrete when the referencer's
///     batch is built. Soft edges order stages: the referencer's
///     group must come AFTER the element type's group.
///
/// Returns a list of stages, each containing indices into `decls`.
/// Errors if Seq-in-payload references form a cycle across hard-edge
/// groups (a single group requiring Seq into itself).
fn topo_stage_enums(
    decls: &[crate::ast::EnumDecl],
    _batch_names: &std::collections::HashSet<&str>,
    internal_cons_set: &std::collections::HashSet<String>,
) -> Result<Vec<Vec<usize>>, RuntimeError> {
    use std::collections::{HashMap, HashSet};

    let n = decls.len();
    let name_to_idx: HashMap<&str, usize> =
        decls.iter().enumerate().map(|(i, d)| (d.name.as_str(), i)).collect();

    // Union-find over enum indices for hard-edge merging.
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while parent[r] != r { r = parent[r]; }
        // Path compression.
        let mut cur = x;
        while parent[cur] != r {
            let next = parent[cur];
            parent[cur] = r;
            cur = next;
        }
        r
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb { parent[ra] = rb; }
    }

    // Walk every variant field; collect hard + soft edges.
    let mut soft: Vec<(usize, usize)> = Vec::new();  // (src_idx, dst_idx) — src needs dst earlier
    for (i, d) in decls.iter().enumerate() {
        for v in &d.variants {
            for f in &v.fields {
                if let Some(inner) = parse_seq_type(&f.type_name) {
                    // Internal-Cons backing: the field becomes a hard
                    // ref to `__SeqOf_T`, NOT a Seq-soft-edge to T.
                    // Without this, the soft-cycle check below would
                    // erroneously reject the mutually-recursive AST
                    // even though the runtime now handles it via the
                    // generated helper.
                    if internal_cons_set.contains(inner) {
                        let helper = internal_cons_helper_name(inner);
                        if let Some(&j) = name_to_idx.get(helper.as_str()) {
                            if j != i { union(&mut parent, i, j); }
                        }
                        continue;
                    }
                    if let Some(&j) = name_to_idx.get(inner) {
                        soft.push((i, j));
                    }
                    continue;
                }
                if let Some(&j) = name_to_idx.get(f.type_name.as_str()) {
                    if j != i {  // self-ref doesn't merge
                        union(&mut parent, i, j);
                    }
                }
            }
        }
    }

    // Group indices by their union-find root.
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }

    // Group-level soft deps.
    let mut group_deps: HashMap<usize, HashSet<usize>> = HashMap::new();
    for &(src, dst) in &soft {
        let rs = find(&mut parent, src);
        let rd = find(&mut parent, dst);
        if rs == rd {
            // Seq inside its own hard-edge group: would need a
            // forward-ref Array sort, which Z3 doesn't support.
            return Err(RuntimeError::Parse(format!(
                "Seq-in-payload references a type in the same hard-edge group: \
                 `{}` has Seq(`{}`) and they're in one mutually-recursive batch",
                decls[src].name, decls[dst].name,
            )));
        }
        group_deps.entry(rs).or_default().insert(rd);
    }

    // Topologically order groups.
    let group_roots: Vec<usize> = groups.keys().copied().collect();
    let mut remaining: Vec<usize> = group_roots.clone();
    let mut built: HashSet<usize> = HashSet::new();
    let mut stages: Vec<Vec<usize>> = Vec::new();
    while !remaining.is_empty() {
        let mut this_round: Vec<usize> = Vec::new();
        let mut next: Vec<usize> = Vec::new();
        for &g in &remaining {
            let deps = group_deps.get(&g);
            let ready = deps.map(|d| d.iter().all(|x| built.contains(x))).unwrap_or(true);
            if ready { this_round.push(g); } else { next.push(g); }
        }
        if this_round.is_empty() {
            let names: Vec<&str> = remaining.iter()
                .flat_map(|g| groups[g].iter().map(|&i| decls[i].name.as_str()))
                .collect();
            return Err(RuntimeError::Parse(format!(
                "circular Seq-in-payload dependency across groups: {:?}", names)));
        }
        for &g in &this_round {
            built.insert(g);
            let mut stage: Vec<usize> = groups[&g].clone();
            stage.sort();
            stages.push(stage);
        }
        remaining = next;
    }
    Ok(stages)
}

pub struct EvidentRuntime {
    program: Program,
    /// Indexed view of program.schemas keyed by name. Mirrors
    /// Python's `EvidentRuntime.schemas`. Used to resolve user-defined
    /// type references during sub-schema expansion.
    schemas: HashMap<String, SchemaDecl>,
    /// Insertion order of `schemas` — used by callers (the multi-FSM
    /// scheduler in particular) that need declaration order rather
    /// than HashMap's nondeterministic key order. New names append;
    /// re-loading an existing name doesn't reorder.
    schema_order: Vec<String>,
    /// Z3 context shared by all cached evaluations from this runtime.
    /// Leaked via Box::leak so its lifetime is `'static`, which lets
    /// us store cached solvers and env entries that borrow from it
    /// without lifetime gymnastics in the public API. The leak is
    /// intentional — one Context per process is fine for a CLI tool
    /// or a test suite. (For long-running embeddings we'd switch to
    /// a Session<'ctx> design — see PROGRESS.md sketch.)
    z3_ctx: &'static Context,
    /// Per-schema cache for `query_cached`. RefCell because we want
    /// `query_cached` to take `&self` (so multiple queries can share
    /// the runtime) while the cache mutates on first access.
    ///
    /// Each entry pairs the cached solver+env with the structural
    /// signature it was built with — the subset of the previous
    /// `given` keyed on names that appear in quantifier bounds. On
    /// the next query, if the signature would be different (i.e. a
    /// structural given changed), we drop the entry and rebuild
    /// against the new given. Non-structural givens (e.g. a player
    /// position used in body arithmetic but not as an unroll bound)
    /// don't trigger a rebuild — `run_cached` just asserts the new
    /// value per-query and Z3 solves with the existing constraint
    /// shape.
    cache: RefCell<HashMap<String, (CachedSchema<'static>, StructuralSignature)>>,
    /// Function-izer cache: maps (schema_name, sorted given-keys) to
    /// either a native substitution chain (Some) or None (claim isn't
    /// fully function-shaped under these inputs — fall through to Z3).
    /// Keyed on given-KEYS, not values, since the chain is the same
    /// shape across runs with different concrete inputs. Populated on
    /// first query per (claim, given-shape) combo. Default ON;
    /// set `EVIDENT_FUNCTIONIZE=0` to disable.
    functionize_cache: RefCell<HashMap<(String, Vec<String>),
                                       Option<crate::functionize::SubstitutionChain>>>,
    /// Per-claim gate-pass cache: name → Some(inlined_schema) if the
    /// claim passes `gate_diagnostics`, None if it's rejected. The
    /// gate result is given-INDEPENDENT (depends only on the body
    /// shape), so it can be cached once per claim regardless of
    /// which given_keys downstream callers use. Pre-populated at
    /// load time so first-tick solves carry zero analysis overhead.
    functionize_gate_cache: RefCell<HashMap<String,
                                            Option<crate::ast::SchemaDecl>>>,
    /// Counter incremented each time a cached entry is rebuilt due
    /// to a structural-signature mismatch. Useful for debugging
    /// performance issues (e.g. "every step is rebuilding — what
    /// structural given is flipping?") and for testing the
    /// invalidation logic.
    cache_rebuilds: RefCell<u64>,
    /// Lazily-built `Z3 DatatypeSort` per user type referenced as the
    /// element of `Seq(UserType)`. Built on first `declare_var`; entries
    /// are `Box::leak`'d to live for `'static` (consistent with the
    /// leaked Context). Shared across `query`, `query_cached`, and
    /// `sample` so a `Seq(Point)` declared in one schema reuses the
    /// same Datatype if another schema references `Point` again — Z3
    /// would otherwise error on duplicate type names.
    datatypes: DatatypeRegistry,
    /// Z3 datatype + variant info for every `enum` declared in loaded
    /// source. Built eagerly at `load_source_with_base` time (one Z3
    /// `DatatypeBuilder` call per enum, with N nullary variants).
    /// Threaded into the translator alongside `datatypes`.
    enums: crate::translate::EnumRegistry,
    /// Stage 3: schemas + enums loaded BEFORE
    /// `mark_system_loads_complete()` was called. Used by the AST
    /// encoder to filter so a self-hosted pass receives only the
    /// user's program, not the pass + stdlib + ast.ev itself.
    /// `None` means no boundary has been drawn — every schema/enum
    /// is "user" (the default for non-self-hosting use cases like
    /// `evident query`).
    system_boundary: RefCell<Option<SystemBoundary>>,
    /// Per-schema source-file tracking: which file each top-level
    /// schema was directly defined in. Schemas pulled in via
    /// `import` chains get the importer's path. Lets the inference
    /// pipeline restrict iteration to "claims defined in the user's
    /// directly-specified file" rather than every transitively
    /// loaded schema — saves substantial time when the user's file
    /// imports a big helper library (mario_shader.ev → engine.ev's
    /// 20+ helper claims).
    schema_origins: RefCell<HashMap<String, PathBuf>>,
    /// Canonicalized paths of every file already loaded via `load_file`
    /// (or transitively via `import`). Used for cycle protection so
    /// `A imports B; B imports A` doesn't recurse forever.
    loaded_files: RefCell<HashSet<PathBuf>>,
    /// Per-schema solve-time history + auto-tuner state. Drives the
    /// dynamic `smt.arith.solver` selection. See `SolveHistory` and
    /// `EvidentRuntime::query_cached` for the pricing protocol.
    solve_history: RefCell<HashMap<String, SolveHistory>>,
}

/// Candidate `smt.arith.solver` values the runtime will try when it
/// hasn't yet committed to one. 2 is the older Simplex-based path that
/// wins on Z3 4.8.12 for our workload; 6 is the newer default that
/// wins for newer Z3 versions and on different schemas. The auto-tuner
/// runs each one for a window of frames and locks in the faster one.
///
/// Add another value here (e.g. `12` if Z3 ever ships a useful new one)
/// and pricing will pick it up automatically.
const ARITH_SOLVER_CANDIDATES: &[u32] = &[2, 6];

/// Number of frames each candidate is timed under during pricing.
/// Long enough to swamp Z3's per-build overhead with steady-state
/// per-frame cost; short enough that pricing finishes well within
/// the warmup window of typical executor sessions.
const PRICING_FRAMES_PER_CANDIDATE: u32 = 30;

/// Per-schema history. Drives the auto-tuner. The state machine:
///
///   Pricing { idx } — currently timing candidate ARITH_SOLVER_CANDIDATES[idx].
///                     After PRICING_FRAMES_PER_CANDIDATE frames the runtime
///                     advances `idx` (rebuilding the cache under the next
///                     candidate). After all candidates are timed, transitions
///                     to Locked under the fastest config seen.
///   Locked { config } — pricing complete. All future queries use this config.
///
/// `EVIDENT_Z3_AUTOTUNE=0` skips pricing entirely and locks immediately
/// to the env-specified `EVIDENT_Z3_ARITH_SOLVER` value (default 2).
struct SolveHistory {
    state: TunerState,
    /// Mean ms/iter observed for each candidate fully priced. Used to
    /// pick the winner when pricing completes.
    measured: HashMap<u32, f64>,
    /// Solve times for the *current* candidate's pricing window. Cleared
    /// every time we advance to the next candidate.
    current_window: Vec<Duration>,
}

#[derive(Debug, Clone, Copy)]
enum TunerState {
    /// Currently timing `ARITH_SOLVER_CANDIDATES[idx]`.
    Pricing { idx: usize },
    /// Pricing complete; this is the winner.
    Locked { config: u32 },
}

impl SolveHistory {
    /// Initial state. If autotune is disabled, lock immediately to the
    /// env-specified config (default 2). Otherwise start pricing with
    /// the first candidate.
    fn new() -> Self {
        let autotune = std::env::var("EVIDENT_Z3_AUTOTUNE").as_deref() != Ok("0");
        if !autotune {
            let initial: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(2);
            return SolveHistory {
                state: TunerState::Locked { config: initial },
                measured: HashMap::new(),
                current_window: Vec::new(),
            };
        }
        SolveHistory {
            state: TunerState::Pricing { idx: 0 },
            measured: HashMap::new(),
            current_window: Vec::with_capacity(PRICING_FRAMES_PER_CANDIDATE as usize),
        }
    }

    /// The `arith_solver` value the cache should be built under right now.
    fn current_config(&self) -> u32 {
        match self.state {
            TunerState::Pricing { idx }     => ARITH_SOLVER_CANDIDATES[idx],
            TunerState::Locked  { config }  => config,
        }
    }

    /// Record a solve time. Returns `Some(new_config)` if the tuner
    /// decided to swap configs (caller should evict the cache so the
    /// next query rebuilds under the new value), `None` otherwise.
    fn record(&mut self, dt: Duration) -> Option<u32> {
        let TunerState::Pricing { idx } = self.state else { return None; };

        self.current_window.push(dt);
        if self.current_window.len() < PRICING_FRAMES_PER_CANDIDATE as usize {
            return None;
        }

        // Window full — finalize this candidate's measurement.
        let total_ms: f64 = self.current_window.iter()
            .map(|d| d.as_secs_f64() * 1000.0).sum();
        let mean_ms = total_ms / self.current_window.len() as f64;
        let cfg = ARITH_SOLVER_CANDIDATES[idx];
        self.measured.insert(cfg, mean_ms);
        self.current_window.clear();

        let next_idx = idx + 1;
        if next_idx < ARITH_SOLVER_CANDIDATES.len() {
            // More candidates to price.
            self.state = TunerState::Pricing { idx: next_idx };
            let next_cfg = ARITH_SOLVER_CANDIDATES[next_idx];
            if std::env::var("EVIDENT_Z3_AUTOTUNE_LOG").as_deref() == Ok("1") {
                eprintln!("[autotune] arith.solver={cfg} → {mean_ms:.2} ms/iter; \
                           probing arith.solver={next_cfg} next");
            }
            Some(next_cfg)
        } else {
            // All candidates priced. Pick the fastest.
            let winner = self.measured.iter()
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(c, _)| *c)
                .unwrap_or(2);
            self.state = TunerState::Locked { config: winner };
            if std::env::var("EVIDENT_Z3_AUTOTUNE_LOG").as_deref() == Ok("1") {
                eprintln!("[autotune] pricing complete: {:?}; locking arith.solver={winner}",
                          self.measured);
            }
            // Return Some only if we need to rebuild cache (i.e. we
            // were timing a different config than the winner).
            if winner != cfg { Some(winner) } else { None }
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
}

#[derive(Debug)]
pub enum RuntimeError {
    Parse(String),
    UnknownSchema(String),
    Io(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RuntimeError::Parse(s) => write!(f, "{}", s),
            RuntimeError::UnknownSchema(s) => write!(f, "unknown schema {:?}", s),
            RuntimeError::Io(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl Default for EvidentRuntime { fn default() -> Self { Self::new() } }

impl EvidentRuntime {
    pub fn new() -> Self {
        let cfg = Config::new();
        let ctx: &'static Context = Box::leak(Box::new(Context::new(&cfg)));
        EvidentRuntime {
            program: Program::default(),
            schemas: HashMap::new(),
            schema_order: Vec::new(),
            z3_ctx: ctx,
            cache: RefCell::new(HashMap::new()),
            functionize_cache: RefCell::new(HashMap::new()),
            functionize_gate_cache: RefCell::new(HashMap::new()),
            cache_rebuilds: RefCell::new(0),
            datatypes: RefCell::new(HashMap::new()),
            enums: crate::translate::EnumRegistry::new(),
            system_boundary: RefCell::new(None),
            schema_origins: RefCell::new(HashMap::new()),
            loaded_files: RefCell::new(HashSet::new()),
            solve_history: RefCell::new(HashMap::new()),
        }
    }

    /// Number of cache rebuilds triggered by structural-signature
    /// mismatches since this runtime was created. Mostly useful for
    /// tests verifying that a change to a non-structural given does
    /// NOT rebuild, and that a change to a structural given DOES.
    /// Also useful as a perf debugging knob — if this counter climbs
    /// every step, you have an unintended structural dependency.
    pub fn cache_rebuilds(&self) -> u64 { *self.cache_rebuilds.borrow() }

    /// Parse and load Evident source. Multiple calls accumulate.
    /// Subclaims (defined inside another claim's body) are also lifted
    /// into the runtime's schemas table so other claims can reference
    /// them by name — same convention as the Python runtime.
    ///
    /// `import "path"` statements are resolved relative to (1) the
    /// path verbatim, then (2) the current working directory. To get
    /// (3) "relative to the file being loaded" resolution, use
    /// `load_file` instead — it tracks the source path and threads it
    /// through.
    pub fn load_source(&mut self, src: &str) -> Result<(), RuntimeError> {
        self.load_source_with_base(src, None)
    }

    /// Load Evident source from a file. Records the file's canonical
    /// path so subsequent `import` statements can resolve relative to
    /// it (and so cycle protection sees the file as already loaded).
    pub fn load_file(&mut self, path: &Path) -> Result<(), RuntimeError> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !self.loaded_files.borrow_mut().insert(canonical.clone()) {
            // Already loaded — cycle / duplicate import. No-op.
            return Ok(());
        }
        let src = std::fs::read_to_string(path)
            .map_err(|e| RuntimeError::Io(format!("read {}: {e}", path.display())))?;
        self.load_source_with_base(&src, Some(&canonical))
    }

    /// Internal entry point that knows the "current file" so it can
    /// resolve relative imports. `base` is None when loading a raw
    /// source string; `Some(path)` when loading from a file.
    fn load_source_with_base(&mut self, src: &str, base: Option<&Path>) -> Result<(), RuntimeError> {
        let prog = parser::parse(src).map_err(|e| RuntimeError::Parse(e.to_string()))?;
        // Process imports first so referenced types/claims exist when
        // the importing file's schemas are registered. This ordering
        // doesn't strictly matter for the runtime (schemas resolve
        // lazily by name) but it matches the textual reading order of
        // the file.
        for import_path in &prog.imports {
            // Known-shimmed stdlib paths (registered with the FTI
            // registry) silently no-op when the file isn't found at
            // the expected location — the registry stands in for the
            // file's contents. See `crate::fti::is_shimmed_stdlib`
            // for the policy and the list itself.
            if crate::fti::is_shimmed_stdlib(import_path) {
                // Try a real resolution first; only no-op if it fails.
                if self.resolve_import(import_path, base).is_err() {
                    continue;
                }
            }
            let resolved = self.resolve_import(import_path, base)?;
            self.load_file(&resolved)?;
        }
        for s in &prog.schemas {
            let mut s = s.clone();
            // Unified-state world syntax: rewrite `_world.X`/`world.X`
            // references into the legacy `world.X`/`world_next.X`
            // pattern so the multi-FSM scheduler's writer detection
            // works without changes. No-op for fsms that already
            // declared `world_next` (legacy pattern stays as is).
            unify_world_syntax(&mut s)?;
            // Flatten Seq concatenations (`a ++ b ++ ⟨…⟩`) into a
            // single SeqLit when all operands resolve to literal
            // sequences. The existing `translate_seq_lit_eq` path
            // handles the result. Recurses into subclaims.
            desugar_seq_concat(&mut s);
            inject_fsm_params(&mut s)?;
            // lhs-eq inference runs BEFORE prev-tick injection so
            // that inferred memberships (e.g., `frame ∈ Int` from
            // `frame = ternary`) are visible when the prev-tick
            // walker resolves `_frame`'s type. Otherwise `_frame`
            // refers to an undeclared name and never gets injected.
            inject_lhs_eq_types(&mut s, &self.schemas, &self.enums);
            inject_prev_tick_decls(&mut s)?;
            // Needs the schemas table — runs against already-loaded
            // claims AND siblings in this same prog batch as they get
            // registered below. Self-reference works because we look
            // up the called claim's signature, not the current claim's.
            inject_claim_arg_types(&mut s, &self.schemas)?;
            enforce_external_only(&s)?;
            if !self.schemas.contains_key(&s.name) {
                self.schema_order.push(s.name.clone());
            }
            self.schemas.insert(s.name.clone(), s.clone());
            register_subclaims(&s.body, &mut self.schemas);
            // Record source file for this schema (and its subclaims).
            // Used by the inference pipeline to skip claims from
            // imported helper files.
            if let Some(path) = base {
                let mut origins = self.schema_origins.borrow_mut();
                origins.insert(s.name.clone(), path.to_path_buf());
                fn record_subclaim_origins(
                    body: &[BodyItem],
                    path: &Path,
                    out: &mut HashMap<String, PathBuf>,
                ) {
                    for item in body {
                        if let BodyItem::SubclaimDecl(s) = item {
                            out.insert(s.name.clone(), path.to_path_buf());
                            record_subclaim_origins(&s.body, path, out);
                        }
                    }
                }
                record_subclaim_origins(&s.body, path, &mut origins);
            }
        }
        // Build all Z3 DatatypeSorts for this batch of enums together
        // via `create_datatypes`. Lets enums forward-reference each
        // other (`Expr` referring to `BinOp` declared later in the
        // file) and be mutually recursive (`A` referring to `B` and
        // vice versa). Variant names must be globally unique across
        // all enums; load fails on collision.
        register_enums(&prog.enums, self.z3_ctx, &self.enums)?;
        self.program.schemas.extend(prog.schemas);
        self.program.enums.extend(prog.enums);

        // After all schemas in this batch are loaded, expand generic
        // type / claim instantiations into monomorphic copies. Each
        // unique `Edge<Rect>` becomes a real schema named "Edge<Rect>"
        // with `T → Rect` substituted throughout its body. Iterates to
        // a fixed point — nested generics resolve in passes.
        monomorphize_generics(&mut self.schemas, &mut self.schema_order)?;
        // Loading new schemas invalidates the cache: new schemas might
        // be referenced by ClaimCall / passthrough in old ones. Also
        // reset the auto-tuner — measurements taken under the old
        // schema body don't apply to the new one.
        self.cache.borrow_mut().clear();
        self.solve_history.borrow_mut().clear();
        // The function-izer's per-claim gate verdicts depend on the
        // currently-loaded schemas (gate's `is_pure_passthrough`
        // walks transitive references). Flush both caches so a
        // re-load doesn't inherit stale verdicts.
        self.functionize_gate_cache.borrow_mut().clear();
        self.functionize_cache.borrow_mut().clear();
        // Pre-classify every loaded claim at load time: run inline
        // + gate once per name so first-tick solves carry zero
        // function-izer analysis cost. Rejections are recorded as
        // `None`; passes store the inlined schema for later chain
        // extraction. The chain itself (per given-shape) is still
        // built lazily on first solve — the cost we hoist here is
        // the gate + inlining pass, which is what was showing up
        // in Mario's regression.
        self.precompile_function_izer();
        // Datatype registry entries reference the previous schema body
        // shape (field order / types). A new load could redefine a type
        // with a different shape; flush so we rebuild on first reference.
        // (The leaked DatatypeSorts themselves stay alive forever, so
        // re-declaring the same name in Z3 will fail — but we have no
        // tests that re-load with a redefined type, so leaving the leak
        // intentional. PROGRESS.md's gotchas section flags this.)
        self.datatypes.borrow_mut().clear();
        Ok(())
    }

    /// Resolve an `import "path"` reference. Tries, in order:
    ///   1. The path verbatim (absolute, or relative to the process
    ///      working directory).
    ///   2. Relative to the file currently being loaded (if any).
    ///   3. Relative to the current working directory (explicitly).
    ///
    /// Returns the first existing path, or an Io error if none match.
    fn resolve_import(&self, import_path: &str, base: Option<&Path>) -> Result<PathBuf, RuntimeError> {
        let p = Path::new(import_path);
        // (1) verbatim
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        // (2) relative to base file's directory
        if let Some(base) = base {
            if let Some(dir) = base.parent() {
                let candidate = dir.join(p);
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
        }
        // (3) relative to current working directory (already covered by
        // (1) for non-absolute paths, but be explicit in case the cwd
        // differs from where the binary was invoked).
        if let Ok(cwd) = std::env::current_dir() {
            let candidate = cwd.join(p);
            if candidate.exists() {
                return Ok(candidate);
            }
        }
        // (4) project-root-relative: programs/sdl_demo/scatter.ev imports
        // "programs/sdl_demo/game_engine.ev" — that's relative to the
        // project root, not the source file. Walk upward from the source
        // file's directory (capped at 10 levels) and try the import path
        // at each ancestor. This also handles `import "packages/sdl.ev"`
        // and similar root-anchored shims when the cwd is somewhere else.
        if let Some(base) = base {
            let mut anc = base.parent();
            for _ in 0..10 {
                let Some(dir) = anc else { break };
                let candidate = dir.join(p);
                if candidate.exists() {
                    return Ok(candidate);
                }
                anc = dir.parent();
            }
        }
        Err(RuntimeError::Io(format!(
            "import not found: {:?} (tried verbatim, relative to source file, cwd, and ancestors of the source file)",
            import_path)))
    }

    /// Pre-compute the function-izer's per-claim gate result for
    /// every loaded schema. Runs at load time so per-tick solves
    /// don't pay the inline + gate cost on the hot path.
    ///
    /// What this DOES populate:
    ///   * `functionize_gate_cache`: name → Some(inlined_schema) if
    ///     the claim passes the gate, None if rejected.
    ///
    /// What this does NOT populate:
    ///   * `functionize_cache`: the per-given-shape chain. That
    ///     still happens lazily because the chain depends on which
    ///     variables are in `given`, which we don't know until the
    ///     caller actually solves.
    ///
    /// Effectively this turns the "first-tick is slow because the
    /// gate has to run" failure mode into a "load-time spends an
    /// extra few ms walking every schema once" — which moves the
    /// cost off the steady-state hot path.
    fn precompile_function_izer(&self) {
        // Build the gate predicates the same way `try_functionize` does.
        let is_enum = |type_name: &str| -> bool {
            self.enums.by_name.borrow().contains_key(type_name)
        };
        fn is_simple_record_rec(
            schemas: &HashMap<String, crate::ast::SchemaDecl>,
            type_name: &str,
            visiting: &mut std::collections::HashSet<String>,
        ) -> bool {
            if matches!(type_name, "Int" | "Real" | "Bool" | "String" | "Nat") { return true; }
            for prefix in &["Seq(", "Set("] {
                if let Some(inner) = type_name.strip_prefix(prefix).and_then(|s| s.strip_suffix(")")) {
                    let inner = inner.trim();
                    if is_simple_record_rec(schemas, inner, visiting) { return true; }
                    if let Some(decl) = schemas.get(inner) {
                        if !matches!(decl.keyword, crate::ast::Keyword::Type) { return false; }
                    }
                    return true;
                }
            }
            let Some(decl) = schemas.get(type_name) else { return false };
            if !matches!(decl.keyword, crate::ast::Keyword::Type) { return false; }
            if decl.external { return true; }
            if !visiting.insert(type_name.to_string()) { return false; }
            let ok = decl.body.iter().all(|item| match item {
                crate::ast::BodyItem::Membership { type_name: ft, .. } =>
                    is_simple_record_rec(schemas, ft, visiting),
                crate::ast::BodyItem::Constraint(_) => false,
                crate::ast::BodyItem::Passthrough(_)
                    | crate::ast::BodyItem::ClaimCall { .. } => false,
                crate::ast::BodyItem::SubclaimDecl(_) => true,
            });
            visiting.remove(type_name);
            ok
        }
        let is_simple_record = |type_name: &str| -> bool {
            let mut visiting = std::collections::HashSet::new();
            is_simple_record_rec(&self.schemas, type_name, &mut visiting)
        };
        fn is_pure_pt(
            schemas: &HashMap<String, crate::ast::SchemaDecl>,
            enums: &crate::translate::EnumRegistry,
            claim_name: &str,
            depth: usize,
        ) -> bool {
            if depth == 0 { return false; }
            let Some(decl) = schemas.get(claim_name) else { return false };
            let is_e = |n: &str| -> bool { enums.by_name.borrow().contains_key(n) };
            let is_r = |n: &str| -> bool {
                let mut v = std::collections::HashSet::new();
                is_simple_record_rec(schemas, n, &mut v)
            };
            let is_p = |n: &str| -> bool { is_pure_pt(schemas, enums, n, depth - 1) };
            crate::functionize::is_pure_assignment_body_xl(decl, &is_e, &is_r, &is_p)
        }
        let is_pure_passthrough = |claim_name: &str| -> bool {
            is_pure_pt(&self.schemas, &self.enums, claim_name, 8)
        };
        let claim_lookup = |name: &str| -> Option<crate::ast::SchemaDecl> {
            self.schemas.get(name).cloned()
        };
        let mut gate_cache = self.functionize_gate_cache.borrow_mut();
        for (name, schema) in &self.schemas {
            // Skip claims we've already analyzed (lazy path may have
            // run first if some pre-load query happened).
            if gate_cache.contains_key(name) { continue; }
            let inlined_body = crate::functionize::inline_positional_calls(
                schema.body.clone(), &claim_lookup);
            let candidate = crate::ast::SchemaDecl {
                body: inlined_body,
                ..schema.clone()
            };
            if crate::functionize::gate_diagnostics(
                &candidate, &is_enum, &is_simple_record, &is_pure_passthrough).is_none()
            {
                gate_cache.insert(name.clone(), Some(candidate));
            } else {
                gate_cache.insert(name.clone(), None);
            }
        }
    }

    /// Evaluate the named schema and return whether it's satisfiable
    /// plus a model. `given` pre-binds variables to concrete values
    /// (mirrors the Python `query(schema, given=...)` parameter).
    pub fn query(&self, name: &str, given: &HashMap<String, Value>) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;

        // Function-izer fast path. When enabled, try to extract and
        // evaluate a substitution chain instead of going through Z3.
        // Falls through cleanly on any miss (claim not function-shaped,
        // chain extraction failed, native eval failed). See
        // `docs/bench/functionize.md` for the design + perf numbers.
        let functionize_on = std::env::var("EVIDENT_FUNCTIONIZE")
            .map(|s| s != "0").unwrap_or(true);
        if functionize_on {
            match self.try_functionize(name, schema, given) {
                Some(result) => {
                    if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                        eprintln!("[fz] HIT {}", name);
                    }
                    return Ok(result);
                }
                None => {
                    if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                        eprintln!("[fz] MISS {}", name);
                    }
                }
            }
        }

        // One-shot query: don't auto-tune (no chance to learn over many
        // calls). Use the env override if set, default 2 (the value
        // that wins on Z3 4.8.12 for our typical workload).
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Fast path: try native substitution-chain evaluation. Returns
    /// `Some(result)` only when every non-given variable in the claim
    /// can be evaluated natively; returns `None` to signal "fall
    /// through to Z3."
    ///
    /// On first call per (claim, given-shape) the chain is extracted
    /// and cached. Subsequent calls hit the cache and only do the
    /// evaluation step (microseconds).
    fn try_functionize(&self, name: &str, schema: &crate::ast::SchemaDecl,
                       given: &HashMap<String, Value>) -> Option<QueryResult>
    {
        use crate::functionize::{evaluate_chain_with_resolvers, extract_chain_xl,
                                 is_pure_assignment_body_xl,
                                 SubstitutionChain};

        // Cache check FIRST — before any analysis (inlining,
        // gate, decomposition). Both outcomes are one-shot per
        // (claim, given-shape):
        //
        //   * Some(chain): try eval. Success → HIT. Failure →
        //     fall through to Z3 this call, but keep the chain
        //     cached for future ticks (data varies; chain shape
        //     doesn't).
        //   * None: cached rejection — gate refused or chain
        //     extraction failed. Skip all analysis, fall through
        //     to Z3.
        //
        // This is the key to keeping the function-izer cheap on
        // claims that will never function-ize (Mario's display,
        // game, level_gen, etc.) — we pay analysis cost once per
        // (claim, given-shape) lifetime, not once per tick.
        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (name.to_string(), given_keys);
        {
            let cache = self.functionize_cache.borrow();
            if let Some(entry) = cache.get(&cache_key) {
                let Some(chain) = entry.as_ref() else { return None; };
                // Build the minimal resolvers needed to evaluate.
                let resolver = |ident: &str| -> Option<Value> {
                    let by_variant = self.enums.by_variant.borrow();
                    let (enum_name, _idx) = by_variant.get(ident)?;
                    let by_name = self.enums.by_name.borrow();
                    let (_, variants) = by_name.get(enum_name)?;
                    let variant = variants.iter().find(|v| v.name == ident)?;
                    if !variant.fields.is_empty() { return None; }
                    Some(Value::Enum {
                        enum_name: enum_name.clone(),
                        variant: ident.to_string(),
                        fields: vec![],
                    })
                };
                let ctor_resolver = |ident: &str, args: &[Value]| -> Option<Value> {
                    let by_variant = self.enums.by_variant.borrow();
                    let (enum_name, _idx) = by_variant.get(ident)?;
                    let by_name = self.enums.by_name.borrow();
                    let (_, variants) = by_name.get(enum_name)?;
                    let variant = variants.iter().find(|v| v.name == ident)?;
                    if variant.fields.len() != args.len() { return None; }
                    Some(Value::Enum {
                        enum_name: enum_name.clone(),
                        variant: ident.to_string(),
                        fields: args.to_vec(),
                    })
                };
                let bindings = evaluate_chain_with_resolvers(
                    chain, given, &resolver, &ctor_resolver)?;
                let mut out = HashMap::new();
                for (k, v) in bindings { out.insert(k, v); }
                return Some(QueryResult { satisfied: true, bindings: out });
            }
        }

        // Enum-aware gate. The native evaluator handles enum-typed
        // memberships via Match dispatch + Value::Enum lookup, so we
        // allow types known to be enums in addition to primitives.
        let is_enum = |type_name: &str| -> bool {
            self.enums.by_name.borrow().contains_key(type_name)
        };
        // User-record type recognizer: a `type` declaration whose
        // body has only primitive Memberships OR Memberships of
        // other simple records. Recursion is allowed with a cycle
        // check via the `visiting` set. The native evaluator
        // handles recursive records because their fields expand to
        // dotted Z3 consts (`box.aabb.pos.x`) which our existing
        // identifier-lookup path resolves.
        fn is_simple_record_rec(
            schemas: &HashMap<String, crate::ast::SchemaDecl>,
            type_name: &str,
            visiting: &mut std::collections::HashSet<String>,
        ) -> bool {
            if matches!(type_name, "Int" | "Real" | "Bool" | "String" | "Nat") { return true; }
            // Seq(T) / Set(T) field types: accept when T is a primitive,
            // a simple record, or any enum (the runtime stores those as
            // Value::SeqEnum and the eval'd chain treats them opaquely
            // when the body doesn't iterate over them).
            for prefix in &["Seq(", "Set("] {
                if let Some(inner) = type_name.strip_prefix(prefix).and_then(|s| s.strip_suffix(")")) {
                    let inner = inner.trim();
                    if is_simple_record_rec(schemas, inner, visiting) { return true; }
                    // Enum element type: deferred to caller (we can't
                    // see the registry from this static function).
                    // Treat any schema whose keyword isn't Type as
                    // potentially-enum (returning true is sound because
                    // the native eval still has to handle the value;
                    // if it can't, evaluate_chain returns None and
                    // rt.query falls through to Z3).
                    if let Some(decl) = schemas.get(inner) {
                        // Non-Type schemas (claims, fsm, etc.) shouldn't
                        // be Seq element types, but if encountered, refuse.
                        if !matches!(decl.keyword, crate::ast::Keyword::Type) {
                            return false;
                        }
                    }
                    // Type we don't know — could be an enum. Accept
                    // conservatively; eval will fail soundly if it can't
                    // handle the value.
                    return true;
                }
            }
            let Some(decl) = schemas.get(type_name) else { return false };
            if !matches!(decl.keyword, crate::ast::Keyword::Type) { return false; }
            // `external type X` — FTI-bridged record. Treat as opaque:
            // its body holds the install Seq + render subclaims, none
            // of which we want to translate. The function-izer only
            // needs to know the leaf-field names; their values flow
            // through `given` from the scheduler's FTI bridge.
            if decl.external { return true; }
            if !visiting.insert(type_name.to_string()) {
                return false;
            }
            let ok = decl.body.iter().all(|item| match item {
                crate::ast::BodyItem::Membership { type_name: ft, .. } => {
                    is_simple_record_rec(schemas, ft, visiting)
                }
                crate::ast::BodyItem::Constraint(_) => false,
                crate::ast::BodyItem::Passthrough(_)
                    | crate::ast::BodyItem::ClaimCall { .. } => false,
                crate::ast::BodyItem::SubclaimDecl(_) => true,
            });
            visiting.remove(type_name);
            ok
        }
        let is_simple_record = |type_name: &str| -> bool {
            let mut visiting = std::collections::HashSet::new();
            is_simple_record_rec(&self.schemas, type_name, &mut visiting)
        };
        // Passthrough predicate: a `..ClaimName` reference is OK iff
        // the referenced claim's body also passes the gate, recursively.
        // We use a fixed depth cap to avoid pathological cycles (none
        // expected in practice — passthroughs form a DAG).
        fn is_pure_pt(
            schemas: &HashMap<String, crate::ast::SchemaDecl>,
            enums: &crate::translate::EnumRegistry,
            claim_name: &str,
            depth: usize,
        ) -> bool {
            if depth == 0 { return false; }
            let Some(decl) = schemas.get(claim_name) else { return false };
            let is_e = |n: &str| -> bool { enums.by_name.borrow().contains_key(n) };
            let is_r = |n: &str| -> bool {
                let mut v = std::collections::HashSet::new();
                is_simple_record_rec(schemas, n, &mut v)
            };
            let is_p = |n: &str| -> bool { is_pure_pt(schemas, enums, n, depth - 1) };
            crate::functionize::is_pure_assignment_body_xl(decl, &is_e, &is_r, &is_p)
        }
        let is_pure_passthrough = |claim_name: &str| -> bool {
            is_pure_pt(&self.schemas, &self.enums, claim_name, 8)
        };
        let passthrough_body = |claim_name: &str| -> Option<Vec<crate::ast::BodyItem>> {
            self.schemas.get(claim_name).map(|s| s.body.clone())
        };
        // Per-claim gate cache. Inlining + gate are given-INDEPENDENT,
        // so we only do them once per claim lifetime. After this
        // block, `schema` points to either:
        //   * the inlined schema (gate passed → continue chain build), or
        //   * `return None` (gate rejected; per-claim and per-shape
        //     caches both record None so future calls skip work).
        let inlined_schema_owned = {
            let gate_cache = self.functionize_gate_cache.borrow();
            gate_cache.get(name).cloned()
        };
        let inlined_schema = match inlined_schema_owned {
            Some(Some(s)) => s,
            Some(None) => {
                // Previously rejected — also remember per-shape so
                // the top-of-function cache check short-circuits.
                self.functionize_cache.borrow_mut().insert(cache_key, None);
                return None;
            }
            None => {
                // First time we've seen this claim. Inline + gate.
                let claim_lookup = |name: &str| -> Option<crate::ast::SchemaDecl> {
                    self.schemas.get(name).cloned()
                };
                let inlined_body = crate::functionize::inline_positional_calls(
                    schema.body.clone(), &claim_lookup);
                let candidate = crate::ast::SchemaDecl {
                    body: inlined_body,
                    ..schema.clone()
                };
                if let Some(why) = crate::functionize::gate_diagnostics(
                    &candidate, &is_enum, &is_simple_record, &is_pure_passthrough)
                {
                    if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                        eprintln!("[fz] {}: rejected by gate ({})", name, why);
                    }
                    self.functionize_gate_cache.borrow_mut()
                        .insert(name.to_string(), None);
                    self.functionize_cache.borrow_mut().insert(cache_key, None);
                    return None;
                }
                self.functionize_gate_cache.borrow_mut()
                    .insert(name.to_string(), Some(candidate.clone()));
                candidate
            }
        };
        let schema = &inlined_schema;

        // Resolver: bare identifiers that aren't in env or given are
        // potentially enum-variant names (`Init`, `Done`, etc.). Look
        // them up in the enum registry and construct nullary
        // Value::Enum on the fly.
        let resolver = |ident: &str| -> Option<Value> {
            let by_variant = self.enums.by_variant.borrow();
            let (enum_name, _idx) = by_variant.get(ident)?;
            // v1: only nullary variants. For variants with payloads,
            // a bare identifier wouldn't be a constructor call anyway
            // — those need `Ctor(args)` syntax which becomes Expr::Call
            // or a different parse, so we wouldn't see them as bare
            // Identifier here. Confirm zero-arity by looking up the
            // variant in by_name.
            let by_name = self.enums.by_name.borrow();
            let (_, variants) = by_name.get(enum_name)?;
            let variant = variants.iter().find(|v| v.name == ident)?;
            if !variant.fields.is_empty() { return None; }
            Some(Value::Enum {
                enum_name: enum_name.clone(),
                variant: ident.to_string(),
                fields: vec![],
            })
        };

        // Ctor resolver: `Println("hello")`, `Exit(0)` etc. The body
        // produces `Expr::Call(name, args)`; we evaluate the args and
        // build a `Value::Enum` with the payload.
        let ctor_resolver = |ident: &str, args: &[Value]| -> Option<Value> {
            let by_variant = self.enums.by_variant.borrow();
            let (enum_name, _idx) = by_variant.get(ident)?;
            let by_name = self.enums.by_name.borrow();
            let (_, variants) = by_name.get(enum_name)?;
            let variant = variants.iter().find(|v| v.name == ident)?;
            if variant.fields.len() != args.len() { return None; }
            Some(Value::Enum {
                enum_name: enum_name.clone(),
                variant: ident.to_string(),
                fields: args.to_vec(),
            })
        };

        // Cache miss already established at function entry. Build the chain.
        //
        // Fast path (Round 12): the gate already vetted that every
        // body Constraint is a pure equality (no Forall, Exists,
        // Implies, top-level Ternary, etc.). Under that constraint,
        // a complete substitution chain (one defining equation per
        // output var, all topo-sortable) is the functional witness
        // — no Z3 2-copy uniqueness check needed. The chain itself
        // PROVES uniqueness: each output gets exactly one expression,
        // expressions only depend on earlier-defined vars and inputs.
        //
        // If `try_extract_one_chain` fails (some output has no
        // defining equation, or there's a cycle), fall through to
        // the Z3-based slow path below.
        let given_keys_set: std::collections::HashSet<&str> = given.keys()
            .map(|s| s.as_str()).collect();
        let is_in_given = |n: &str| -> bool { given.contains_key(n) };
        let is_external_type = |type_name: &str| -> bool {
            self.schemas.get(type_name).map_or(false, |s| s.external)
        };
        let chain = if let Some(mut ch) = crate::functionize::try_extract_one_chain(
            schema, &given_keys_set, &is_enum, &is_simple_record,
            &is_pure_passthrough, &passthrough_body, &is_external_type)
        {
            // Schema-wide consistency checks attach to the chain
            // alongside the merged steps.
            let extra_checks = crate::functionize::extract_schema_wide_checks(
                schema, &is_in_given, &passthrough_body);
            ch.checks.extend(extra_checks);
            ch
        } else {
            // Slow path: classify per-component (Z3), then merge
            // component chains. Used when the one-big-chain extract
            // failed because the body has independent sub-models
            // that the merged-chain approach can't topo-sort.
            let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(2);
            let comps = crate::translate::classify_components(
                schema, given, &self.schemas, self.z3_ctx,
                &self.datatypes, Some(&self.enums), arith);
            if comps.iter().any(|c| !c.functional) {
                if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                    let non: Vec<&str> = comps.iter().filter(|c| !c.functional)
                        .flat_map(|c| c.component.vars.iter().map(|s| s.as_str())).collect();
                    eprintln!("[fz] {}: non-functional components: {:?}", name, non);
                }
                self.functionize_cache.borrow_mut().insert(cache_key, None);
                return None;
            }
            let mut steps = Vec::new();
            for c in &comps {
                match extract_chain_xl(schema, &c.component, &is_enum, &is_simple_record,
                                       &is_pure_passthrough, &passthrough_body) {
                    Some(ch) => steps.extend(ch.steps),
                    None => {
                        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                            eprintln!("[fz] {}: extract_chain failed for vars {:?}",
                                name, c.component.vars);
                        }
                        self.functionize_cache.borrow_mut().insert(cache_key, None);
                        return None;
                    }
                }
            }
            let checks = crate::functionize::extract_schema_wide_checks(
                schema, &is_in_given, &passthrough_body);
            SubstitutionChain { steps, checks }
        };
        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
            eprintln!("[fz] {}: chain has {} steps: {:?}", name, chain.steps.len(),
                chain.steps.iter().map(|s| &s.var).collect::<Vec<_>>());
        }
        // Evaluate once to populate bindings. If eval fails on
        // THIS call (e.g. tick 0 has empty last_results so a
        // `match last_results[i] …` returns None), still cache the
        // chain so later ticks — with realistic last_results — can
        // hit it. Only the data flow varies per tick; the chain
        // structure is stable.
        let bindings_opt = evaluate_chain_with_resolvers(
            &chain, given, &resolver, &ctor_resolver);
        self.functionize_cache.borrow_mut().insert(cache_key, Some(chain));
        let bindings = match bindings_opt {
            Some(b) => b,
            None => {
                if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                    eprintln!("[fz] {}: eval failed (chain cached for later ticks)", name);
                }
                return None;
            }
        };
        let mut out = HashMap::new();
        for (k, v) in bindings { out.insert(k, v); }
        Some(QueryResult { satisfied: true, bindings: out })
    }

    /// Structural decomposition pass: re-separate the named claim into
    /// the independent sub-models it was composed from. Returns a list
    /// of `Component`s, each holding the variable names in that
    /// independent piece. See `docs/design/compile-claims-to-functions.md`
    /// ("Decomposition") for the architectural framing.
    pub fn analyze_decomposition(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<Vec<crate::decompose::Component>, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        Ok(crate::translate::analyze_decomposition(
            schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith))
    }

    /// Decomposition + per-component functionality verdict via the
    /// 2-copy uniqueness check. Returns a list of `ClassifiedComponent`s
    /// flagging which components are function-shaped (outputs uniquely
    /// determined by inputs) vs search-shaped. Cost: roughly 1+N Z3
    /// calls (the initial solve plus one check per component); each
    /// component-level check is small.
    pub fn classify_components(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<Vec<crate::translate::ClassifiedComponent>, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        Ok(crate::translate::classify_components(
            schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith))
    }

    /// Like `query`, but on UNSAT also returns the unsat-core: indices
    /// into the schema's `body` for the constraints Z3 identified as
    /// the conflicting subset. Used by `evident test` to highlight
    /// which assertions made a `sat_*` test fail. Givens are not
    /// tracked — the core only includes schema body items.
    pub fn query_with_core(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<(QueryResult, Option<Vec<usize>>), RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate_with_core(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        let qr = QueryResult { satisfied: r.satisfied, bindings: r.bindings };
        Ok((qr, r.unsat_core_items))
    }

    /// Convenience: query without any pre-bound values.
    pub fn query_free(&self, name: &str) -> Result<QueryResult, RuntimeError> {
        self.query(name, &HashMap::new())
    }

    /// Iterator over the names of every loaded schema (top-level decls
    /// AND lifted subclaims). Useful for tooling.
    pub fn schema_names(&self) -> impl Iterator<Item = &str> {
        self.schema_order.iter().map(|s| s.as_str())
    }

    /// Look up a loaded schema by name. Used by the executor (and other
    /// tooling) to inspect the body of `main` for variable declarations,
    /// passthroughs, and state pairs.
    pub fn get_schema(&self, name: &str) -> Option<&SchemaDecl> {
        self.schemas.get(name)
    }

    /// Inject a `Membership` body item at the head of the named claim.
    /// Used by the `--infer-types` flag pipeline: after running the
    /// self-hosted inference passes against a separate runtime, the
    /// query path calls this to graft the inferred declarations onto
    /// the user's claims before solving.
    ///
    /// Returns `Ok(true)` if a Membership was added, `Ok(false)` if
    /// the variable was already declared in the claim's body (the
    /// idempotent skip lets callers loop over inferences without
    /// double-checking). `Err(UnknownSchema)` if the named claim
    /// doesn't exist.
    ///
    /// Mutates both `self.schemas` (the lookup table) and
    /// `self.program.schemas` (the parsed Program — for encoder
    /// consistency on subsequent calls). Clears the cache so a
    /// re-query rebuilds with the new shape.
    pub fn add_membership_to_claim(
        &mut self,
        claim_name: &str,
        var_name: &str,
        type_name: &str,
    ) -> Result<bool, RuntimeError> {
        use crate::ast::{BodyItem, Pins};
        let already_declared = |body: &[BodyItem]| -> bool {
            body.iter().any(|i| matches!(
                i, BodyItem::Membership { name, .. } if name == var_name
            ))
        };
        let new_item = BodyItem::Membership {
            name: var_name.to_string(),
            type_name: type_name.to_string(),
            pins: Pins::None,
        };
        // Update the lookup table.
        let schema = self.schemas.get_mut(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        if already_declared(&schema.body) {
            return Ok(false);
        }
        schema.body.insert(0, new_item.clone());
        // Mirror in self.program.schemas so the encoder sees the same
        // body shape on subsequent queries.
        for s in &mut self.program.schemas {
            if s.name == claim_name && !already_declared(&s.body) {
                s.body.insert(0, new_item.clone());
            }
        }
        // Cached solver still has the old body asserted; flush.
        self.cache.borrow_mut().clear();
        Ok(true)
    }

    /// Stage 3: snapshot everything currently loaded as "system"
    /// (stdlib/ast.ev, the pass file, etc.). Subsequent `load_*`
    /// calls register schemas/enums as user-side. `encode_program_value`
    /// and `query_with_program` then encode only the user's program,
    /// not the system layer — so a self-hosted pass sees exactly what
    /// the user wrote.
    ///
    /// Idempotent: calling twice replaces the boundary with the
    /// current state. (The earlier snapshot is lost, but in practice
    /// you set the boundary once between system and user loads.)
    pub fn mark_system_loads_complete(&self) {
        let schemas: HashSet<String> = self.schemas.keys().cloned().collect();
        let enums: HashSet<String> = self.enums.by_name.borrow().keys().cloned().collect();
        *self.system_boundary.borrow_mut() = Some(SystemBoundary { schemas, enums });
    }

    /// Return a `Program` view containing only schemas/enums loaded
    /// AFTER `mark_system_loads_complete()` was called. If no
    /// boundary has been drawn, returns the full program (no
    /// filtering — matches existing `encode_program_value` semantics).
    fn user_program(&self) -> Program {
        let boundary = self.system_boundary.borrow();
        let Some(b) = boundary.as_ref() else { return self.program.clone() };
        Program {
            schemas: self.program.schemas.iter()
                .filter(|s| !b.schemas.contains(&s.name))
                .cloned().collect(),
            enums: self.program.enums.iter()
                .filter(|e| !b.enums.contains(&e.name))
                .cloned().collect(),
            imports: Vec::new(),
        }
    }

    /// Encode this runtime's accumulated `Program` as a Z3 Datatype
    /// value matching `stdlib/ast.ev`'s `Program` enum. Caller is
    /// expected to have loaded `stdlib/ast.ev` first; if any AST
    /// enum is missing from the registry, `encode_program` returns
    /// `EnumNotRegistered`.
    ///
    /// Used by `evident dump-ast` and (in Stage 3) by the CLI hooks
    /// that hand a parsed Program to a self-hosted pass as a `given`.
    pub fn encode_program_value(
        &self,
    ) -> std::result::Result<z3::ast::Datatype<'static>,
                              crate::translate::ast_encoder::EncodeError> {
        let prog = self.user_program();
        crate::translate::ast_encoder::encode_program(
            &prog,
            self.z3_ctx,
            &self.enums,
        )
    }

    /// Return a clone of the user-side `Program` AST (everything
    /// loaded after `mark_system_loads_complete()`). When the system
    /// boundary hasn't been drawn, returns the full program — same
    /// semantics as `encode_program_value`.
    ///
    /// Used by the reflection world-plugin to build a `Value::Enum`
    /// tree without having to construct Z3 datatype values. Also
    /// useful for any future consumer that wants the raw AST shape
    /// (lints walking the program, custom encoders, etc.).
    pub fn program_ast(&self) -> Program {
        self.user_program()
    }

    /// Stage 5.5 plumbing: like `query_with_program`, but ALSO
    /// injects the user's first claim's body as a `Seq(BodyItem)`
    /// for the named seq variable. Lets a self-hosted pass iterate
    /// over arbitrary-length user programs via `∀ i ∈ {0..#body-1} : …`.
    ///
    /// The user's "first claim" is `user_program().schemas[0]` — the
    /// first user-loaded schema after `mark_system_loads_complete()`.
    /// If the user has no schemas, `body_var` is constrained to
    /// length 0; the pass can detect this via `#body = 0`.
    ///
    /// `program_var` and `body_var` must both be declared in the
    /// pass schema (`program ∈ Program` and `body ∈ Seq(BodyItem)`,
    /// typically). Passes can use either or both — having `body`
    /// makes iteration possible without recursing through the
    /// `BodyItemList` linked-list shape.
    /// Stage 8: like `query_with_program_and_body` but lets the
    /// caller pick which user claim's body to inject. Index is into
    /// `user_program().schemas` (the user-loaded subset). Returns
    /// `None` if `claim_idx` is out of range. Lets the CLI iterate
    /// over every user claim and aggregate per-claim inferences.
    pub fn query_with_program_and_nth_claim_body(
        &self,
        claim_name: &str,
        program_var: &str,
        body_var: &str,
        claim_idx: usize,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        let prog_value = self.encode_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode failed: {e}")))?;
        self.query_with_program_and_nth_claim_body_value(
            claim_name, program_var, body_var, claim_idx, prog_value,
        )
    }

    /// Variant of `query_with_program_and_nth_claim_body` that skips
    /// the encoded-Program injection. Most iter-style rules
    /// (`iter_types.ev`, `propagation.ev`, `consistency.ev`,
    /// `lint_duplicate_decls.ev`) declare `program ∈ Program` but
    /// never reference it — they only iterate over `body`. Skipping
    /// the encoded-Program assertion eliminates the dominant Z3 cost
    /// (asserting an equality against a deep recursive datatype
    /// value), which on big programs like mario_shader is several
    /// seconds of solver time.
    ///
    /// Returns `Ok(None)` for out-of-range claim_idx, same as the
    /// program+body variant.
    pub fn query_with_nth_claim_body_only(
        &self,
        claim_name: &str,
        body_var: &str,
        claim_idx: usize,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        // Pass an empty Program value as the program injection.
        // Cheap to construct (no recursive walk); the rule's
        // `program ∈ Program` declaration just gets bound to the
        // empty program, which is harmless because the rule never
        // references it.
        let empty_prog = self.encode_empty_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode empty program: {e}")))?;
        // Reuse the existing implementation with the cheap value.
        // The "program_var" name doesn't have to match a declared var —
        // if it does, it gets bound to empty; if not, the runtime
        // warns and continues.
        self.query_with_program_and_nth_claim_body_value(
            claim_name, "program", body_var, claim_idx, empty_prog,
        )
    }

    /// Build a trivial `MakeProgram(SchLNil, EDLNil)` Z3 Datatype
    /// value. Used by `query_with_nth_claim_body_only` to satisfy
    /// the program-var assertion without paying the recursive-walk
    /// cost on the user's full AST.
    fn encode_empty_program_value(
        &self,
    ) -> std::result::Result<z3::ast::Datatype<'static>,
                              crate::translate::ast_encoder::EncodeError> {
        let empty = Program::default();
        crate::translate::ast_encoder::encode_program(
            &empty, self.z3_ctx, &self.enums,
        )
    }

    /// Same as `query_with_program_and_nth_claim_body` but takes the
    /// encoded `Program` value directly. Pair with
    /// `query_with_program_value` for the inference-pipeline use case
    /// where one encoded value feeds many rule queries.
    pub fn query_with_program_and_nth_claim_body_value(
        &self,
        claim_name: &str,
        program_var: &str,
        body_var: &str,
        claim_idx: usize,
        program_value: z3::ast::Datatype<'static>,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let user = self.user_program();
        let Some(target_claim) = user.schemas.get(claim_idx) else {
            return Ok(None);
        };
        let body_items = &target_claim.body;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("body_len".to_string(), Value::Int(body_items.len() as i64));
        let r = crate::translate::evaluate_with_program_and_body(
            schema, &given, &self.schemas, self.z3_ctx,
            &self.datatypes, &self.enums, arith,
            program_var, program_value,
            body_var, body_items,
        );
        Ok(Some(QueryResult { satisfied: r.satisfied, bindings: r.bindings }))
    }

    /// Body-only query variant that accepts an extra `given` map for
    /// caller-pinned variables (e.g. `target_idx → 3`). Same cheap
    /// empty-Program injection as `query_with_nth_claim_body_only`.
    /// Used by the desugar pipeline to ask "is body[i] of shape X?"
    /// one index at a time.
    pub fn query_with_nth_claim_body_only_given(
        &self,
        claim_name: &str,
        body_var: &str,
        claim_idx: usize,
        extra_given: HashMap<String, Value>,
    ) -> Result<Option<QueryResult>, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let user = self.user_program();
        let Some(target_claim) = user.schemas.get(claim_idx) else {
            return Ok(None);
        };
        let body_items = &target_claim.body;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let mut given: HashMap<String, Value> = extra_given;
        given.insert("body_len".to_string(), Value::Int(body_items.len() as i64));
        let empty_prog = self.encode_empty_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode empty program: {e}")))?;
        let r = crate::translate::evaluate_with_program_and_body(
            schema, &given, &self.schemas, self.z3_ctx,
            &self.datatypes, &self.enums, arith,
            "program", empty_prog,
            body_var, body_items,
        );
        Ok(Some(QueryResult { satisfied: r.satisfied, bindings: r.bindings }))
    }

    /// Replace `body[body_idx]` of the named claim with `new_item`.
    /// Mirrors `add_membership_to_claim`'s dual-update pattern so
    /// both the schemas lookup and the encoder see the rewrite.
    pub fn replace_body_item_in_claim(
        &mut self,
        claim_name: &str,
        body_idx: usize,
        new_item: crate::ast::BodyItem,
    ) -> Result<bool, RuntimeError> {
        let schema = self.schemas.get_mut(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        if body_idx >= schema.body.len() { return Ok(false); }
        schema.body[body_idx] = new_item.clone();
        for s in &mut self.program.schemas {
            if s.name == claim_name && body_idx < s.body.len() {
                s.body[body_idx] = new_item.clone();
            }
        }
        self.cache.borrow_mut().clear();
        Ok(true)
    }

    /// Number of claims the user has loaded (after
    /// `mark_system_loads_complete`). Used by callers iterating over
    /// claims with `query_with_program_and_nth_claim_body`.
    pub fn user_claim_count(&self) -> usize {
        self.user_program().schemas.len()
    }

    /// Name of the n-th user claim, if any. Used by the CLI to
    /// label per-claim inference output.
    pub fn user_claim_name(&self, idx: usize) -> Option<String> {
        self.user_program().schemas.get(idx).map(|s| s.name.clone())
    }

    /// Body length of the n-th user claim. Used by the desugar
    /// pipeline to bound the index loop over `body[i]` queries.
    pub fn user_claim_body_len(&self, idx: usize) -> Option<usize> {
        self.user_program().schemas.get(idx).map(|s| s.body.len())
    }

    /// Indices into `user_program().schemas` for claims directly
    /// defined in `path` (not pulled in via `import`). Used by the
    /// inference pipeline to skip helper claims from imported
    /// libraries — for `mario_shader.ev` (which imports `engine.ev`
    /// and `level_data.ev` adding 20+ helper claims), this cuts
    /// per-claim iteration from 26 schemas to typically 1-3.
    ///
    /// Returns indices in the same order as `user_program().schemas`.
    /// Falls back to all user-claim indices if the runtime has no
    /// origin tracking for `path` (which can happen with
    /// `load_source` instead of `load_file`).
    pub fn user_claim_indices_in_file(&self, path: &Path) -> Vec<usize> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let origins = self.schema_origins.borrow();
        let mut out = Vec::new();
        let user = self.user_program();
        // If we have NO origins recorded for this path, the file
        // likely wasn't loaded via load_file (e.g. tests use
        // load_source). Fall back to all user claims.
        let has_any = origins.values().any(|p| *p == canonical);
        if !has_any {
            return (0..user.schemas.len()).collect();
        }
        for (i, s) in user.schemas.iter().enumerate() {
            if let Some(origin) = origins.get(&s.name) {
                if *origin == canonical {
                    out.push(i);
                }
            }
        }
        out
    }

    pub fn query_with_program_and_body(
        &self,
        claim_name: &str,
        program_var: &str,
        body_var: &str,
    ) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let user = self.user_program();
        let prog_value = crate::translate::ast_encoder::encode_program(
            &user, self.z3_ctx, &self.enums,
        ).map_err(|e| RuntimeError::Parse(format!("encode failed: {e}")))?;
        let body_items: Vec<crate::ast::BodyItem> = user.schemas.first()
            .map(|s| s.body.clone())
            .unwrap_or_default();
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        // Inject body length as a `given` Int so the literal-int +
        // seq-length pre-passes can pin any `body_len ∈ Nat` /
        // `n = #body` references for quantifier unrolling. The
        // convention: pass `body_len` as the variable name; passes
        // declare it themselves and use it as the upper bound of
        // `∀ i ∈ {0..body_len - 1} : …`.
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("body_len".to_string(), Value::Int(body_items.len() as i64));
        let r = crate::translate::evaluate_with_program_and_body(
            schema, &given, &self.schemas, self.z3_ctx,
            &self.datatypes, &self.enums, arith,
            program_var, prog_value,
            body_var, &body_items,
        );
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
    /// accumulated `Program` injected as a `given` for one of the
    /// pass's variables.
    ///
    /// Concretely: encode the program as a Z3 Datatype value matching
    /// `stdlib/ast.ev`'s `Program` enum, then evaluate `claim_name`
    /// while asserting that the variable named `program_var` (declared
    /// as `program ∈ Program` in the pass) equals that value. Any
    /// other free variables in the pass behave normally — Z3 picks
    /// values that satisfy the pass's constraints.
    ///
    /// Returns `RuntimeError::Encode` if `stdlib/ast.ev` isn't
    /// loaded; `UnknownSchema` if the named claim doesn't exist.
    pub fn query_with_program(
        &self,
        claim_name: &str,
        program_var: &str,
    ) -> Result<QueryResult, RuntimeError> {
        let prog_value = self.encode_program_value()
            .map_err(|e| RuntimeError::Parse(format!("encode failed: {e}")))?;
        self.query_with_program_value(claim_name, program_var, prog_value)
    }

    /// Same as `query_with_program` but takes the encoded `Program`
    /// value directly. Lets callers running many rules over the same
    /// program (like the inference pipeline) encode once and reuse,
    /// avoiding the recursive-AST walk on every rule. Saves ~70-85%
    /// of the per-rule cost on big programs.

    /// Pin one or more enum-typed (Datatype) variables across a
    /// single query. Each entry of `pins` is `(var_name, value)`.
    /// Used by the multi-FSM scheduler to fix `state` and
    /// `last_results` per tick — see the "execution-layer
    /// extension surface" section in the module docs.
    pub fn query_with_pinned_datatypes(
        &self,
        claim_name: &str,
        pins: &[(&str, z3::ast::Datatype<'static>)],
    ) -> Result<QueryResult, RuntimeError> {
        self.query_with_pins_and_given(claim_name, pins, &HashMap::new())
    }

    /// Like `query_with_pinned_datatypes` but also accepts a
    /// `given` map for scalar pins (Int/Bool/String/Real values).
    /// Used by the multi-FSM scheduler to thread `world_next.*`
    /// writer values into reader `world.*` slots within the same
    /// tick — see the "execution-layer extension surface"
    /// section in the module docs.
    pub fn query_with_pins_and_given(
        &self,
        claim_name: &str,
        pins: &[(&str, z3::ast::Datatype<'static>)],
        given: &HashMap<String, Value>,
    ) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        // Function-izer fast path on the SCHEDULER side. The
        // scheduler passes realistic per-tick given values (state,
        // last_results, _world.X). State-pair FSMs ALSO get a
        // `pins` array with the state pinned as a Z3 Datatype —
        // we used to bail in that case, but the scheduler now also
        // surfaces the state's Value form in `given` (see
        // `effect_loop.rs::run_with_ctx` around the
        // `current_state_v` insertion). So the function-izer can
        // fire even with non-empty pins; the pinned Datatype is
        // simply redundant with the given Value. If function-izer
        // rejects, fall through to Z3 with `pins` intact.
        let functionize_on = std::env::var("EVIDENT_FUNCTIONIZE")
            .map(|s| s != "0").unwrap_or(true);
        if functionize_on {
            if let Some(result) = self.try_functionize(claim_name, schema, given) {
                if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                    eprintln!("[fz] HIT (scheduler) {}", claim_name);
                }
                return Ok(result);
            }
        }
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate_with_extra_assertions(
            schema,
            given,
            &self.schemas,
            self.z3_ctx,
            &self.datatypes,
            Some(&self.enums),
            arith,
            pins,
        );
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Read-only access to the EnumRegistry. Execution-layer
    /// callers use this to look up DatatypeSorts when re-encoding
    /// values for subsequent solves — see the "execution-layer
    /// extension surface" section in the module docs.
    pub fn enums_registry(&self) -> &crate::translate::EnumRegistry {
        &self.enums
    }

    /// The `'static` Z3 context this runtime allocates against.
    /// Execution-layer callers need this when constructing
    /// Datatype values (e.g. an enum constructor application)
    /// for subsequent pins — see the "execution-layer extension
    /// surface" section in the module docs.
    pub fn z3_context(&self) -> &'static z3::Context {
        self.z3_ctx
    }

    /// Read-only access to the DatatypeRegistry. Used by the
    /// Z3-AST functionizer pipeline to build cached schemas.
    pub fn datatypes_registry(&self) -> &crate::translate::DatatypeRegistry {
        &self.datatypes
    }

    /// Read-only access to the loaded schemas map.
    pub fn schemas_map(&self) -> &HashMap<String, SchemaDecl> {
        &self.schemas
    }

    /// Build a `Value::SeqEnum` of `Result` enums. Used by the
    /// multi-FSM scheduler to pin `last_results ∈ Seq(Result)`
    /// via the `given` map (`assert_seq_given` handles the
    /// `(DatatypeSeqVar, SeqEnum)` pair).
    pub fn effect_results_to_value(
        &self,
        items: &[crate::ast::EffectResult],
    ) -> crate::translate::Value {
        crate::translate::ast_encoder::effect_results_to_value(items)
    }

    pub fn query_with_program_value(
        &self,
        claim_name: &str,
        program_var: &str,
        program_value: z3::ast::Datatype<'static>,
    ) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate_with_extra_assertion(
            schema,
            &HashMap::new(),
            &self.schemas,
            self.z3_ctx,
            &self.datatypes,
            Some(&self.enums),
            arith,
            program_var,
            program_value,
        );
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Faster query — translates the schema once on first call and
    /// reuses the resulting Z3 solver across subsequent calls
    /// (push/pop per query). Mirrors Python's `query(name, given,
    /// cached=True)` and the `evaluate_cached` optimization.
    ///
    /// **Structural-signature invalidation.** The cache stores the
    /// subset of the previous `given` keyed on names that appear in
    /// quantifier bounds — the structural signature. If this query's
    /// signature differs (e.g. a config value that drives an unroll
    /// count just changed), the cache is dropped and rebuilt against
    /// the new given. Non-structural changes (player position, etc.)
    /// reuse the cache and just re-assert the new value per-query.
    ///
    /// Bindings, satisfaction result, and overall semantics are
    /// identical to `query()`. Faster when called many times against
    /// the same schema with mostly-stable structural givens (e.g. an
    /// executor stepping a state machine 60×/sec where lengths and
    /// bound names don't change).
    pub fn query_cached(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<QueryResult, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?
            .clone();   // cheap: SchemaDecl is small + Arc-friendly clones
        let cur_sig = structural_signature(&schema.body, given);

        // Auto-tuner: which arith.solver should the cache use right now?
        let arith_solver = {
            let mut hist = self.solve_history.borrow_mut();
            hist.entry(name.to_string()).or_insert_with(SolveHistory::new)
                .current_config()
        };

        let mut cache = self.cache.borrow_mut();
        // Rebuild if (a) no entry, (b) structural signature changed, or
        // (c) cached config doesn't match the auto-tuner's current pick.
        let needs_rebuild = match cache.get(name) {
            Some((cached, cached_sig)) =>
                cached_sig != &cur_sig || cached.arith_solver != arith_solver,
            None => true,
        };
        if needs_rebuild {
            if cache.contains_key(name) {
                *self.cache_rebuilds.borrow_mut() += 1;
            }
            let names = crate::translate::structural_names(&schema.body);
            let structural_given: HashMap<String, Value> = given.iter()
                .filter(|(k, _)| names.contains(k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let new_cached = build_cache(
                &schema, &self.schemas, self.z3_ctx, &self.datatypes,
                Some(&self.enums), &structural_given, arith_solver);
            cache.insert(name.to_string(), (new_cached, cur_sig));
        }
        let entry = cache.get(name).unwrap();

        // Time the actual solve so the auto-tuner can decide whether to
        // advance to the next pricing window.
        let t0 = Instant::now();
        let r = run_cached(&entry.0, given, self.z3_ctx, Some(&self.enums));
        let dt = t0.elapsed();
        drop(cache);  // release before we may invalidate below

        // Record the timing. If the tuner says to switch configs,
        // evict so the next call rebuilds under the new value.
        if let Some(_new_cfg) = self.solve_history.borrow_mut()
            .get_mut(name).and_then(|h| h.record(dt))
        {
            self.cache.borrow_mut().remove(name);
        }
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Return up to `n` distinct satisfying models. Uses the cached
    /// solver: one push for the per-query givens, then accumulating
    /// blocking clauses (¬(b1=v1 ∧ … ∧ bn=vn) for each scalar binding)
    /// across iterations until either `n` distinct models or UNSAT.
    /// All blocking clauses + givens are popped before returning so the
    /// cached solver is unchanged from the caller's perspective.
    ///
    /// Limitation (v1): blocking only covers Bool, Int, Str bindings.
    /// Seq/Set values are skipped from the blocking conjunction, so
    /// schemas whose only varying outputs are sequences will return
    /// duplicates. See `sample_cached_inner` in translate.rs.
    pub fn sample(&self, name: &str, given: &HashMap<String, Value>, n: usize)
        -> Result<Vec<HashMap<String, Value>>, RuntimeError>
    {
        let schema = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?
            .clone();
        // Sample uses its own fresh, non-shared cached solver. Two reasons:
        //   1. `arith.solver=2` (the runtime's per-frame default and a
        //      candidate in the auto-tuner) is pathologically slow on
        //      sample_cached_inner's cumulative blocking-clause workload.
        //   2. The blocking clauses asserted inside sample's outer push
        //      shouldn't influence the per-frame solver state that the
        //      auto-tuner is timing.
        // Sample is rare and amortizes the build_cache cost across N
        // models, so the lack of cross-call caching is acceptable.
        let names = crate::translate::structural_names(&schema.body);
        let structural_given: HashMap<String, Value> = given.iter()
            .filter(|(k, _)| names.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        // Sample's "safe" config: leave Z3 at its default arith path.
        // 0 means "don't call set_params". Empirically this avoids the
        // solver=2 blocking-clause pathology.
        let cached = build_cache(
            &schema, &self.schemas, self.z3_ctx, &self.datatypes,
            Some(&self.enums), &structural_given, 0);
        Ok(sample_cached_inner(&cached, given, n, self.z3_ctx, Some(&self.enums)))
    }
}
