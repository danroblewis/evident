//! Z3 program extraction: simplify Z3 ASTs → `Z3Program` (ordered steps + checks).
//! The JIT in `core::functionizer` compiles the result to native code.

use std::collections::{BTreeMap, HashMap};
use z3::ast::{Array, Ast, Bool, Dynamic, Int};
use z3::{AstKind, Context, Goal, Sort, SortKind, Tactic};
use z3_sys::DeclKind;

use crate::core::{DatatypeRegistry, FieldKind, Value};

// Re-export so `crate::core::Z3Program` / `Z3Step` / etc. paths continue to resolve.
pub use crate::core::{GuardedBody, GuardedBranch, Z3Program, Z3Step};

/// Apply `simplify` + `propagate-values`. Excludes `solve-eqs`: it destroys the
/// `(= var expr)` shape needed for extraction via a model-converter z3 0.12 doesn't expose.
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
    // Any assertion folded to `false` by the tactics → UNSAT.
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

/// Partition simplified assertions into per-output assignments + consistency checks.
/// Returns `None` if any output lacks a defining assignment (body not function-shaped).
pub fn extract_program<'ctx>(
    assertions: &[Bool<'ctx>],
    outputs: &[String],
) -> Option<Z3Program<'ctx>> {
    let output_set: std::collections::HashSet<&str> = outputs.iter()
        .map(|s| s.as_str()).collect();

    let mut scalar_assign: HashMap<String, Dynamic<'ctx>> = HashMap::new();
    let mut seq_lengths:   HashMap<String, i64> = HashMap::new();
    let mut seq_elements:  HashMap<String, HashMap<i64, Dynamic<'ctx>>> = HashMap::new();
    // Guarded assignments: `(or (not P) Q)` → per-output (guard, body) list.
    let mut guarded: HashMap<String, Vec<GuardedBranch<'ctx>>> = HashMap::new();
    let mut checks: Vec<(Dynamic<'ctx>, Dynamic<'ctx>)> = Vec::new();
    // Non-equality predicates (`x < 5`, Nat bounds, etc.) that must hold at runtime.
    let mut predicates: Vec<Bool<'ctx>> = Vec::new();

    for a in assertions {
        if let Some((guard, consequent)) = try_guarded(a) {
            if classify_guarded_consequent(&consequent, &output_set,
                &mut guarded, &guard).is_some()
            {
                continue;
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

    // Build Seq steps: prefer explicit `var__len` pin; fall back to inferring length
    // from contiguous per-element select pins (Z3 sometimes folds away the len pin).
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

    let mut guarded_assign: HashMap<String, Vec<GuardedBranch<'ctx>>> = HashMap::new();
    for arr in outputs {
        if scalar_assign.contains_key(arr) || seq_assign.contains_key(arr) { continue; }
        if let Some(branches) = guarded.remove(arr) {
            if !branches.is_empty() {
                guarded_assign.insert(arr.clone(), branches);
            }
        }
    }

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

/// Like `extract_program` but tolerates missing outputs; returns the partial program
/// plus the names that couldn't be substituted (caller fills gaps or falls through).
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
        // `(not (= X name))` → `name = ¬X`: Z3 emits this for Bool outputs after propagation.
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

    let missing: Vec<String> = outputs.iter()
        .filter(|v| !scalar_assign.contains_key(*v)
            && !seq_assign.contains_key(*v)
            && !guarded_assign.contains_key(*v))
        .cloned()
        .collect();

    let covered: Vec<String> = outputs.iter()
        .filter(|v| !missing.contains(v))
        .cloned()
        .collect();
    let mut program = extract_program_inner(&covered, scalar_assign, seq_assign, guarded_assign, checks, predicates)?;
    let mut missing = missing;
    // Sampler recovery: gated on EVIDENT_SATISFIER so Cranelift path is unchanged.
    if samplers_enabled() {
        recover_samplers(&mut program, &mut missing, assertions);
    }
    Some((program, missing))
}

/// True when EVIDENT_SATISFIER is set — gates sampler recovery; must agree with the
/// functionizer strategy selector in `functionize::default` / `commands::effect_run`.
fn samplers_enabled() -> bool {
    std::env::var("EVIDENT_SATISFIER").is_ok()
}

// ── Sampler recovery ─────────────────────────────────────────────
// Finite-domain missing outputs (range/enum/set) become `Sample*` steps for deterministic draw.

/// One side of an integer bound predicate, normalized to `var <relation> literal`.
enum Bound { Ge(i64), Gt(i64), Le(i64), Lt(i64) }

/// For each var still in `missing`, emit a `Sample*` step if the constraints form a
/// recognized finite domain (range/enum/set). Conservative: partial shapes stay missing.
pub fn recover_samplers<'ctx>(
    program: &mut Z3Program<'ctx>,
    missing: &mut Vec<String>,
    assertions: &[Bool<'ctx>],
) {
    let trace = std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok();
    let mut sample_steps: Vec<Z3Step<'ctx>> = Vec::new();
    let mut consumed_pred: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for var in missing.clone() {
        let mentioning: Vec<usize> = program.predicates.iter().enumerate()
            .filter(|(_, p)| mentions_name(&Dynamic::from_ast(*p), &var))
            .map(|(i, _)| i)
            .collect();
        // A var in a consistency check is equality-constrained, not a clean sampler.
        let in_check = program.checks.iter()
            .any(|(l, r)| mentions_name(l, &var) || mentions_name(r, &var));
        if in_check { continue; }

        if mentioning.len() == 1 {
            if let Some(candidates) = match_set_or(&program.predicates[mentioning[0]], &var) {
                if !candidates.is_empty() {
                    if trace {
                        eprintln!("[fz/z3] sampler: {var:?} ∈ finite set \
                                  ({} candidates)", candidates.len());
                    }
                    sample_steps.push(Z3Step::SampleSet { var: var.clone(), candidates });
                    consumed_pred.insert(mentioning[0]);
                    missing.retain(|m| m != &var);
                    continue;
                }
            }
        }

        if !mentioning.is_empty() {
            let mut lo: Option<i64> = None;
            let mut hi: Option<i64> = None;
            let mut all_bounds = true;
            for &i in &mentioning {
                match match_bound(&program.predicates[i], &var) {
                    Some(Bound::Ge(v)) => lo = Some(lo.map_or(v, |c| c.max(v))),
                    Some(Bound::Gt(v)) => lo = Some(lo.map_or(v + 1, |c| c.max(v + 1))),
                    Some(Bound::Le(v)) => hi = Some(hi.map_or(v, |c| c.min(v))),
                    Some(Bound::Lt(v)) => hi = Some(hi.map_or(v - 1, |c| c.min(v - 1))),
                    None => { all_bounds = false; break; }
                }
            }
            if all_bounds {
                if let (Some(lo), Some(hi)) = (lo, hi) {
                    if lo <= hi {
                        if trace { eprintln!("[fz/z3] sampler: {var:?} ∈ [{lo}, {hi}]"); }
                        sample_steps.push(Z3Step::SampleRange { var: var.clone(), lo, hi });
                        for &i in &mentioning { consumed_pred.insert(i); }
                        missing.retain(|m| m != &var);
                        continue;
                    }
                }
            }
            continue; // predicates present but not a closed range → refuse
        }

        if let Some(type_name) = enum_sort_of(assertions, &var) {
            if trace { eprintln!("[fz/z3] sampler: {var:?} ∈ enum {type_name}"); }
            sample_steps.push(Z3Step::SampleEnum { var: var.clone(), type_name });
            missing.retain(|m| m != &var);
            continue;
        }
    }

    if sample_steps.is_empty() { return; }
    let kept: Vec<Bool<'ctx>> = std::mem::take(&mut program.predicates)
        .into_iter()
        .enumerate()
        .filter_map(|(i, p)| if consumed_pred.contains(&i) { None } else { Some(p) })
        .collect();
    program.predicates = kept;
    // Sample steps have no inputs → prepend so downstream steps that consume them follow.
    let mut rest = std::mem::take(&mut program.steps);
    sample_steps.append(&mut rest);
    program.steps = sample_steps;
}

/// Like `extract_program` but always runs sampler recovery (no env gate). Returns
/// `None` if any output remains unbound after recovery.
pub fn extract_program_with_samplers<'ctx>(
    assertions: &[Bool<'ctx>],
    outputs: &[String],
) -> Option<Z3Program<'ctx>> {
    let (mut program, mut missing) = extract_program_partial(assertions, outputs)?;
    // No-op if partial already recovered (env set); actual recovery when env unset.
    recover_samplers(&mut program, &mut missing, assertions);
    if !missing.is_empty() {
        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
            eprintln!("[fz/z3] extract_with_samplers: outputs still unbound: {missing:?}");
        }
        return None;
    }
    Some(program)
}

