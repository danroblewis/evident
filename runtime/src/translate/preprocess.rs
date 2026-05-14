//! Pre-translation passes: pin literal-int variables, propagate
//! sequence lengths, fold quantifier bounds. All of these run before
//! constraint translation so the translator sees concrete integers
//! where possible (and can then unroll quantifiers, fold Cardinality,
//! etc.).
//!
//! Pure data-shape passes — input AST + small `Value` map → updated
//! `Value` / env map. No Z3 expression building, no `&Context`,
//! no Solver. The two pieces that DID need a Context (`literal_range`,
//! which evaluates `Range(lo, hi)` bounds, and `apply_seq_lengths`,
//! which substitutes a literal Int into typed Seq bindings) live in
//! their proper homes — `exprs::literal_range` and
//! `declare::apply_seq_lengths`.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use super::types::{Value, Var};

/// Walk the schema body to find every name that appears in a
/// quantifier bound (the `range` of a `∀` / `∃`). Those names are
/// "structural" — changing their value changes how many iterations
/// the quantifier unrolls into, which means the cached constraint
/// set built from the previous value is wrong. Used by the runtime's
/// cache-invalidation logic: when a `given` value for a structural
/// name changes between steps, rebuild the cache; otherwise reuse it.
///
/// Names appear in bounds via either:
///   - Direct: `∀ i ∈ {0..n - 1}` → `n` is structural.
///   - Cardinality: `∀ i ∈ {0..#s - 1}` → `s` is structural (changing
///     `#s`'s pinned value via a `Seq` given changes the unroll).
///
/// Pure value-only givens (e.g. player position used in body
/// arithmetic but never as a quantifier bound) are NOT structural —
/// the constraint structure is the same regardless of their value,
/// and `run_cached` asserts them per-query without rebuilding.
pub fn structural_names(body: &[BodyItem]) -> HashSet<String> {
    let mut out = HashSet::new();
    for item in body {
        if let BodyItem::Constraint(e) = item {
            walk_for_quantifier_bounds(e, &mut out);
        }
        // ClaimCall / Passthrough / SubclaimDecl bodies aren't walked
        // — they live in other schemas and their bounds typically only
        // reference the claim's own internal vars, not top-level
        // givens. If a real cross-claim case shows up, walk
        // schemas[claim_name].body here too.
    }
    out
}

fn walk_for_quantifier_bounds(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) => {
            collect_referenced_names(range, out);
            walk_for_quantifier_bounds(body, out);
        }
        Expr::Binary(_, lhs, rhs) => {
            walk_for_quantifier_bounds(lhs, out);
            walk_for_quantifier_bounds(rhs, out);
        }
        Expr::Not(inner) => walk_for_quantifier_bounds(inner, out),
        Expr::InExpr(lhs, rhs) => {
            walk_for_quantifier_bounds(lhs, out);
            walk_for_quantifier_bounds(rhs, out);
        }
        _ => {}
    }
}

pub fn collect_referenced_names(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Identifier(n) => { out.insert(n.clone()); }
        Expr::Cardinality(inner) => {
            // The seq name itself is structural — a `given` Seq value
            // for it determines the length, which the bound consumes.
            if let Expr::Identifier(name) = inner.as_ref() {
                out.insert(name.clone());
            }
            collect_referenced_names(inner, out);
        }
        Expr::Binary(_, lhs, rhs) => {
            collect_referenced_names(lhs, out);
            collect_referenced_names(rhs, out);
        }
        Expr::Not(inner) => collect_referenced_names(inner, out),
        Expr::Range(lo, hi) => {
            collect_referenced_names(lo, out);
            collect_referenced_names(hi, out);
        }
        Expr::Index(s, i) => {
            collect_referenced_names(s, out);
            collect_referenced_names(i, out);
        }
        Expr::Field(r, _) => collect_referenced_names(r, out),
        Expr::InExpr(lhs, rhs) => {
            collect_referenced_names(lhs, out);
            collect_referenced_names(rhs, out);
        }
        Expr::SetLit(items) | Expr::SeqLit(items) => {
            for it in items { collect_referenced_names(it, out); }
        }
        Expr::Forall(_, range, body) | Expr::Exists(_, range, body) => {
            collect_referenced_names(range, out);
            collect_referenced_names(body, out);
        }
        Expr::Call(_, args) => {
            for a in args { collect_referenced_names(a, out); }
        }
        _ => {}
    }
}

