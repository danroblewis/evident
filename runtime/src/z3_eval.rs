//! Z3 program extraction.
//!
//! Pipeline:
//!
//! ```text
//!   Evident AST  ──translate──▶  Z3 ASTs (raw)
//!                                     │
//!                                     ▼
//!                              simplify + propagate-values
//!                                     │
//!                                     ▼
//!                              Z3 ASTs (canonical)
//!                                     │
//!                                     ▼
//!                            per-output assignment extraction
//!                                     │
//!                                     ▼
//!                              Z3Program (steps + checks)
//! ```
//!
//! Downstream consumers (the JIT in `core::functionizer`) compile
//! a `Z3Program` to native code; this module's job is only to
//! produce the canonical program shape from Z3's simplified ASTs.

use std::collections::HashMap;
use z3::ast::{Ast, Bool, Dynamic};
use z3::{AstKind, Context, Goal, Tactic};
use z3_sys::DeclKind;

// Re-export the program shape so existing `crate::core::Z3Program`,
// `Z3Step`, `GuardedBody`, `GuardedBranch` paths continue to resolve.
// The types themselves live in `crate::core::z3_program`.
pub use crate::core::{GuardedBody, GuardedBranch, Z3Program, Z3Step};

/// Apply Z3's preprocessing tactic chain to the given Bool
/// assertions. Returns the simplified assertions (the residual
/// constraints after `simplify` + `propagate-values`).
///
/// We deliberately exclude `solve-eqs` here — that tactic
/// substitutes equality-defined variables AWAY, destroying the
/// `(= var expr)` shape we need for per-output extraction. Z3
/// would record the substitutions in a "model converter" that the
/// Rust z3 0.12 bindings don't expose, so we'd lose the
/// information.
///
/// What `simplify` + `propagate-values` give us:
///   * constant folding (1 + 2 → 3)
///   * algebraic identities (x + 0 → x, x * 1 → x)
///   * boolean simplification (a ∧ ¬a → false)
///   * ITE simplification when both branches equal
///   * datatype simplification (recognizer/accessor of known ctor folded)
///   * propagation of known constants through the formula
pub fn simplify_assertions<'ctx>(
    ctx: &'ctx Context,
    assertions: &[Bool<'ctx>],
) -> SimplifyResult<'ctx> {
    let goal = Goal::new(ctx, false, false, false);
    for a in assertions {
        goal.assert(a);
    }
    let simplify  = Tactic::new(ctx, "simplify");
    let propagate = Tactic::new(ctx, "propagate-values");
    let chain     = simplify.and_then(&propagate);
    let result    = chain.apply(&goal, None).expect("tactic apply");
    let mut formulas: Vec<Bool<'ctx>> = Vec::new();
    let mut unsat = false;
    for sub in result.list_subgoals() {
        if sub.is_decided_unsat() { unsat = true; }
        formulas.extend(sub.get_formulas::<Bool>());
    }
    // Conservative UNSAT detection: any assertion folded to `false`
    // by the tactics. This catches contradictions like `x = 3 ∧
    // x = 4` (after pinning x = 3, the second becomes `false`).
    for f in &formulas {
        if let Some(false) = f.as_bool() {
            unsat = true;
        }
    }
    SimplifyResult { formulas, unsat }
}

#[derive(Debug)]
pub struct SimplifyResult<'ctx> {
    pub formulas: Vec<Bool<'ctx>>,
    pub unsat:    bool,
}