/// Match `(op var lit)` or `(op lit var)` where op ∈ {≥,>,≤,<}, returning
/// the bound normalized to `var <relation> literal`. Nested `var` → `None`.
fn match_bound<'ctx>(pred: &Bool<'ctx>, var: &str) -> Option<Bound> {
    if pred.kind() != AstKind::App { return None; }
    let decl = pred.safe_decl().ok()?;
    let kind = decl.kind();
    let children = pred.children();
    if children.len() != 2 { return None; }
    if ast_app_name(&children[0]).as_deref() == Some(var) {
        let lit = numeral_to_i64(&children[1])?;
        return match kind {
            DeclKind::GE => Some(Bound::Ge(lit)),
            DeclKind::GT => Some(Bound::Gt(lit)),
            DeclKind::LE => Some(Bound::Le(lit)),
            DeclKind::LT => Some(Bound::Lt(lit)),
            _ => None,
        };
    }
    // var on the right: flip the relation (lit ≥ var ⇔ var ≤ lit, etc.)
    if ast_app_name(&children[1]).as_deref() == Some(var) {
        let lit = numeral_to_i64(&children[0])?;
        return match kind {
            DeclKind::GE => Some(Bound::Le(lit)),
            DeclKind::GT => Some(Bound::Lt(lit)),
            DeclKind::LE => Some(Bound::Ge(lit)),
            DeclKind::LT => Some(Bound::Gt(lit)),
            _ => None,
        };
    }
    None
}