/// Structural signature for cache invalidation. Two queries with
/// equal signatures share the same cached, unrolled constraint set.
///
/// Captured as `(filtered_pinned, filtered_seq_lens)` — the literal
/// integer values that, after pre-translation, drive quantifier
/// unrolling. Filtered to names that actually appear in some
/// quantifier bound (`structural_names`); a non-structural Int
/// given like `pos = 42` lands in `pinned` but is filtered out so
/// it doesn't force a rebuild every step.
///
/// Why this isn't just "the structural subset of given":
///   - A `Seq` given changing values but keeping the same length
///     must NOT rebuild (constraint shape unchanged) — using length
///     instead of the whole seq value gets this right.
///   - A pinned-int derived via `n = #s` (chain) becomes part of
///     `pinned` and is also filtered against structural names.
pub type StructuralSignature = (HashMap<String, i64>, HashMap<String, i64>);

pub fn structural_signature(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
) -> StructuralSignature {
    let names = structural_names(body);
    let seq_lens = collect_seq_lengths(body, given);
    let pinned = collect_pinned_ints(body, given, &seq_lens);
    let pinned_filtered: HashMap<String, i64> = pinned.into_iter()
        .filter(|(k, _)| names.contains(k.as_str()))
        .collect();
    let seq_lens_filtered: HashMap<String, i64> = seq_lens.into_iter()
        .filter(|(k, _)| names.contains(k.as_str()))
        .collect();
    (pinned_filtered, seq_lens_filtered)
}

