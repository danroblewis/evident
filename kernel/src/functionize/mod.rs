//! Functionizer (Z3-tactic version) for the kernel tick loop.
//!
//! Design + reference: `docs/plans/functionizer-integration.md` and the
//! high-level-`z3` reference port in `legacy-rust/functionizer/` (`z3_eval.rs`,
//! `z3_program.rs`, `cranelift.rs`). This is a raw-`z3-sys` re-port targeting
//! the kernel's SMT-LIB pipeline (the kernel uses `z3-sys`, not the `z3`
//! crate), per the integration doc §2 option (a) "keep the kernel minimal".
//!
//! Pipeline:
//!   1. `simplify_assertions` — Z3 tactic chain `simplify` + `propagate-values`
//!      over the cached body assertions (matches the reference; `solve-eqs` is
//!      deliberately excluded so `(= var expr)` shapes survive).
//!   2. `extract_program` — partition the simplified assertions, keyed by the
//!      manifest's state fields + `effects`, into per-output `Step`s
//!      (`Scalar` / `Guarded` / `Seq`). Any output without a defining
//!      assignment ⇒ `None` ⇒ the whole tick falls through to a real Z3 solve.
//!   3. JIT — each scalar Int/Bool step is handed to `jit::compile_step`;
//!      steps that compile call native code per tick, the rest interpret.
//!   4. Verify — before returning, run the extracted program against a real Z3
//!      solve on tick 0 AND tick 1 and compare state + effects. A mismatch
//!      disables the fast path entirely (returns `None`). This makes the fast
//!      path sound even though the extractor is conservative: a shape we
//!      mis-read can never produce wrong output — it just reverts to Z3.
//!
//! Env flags (read in `tick.rs`):
//!   - `EVIDENT_FUNCTIONIZE=0`   — bypass extraction + fast path entirely.
//!   - `EVIDENT_FUNCTIONIZE_JIT=0` — extract + interpret, but don't JIT.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use z3_sys::*;

use crate::manifest::Manifest;
use crate::tick::{self, Sv};

pub mod eval;
pub mod jit;

// ── Program IR ──────────────────────────────────────────────────

pub enum GBody {
    Scalar(Z3_ast),
    Seq(Vec<Z3_ast>),
}

pub struct Branch {
    /// The guard AST. When `neg` is set, the branch fires on its *negation*
    /// — this captures `(or X Q)` ⇒ `¬X ⇒ Q` where Z3 emitted the negated
    /// guard `X` as a plain predicate rather than `(not …)`.
    pub guard: Z3_ast,
    pub neg: bool,
    pub body: GBody,
}

pub enum StepBody {
    Scalar(Z3_ast),
    Seq(Vec<Z3_ast>),
    Guarded(Vec<Branch>),
}

pub struct Step {
    pub var: String,
    pub body: StepBody,
    /// Bool-sorted scalar output (vs Int). Selects how a JIT i64 result and an
    /// eval result are interpreted. Irrelevant for Seq steps.
    pub result_is_bool: bool,
    /// Present only for scalar Int/Bool steps the JIT accepted.
    pub jit: Option<jit::JitStep>,
}

pub struct Program {
    pub steps: Vec<Step>,
    pub predicates: Vec<Z3_ast>,
    /// Number of scalar steps the JIT compiled vs interpreted (reporting).
    pub jit_count: usize,
    pub interp_count: usize,
    /// inc_ref'd simplified formulas; keeps every sub-AST in `steps` alive for
    /// the program's (process) lifetime. Never dec_ref'd — the kernel is a
    /// short-lived process.
    _keepalive: Vec<Z3_ast>,
}

pub struct RunOut {
    pub scalars: HashMap<String, Sv>,
    pub effects: Vec<Sv>,
}

// ── AST helpers (shared with eval.rs / jit.rs) ──────────────────