/// Match `(or (= var c0) (= var c1) …)` and return literal candidates.
/// Every disjunct must equate `var` to an Int/Bool literal; otherwise `None`.
fn match_set_or<'ctx>(pred: &Bool<'ctx>, var: &str) -> Option<Vec<Value>> {
    if pred.kind() != AstKind::App { return None; }
    let decl = pred.safe_decl().ok()?;
    if decl.kind() != DeclKind::OR { return None; }
    let children = pred.children();
    if children.is_empty() { return None; }
    let mut candidates = Vec::with_capacity(children.len());
    for c in &children {
        let cb = c.as_bool()?;
        let (l, r) = split_equality(&cb)?;
        let lit = if ast_app_name(&l).as_deref() == Some(var) {
            &r
        } else if ast_app_name(&r).as_deref() == Some(var) {
            &l
        } else {
            return None;
        };
        candidates.push(literal_to_value(lit)?);
    }
    Some(candidates)
}

/// Convert a Z3 literal to `Value` (Int/Bool); `None` for non-literals.
fn literal_to_value<'ctx>(d: &Dynamic<'ctx>) -> Option<Value> {
    if let Some(i) = numeral_to_i64(d) { return Some(Value::Int(i)); }
    if let Some(b) = d.as_bool() {
        if let Some(bv) = b.as_bool() { return Some(Value::Bool(bv)); }
    }
    None
}