/// Given a list of simplified Bool assertions and the set of
/// output variable names, partition the assertions into
/// per-output substitutions plus consistency checks.
///
/// For each assertion of form `(= a b)`:
///   * If LHS is a 0-arity App with name in `outputs` AND RHS
///     doesn't mention that name → `outputs[name] = RHS`.
///   * Symmetric for RHS as the output.
///   * Otherwise → check (must hold under given values).
///
/// Returns None if any output lacks a defining assignment after
/// simplification — that means the body isn't fully function-shaped
/// under these inputs, and the caller should fall through to Z3.
pub fn extract_program<'ctx>(
    assertions: &[Bool<'ctx>],
    outputs: &[String],
) -> Option<Z3Program<'ctx>> {
    let output_set: std::collections::HashSet<&str> = outputs.iter()
        .map(|s| s.as_str()).collect();

    // Buckets for unconditional assignments:
    let mut scalar_assign: HashMap<String, Dynamic<'ctx>> = HashMap::new();
    let mut seq_lengths:   HashMap<String, i64> = HashMap::new();
    let mut seq_elements:  HashMap<String, HashMap<i64, Dynamic<'ctx>>> = HashMap::new();
    // Guarded assignments — `(or (not P) Q)` style. Per output var,
    // a list of (guard, body) candidates that the eval walks at
    // runtime.
    let mut guarded: HashMap<String, Vec<GuardedBranch<'ctx>>> = HashMap::new();
    let mut checks: Vec<(Dynamic<'ctx>, Dynamic<'ctx>)> = Vec::new();
    // Non-equality predicates (`x < 5`, `flag = true` after
    // simplify folded both sides to Bool, etc.) that must hold
    // at runtime.
    let mut predicates: Vec<Bool<'ctx>> = Vec::new();

    for a in assertions {
        // (or (not P) Q) pattern — guarded assertion `P ⇒ Q`.
        if let Some((guard, consequent)) = try_guarded(a) {
            if classify_guarded_consequent(&consequent, &output_set,
                &mut guarded, &guard).is_some()
            {
                continue;
            }
        }

        let Some((lhs, rhs)) = split_equality(a) else {
            // Non-equality assertion: record it as a predicate
            // that must evaluate to true at runtime. Catches
            // type-bound constraints like `(>= x 0)` from Nat and
            // user inequalities like `x < 5`.
            predicates.push(a.clone());
            continue;
        };

        // Length pin: `(= var__len N)` or symmetric.
        if let Some((name, n)) = match_len_pin(&lhs, &rhs)
            .or_else(|| match_len_pin(&rhs, &lhs))
        {
            seq_lengths.insert(name, n);
            continue;
        }

        // Per-element pin: `(= (select arr idx_lit) elem)`.
        if let Some((arr, idx, elem)) = match_select_pin(&lhs, &rhs)
            .or_else(|| match_select_pin(&rhs, &lhs))
        {
            if output_set.contains(arr.as_str()) {
                seq_elements.entry(arr).or_default().insert(idx, elem);
                continue;
            }
        }

        // Plain output assignment: `(= output_var expr)`.
        if let Some(name) = ast_app_name(&lhs) {
            if output_set.contains(name.as_str())
                && !scalar_assign.contains_key(&name)
                && !mentions_name(&rhs, &name)
            {
                scalar_assign.insert(name, rhs);
                continue;
            }
        }
        if let Some(name) = ast_app_name(&rhs) {
            if output_set.contains(name.as_str())
                && !scalar_assign.contains_key(&name)
                && !mentions_name(&lhs, &name)
            {
                scalar_assign.insert(name, lhs);
                continue;
            }
        }
        // Falls through as a consistency check.
        checks.push((lhs, rhs));
    }

    // Compose Seq steps where length + all N elements are pinned.
    //
    // Length sources, in priority order:
    //   1. Explicit `(= var__len N)` pin from the body.
    //   2. Inferred from consecutive `(select var 0..K)` per-element
    //      pins — Z3 folds away the literal length pin via
    //      apply_seq_lengths BEFORE body translation, so seq-of-
    //      datatype assignments often arrive without an explicit
    //      length clause. If we see (select var 0), (select var 1),
    //      …, (select var K) and no (select var K+1), infer
    //      length = K+1.
    let mut seq_assign: HashMap<String, Vec<Dynamic<'ctx>>> = HashMap::new();
    for arr in outputs {
        if scalar_assign.contains_key(arr) { continue; }
        let explicit = seq_lengths.get(arr).copied();
        let inferred = seq_elements.get(arr).and_then(|m| {
            // Largest index i such that all of 0..=i are present.
            let mut i = 0i64;
            while m.contains_key(&i) { i += 1; }
            if i == 0 { None } else { Some(i) }
        });
        let n = match (explicit, inferred) {
            (Some(e), Some(i)) if e == i => e,
            (Some(e), Some(i)) => e.max(i),  // explicit wins if elements gap
            (Some(e), None)    => e,
            (None,    Some(i)) => i,
            (None,    None)    => continue,
        };
        let empty: HashMap<i64, Dynamic<'ctx>> = HashMap::new();
        let elements = seq_elements.get(arr).unwrap_or(&empty);
        let mut elems = Vec::with_capacity(n as usize);
        let mut ok = true;
        for i in 0..n {
            if let Some(e) = elements.get(&i) {
                elems.push(e.clone());
            } else if n == 0 {
                // empty seq, nothing to push
            } else {
                ok = false;
                break;
            }
        }
        if ok {
            seq_assign.insert(arr.clone(), elems);
        }
    }

    // Build Guarded steps for outputs covered by `guarded` map.
    let mut guarded_assign: HashMap<String, Vec<GuardedBranch<'ctx>>> = HashMap::new();
    for arr in outputs {
        if scalar_assign.contains_key(arr) || seq_assign.contains_key(arr) { continue; }
        if let Some(branches) = guarded.remove(arr) {
            if !branches.is_empty() {
                guarded_assign.insert(arr.clone(), branches);
            }
        }
    }

    // Every output must be covered by some assignment.
    for v in outputs {
        if !scalar_assign.contains_key(v)
            && !seq_assign.contains_key(v)
            && !guarded_assign.contains_key(v)
        {
            if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                eprintln!("[fz/z3] extract: output {v:?} has no substitution");
            }
            return None;
        }
    }
    extract_program_inner(outputs, scalar_assign, seq_assign, guarded_assign, checks, predicates)
}