pub(crate) unsafe fn decl_kind(ctx: Z3_context, a: Z3_ast) -> Option<DeclKind> {
    if Z3_get_ast_kind(ctx, a) != AstKind::App {
        return None;
    }
    let app = Z3_to_app(ctx, a);
    if app.is_null() {
        return None;
    }
    Some(Z3_get_decl_kind(ctx, Z3_get_app_decl(ctx, app)))
}

pub(crate) unsafe fn app_decl_name(ctx: Z3_context, a: Z3_ast) -> Option<String> {
    if Z3_get_ast_kind(ctx, a) != AstKind::App {
        return None;
    }
    let app = Z3_to_app(ctx, a);
    if app.is_null() {
        return None;
    }
    let decl = Z3_get_app_decl(ctx, app);
    Some(tick::decode_sym_pub(ctx, Z3_get_decl_name(ctx, decl)))
}

pub(crate) unsafe fn children(ctx: Z3_context, a: Z3_ast) -> Vec<Z3_ast> {
    let app = Z3_to_app(ctx, a);
    if app.is_null() {
        return Vec::new();
    }
    let n = Z3_get_app_num_args(ctx, app);
    (0..n).map(|i| Z3_get_app_arg(ctx, app, i)).collect()
}

/// Name of a 0-arity application (a Z3 "constant"/variable).
pub(crate) unsafe fn ast_app_name(ctx: Z3_context, a: Z3_ast) -> Option<String> {
    if Z3_get_ast_kind(ctx, a) != AstKind::App {
        return None;
    }
    let app = Z3_to_app(ctx, a);
    if app.is_null() || Z3_get_app_num_args(ctx, app) != 0 {
        return None;
    }
    let decl = Z3_get_app_decl(ctx, app);
    Some(tick::decode_sym_pub(ctx, Z3_get_decl_name(ctx, decl)))
}

unsafe fn numeral_i64(ctx: Z3_context, a: Z3_ast) -> Option<i64> {
    if Z3_get_ast_kind(ctx, a) != AstKind::Numeral {
        return None;
    }
    let mut n: i64 = 0;
    if Z3_get_numeral_int64(ctx, a, &mut n) {
        Some(n)
    } else {
        None
    }
}

/// Does `a`'s tree mention a 0-arity application named `name`?
pub(crate) unsafe fn mentions_name(ctx: Z3_context, a: Z3_ast, name: &str) -> bool {
    if let Some(n) = ast_app_name(ctx, a) {
        if n == name {
            return true;
        }
    }
    for c in children(ctx, a) {
        if mentions_name(ctx, c, name) {
            return true;
        }
    }
    false
}

/// Collect every free 0-arity uninterpreted constant name in `a`.
pub(crate) unsafe fn collect_inputs(ctx: Z3_context, a: Z3_ast, out: &mut HashSet<String>) {
    if Z3_get_ast_kind(ctx, a) == AstKind::App {
        let app = Z3_to_app(ctx, a);
        if !app.is_null() && Z3_get_app_num_args(ctx, app) == 0 {
            let dk = Z3_get_decl_kind(ctx, Z3_get_app_decl(ctx, app));
            if dk == DeclKind::UNINTERPRETED {
                out.insert(tick::decode_sym_pub(ctx, Z3_get_decl_name(ctx, Z3_get_app_decl(ctx, app))));
            }
            return;
        }
        for c in children(ctx, a) {
            collect_inputs(ctx, c, out);
        }
    }
}

unsafe fn is_bool_sorted(ctx: Z3_context, a: Z3_ast) -> bool {
    Z3_get_sort_kind(ctx, Z3_get_sort(ctx, a)) == SortKind::Bool
}

// ── Step 1: tactic chain ────────────────────────────────────────