/// Return the Datatype sort name for `var` in `assertions`, or `None` for non-enum vars.
fn enum_sort_of<'ctx>(assertions: &[Bool<'ctx>], var: &str) -> Option<String> {
    for a in assertions {
        if let Some(node) = find_var_node(&Dynamic::from_ast(a), var) {
            let sort = node.get_sort();
            if sort.kind() == SortKind::Datatype {
                return Some(format!("{sort}"));
            }
            return None;
        }
    }
    None
}

/// Find the first 0-arity App named `var` in `d`.
fn find_var_node<'ctx>(d: &Dynamic<'ctx>, var: &str) -> Option<Dynamic<'ctx>> {
    if d.kind() == AstKind::App && d.num_children() == 0 {
        if let Ok(decl) = d.safe_decl() {
            if decl.name() == var { return Some(d.clone()); }
        }
    }
    for c in d.children() {
        if let Some(n) = find_var_node(&c, var) { return Some(n); }
    }
    None
}

// Record-element Seq recomposition: Z3 `simplify` explodes record-element pins into per-field
// accessors; `recompose_record_seqs` groups by index → `Z3Step::Seq` / `Value::SeqComposite`.

/// One pin LHS rooted at `(select var idx)`. Path = field-accessor names, `__arr`/`__len` stripped.
enum PinKind {
    /// Leaf primitive / nested-record field: `element.path = value`.
    Scalar(Vec<String>),
    /// `Seq(T)` field length: `#element.path = N`.
    SeqLen(Vec<String>),
    /// `Seq(T)` field element j: `element.path[j] = value`.
    SeqElem(Vec<String>, i64),
}

#[derive(Default)]
struct ElemPins {
    scalars:   HashMap<Vec<String>, Dynamic<'static>>,
    seq_lens:  HashMap<Vec<String>, i64>,
    seq_elems: HashMap<Vec<String>, BTreeMap<i64, Dynamic<'static>>>,
}

enum RawSeg { Acc(String), Index(i64) }