/// Like `extract_program` but tolerates missing outputs — returns the
/// partial `Z3Program` plus a Vec<String> naming the outputs that
/// couldn't be substituted. Callers can fill in the gaps via model
/// extraction (encoded as `Z3Step::PreBaked`) or fall through.
pub fn extract_program_partial<'ctx>(
    assertions: &[Bool<'ctx>],
    outputs: &[String],
) -> Option<(Z3Program<'ctx>, Vec<String>)> {
    let output_set: std::collections::HashSet<&str> = outputs.iter()
        .map(|s| s.as_str()).collect();

    let mut scalar_assign: HashMap<String, Dynamic<'ctx>> = HashMap::new();
    let mut seq_lengths:   HashMap<String, i64> = HashMap::new();
    let mut seq_elements:  HashMap<String, HashMap<i64, Dynamic<'ctx>>> = HashMap::new();
    let mut guarded: HashMap<String, Vec<GuardedBranch<'ctx>>> = HashMap::new();
    let mut checks: Vec<(Dynamic<'ctx>, Dynamic<'ctx>)> = Vec::new();
    let mut predicates: Vec<Bool<'ctx>> = Vec::new();

    for a in assertions {
        if let Some((guard, consequent)) = try_guarded(a) {
            if classify_guarded_consequent(&consequent, &output_set,
                &mut guarded, &guard).is_some()
            {
                continue;
            }
        }
        // `(not (= X name))` / `(not (= name X))` → `name = ¬X`
        // for Bool-typed outputs. Z3 emits this for `name = ¬X`
        // after propagation flips polarity.
        if let Some(inner) = try_negation(a) {
            if let Some((lhs, rhs)) = split_equality(&inner) {
                if let Some(name) = ast_app_name(&lhs) {
                    if output_set.contains(name.as_str())
                        && !scalar_assign.contains_key(&name)
                        && !mentions_name(&rhs, &name)
                    {
                        let neg = rhs.as_bool().map(|b| b.not()).map(|b| z3::ast::Dynamic::from_ast(&b));
                        if let Some(neg) = neg {
                            scalar_assign.insert(name, neg);
                            continue;
                        }
                    }
                }
                if let Some(name) = ast_app_name(&rhs) {
                    if output_set.contains(name.as_str())
                        && !scalar_assign.contains_key(&name)
                        && !mentions_name(&lhs, &name)
                    {
                        let neg = lhs.as_bool().map(|b| b.not()).map(|b| z3::ast::Dynamic::from_ast(&b));
                        if let Some(neg) = neg {
                            scalar_assign.insert(name, neg);
                            continue;
                        }
                    }
                }
            }
        }
        let Some((lhs, rhs)) = split_equality(a) else {
            predicates.push(a.clone());
            continue;
        };
        if let Some((name, n)) = match_len_pin(&lhs, &rhs)
            .or_else(|| match_len_pin(&rhs, &lhs))
        {
            seq_lengths.insert(name, n);
            continue;
        }
        if let Some((arr, idx, elem)) = match_select_pin(&lhs, &rhs)
            .or_else(|| match_select_pin(&rhs, &lhs))
        {
            if output_set.contains(arr.as_str()) {
                seq_elements.entry(arr).or_default().insert(idx, elem);
                continue;
            }
        }
        if let Some(name) = ast_app_name(&lhs) {
            if output_set.contains(name.as_str())
                && !scalar_assign.contains_key(&name)
                && !mentions_name(&rhs, &name)
            {
                scalar_assign.insert(name, rhs);
                continue;
            }
        }
        if let Some(name) = ast_app_name(&rhs) {
            if output_set.contains(name.as_str())
                && !scalar_assign.contains_key(&name)
                && !mentions_name(&lhs, &name)
            {
                scalar_assign.insert(name, lhs);
                continue;
            }
        }
        checks.push((lhs, rhs));
    }

    let mut seq_assign: HashMap<String, Vec<Dynamic<'ctx>>> = HashMap::new();
    for arr in outputs {
        if scalar_assign.contains_key(arr) { continue; }
        let explicit = seq_lengths.get(arr).copied();
        let inferred = seq_elements.get(arr).and_then(|m| {
            let mut i = 0i64;
            while m.contains_key(&i) { i += 1; }
            if i == 0 { None } else { Some(i) }
        });
        let n = match (explicit, inferred) {
            (Some(e), Some(i)) if e == i => e,
            (Some(e), Some(i)) => e.max(i),
            (Some(e), None)    => e,
            (None,    Some(i)) => i,
            (None,    None)    => continue,
        };
        let empty: HashMap<i64, Dynamic<'ctx>> = HashMap::new();
        let elements = seq_elements.get(arr).unwrap_or(&empty);
        let mut elems = Vec::with_capacity(n as usize);
        let mut ok = true;
        for i in 0..n {
            if let Some(e) = elements.get(&i) {
                elems.push(e.clone());
            } else if n == 0 {
            } else { ok = false; break; }
        }
        if ok { seq_assign.insert(arr.clone(), elems); }
    }

    let mut guarded_assign: HashMap<String, Vec<GuardedBranch<'ctx>>> = HashMap::new();
    for arr in outputs {
        if scalar_assign.contains_key(arr) || seq_assign.contains_key(arr) { continue; }
        if let Some(branches) = guarded.remove(arr) {
            if !branches.is_empty() {
                guarded_assign.insert(arr.clone(), branches);
            }
        }
    }

    // Identify missing outputs.
    let missing: Vec<String> = outputs.iter()
        .filter(|v| !scalar_assign.contains_key(*v)
            && !seq_assign.contains_key(*v)
            && !guarded_assign.contains_key(*v))
        .cloned()
        .collect();

    // Build a program over the covered outputs.
    let covered: Vec<String> = outputs.iter()
        .filter(|v| !missing.contains(v))
        .cloned()
        .collect();
    let program = extract_program_inner(&covered, scalar_assign, seq_assign, guarded_assign, checks, predicates)?;
    Some((program, missing))
}