unsafe fn simplify_assertions(ctx: Z3_context, body: &[Z3_ast]) -> Vec<Z3_ast> {
    let goal = Z3_mk_goal(ctx, false, false, false);
    Z3_goal_inc_ref(ctx, goal);
    for &a in body {
        Z3_goal_assert(ctx, goal, a);
    }
    let c_simplify = CString::new("simplify").unwrap();
    let c_propagate = CString::new("propagate-values").unwrap();
    let t1 = Z3_mk_tactic(ctx, c_simplify.as_ptr());
    Z3_tactic_inc_ref(ctx, t1);
    let t2 = Z3_mk_tactic(ctx, c_propagate.as_ptr());
    Z3_tactic_inc_ref(ctx, t2);
    let chain = Z3_tactic_and_then(ctx, t1, t2);
    Z3_tactic_inc_ref(ctx, chain);

    let res = Z3_tactic_apply(ctx, chain, goal);
    Z3_apply_result_inc_ref(ctx, res);

    let mut out = Vec::new();
    let ng = Z3_apply_result_get_num_subgoals(ctx, res);
    for i in 0..ng {
        let sg = Z3_apply_result_get_subgoal(ctx, res, i);
        Z3_goal_inc_ref(ctx, sg);
        let n = Z3_goal_size(ctx, sg);
        for j in 0..n {
            let f = Z3_goal_formula(ctx, sg, j);
            Z3_inc_ref(ctx, f);
            out.push(f);
        }
        Z3_goal_dec_ref(ctx, sg);
    }

    Z3_apply_result_dec_ref(ctx, res);
    Z3_tactic_dec_ref(ctx, chain);
    Z3_tactic_dec_ref(ctx, t2);
    Z3_tactic_dec_ref(ctx, t1);
    Z3_goal_dec_ref(ctx, goal);
    out
}

/// Split top-level conjunctions into separate assertions so `extract_program`
/// sees each `(=> P Q)` / `(= var expr)` clause individually. The tactic chain
/// does not always flatten a top-level `(and …)` into separate goal formulas.
unsafe fn flatten_conjunctions(ctx: Z3_context, asts: &[Z3_ast]) -> Vec<Z3_ast> {
    fn push(ctx: Z3_context, a: Z3_ast, out: &mut Vec<Z3_ast>) {
        unsafe {
            if decl_kind(ctx, a) == Some(DeclKind::AND) {
                for c in children(ctx, a) {
                    push(ctx, c, out);
                }
            } else {
                out.push(a);
            }
        }
    }
    let mut out = Vec::new();
    for &a in asts {
        push(ctx, a, &mut out);
    }
    out
}

fn trace_enabled() -> bool {
    std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok()
}

// ── Step 2: extraction ──────────────────────────────────────────

unsafe fn split_equality(ctx: Z3_context, a: Z3_ast) -> Option<(Z3_ast, Z3_ast)> {
    if decl_kind(ctx, a)? != DeclKind::EQ {
        return None;
    }
    let ch = children(ctx, a);
    if ch.len() != 2 {
        return None;
    }
    Some((ch[0], ch[1]))
}

/// Try to read `a` as a guarded implication that constrains an output, and
/// record the branch in `raw`. Handles `(=> P Q)` (guard `P`) and `(or X Q)`
/// (guard `¬X`, with `Q` whichever side constrains an output).
unsafe fn try_record_guarded(
    ctx: Z3_context,
    a: Z3_ast,
    outputs: &HashSet<String>,
    raw: &mut Raw,
) -> bool {
    let Some(dk) = decl_kind(ctx, a) else { return false };
    let ch = children(ctx, a);
    if dk == DeclKind::IMPLIES && ch.len() == 2 {
        return classify_guarded(ctx, ch[1], ch[0], false, outputs, raw);
    }
    if dk == DeclKind::OR && ch.len() == 2 {
        // `(or X Q)` ≡ `¬X ⇒ Q`. Try each side as the consequent.
        return classify_guarded(ctx, ch[1], ch[0], true, outputs, raw)
            || classify_guarded(ctx, ch[0], ch[1], true, outputs, raw);
    }
    false
}