/// Peel a chain of `DT_ACCESSOR`/`SELECT` ops to the base `(select var idx)`.
/// Returns the base plus segments in element→outer order.
fn peel_chain(term: &Dynamic<'static>, var: &str)
    -> Option<(Dynamic<'static>, Vec<RawSeg>)>
{
    if term.kind() != AstKind::App { return None; }
    let decl = term.safe_decl().ok()?;
    match decl.kind() {
        DeclKind::SELECT => {
            let ch = term.children();
            if ch.len() != 2 { return None; }
            if ast_app_name(&ch[0]).as_deref() == Some(var) {
                numeral_to_i64(&ch[1])?;  // ensure literal index
                return Some((term.clone(), vec![]));
            }
            // Seq-element select: inner is an accessor chain; ch[1] is the element index.
            let j = numeral_to_i64(&ch[1])?;
            let (base, mut segs) = peel_chain(&ch[0], var)?;
            segs.push(RawSeg::Index(j));
            Some((base, segs))
        }
        DeclKind::DT_ACCESSOR => {
            let ch = term.children();
            if ch.len() != 1 { return None; }
            let (base, mut segs) = peel_chain(&ch[0], var)?;
            segs.push(RawSeg::Acc(decl.name()));
            Some((base, segs))
        }
        _ => None,
    }
}

/// Parse a pin LHS into `(base_select_term, element_idx, PinKind)`.
fn parse_pin(term: &Dynamic<'static>, var: &str)
    -> Option<(Dynamic<'static>, i64, PinKind)>
{
    let (base, segs) = peel_chain(term, var)?;
    let idx = numeral_to_i64(&base.children()[1])?;
    if segs.is_empty() { return None; }  // whole-element pin not handled here
    match segs.last()? {
        RawSeg::Index(j) => {
            let j = *j;
            // second-to-last seg is the `<field>__arr` accessor; strip suffix for path.
            let mut path = Vec::new();
            let last_acc = segs.len() - 2;
            for (k, s) in segs.iter().enumerate() {
                if let RawSeg::Acc(name) = s {
                    if k == last_acc {
                        path.push(name.strip_suffix("__arr")?.to_string());
                    } else {
                        path.push(name.clone());
                    }
                }
            }
            Some((base, idx, PinKind::SeqElem(path, j)))
        }
        RawSeg::Acc(name) => {
            if let Some(field) = name.strip_suffix("__len") {
                let mut path: Vec<String> = segs[..segs.len() - 1].iter()
                    .filter_map(|s| match s { RawSeg::Acc(n) => Some(n.clone()), _ => None })
                    .collect();
                path.push(field.to_string());
                Some((base, idx, PinKind::SeqLen(path)))
            } else if name.ends_with("__arr") {
                None  // array accessor without trailing index — unexpected
            } else {
                let path: Vec<String> = segs.iter()
                    .filter_map(|s| match s { RawSeg::Acc(n) => Some(n.clone()), _ => None })
                    .collect();
                Some((base, idx, PinKind::Scalar(path)))
            }
        }
    }
}

/// True when `v` is a `select`/accessor into another output (a cross-link, not a value).
/// Prefer the defining side (constructor/literal) when the same equality parses both ways.
fn is_crosslink(v: &Dynamic<'static>) -> bool {
    if v.kind() != AstKind::App { return false; }
    matches!(v.safe_decl().map(|d| d.kind()),
        Ok(DeclKind::SELECT) | Ok(DeclKind::DT_ACCESSOR))
}

/// Insert `val` for `key`, replacing an existing entry only if it's a cross-link and `val` isn't.
fn prefer_insert<K: std::hash::Hash + Eq + Ord>(
    map: &mut impl PinMap<K>, key: K, val: Dynamic<'static>,
) {
    let replace = match map.pin_get(&key) {
        None => true,
        Some(ex) => is_crosslink(ex) && !is_crosslink(&val),
    };
    if replace { map.pin_insert(key, val); }
}

/// Allows `prefer_insert` to work over both `HashMap` and `BTreeMap`.
trait PinMap<K> {
    fn pin_get(&self, k: &K) -> Option<&Dynamic<'static>>;
    fn pin_insert(&mut self, k: K, v: Dynamic<'static>);
}
impl PinMap<Vec<String>> for HashMap<Vec<String>, Dynamic<'static>> {
    fn pin_get(&self, k: &Vec<String>) -> Option<&Dynamic<'static>> { self.get(k) }
    fn pin_insert(&mut self, k: Vec<String>, v: Dynamic<'static>) { self.insert(k, v); }
}
impl PinMap<i64> for BTreeMap<i64, Dynamic<'static>> {
    fn pin_get(&self, k: &i64) -> Option<&Dynamic<'static>> { self.get(k) }
    fn pin_insert(&mut self, k: i64, v: Dynamic<'static>) { self.insert(k, v); }
}

/// Build the constructor application for one record element from its collected pins.
/// `prefix` = accessor path of this (possibly nested) record relative to element root.
fn build_record(
    prefix: &[String],
    fields: &[FieldKind],
    dt: &z3::DatatypeSort<'static>,
    pins: &ElemPins,
    ctx: &'static Context,
) -> Option<Dynamic<'static>> {
    let mut args: Vec<Dynamic<'static>> = Vec::new();
    for fk in fields {
        let mut path = prefix.to_vec();
        path.push(fk.name().to_string());
        match fk {
            FieldKind::Primitive { .. } => {
                args.push(pins.scalars.get(&path)?.clone());
            }
            FieldKind::Nested { dt: ndt, sub_fields, .. } => {
                args.push(build_record(&path, sub_fields, ndt, pins, ctx)?);
            }
            FieldKind::SeqField { .. } => {
                // Seq(T) field: two constructor args (Array(Int→T) + Int length).
                let elems = pins.seq_elems.get(&path)?;
                if elems.is_empty() { return None; }
                let len = pins.seq_lens.get(&path).copied()
                    .unwrap_or(elems.len() as i64);
                let int_sort = Sort::int(ctx);
                let default = elems.values().next()?;
                let mut arr = Array::const_array(ctx, &int_sort, default);
                for (&j, v) in elems {
                    arr = arr.store(&Int::from_i64(ctx, j), v);
                }
                args.push(Dynamic::from_ast(&arr));
                args.push(Dynamic::from_ast(&Int::from_i64(ctx, len)));
            }
        }
    }
    let arg_refs: Vec<&dyn Ast<'static>> =
        args.iter().map(|a| a as &dyn Ast<'static>).collect();
    Some(dt.variants.first()?.constructor.apply(&arg_refs))
}

/// Try to rebuild element constructors for one missing record-Seq output.
/// Returns per-element `Dynamic`s, or `None` if pins are incomplete.
fn try_recompose_one(
    assertions: &[Bool<'static>],
    var: &str,
    datatypes: &DatatypeRegistry,
    ctx: &'static Context,
) -> Option<Vec<Dynamic<'static>>> {
    let mut per_idx: HashMap<i64, ElemPins> = HashMap::new();
    let mut base_term: Option<Dynamic<'static>> = None;
    let mut max_idx: i64 = -1;

    for a in assertions {
        let Some((lhs, rhs)) = split_equality(a) else { continue };
        let parsed = parse_pin(&lhs, var).map(|(b, i, k)| (b, i, k, rhs.clone()))
            .or_else(|| parse_pin(&rhs, var).map(|(b, i, k)| (b, i, k, lhs.clone())));
        let Some((base, idx, kind, value)) = parsed else { continue };
        if base_term.is_none() { base_term = Some(base); }
        if idx > max_idx { max_idx = idx; }
        let e = per_idx.entry(idx).or_default();
        match kind {
            PinKind::Scalar(path) => { prefer_insert(&mut e.scalars, path, value); }
            PinKind::SeqLen(path) => {
                if let Some(n) = numeral_to_i64(&value) { e.seq_lens.insert(path, n); }
            }
            PinKind::SeqElem(path, j) => {
                prefer_insert(e.seq_elems.entry(path).or_default(), j, value);
            }
        }
    }

    let base = base_term?;
    if max_idx < 0 { return None; }
    let n = max_idx + 1;

    let sort_name = format!("{}", base.get_sort());  // element sort name keys DatatypeRegistry
    let dts = datatypes.borrow();
    let (dt, fields) = dts.get(&sort_name)?;

    let mut elems = Vec::with_capacity(n as usize);
    for i in 0..n {
        let pins = per_idx.get(&i)?;
        elems.push(build_record(&[], fields, dt, pins, ctx)?);
    }
    Some(elems)
}

/// Recompose `Seq(Record)` outputs left in `missing` into `Z3Step::Seq` steps.
pub fn recompose_record_seqs(
    assertions: &[Bool<'static>],
    missing: &mut Vec<String>,
    program: &mut Z3Program<'static>,
    datatypes: &DatatypeRegistry,
    ctx: &'static Context,
) {
    let targets: Vec<String> = missing.clone();
    let mut added = false;
    for var in targets {
        if let Some(elem_exprs) = try_recompose_one(assertions, &var, datatypes, ctx) {
            if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                eprintln!("[fz/z3] recomposed record-Seq {var:?} \
                          ({} elements)", elem_exprs.len());
            }
            program.steps.push(Z3Step::Seq { var: var.clone(), elem_exprs });
            missing.retain(|m| m != &var);
            added = true;
        }
    }
    // Re-topo-sort so recomposed Seq steps follow their scalar dependencies.
    if added {
        let steps = std::mem::take(&mut program.steps);
        program.steps = topo_sort_steps(steps);
    }
}

/// Sub-expressions a step evaluates, for dependency analysis.
fn step_exprs<'a>(step: &'a Z3Step<'static>) -> Vec<&'a Dynamic<'static>> {
    match step {
        Z3Step::Scalar { expr, .. } => vec![expr],
        Z3Step::Seq { elem_exprs, .. } => elem_exprs.iter().collect(),
        Z3Step::Guarded { branches, .. } => {
            let mut v = Vec::new();
            for b in branches {
                v.push(&b.guard);
                match &b.body {
                    GuardedBody::Scalar(e) => v.push(e),
                    GuardedBody::Seq(es)   => v.extend(es.iter()),
                }
            }
            v
        }
        Z3Step::PreBaked { .. } => vec![],
        Z3Step::SampleRange { .. }
        | Z3Step::SampleEnum { .. }
        | Z3Step::SampleSet { .. } => vec![],
    }
}

/// Topologically order steps by output-variable dependency (Kahn's algorithm).
/// On a cycle (shouldn't happen for function-shaped bodies) returns original order.
fn topo_sort_steps(steps: Vec<Z3Step<'static>>) -> Vec<Z3Step<'static>> {
    let n = steps.len();
    let names: Vec<String> = steps.iter().map(|s| s.var().to_string()).collect();
    let mut indeg = vec![0usize; n];
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        let exprs = step_exprs(&steps[i]);
        for j in 0..n {
            if i == j { continue; }
            if exprs.iter().any(|e| mentions_name(e, &names[j])) {
                succ[j].push(i);
                indeg[i] += 1;
            }
        }
    }
    let mut ready: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    let mut order: Vec<usize> = Vec::with_capacity(n);
    while let Some(i) = ready.pop() {
        order.push(i);
        for &j in &succ[i] {
            indeg[j] -= 1;
            if indeg[j] == 0 { ready.push(j); }
        }
    }
    if order.len() != n {
        if std::env::var("EVIDENT_JIT_TRACE").is_ok() {
            eprintln!("[fz/z3] topo_sort_steps: cycle ({}/{} ordered) — order unchanged",
                order.len(), n);
        }
        return steps;
    }
    let mut slots: Vec<Option<Z3Step<'static>>> = steps.into_iter().map(Some).collect();
    order.into_iter().map(|i| slots[i].take().unwrap()).collect()
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
    if order.len() != outputs.len() { return None; }  // cycle
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
    Some(Z3Program { steps, checks, predicates, label: None })
}

/// Return the inner Bool of `(not X)`, or `None`.
fn try_negation<'ctx>(a: &Bool<'ctx>) -> Option<Bool<'ctx>> {
    if a.kind() != AstKind::App { return None; }
    let decl = a.safe_decl().ok()?;
    if decl.kind() != DeclKind::NOT { return None; }
    let mut iter = a.children().into_iter();
    let child = iter.next()?;
    child.as_bool()
}

/// Recognize `(or (not P) Q)` — Z3's canonical form for `P ⇒ Q`. Returns `(P, Q)` or `None`.
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

/// If the guarded consequent constrains an output variable (scalar or Seq),
/// record a branch in `guarded`.
fn classify_guarded_consequent<'ctx>(
    conseq: &Bool<'ctx>,
    output_set: &std::collections::HashSet<&str>,
    guarded: &mut HashMap<String, Vec<GuardedBranch<'ctx>>>,
    guard: &Dynamic<'ctx>,
) -> Option<()> {
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
                // Mixed AND: split per-output; each child must be a scalar or Seq pin.
                if let Some(()) = classify_mixed_and(conseq, output_set, guarded, guard) {
                    return Some(());
                }
            }
        }
    }
    // `(= var__len 0)` alone — empty Seq guarded branch.
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