fn extract_program_inner<'ctx>(
    outputs: &[String],
    scalar_assign: HashMap<String, Dynamic<'ctx>>,
    seq_assign: HashMap<String, Vec<Dynamic<'ctx>>>,
    guarded_assign: HashMap<String, Vec<GuardedBranch<'ctx>>>,
    checks: Vec<(Dynamic<'ctx>, Dynamic<'ctx>)>,
    predicates: Vec<Bool<'ctx>>,
) -> Option<Z3Program<'ctx>> {
    let mut scalar_assign = scalar_assign;
    let mut seq_assign = seq_assign;
    let mut guarded_assign = guarded_assign;

    // Topo-sort by dependency on other outputs.
    let mut in_deg: HashMap<&str, usize> = outputs.iter()
        .map(|v| (v.as_str(), 0)).collect();
    let mut reverse: HashMap<&str, Vec<&str>> = HashMap::new();
    let mentions_any = |exprs: &[&Dynamic<'ctx>], name: &str| -> bool {
        exprs.iter().any(|e| mentions_name(e, name))
    };
    for v in outputs {
        let mut exprs: Vec<&Dynamic<'ctx>> = Vec::new();
        if let Some(e) = scalar_assign.get(v) {
            exprs.push(e);
        } else if let Some(es) = seq_assign.get(v) {
            exprs.extend(es.iter());
        } else if let Some(bs) = guarded_assign.get(v) {
            for b in bs {
                exprs.push(&b.guard);
                match &b.body {
                    GuardedBody::Scalar(e)  => exprs.push(e),
                    GuardedBody::Seq(es)    => exprs.extend(es.iter()),
                }
            }
        }
        for other in outputs {
            if other == v { continue; }
            if mentions_any(&exprs, other) {
                *in_deg.get_mut(v.as_str()).unwrap() += 1;
                reverse.entry(other.as_str()).or_default().push(v.as_str());
            }
        }
    }
    let mut ready: Vec<&str> = in_deg.iter()
        .filter(|(_, &d)| d == 0).map(|(&n, _)| n).collect();
    ready.sort_unstable();
    let mut order: Vec<&str> = Vec::with_capacity(outputs.len());
    while let Some(n) = ready.pop() {
        order.push(n);
        if let Some(succs) = reverse.get(n) {
            for &m in succs {
                let d = in_deg.get_mut(m).unwrap();
                *d -= 1;
                if *d == 0 { ready.push(m); }
            }
        }
        ready.sort_unstable();
    }
    if order.len() != outputs.len() {
        return None;  // cycle
    }
    let steps: Vec<Z3Step> = order.into_iter().map(|v| {
        if let Some(expr) = scalar_assign.remove(v) {
            Z3Step::Scalar { var: v.to_string(), expr }
        } else if let Some(elem_exprs) = seq_assign.remove(v) {
            Z3Step::Seq { var: v.to_string(), elem_exprs }
        } else {
            let branches = guarded_assign.remove(v).unwrap();
            Z3Step::Guarded { var: v.to_string(), branches }
        }
    }).collect();
    Some(Z3Program { steps, checks, predicates })
}