/// `(= var__len N)` ⇒ `(var, N)`.
unsafe fn match_len_pin(ctx: Z3_context, l: Z3_ast, r: Z3_ast) -> Option<(String, i64)> {
    let name = ast_app_name(ctx, l)?;
    let base = name.strip_suffix("__len")?;
    let n = numeral_i64(ctx, r)?;
    Some((base.to_string(), n))
}

/// `(= (select arr idx_lit) elem)` ⇒ `(arr, idx, elem)`.
unsafe fn match_select_pin(ctx: Z3_context, l: Z3_ast, r: Z3_ast) -> Option<(String, i64, Z3_ast)> {
    if decl_kind(ctx, l)? != DeclKind::SELECT {
        return None;
    }
    let ch = children(ctx, l);
    if ch.len() != 2 {
        return None;
    }
    let arr = ast_app_name(ctx, ch[0])?;
    let idx = numeral_i64(ctx, ch[1])?;
    Some((arr, idx, r))
}

/// `(and (= arr__len N) (= (select arr 0) e0) …)` over a single output `arr`.
unsafe fn collect_seq_in_and(
    ctx: Z3_context,
    and_expr: Z3_ast,
    outputs: &HashSet<String>,
) -> Option<(String, Vec<Z3_ast>)> {
    if decl_kind(ctx, and_expr)? != DeclKind::AND {
        return None;
    }
    let mut arr_name: Option<String> = None;
    let mut declared_len: Option<i64> = None;
    let mut indexed: HashMap<i64, Z3_ast> = HashMap::new();
    for c in children(ctx, and_expr) {
        let (l, r) = split_equality(ctx, c)?;
        if let Some((name, n)) = match_len_pin(ctx, l, r).or_else(|| match_len_pin(ctx, r, l)) {
            if !outputs.contains(&name) {
                return None;
            }
            match &arr_name {
                Some(p) if *p != name => return None,
                _ => arr_name = Some(name),
            }
            declared_len = Some(n);
            continue;
        }
        if let Some((name, idx, elem)) =
            match_select_pin(ctx, l, r).or_else(|| match_select_pin(ctx, r, l))
        {
            if !outputs.contains(&name) {
                return None;
            }
            match &arr_name {
                Some(p) if *p != name => return None,
                _ => arr_name = Some(name),
            }
            indexed.insert(idx, elem);
            continue;
        }
        return None;
    }
    let arr = arr_name?;
    let n = declared_len.unwrap_or(indexed.len() as i64);
    let mut elems = Vec::with_capacity(n as usize);
    for i in 0..n {
        elems.push(*indexed.get(&i)?);
    }
    Some((arr, elems))
}

#[derive(Default)]
struct Raw {
    scalar: HashMap<String, Z3_ast>,
    seq_lengths: HashMap<String, i64>,
    seq_elements: HashMap<String, HashMap<i64, Z3_ast>>,
    guarded: HashMap<String, Vec<Branch>>,
    predicates: Vec<Z3_ast>,
}

/// Classify a guarded consequent `Q` (under guard `P`); returns true if it
/// constrained an output.
unsafe fn classify_guarded(
    ctx: Z3_context,
    conseq: Z3_ast,
    guard: Z3_ast,
    neg: bool,
    outputs: &HashSet<String>,
    raw: &mut Raw,
) -> bool {
    // `Q = (= var expr)` — scalar guarded.
    if let Some((l, r)) = split_equality(ctx, conseq) {
        if let Some(name) = ast_app_name(ctx, l) {
            if outputs.contains(&name) && !mentions_name(ctx, r, &name) {
                raw.guarded.entry(name).or_default().push(Branch { guard, neg, body: GBody::Scalar(r) });
                return true;
            }
        }
        if let Some(name) = ast_app_name(ctx, r) {
            if outputs.contains(&name) && !mentions_name(ctx, l, &name) {
                raw.guarded.entry(name).or_default().push(Branch { guard, neg, body: GBody::Scalar(l) });
                return true;
            }
        }
    }
    // `Q = (and (= var__len N) (= (select var i) e) …)` — seq guarded.
    if let Some((arr, elems)) = collect_seq_in_and(ctx, conseq, outputs) {
        raw.guarded.entry(arr).or_default().push(Branch { guard, neg, body: GBody::Seq(elems) });
        return true;
    }
    false
}