/// Handle a guarded AND consequent that constrains multiple outputs. Each gets its own
/// branch in `guarded`. Returns `Some(())` iff every AND child is classifiable.
fn classify_mixed_and<'ctx>(
    and_expr: &Bool<'ctx>,
    output_set: &std::collections::HashSet<&str>,
    guarded: &mut HashMap<String, Vec<GuardedBranch<'ctx>>>,
    guard: &Dynamic<'ctx>,
) -> Option<()> {
    let mut scalar_assigns: Vec<(String, Dynamic<'ctx>)> = Vec::new();
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
    let mut all_names: std::collections::HashSet<String> = seq_lens.keys().cloned().collect();
    for k in seq_elems.keys() { all_names.insert(k.clone()); }
    for name in &all_names {
        let n = seq_lens.get(name).copied().unwrap_or_else(|| {
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

/// `split_equality` on a `Bool` obtained via `try_guarded`'s `as_bool()` round-trip.
fn split_equality_dyn<'ctx>(b: &Bool<'ctx>) -> Option<(Dynamic<'ctx>, Dynamic<'ctx>)> {
    split_equality(b)
}

/// Match `(and (= arr__len N) (= (select arr 0) e0) …)` and return the arr name + elements.
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
        return None;
    }

    let arr = arr_name?;
    let n = declared_len?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        out.push(indexed.remove(&i)?);
    }
    Some((arr, out))
}

/// Match `(= var__len N)`, returning the seq base name and length.
fn match_len_pin<'ctx>(a: &Dynamic<'ctx>, b: &Dynamic<'ctx>) -> Option<(String, i64)> {
    let name = ast_app_name(a)?;
    let arr = name.strip_suffix("__len")?;
    let n = numeral_to_i64(b)?;
    Some((arr.to_string(), n))
}