/// Return the inner Bool if `a` is `(not X)`, else None.
fn try_negation<'ctx>(a: &Bool<'ctx>) -> Option<Bool<'ctx>> {
    if a.kind() != AstKind::App { return None; }
    let decl = a.safe_decl().ok()?;
    if decl.kind() != DeclKind::NOT { return None; }
    let mut iter = a.children().into_iter();
    let child = iter.next()?;
    child.as_bool()
}

/// Recognize `(or (not P) Q)` patterns and return `(P, Q)`. This
/// is the canonical form Z3's tactic chain emits for material
/// implications `P ⇒ Q`. Returns None for any other shape.
fn try_guarded<'ctx>(a: &Bool<'ctx>) -> Option<(Dynamic<'ctx>, Bool<'ctx>)> {
    if a.kind() != AstKind::App { return None; }
    let decl = a.safe_decl().ok()?;
    if decl.kind() != DeclKind::OR { return None; }
    let children = a.children();
    if children.len() != 2 { return None; }
    let try_pair = |neg: &Dynamic<'ctx>, conseq: &Dynamic<'ctx>|
        -> Option<(Dynamic<'ctx>, Bool<'ctx>)>
    {
        if neg.kind() != AstKind::App { return None; }
        let nd = neg.safe_decl().ok()?;
        if nd.kind() != DeclKind::NOT { return None; }
        let pred = neg.children().into_iter().next()?;
        let conseq_bool = conseq.as_bool()?;
        Some((pred, conseq_bool))
    };
    try_pair(&children[0], &children[1])
        .or_else(|| try_pair(&children[1], &children[0]))
}