/// Build the raw partition. `None` if any output lacks a covering assignment.
unsafe fn extract_program(
    ctx: Z3_context,
    assertions: &[Z3_ast],
    outputs: &[String],
) -> Option<(Vec<(String, StepBody)>, Vec<Z3_ast>)> {
    let output_set: HashSet<String> = outputs.iter().cloned().collect();
    let mut raw = Raw::default();

    for &a in assertions {
        if try_record_guarded(ctx, a, &output_set, &mut raw) {
            continue;
        }
        let Some((l, r)) = split_equality(ctx, a) else {
            raw.predicates.push(a);
            continue;
        };
        if let Some((name, n)) = match_len_pin(ctx, l, r).or_else(|| match_len_pin(ctx, r, l)) {
            if output_set.contains(&name) {
                raw.seq_lengths.insert(name, n);
                continue;
            }
        }
        if let Some((arr, idx, elem)) =
            match_select_pin(ctx, l, r).or_else(|| match_select_pin(ctx, r, l))
        {
            if output_set.contains(&arr) {
                raw.seq_elements.entry(arr).or_default().insert(idx, elem);
                continue;
            }
        }
        if let Some(name) = ast_app_name(ctx, l) {
            if output_set.contains(&name) && !raw.scalar.contains_key(&name) && !mentions_name(ctx, r, &name) {
                raw.scalar.insert(name, r);
                continue;
            }
        }
        if let Some(name) = ast_app_name(ctx, r) {
            if output_set.contains(&name) && !raw.scalar.contains_key(&name) && !mentions_name(ctx, l, &name) {
                raw.scalar.insert(name, l);
                continue;
            }
        }
        // A consistency check between non-outputs — keep as a predicate.
        raw.predicates.push(a);
    }

    // Assemble per-output bodies.
    let mut bodies: HashMap<String, StepBody> = HashMap::new();
    for v in outputs {
        if let Some(e) = raw.scalar.remove(v) {
            bodies.insert(v.clone(), StepBody::Scalar(e));
            continue;
        }
        if let Some(branches) = raw.guarded.remove(v) {
            if !branches.is_empty() {
                bodies.insert(v.clone(), StepBody::Guarded(branches));
                continue;
            }
        }
        // Seq from explicit/inferred length + contiguous element pins.
        let elems = raw.seq_elements.get(v);
        let explicit = raw.seq_lengths.get(v).copied();
        let inferred = elems.and_then(|m| {
            let mut i = 0i64;
            while m.contains_key(&i) { i += 1; }
            if i == 0 { None } else { Some(i) }
        });
        let n = match (explicit, inferred) {
            (Some(e), _) => e,
            (None, Some(i)) => i,
            (None, None) => return None, // uncovered output
        };
        let map = elems?;
        let mut seq = Vec::with_capacity(n as usize);
        for i in 0..n {
            seq.push(*map.get(&i)?);
        }
        bodies.insert(v.clone(), StepBody::Seq(seq));
    }

    // Topo-order by reference to other outputs.
    let order = topo_order(ctx, outputs, &bodies)?;
    let ordered: Vec<(String, StepBody)> =
        order.into_iter().map(|v| (v.clone(), bodies.remove(&v).unwrap())).collect();
    Some((ordered, raw.predicates))
}