/// Pre-scan the schema body and `given` for variables that can be
/// pinned to a literal int *before* the solver runs:
///
///   - any `given` entry of value `Value::Int(n)` → `name → n`
///   - any body constraint of shape `name = literal_int_expr` (or
///     reverse) where the literal side resolves to a constant under
///     the names already pinned → `name → value`
///   - any body constraint of shape `name = #seq` where `#seq`'s
///     length itself reduces (e.g. via a sibling `#seq = N` constraint)
///     → `name → length` (length-propagation, mirrors Python's Pass 3)
///
/// Iterates to a fixed point so chains like `n = #s ∧ #s = 4 ∧ k = n - 1`
/// all resolve. The result is fed into `apply_pinned_ints` to upgrade
/// env entries to `Var::PinnedInt`, which lets `literal_range` unroll
/// quantifiers like `∀ i ∈ {0..n - 1}` even when `n` is symbolic.
pub(super) fn collect_pinned_ints(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
    seq_lengths: &HashMap<String, i64>,
) -> HashMap<String, i64> {
    let mut pinned: HashMap<String, i64> = HashMap::new();
    for (k, v) in given {
        if let Value::Int(n) = v { pinned.insert(k.clone(), *n); }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for item in body {
            if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                    if let Expr::Identifier(name) = a.as_ref() {
                        if !pinned.contains_key(name) {
                            // Try as a pure-int expression over already-pinned
                            // names + literal Ints + #seq lengths.
                            if let Some(v) = eval_pure_int(b, &pinned, seq_lengths) {
                                pinned.insert(name.clone(), v);
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
    }
    pinned
}

/// Pure constant-folding evaluator over Int expressions. Honors PinnedInt
/// names, literal Ints, arithmetic, and `#seq` references whose lengths
/// are concrete in `seq_lengths`.
fn eval_pure_int(
    e: &Expr,
    pinned: &HashMap<String, i64>,
    seq_lengths: &HashMap<String, i64>,
) -> Option<i64> {
    match e {
        Expr::Int(n) => Some(*n),
        Expr::Identifier(name) => pinned.get(name).copied(),
        Expr::Cardinality(inner) => match inner.as_ref() {
            Expr::Identifier(name) => seq_lengths.get(name).copied(),
            _ => None,
        },
        Expr::Binary(op, lhs, rhs) => {
            let l = eval_pure_int(lhs, pinned, seq_lengths)?;
            let r = eval_pure_int(rhs, pinned, seq_lengths)?;
            Some(match op {
                BinOp::Add => l.checked_add(r)?,
                BinOp::Sub => l.checked_sub(r)?,
                BinOp::Mul => l.checked_mul(r)?,
                BinOp::Div => if r == 0 { return None } else { l / r },
                _ => return None,
            })
        }
        _ => None,
    }
}

/// Pre-scan body for sequence-length pins. Three pin shapes:
///
///   - `given` value with a `Value::Seq*` payload — length comes from
///     the Vec.
///   - `seq = ⟨e1, e2, …⟩` — pins `#seq` to the literal's arity.
///   - `#seq = expr` (or `expr = #seq`) where `expr` reduces to a
///     literal int via `eval_pure_int`. This includes the simple case
///     `#seq = 5` AND chains like `#b = #a` and `#b = #a + 1`. Iterates
///     to a fixed point so a chain of N cardinality references resolves
///     in N passes.
///
/// The chained form is what makes `#state_next.cells = #state.cells`
/// work — the natural state-forwarding shape for stateful programs.
/// Without it, only `#state.cells` would be pinned and any quantifier
/// over `#state_next.cells` would silently drop.
///
/// Result is consumed by `collect_pinned_ints` so e.g. `n = #s` resolves
/// through it via `eval_pure_int`'s `seq_lengths` lookup.
pub(super) fn collect_seq_lengths(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
) -> HashMap<String, i64> {
    collect_seq_lengths_with_schemas(body, given, None)
}

/// `collect_seq_lengths` variant that follows `..Passthrough` body items
/// into the named claim's body. Lets a fsm with `..Level; ∀ i : … platforms[i] …`
/// see `#platforms = N` even when the pin lives in `Level`'s body.
pub(super) fn collect_seq_lengths_with_schemas(
    body: &[BodyItem],
    given: &HashMap<String, Value>,
    schemas: Option<&HashMap<String, SchemaDecl>>,
) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    // Seq lengths from `given` Seq values are exact.
    for (k, v) in given {
        let len = match v {
            Value::SeqInt(v)       => v.len() as i64,
            Value::SeqBool(v)      => v.len() as i64,
            Value::SeqStr(v)       => v.len() as i64,
            Value::SeqComposite(v) => v.len() as i64,
            Value::SeqEnum(v)      => v.len() as i64,
            _ => continue,
        };
        out.insert(k.clone(), len);
    }
    // `given` Int values seed `pinned` for the fixed-point walk so
    // chains like `#position = n` resolve when the caller pinned
    // `n` via given (e.g. invoking `Toposort` from Rust). Without
    // this, only seq-length pins propagate; an int-named length
    // bound stays symbolic and the ∀-unroll bails.
    let mut pinned: HashMap<String, i64> = HashMap::new();
    for (k, v) in given {
        if let Value::Int(n) = v { pinned.insert(k.clone(), *n); }
    }
    let mut changed = true;
    while changed {
        changed = false;
        walk_constraints(body, schemas, &pinned, &mut out, &mut changed);
    }
    out
}

/// Walk a body's Eq constraints for cardinality pins, recursing into
/// `..Passthrough` body items via `schemas` (when supplied). Marks
/// `changed = true` whenever a new pin is discovered.
fn walk_constraints(
    body: &[BodyItem],
    schemas: Option<&HashMap<String, SchemaDecl>>,
    no_pinned: &HashMap<String, i64>,
    out: &mut HashMap<String, i64>,
    changed: &mut bool,
) {
    for item in body {
        match item {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) => {
                for (a, b) in [(lhs, rhs), (rhs, lhs)] {
                    // `#name = pure-int-expr` (including `#name = #other`).
                    if let Expr::Cardinality(inner) = a.as_ref() {
                        if let Expr::Identifier(name) = inner.as_ref() {
                            if !out.contains_key(name) {
                                if let Some(v) = eval_pure_int(b, no_pinned, out) {
                                    out.insert(name.clone(), v);
                                    *changed = true;
                                }
                            }
                        }
                    }
                    // `seq_var = ⟨e1, e2, …⟩` pins #seq_var to items.len().
                    if let (Expr::Identifier(name), Expr::SeqLit(items)) =
                        (a.as_ref(), b.as_ref())
                    {
                        if !out.contains_key(name) {
                            out.insert(name.clone(), items.len() as i64);
                            *changed = true;
                        }
                    }
                }
            }
            BodyItem::Passthrough(claim_name) => {
                if let Some(schemas) = schemas {
                    if let Some(claim) = schemas.get(claim_name) {
                        walk_constraints(&claim.body, Some(schemas), no_pinned, out, changed);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Replace env entries for pinned names with `Var::PinnedInt(value)`.
/// The replacement is a no-op for names not in env (e.g. a `n = 5`
/// constraint where `n` was never declared with `n ∈ ...`).
pub(super) fn apply_pinned_ints<'ctx>(
    env: &mut HashMap<String, Var<'ctx>>,
    pinned: &HashMap<String, i64>,
) {
    for (name, value) in pinned {
        if env.contains_key(name) {
            env.insert(name.clone(), Var::PinnedInt(*value));
        }
    }
}