/// Inspect the consequent of a guarded assertion. If it
/// constrains an output variable (either directly or via Seq
/// pinning), record a branch in `guarded`.
fn classify_guarded_consequent<'ctx>(
    conseq: &Bool<'ctx>,
    output_set: &std::collections::HashSet<&str>,
    guarded: &mut HashMap<String, Vec<GuardedBranch<'ctx>>>,
    guard: &Dynamic<'ctx>,
) -> Option<()> {
    // Direct `(= var expr)` — scalar guarded equality.
    if let Some((lhs, rhs)) = split_equality_dyn(conseq) {
        if let Some(name) = ast_app_name(&lhs) {
            if output_set.contains(name.as_str()) {
                guarded.entry(name).or_default().push(GuardedBranch {
                    guard: guard.clone(),
                    body:  GuardedBody::Scalar(rhs),
                });
                return Some(());
            }
        }
        if let Some(name) = ast_app_name(&rhs) {
            if output_set.contains(name.as_str()) {
                guarded.entry(name).or_default().push(GuardedBranch {
                    guard: guard.clone(),
                    body:  GuardedBody::Scalar(lhs),
                });
                return Some(());
            }
        }
    }
    // `(and (= var__len N) (= (select var 0) x) ...)` — seq guarded.
    if conseq.kind() == AstKind::App {
        if let Ok(decl) = conseq.safe_decl() {
            if decl.kind() == DeclKind::AND {
                if let Some((arr, elems)) = collect_seq_in_and(conseq, output_set) {
                    guarded.entry(arr).or_default().push(GuardedBranch {
                        guard: guard.clone(),
                        body:  GuardedBody::Seq(elems),
                    });
                    return Some(());
                }
                // Mixed AND: `(and (= scalar_var expr) (= other_var__len N) ...)`
                // — split into per-output guarded branches. Each child
                // must either be a scalar-output assignment or contribute
                // to a single Seq's per-element pinning.
                if let Some(()) = classify_mixed_and(conseq, output_set, guarded, guard) {
                    return Some(());
                }
            }
        }
    }
    // `(= var__len 0)` alone — empty seq case.
    if let Some((lhs, rhs)) = split_equality_dyn(conseq) {
        let try_empty = |a: &Dynamic<'ctx>, b: &Dynamic<'ctx>| -> Option<String> {
            let name = ast_app_name(a)?;
            let arr  = name.strip_suffix("__len")?;
            let n = numeral_to_i64(b)?;
            if n == 0 && output_set.contains(arr) {
                return Some(arr.to_string());
            }
            None
        };
        if let Some(arr) = try_empty(&lhs, &rhs).or_else(|| try_empty(&rhs, &lhs)) {
            guarded.entry(arr).or_default().push(GuardedBranch {
                guard: guard.clone(),
                body:  GuardedBody::Seq(vec![]),
            });
            return Some(());
        }
    }
    None
}

/// `(and (= scalar_var expr) (= other_seq__len N) (= (select other_seq 0) e0) ...)`
/// — handle a guarded consequent that constrains MULTIPLE outputs.
/// Each output gets its own branch added to `guarded`. Returns Some(())
/// if at least one output was successfully recognized AND every AND child
/// was classifiable; None otherwise.
fn classify_mixed_and<'ctx>(
    and_expr: &Bool<'ctx>,
    output_set: &std::collections::HashSet<&str>,
    guarded: &mut HashMap<String, Vec<GuardedBranch<'ctx>>>,
    guard: &Dynamic<'ctx>,
) -> Option<()> {
    let mut scalar_assigns: Vec<(String, Dynamic<'ctx>)> = Vec::new();
    // For each Seq output: declared length + per-index elements.
    let mut seq_lens: HashMap<String, i64> = HashMap::new();
    let mut seq_elems: HashMap<String, HashMap<i64, Dynamic<'ctx>>> = HashMap::new();
    for c in and_expr.children() {
        let Some(bool_child) = c.as_bool() else { return None };
        let Some((lhs, rhs)) = split_equality(&bool_child) else { return None };
        if let Some((name, n)) = match_len_pin(&lhs, &rhs)
            .or_else(|| match_len_pin(&rhs, &lhs))
        {
            if !output_set.contains(name.as_str()) { return None; }
            seq_lens.insert(name, n);
            continue;
        }
        if let Some((name, idx, elem)) = match_select_pin(&lhs, &rhs)
            .or_else(|| match_select_pin(&rhs, &lhs))
        {
            if !output_set.contains(name.as_str()) { return None; }
            seq_elems.entry(name).or_default().insert(idx, elem);
            continue;
        }
        // Scalar output assignment.
        if let Some(name) = ast_app_name(&lhs) {
            if output_set.contains(name.as_str()) {
                scalar_assigns.push((name, rhs));
                continue;
            }
        }
        if let Some(name) = ast_app_name(&rhs) {
            if output_set.contains(name.as_str()) {
                scalar_assigns.push((name, lhs));
                continue;
            }
        }
        return None;  // unrecognized child
    }
    // Validate Seq covered: every name in seq_lens must have all elements.
    let mut all_names: std::collections::HashSet<String> = seq_lens.keys().cloned().collect();
    for k in seq_elems.keys() { all_names.insert(k.clone()); }
    for name in &all_names {
        let n = seq_lens.get(name).copied().unwrap_or_else(|| {
            // No explicit length pin — infer from contiguous element pins.
            let m = seq_elems.get(name).cloned().unwrap_or_default();
            let mut i = 0i64;
            while m.contains_key(&i) { i += 1; }
            i
        });
        let elems = seq_elems.remove(name).unwrap_or_default();
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            out.push(elems.get(&i)?.clone());
        }
        guarded.entry(name.clone()).or_default().push(GuardedBranch {
            guard: guard.clone(),
            body:  GuardedBody::Seq(out),
        });
    }
    for (name, expr) in scalar_assigns {
        guarded.entry(name).or_default().push(GuardedBranch {
            guard: guard.clone(),
            body:  GuardedBody::Scalar(expr),
        });
    }
    Some(())
}