unsafe fn body_exprs<'a>(body: &'a StepBody) -> Vec<Z3_ast> {
    match body {
        StepBody::Scalar(e) => vec![*e],
        StepBody::Seq(es) => es.clone(),
        StepBody::Guarded(branches) => {
            let mut v = Vec::new();
            for b in branches {
                v.push(b.guard);
                match &b.body {
                    GBody::Scalar(e) => v.push(*e),
                    GBody::Seq(es) => v.extend(es.iter().copied()),
                }
            }
            v
        }
    }
}

unsafe fn topo_order(
    ctx: Z3_context,
    outputs: &[String],
    bodies: &HashMap<String, StepBody>,
) -> Option<Vec<String>> {
    let mut indeg: HashMap<String, usize> = outputs.iter().map(|v| (v.clone(), 0)).collect();
    let mut succ: HashMap<String, Vec<String>> = HashMap::new();
    for v in outputs {
        let exprs = body_exprs(bodies.get(v)?);
        for other in outputs {
            if other == v {
                continue;
            }
            if exprs.iter().any(|&e| mentions_name(ctx, e, other)) {
                *indeg.get_mut(v).unwrap() += 1;
                succ.entry(other.clone()).or_default().push(v.clone());
            }
        }
    }
    let mut ready: Vec<String> = indeg.iter().filter(|(_, &d)| d == 0).map(|(n, _)| n.clone()).collect();
    ready.sort();
    let mut order = Vec::with_capacity(outputs.len());
    while let Some(n) = ready.pop() {
        order.push(n.clone());
        if let Some(s) = succ.get(&n) {
            for m in s {
                let d = indeg.get_mut(m).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.push(m.clone());
                }
            }
        }
        ready.sort();
    }
    if order.len() == outputs.len() {
        Some(order)
    } else {
        None // cycle — not function-shaped
    }
}

// ── Step 3+4: assemble, JIT, verify ─────────────────────────────