/// Match `(= (select arr idx_literal) elem)`.
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

/// Extract a literal i64 from a Z3 Numeral node.
fn numeral_to_i64<'ctx>(d: &Dynamic<'ctx>) -> Option<i64> {
    if d.kind() != AstKind::Numeral { return None; }
    d.as_int().and_then(|i| i.as_i64())
}

/// Return `(lhs, rhs)` if `b` is `(= lhs rhs)`, else `None`.
fn split_equality<'ctx>(b: &Bool<'ctx>) -> Option<(Dynamic<'ctx>, Dynamic<'ctx>)> {
    if b.kind() != AstKind::App { return None; }
    let decl = b.safe_decl().ok()?;
    if decl.kind() != DeclKind::EQ { return None; }
    let children = b.children();
    if children.len() != 2 { return None; }
    Some((children[0].clone(), children[1].clone()))
}

/// Return the name of a 0-arity App (Z3 constant/variable), or `None`.
fn ast_app_name<'ctx>(a: &Dynamic<'ctx>) -> Option<String> {
    if a.kind() != AstKind::App { return None; }
    if a.num_children() != 0 { return None; }
    let decl = a.safe_decl().ok()?;
    Some(decl.name())
}

/// True if `a`'s AST tree contains a 0-arity App with the given name.
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


/// True when the body contains a known translator gap that blocks functionization.
/// Current gap: `Ctor(SeqLit(…))` — enum constructor with a sequence-literal payload.
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

/// Collect every uninterpreted 0-arity App name in `a` into `out`.
/// Vars not appearing in the simplified body can't be functionizer outputs.
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

/// Extract variant name from `((_ is <Variant>) <arg>)`.
/// Workaround: z3 0.12 bindings don't expose FuncDecl params.
pub fn extract_is_variant_pub(s: &str) -> Option<String> {
    let idx = s.find("(_ is ")?;
    let rest = &s[idx + 6 ..];   // after "(_ is "
    let end = rest.find(|c: char| c.is_whitespace() || c == ')')?;
    Some(rest[..end].to_string())
}