/// Same as `split_equality` but accepts a Dynamic (the consequent
/// of a guarded assertion may be typed as Bool but coming in via
/// `try_guarded`'s `as_bool()` round-trip).
fn split_equality_dyn<'ctx>(b: &Bool<'ctx>) -> Option<(Dynamic<'ctx>, Dynamic<'ctx>)> {
    split_equality(b)
}

/// If `and_expr` is `(and (= arr__len N) (= (select arr 0) e0)
/// (= (select arr 1) e1) ...)` returning the arr name (must be in
/// `output_set`) plus the ordered elements.
fn collect_seq_in_and<'ctx>(
    and_expr: &Bool<'ctx>,
    output_set: &std::collections::HashSet<&str>,
) -> Option<(String, Vec<Dynamic<'ctx>>)> {
    let mut arr_name: Option<String> = None;
    let mut declared_len: Option<i64> = None;
    let mut indexed: HashMap<i64, Dynamic<'ctx>> = HashMap::new();

    for c in and_expr.children() {
        let Some(bool_child) = c.as_bool() else { return None; };
        let Some((lhs, rhs)) = split_equality(&bool_child) else { return None; };

        // Try len pin.
        if let Some((name, n)) = match_len_pin(&lhs, &rhs)
            .or_else(|| match_len_pin(&rhs, &lhs))
        {
            if !output_set.contains(name.as_str()) { return None; }
            if let Some(prev) = &arr_name {
                if *prev != name { return None; }
            } else { arr_name = Some(name); }
            declared_len = Some(n);
            continue;
        }
        // Try select pin.
        let pin = match_select_pin(&lhs, &rhs)
            .or_else(|| match_select_pin(&rhs, &lhs));
        if let Some((name, idx, elem)) = pin {
            if !output_set.contains(name.as_str()) { return None; }
            if let Some(prev) = &arr_name {
                if *prev != name { return None; }
            } else { arr_name = Some(name); }
            indexed.insert(idx, elem);
            continue;
        }
        return None;  // unrecognized child in the `and`.
    }

    let arr = arr_name?;
    let n = declared_len?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        out.push(indexed.remove(&i)?);
    }
    Some((arr, out))
}

/// Match `(= var__len N)` patterns. Returns the seq's base name
/// and length.
fn match_len_pin<'ctx>(a: &Dynamic<'ctx>, b: &Dynamic<'ctx>) -> Option<(String, i64)> {
    let name = ast_app_name(a)?;
    let arr = name.strip_suffix("__len")?;
    let n = numeral_to_i64(b)?;
    Some((arr.to_string(), n))
}

/// Match `(= (select arr idx_literal) elem)` patterns.
fn match_select_pin<'ctx>(
    a: &Dynamic<'ctx>,
    b: &Dynamic<'ctx>,
) -> Option<(String, i64, Dynamic<'ctx>)> {
    if a.kind() != AstKind::App { return None; }
    let decl = a.safe_decl().ok()?;
    if decl.kind() != DeclKind::SELECT { return None; }
    let children = a.children();
    if children.len() != 2 { return None; }
    let arr = ast_app_name(&children[0])?;
    let idx = numeral_to_i64(&children[1])?;
    Some((arr, idx, b.clone()))
}

/// Extract a literal i64 from a Numeral Dynamic.
fn numeral_to_i64<'ctx>(d: &Dynamic<'ctx>) -> Option<i64> {
    if d.kind() != AstKind::Numeral { return None; }
    d.as_int().and_then(|i| i.as_i64())
}

/// Helper: if `b` is `(= lhs rhs)`, return `(lhs, rhs)` as Dynamics.
fn split_equality<'ctx>(b: &Bool<'ctx>) -> Option<(Dynamic<'ctx>, Dynamic<'ctx>)> {
    if b.kind() != AstKind::App { return None; }
    let decl = b.safe_decl().ok()?;
    if decl.kind() != DeclKind::EQ { return None; }
    let children = b.children();
    if children.len() != 2 { return None; }
    Some((children[0].clone(), children[1].clone()))
}