/// Build the `Program` for this body, or `None` to leave the kernel on the
/// existing Z3 path. `decl_preamble` is the body's declaration s-expressions
/// (from `tick::extract_declarations`) used by the verification solves.
pub unsafe fn functionize(
    ctx: Z3_context,
    body: &[Z3_ast],
    manifest: &Manifest,
    decl_preamble: &str,
    jit_enabled: bool,
) -> Option<Program> {
    let simplified = simplify_assertions(ctx, body);
    let flat = flatten_conjunctions(ctx, &simplified);
    if std::env::var("EVIDENT_FUNCTIONIZE_DUMP").is_ok() {
        for (i, &a) in flat.iter().enumerate() {
            let p = Z3_ast_to_string(ctx, a);
            let s = if p.is_null() { String::new() } else { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() };
            eprintln!("[fz/dump] flat[{i}] = {s}");
        }
    }

    let mut outputs: Vec<String> = manifest.state_fields.iter().map(|(n, _)| n.clone()).collect();
    outputs.push(manifest.effects_name.clone());

    let Some((raw_steps, predicates)) = extract_program(ctx, &flat, &outputs) else {
        if trace_enabled() {
            eprintln!("[fz] extract_program: an output had no covering assignment");
        }
        return None;
    };

    // Enforce: each state field is a scalar/guarded-scalar; effects is a
    // seq/guarded-seq. Anything else is unsupported ⇒ refuse.
    let mut steps: Vec<Step> = Vec::new();
    let mut jit_count = 0usize;
    let mut interp_count = 0usize;
    for (var, body) in raw_steps {
        let is_effects = var == manifest.effects_name;
        match &body {
            StepBody::Seq(_) => {
                if !is_effects {
                    return None;
                }
            }
            StepBody::Guarded(branches) => {
                let seqish = branches.iter().any(|b| matches!(b.body, GBody::Seq(_)));
                if seqish != is_effects {
                    return None;
                }
            }
            StepBody::Scalar(_) => {
                if is_effects {
                    return None;
                }
            }
        }

        let (result_is_bool, jit) = match &body {
            StepBody::Scalar(e) => {
                let is_bool = is_bool_sorted(ctx, *e);
                let mut j = None;
                if jit_enabled && (is_bool || Z3_get_sort_kind(ctx, Z3_get_sort(ctx, *e)) == SortKind::Int) {
                    let mut set = HashSet::new();
                    collect_inputs(ctx, *e, &mut set);
                    let inputs: Vec<String> = set.into_iter().collect();
                    j = jit::compile_step(ctx, *e, &inputs);
                }
                if j.is_some() { jit_count += 1; } else { interp_count += 1; }
                (is_bool, j)
            }
            _ => (false, None),
        };
        steps.push(Step { var, body, result_is_bool, jit });
    }

    let mut keepalive = simplified;
    keepalive.shrink_to_fit();
    let prog = Program { steps, predicates, jit_count, interp_count, _keepalive: keepalive };

    // ── Verify on tick 0 and tick 1 against a real Z3 solve. ──
    let empty_prev: Vec<Option<Sv>> = vec![None; manifest.state_fields.len()];
    let Ok(Some(z3_0)) = tick::solve_tick_sv(ctx, body, decl_preamble, manifest, true, &empty_prev) else {
        if trace_enabled() { eprintln!("[fz] verify: tick-0 Z3 solve failed"); }
        return None;
    };
    let Some(mine_0) = run_program(ctx, &prog, &build_inputs(true, &empty_prev, manifest)) else {
        if trace_enabled() { eprintln!("[fz] verify: tick-0 eval refused (unsupported op)"); }
        return None;
    };
    if !outputs_match(manifest, &z3_0, &mine_0) {
        if trace_enabled() { eprintln!("[fz] verify: tick-0 mismatch vs Z3"); }
        return None;
    }
    let prev1: Vec<Option<Sv>> = z3_0.0.iter().cloned().map(Some).collect();
    let Ok(Some(z3_1)) = tick::solve_tick_sv(ctx, body, decl_preamble, manifest, false, &prev1) else {
        if trace_enabled() { eprintln!("[fz] verify: tick-1 Z3 solve failed"); }
        return None;
    };
    let Some(mine_1) = run_program(ctx, &prog, &build_inputs(false, &prev1, manifest)) else {
        if trace_enabled() { eprintln!("[fz] verify: tick-1 eval refused (unsupported op)"); }
        return None;
    };
    if !outputs_match(manifest, &z3_1, &mine_1) {
        if trace_enabled() { eprintln!("[fz] verify: tick-1 mismatch vs Z3"); }
        return None;
    }

    Some(prog)
}

/// Inputs for a tick: `is_first_tick` + each `_<name>` state-carry.
pub fn build_inputs(is_first: bool, prev_state: &[Option<Sv>], manifest: &Manifest) -> HashMap<String, Sv> {
    let mut env = HashMap::new();
    env.insert("is_first_tick".to_string(), Sv::Bool(is_first));
    for (i, (name, ty)) in manifest.state_fields.iter().enumerate() {
        let key = format!("_{name}");
        if is_first {
            // On tick 0 the `_<name>` carries are unconstrained; supply a
            // type-correct sentinel so a JIT step that eagerly loads `_<name>`
            // (e.g. the untaken `ite` arm) has a slot. Under the is_first
            // branch the value is never observed.
            if ty == "Int" {
                env.insert(key, Sv::Int(0));
            } else if ty == "Bool" {
                env.insert(key, Sv::Bool(false));
            }
        } else if let Some(v) = &prev_state[i] {
            env.insert(key, v.clone());
        }
    }
    env
}