/// Helper: if `a` is a 0-arity App (a "constant"/variable in Z3
/// parlance), return its name.
fn ast_app_name<'ctx>(a: &Dynamic<'ctx>) -> Option<String> {
    if a.kind() != AstKind::App { return None; }
    if a.num_children() != 0 { return None; }
    let decl = a.safe_decl().ok()?;
    Some(decl.name())
}

/// Helper: does `a`'s tree mention a 0-arity App with the given
/// name? Used both for cycle detection and topo-sort dependency.
fn mentions_name<'ctx>(a: &Dynamic<'ctx>, name: &str) -> bool {
    if a.kind() == AstKind::App && a.num_children() == 0 {
        if let Ok(decl) = a.safe_decl() {
            if decl.name() == name { return true; }
        }
    }
    for c in a.children() {
        if mentions_name(&c, name) { return true; }
    }
    false
}


/// Scan a body for known translator-gap shapes that would fatal-
/// exit `build_cache` via the dropped-constraint path. We refuse
/// to function-ize these and let the slow path handle them.
///
/// Known gap: `Ctor(SeqLit(...))` — enum constructor whose payload
/// is a sequence literal (e.g. `Many(⟨Red, Green, Blue⟩)`). The Z3
/// translator can't currently express this assertion.
pub fn has_known_translator_gap(body: &[crate::core::ast::BodyItem]) -> bool {
    body.iter().any(|item| {
        let crate::core::ast::BodyItem::Constraint(e) = item else { return false };
        expr_has_ctor_seqlit_payload(e)
    })
}

fn expr_has_ctor_seqlit_payload(e: &crate::core::ast::Expr) -> bool {
    use crate::core::ast::Expr;
    match e {
        Expr::Call(_, args) => {
            args.iter().any(|a| matches!(a, Expr::SeqLit(_)))
                || args.iter().any(expr_has_ctor_seqlit_payload)
        }
        Expr::Binary(_, l, r) =>
            expr_has_ctor_seqlit_payload(l) || expr_has_ctor_seqlit_payload(r),
        Expr::Not(x) | Expr::Cardinality(x) => expr_has_ctor_seqlit_payload(x),
        Expr::Ternary(c, a, b) =>
            expr_has_ctor_seqlit_payload(c)
            || expr_has_ctor_seqlit_payload(a)
            || expr_has_ctor_seqlit_payload(b),
        Expr::Match(scrut, arms) =>
            expr_has_ctor_seqlit_payload(scrut)
            || arms.iter().any(|arm| expr_has_ctor_seqlit_payload(&arm.body)),
        Expr::Index(s, i) =>
            expr_has_ctor_seqlit_payload(s) || expr_has_ctor_seqlit_payload(i),
        Expr::Field(r, _) => expr_has_ctor_seqlit_payload(r),
        _ => false,
    }
}

/// Walk a Bool AST and collect every 0-arity App name (i.e.,
/// every UNINTERPRETED constant or DT recogniser referent) into
/// `out`. The runtime uses this to identify which env vars
/// actually appear in the simplified body — vars not touched
/// can't be outputs of the function-izer.
pub fn collect_touched_names<'ctx>(
    a: &z3::ast::Bool<'ctx>,
    out: &mut std::collections::HashSet<String>,
) {
    let d = z3::ast::Dynamic::from_ast(a);
    collect_touched_names_dyn(&d, out);
}

fn collect_touched_names_dyn<'ctx>(
    a: &Dynamic<'ctx>,
    out: &mut std::collections::HashSet<String>,
) {
    if a.kind() == AstKind::App {
        if let Ok(decl) = a.safe_decl() {
            if decl.kind() == DeclKind::UNINTERPRETED && a.num_children() == 0 {
                out.insert(decl.name());
                return;
            }
        }
        for c in a.children() {
            collect_touched_names_dyn(&c, out);
        }
    }
}

/// Pull the variant name from a Z3 application formatted as
/// `((_ is <Variant>) <arg>)`. Workaround for the Rust z3 0.12
/// binding not exposing FuncDecl parameters. Used by the JIT
/// codegen to parse recognizer applications.
pub fn extract_is_variant_pub(s: &str) -> Option<String> {
    let idx = s.find("(_ is ")?;
    let rest = &s[idx + 6 ..];   // after "(_ is "
    let end = rest.find(|c: char| c.is_whitespace() || c == ')')?;
    Some(rest[..end].to_string())
}