/// Run the extracted program for one tick. `None` ⇒ a shape/predicate the fast
/// path can't honour ⇒ caller falls through to Z3.
pub unsafe fn run_program(ctx: Z3_context, prog: &Program, inputs: &HashMap<String, Sv>) -> Option<RunOut> {
    let mut env = inputs.clone();
    let mut effects: Vec<Sv> = Vec::new();

    for step in &prog.steps {
        match &step.body {
            StepBody::Scalar(ast) => {
                let v = if let Some(j) = &step.jit {
                    match j.call(&env) {
                        Some(r) => if step.result_is_bool { Sv::Bool(r != 0) } else { Sv::Int(r) },
                        None => {
                            if trace_enabled() {
                                eprintln!("[fz/run] scalar step {:?} JIT call refused; inputs={:?} env keys={:?}",
                                    step.var, j.inputs, env.keys().collect::<Vec<_>>());
                            }
                            return None;
                        }
                    }
                } else {
                    match eval::eval_scalar(ctx, *ast, &env) {
                        Some(v) => v,
                        None => {
                            if trace_enabled() { eprintln!("[fz/run] scalar step {:?} eval refused", step.var); }
                            return None;
                        }
                    }
                };
                env.insert(step.var.clone(), v);
            }
            StepBody::Seq(asts) => {
                let mut seq = Vec::with_capacity(asts.len());
                for &e in asts {
                    match eval::eval_scalar(ctx, e, &env) {
                        Some(v) => seq.push(v),
                        None => {
                            if trace_enabled() { eprintln!("[fz/run] seq step {:?} elem eval refused", step.var); }
                            return None;
                        }
                    }
                }
                effects = seq;
            }
            StepBody::Guarded(branches) => {
                let mut chosen: Option<&GBody> = None;
                for b in branches {
                    match eval::eval_scalar(ctx, b.guard, &env) {
                        Some(Sv::Bool(g)) => {
                            let fires = if b.neg { !g } else { g };
                            if fires { chosen = Some(&b.body); break; }
                        }
                        Some(_) => {}
                        None => {
                            if trace_enabled() { eprintln!("[fz/run] guarded step {:?} guard eval refused", step.var); }
                            return None;
                        }
                    }
                }
                let Some(body) = chosen else {
                    if trace_enabled() { eprintln!("[fz/run] guarded step {:?}: no branch guard matched", step.var); }
                    return None;
                };
                match body {
                    GBody::Scalar(e) => {
                        let Some(v) = eval::eval_scalar(ctx, *e, &env) else {
                            if trace_enabled() { eprintln!("[fz/run] guarded step {:?} scalar body refused", step.var); }
                            return None;
                        };
                        env.insert(step.var.clone(), v);
                    }
                    GBody::Seq(es) => {
                        let mut seq = Vec::with_capacity(es.len());
                        for &e in es {
                            match eval::eval_scalar(ctx, e, &env) {
                                Some(v) => seq.push(v),
                                None => {
                                    if trace_enabled() { eprintln!("[fz/run] guarded step {:?} seq elem refused", step.var); }
                                    return None;
                                }
                            }
                        }
                        effects = seq;
                    }
                }
            }
        }
    }

    // Enforce predicates that reference only bound vars. A predicate that
    // evaluates false ⇒ this tick is UNSAT for the fast path ⇒ fall through.
    for &p in &prog.predicates {
        if let Some(Sv::Bool(b)) = eval::eval_scalar(ctx, p, &env) {
            if !b {
                return None;
            }
        }
    }

    Some(RunOut { scalars: env, effects })
}

type Z3Tick = (Vec<Sv>, Vec<Sv>);

fn outputs_match(manifest: &Manifest, z3: &Z3Tick, mine: &RunOut) -> bool {
    for (i, (name, _)) in manifest.state_fields.iter().enumerate() {
        match mine.scalars.get(name) {
            Some(v) if tick::compare_sv_pub(v, &z3.0[i]) => {}
            _ => return false,
        }
    }
    if mine.effects.len() != z3.1.len() {
        return false;
    }
    mine.effects.iter().zip(z3.1.iter()).all(|(a, b)| tick::compare_sv_pub(a, b))
}
