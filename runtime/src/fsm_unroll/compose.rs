//! Substitution-based exponentiation-by-squaring composition of an
//! FSM body, for `halts_within(F, N)` lowering.
//!
//! Strategy (the "closed-form" approach from Z's measurement):
//!
//! 1. Run F's body through the existing translate pipeline
//!    (`build_cache`) to get its one-tick Z3 assertions.
//! 2. Identify state pairs: every `name, name_next ∈ T` declaration
//!    in F's body. The bare-name var is the *input* state, the
//!    `_next` is the *output* state. F also must declare `halt ∈ Bool`.
//! 3. Extract per-output Z3 expressions. After
//!    `simplify_assertions`, the body is a set of `(= out expr)`
//!    equations; pull out the RHS for each output. Resolve forward
//!    references (`halt = (count_next ≤ 0)`) so every expression
//!    bottoms out in input consts only.
//! 4. Build cached powers F^1, F^2, F^4, ..., F^(2^p) where 2^p ≤ N.
//!    Each power holds a `state_next` Dynamic per state var plus a
//!    cumulative `halt_aggregate` Bool, both as expressions in the
//!    F^1 input consts. F^(2k) is built by substituting F^k's
//!    input consts with F^k's `state_next` exprs in F^k's own
//!    `state_next` and halt — pure expression composition, no
//!    intermediate consts to eliminate.
//! 5. Each `.simplify()` after composition collapses affine forms
//!    to constant size (Z's measurement).
//! 6. After F^2 is built, the affine-step detector compares its
//!    AST node count against F^1. Ratio > 1.5 → refuse cleanly
//!    (the body is branching; log-unroll provably doesn't help).
//! 7. For arbitrary N, assemble F^N from the cached powers via N's
//!    binary expansion: pick the largest power ≤ remaining, compose
//!    onto the running result, subtract, repeat.
//! 8. Substitute F^N's input consts with the outer claim's matching
//!    Z3 vars (names-match composition) and assert
//!    `halt_aggregate = true` on the outer solver.
//!
//! Trace via `EVIDENT_FSM_UNROLL_TRACE=1`.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use z3::ast::{Ast, Bool, Dynamic, Int};
use z3::{AstKind, Context, Solver};
use z3_sys::DeclKind;

use crate::core::ast::{BodyItem, SchemaDecl};
use crate::core::{DatatypeRegistry, EnumRegistry, Value, Var};
use crate::z3_eval::simplify_assertions;

use super::detector::{classify, count_nodes, Verdict, PROBE_POWER};

/// Largest power of two ≤ `n` (≥ 1). `largest_power_le(1000) = 512`,
/// `largest_power_le(5) = 4`, `largest_power_le(1) = 1`.
fn largest_power_le(n: u64) -> u64 {
    if n == 0 { return 1; }
    let mut p = 1u64;
    while p * 2 <= n { p *= 2; }
    p
}

/// Error returned by `assert_halts_within` when the FSM body can't be
/// log-unrolled. The caller (the inline walker) translates each
/// variant into a solver-level outcome: typically `assert false` to
/// resolve the enclosing claim UNSAT, plus a stderr diagnostic.
#[derive(Debug, Clone)]
pub enum HaltsWithinError {
    /// `halts_within(F, N)` named a claim that doesn't exist.
    UnknownFsm(String),
    /// F has no `name, name_next ∈ T` state pair. Required for
    /// composition.
    NoStatePair(String),
    /// F has no `halt ∈ Bool` declaration. Required for the halt
    /// witness.
    NoHaltVar(String),
    /// F's body doesn't lower to a clean state→state vector function —
    /// some output isn't defined by a top-level `(= out expr)`. May
    /// indicate a translator gap in F's body or a body shape this
    /// closed-form approach can't see (guarded assignments,
    /// quantifier outputs, etc.).
    NotFunctionShape { fsm: String, missing: Vec<String> },
    /// The state-transition node count was still growing > 1.5× per
    /// doubling at the probe depth (F^8 or N's largest power) — the
    /// body is branching / data-dependent. Per Z's measurement, more
    /// doublings won't help; it plateaus at ~2×.
    BranchingRefused { fsm: String, ratio: f64, probed_to: u64, nodes: usize },
    /// Catch-all for runtime invariant violations (missing env entry,
    /// unexpected sort kind, …). Shouldn't fire on well-formed bodies.
    Internal(String),
}

impl std::fmt::Display for HaltsWithinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HaltsWithinError::UnknownFsm(name) =>
                write!(f, "halts_within: unknown FSM claim {name:?}"),
            HaltsWithinError::NoStatePair(name) =>
                write!(f, "halts_within({name}, ..): FSM body must declare a `name, name_next ∈ T` state pair"),
            HaltsWithinError::NoHaltVar(name) =>
                write!(f, "halts_within({name}, ..): FSM body must declare `halt ∈ Bool`"),
            HaltsWithinError::NotFunctionShape { fsm, missing } =>
                write!(f, "halts_within({fsm}, ..): can't extract closed-form for outputs {missing:?} \
                          (FSM body must shape-up as `out_var = expr` for each state-next + halt)"),
            HaltsWithinError::BranchingRefused { fsm, ratio, probed_to, nodes } =>
                write!(f, "halts_within({fsm}, ..): simplify ratio {ratio:.2} (state nodes={nodes} at F^{probed_to}) \
                          > 1.5 — log-unroll declined; FSM body is branching/data-dependent."),
            HaltsWithinError::Internal(s) =>
                write!(f, "halts_within: internal: {s}"),
        }
    }
}

/// `(input_name, output_name)` pair plus the shared type name.
#[derive(Debug, Clone)]
struct StatePair {
    input: String,
    output: String,
    #[allow(dead_code)]
    type_name: String,
}

fn trace_enabled() -> bool {
    std::env::var("EVIDENT_FSM_UNROLL_TRACE")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

/// Identify `(name, name_next, type)` pairs declared in F's body. Both
/// halves of the pair must share the type name. Ignores any Membership
/// whose `_next` partner isn't declared.
fn detect_state_pairs(schema: &SchemaDecl) -> Vec<StatePair> {
    let mut decls: HashMap<String, String> = HashMap::new();
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name, .. } = item {
            decls.insert(name.clone(), type_name.clone());
        }
    }
    let mut pairs = Vec::new();
    let mut seen_inputs: HashSet<String> = HashSet::new();
    for (name, type_name) in &decls {
        if name.ends_with("_next") { continue; }
        let next_name = format!("{name}_next");
        if let Some(next_type) = decls.get(&next_name) {
            if next_type == type_name && !seen_inputs.contains(name) {
                seen_inputs.insert(name.clone());
                pairs.push(StatePair {
                    input: name.clone(),
                    output: next_name,
                    type_name: type_name.clone(),
                });
            }
        }
    }
    pairs.sort_by(|a, b| a.input.cmp(&b.input));
    pairs
}

/// `b` is `(= a b')` → `(a, b')`.
fn split_equality<'ctx>(b: &Bool<'ctx>) -> Option<(Dynamic<'ctx>, Dynamic<'ctx>)> {
    if b.kind() != AstKind::App { return None; }
    let decl = b.safe_decl().ok()?;
    if decl.kind() != DeclKind::EQ { return None; }
    let children = b.children();
    if children.len() != 2 { return None; }
    Some((children[0].clone(), children[1].clone()))
}

/// `a` is a 0-arity App (a Z3 const / "variable") → its declared name.
fn ast_app_name<'ctx>(a: &Dynamic<'ctx>) -> Option<String> {
    if a.kind() != AstKind::App { return None; }
    if a.num_children() != 0 { return None; }
    let decl = a.safe_decl().ok()?;
    Some(decl.name())
}

/// `a`'s tree mentions a 0-arity App with the given name.
fn mentions_name<'ctx>(a: &Dynamic<'ctx>, name: &str) -> bool {
    if a.kind() == AstKind::App && a.num_children() == 0 {
        if let Ok(decl) = a.safe_decl() {
            if decl.name() == name { return true; }
        }
    }
    a.children().iter().any(|c| mentions_name(c, name))
}

/// One power of F: F^k. Holds the per-state-var output expressions
/// (in terms of the F^1 input consts) plus the cumulative halt Bool.
#[derive(Clone)]
struct Power<'ctx> {
    /// Number of F applications (always a power of 2 for the cache
    /// entries; the running result during binary-expansion assembly
    /// can be any value).
    k: u64,
    /// `state_next` expression per state var name (keyed by INPUT
    /// name — `"count"`, not `"count_next"`). Each value is a
    /// Dynamic computing the var after k ticks, in terms of the F^1
    /// input consts only.
    state_exprs: HashMap<String, Dynamic<'ctx>>,
    /// Cumulative halt — true iff `halt` was true at any of the k
    /// ticks. In terms of the F^1 input consts only.
    halt_aggregate: Bool<'ctx>,
}

/// F^1: extract per-output expressions from the simplified body.
fn build_f1<'ctx>(
    fsm_name: &str,
    schema: &SchemaDecl,
    schemas: &HashMap<String, SchemaDecl>,
    ctx: &'static Context,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
) -> Result<(Power<'static>, HashMap<String, Dynamic<'static>>, Vec<StatePair>), HaltsWithinError>
where 'ctx: 'static
{
    let pairs = detect_state_pairs(schema);
    if pairs.is_empty() {
        return Err(HaltsWithinError::NoStatePair(fsm_name.to_string()));
    }
    let halt_declared = schema.body.iter().any(|item| matches!(item,
        BodyItem::Membership { name, type_name, .. }
            if name == "halt" && type_name == "Bool"
    ));
    if !halt_declared {
        return Err(HaltsWithinError::NoHaltVar(fsm_name.to_string()));
    }

    // Translate F's body once into a fresh cached solver. We use
    // build_cache (not a hand-rolled translate pass) so every body
    // shape supported by the rest of the runtime — record lifts,
    // chained-membership, ternaries, lookup-table membership — works
    // here too without us re-implementing.
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = crate::translate::build_cache(
        schema, schemas, ctx, registry, enums, &empty_given, 0,
    );

    // Pull the asserted body back out and run the production simplify
    // pass. Same shape as the per-claim cache that the functionizer
    // consumes.
    let assertions_local = cached.solver.get_assertions();
    let assertions: Vec<Bool<'static>> = unsafe {
        std::mem::transmute::<Vec<Bool<'_>>, Vec<Bool<'static>>>(assertions_local)
    };
    let simp = simplify_assertions(ctx, &assertions);
    if simp.unsat {
        return Err(HaltsWithinError::Internal(format!(
            "halts_within({fsm_name}, ..): body simplified to UNSAT before any unrolling"
        )));
    }
    let simplified = simp.formulas;

    // Resolve the input + output state consts from the cache's env.
    // The cache's env contains every declared name → Z3 Var. We pull
    // the Z3 Dynamic out per state-var.
    let mut input_consts: HashMap<String, Dynamic<'static>> = HashMap::new();
    let mut output_consts: HashMap<String, Dynamic<'static>> = HashMap::new();
    for pair in &pairs {
        let in_var = cached.env.get(&pair.input).ok_or_else(|| HaltsWithinError::Internal(
            format!("halts_within({fsm_name}, ..): input state {:?} not in env", pair.input)))?;
        let out_var = cached.env.get(&pair.output).ok_or_else(|| HaltsWithinError::Internal(
            format!("halts_within({fsm_name}, ..): output state {:?} not in env", pair.output)))?;
        let in_dyn = var_to_dynamic(in_var).ok_or_else(|| HaltsWithinError::Internal(
            format!("halts_within({fsm_name}, ..): can't extract Dynamic for {:?}", pair.input)))?;
        let out_dyn = var_to_dynamic(out_var).ok_or_else(|| HaltsWithinError::Internal(
            format!("halts_within({fsm_name}, ..): can't extract Dynamic for {:?}", pair.output)))?;
        input_consts.insert(pair.input.clone(), in_dyn);
        output_consts.insert(pair.output.clone(), out_dyn);
    }
    let halt_var = cached.env.get("halt").ok_or_else(|| HaltsWithinError::Internal(
        format!("halts_within({fsm_name}, ..): halt not in env")))?;
    let halt_dyn = var_to_dynamic(halt_var).ok_or_else(|| HaltsWithinError::Internal(
        format!("halts_within({fsm_name}, ..): can't extract halt Dynamic")))?;

    // Build the output-name → defining expression table by scanning
    // every simplified assertion of form `(= out_const expr)` or
    // `(= expr out_const)`. We need entries for each state-output and
    // for halt; any miss means the body doesn't shape up as a pure
    // function and we refuse cleanly.
    let mut output_names_to_find: HashSet<String> = pairs.iter()
        .map(|p| p.output.clone()).collect();
    output_names_to_find.insert("halt".to_string());
    let mut defining: HashMap<String, Dynamic<'static>> = HashMap::new();
    for a in &simplified {
        let Some((l, r)) = split_equality(a) else { continue };
        if let Some(name) = ast_app_name(&l) {
            if output_names_to_find.contains(&name)
                && !defining.contains_key(&name)
                && !mentions_name(&r, &name)
            {
                defining.insert(name, r);
                continue;
            }
        }
        if let Some(name) = ast_app_name(&r) {
            if output_names_to_find.contains(&name)
                && !defining.contains_key(&name)
                && !mentions_name(&l, &name)
            {
                defining.insert(name, l);
                continue;
            }
        }
    }
    let missing: Vec<String> = output_names_to_find.iter()
        .filter(|n| !defining.contains_key(*n))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(HaltsWithinError::NotFunctionShape {
            fsm: fsm_name.to_string(),
            missing,
        });
    }

    // Resolve forward references: defining exprs may reference each
    // other (e.g. `halt = (count_next ≤ 0)` references `count_next`,
    // which is defined as `count - 1`). Substitute until every
    // expression bottoms out in input consts only.
    //
    // We do this as a fixed-point loop bounded by output-count
    // iterations — for a non-cyclic dependency graph that converges
    // in at most that many passes.
    let output_consts_vec: Vec<(String, Dynamic<'static>)> = output_consts.iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let halt_pair = ("halt".to_string(), halt_dyn.clone());
    let mut resolve_order: Vec<(Dynamic<'static>, String)> = Vec::new();
    for (out_name, out_const) in &output_consts_vec {
        resolve_order.push((out_const.clone(), out_name.clone()));
    }
    resolve_order.push((halt_pair.1.clone(), halt_pair.0.clone()));
    // Fixed-point substitute.
    let n_outputs = output_consts_vec.len() + 1; // + halt
    for _ in 0..(n_outputs + 1) {
        let snapshot: HashMap<String, Dynamic<'static>> = defining.clone();
        for (_const, name) in &resolve_order {
            let mut expr = defining[name].clone();
            for (other_const, other_name) in &resolve_order {
                if other_name == name { continue; }
                let other_expr = &snapshot[other_name];
                expr = expr.substitute(&[(other_const, other_expr)]);
            }
            defining.insert(name.clone(), expr);
        }
    }
    // Verify no defining expr still references an output const.
    for (out_const, out_name) in &resolve_order {
        let _ = out_const;
        for (other_const, _) in &resolve_order {
            for (_, expr) in defining.iter() {
                if mentions_dynamic(expr, other_const) {
                    return Err(HaltsWithinError::NotFunctionShape {
                        fsm: fsm_name.to_string(),
                        missing: vec![format!("{out_name} (cycle through outputs)")],
                    });
                }
            }
        }
    }

    // Build F^1: state_exprs are keyed by INPUT name (e.g. "count"),
    // pointing to the next-tick expression (the resolved RHS of
    // count_next = ...). halt_aggregate = halt's resolved expression.
    let mut state_exprs: HashMap<String, Dynamic<'static>> = HashMap::new();
    for pair in &pairs {
        let expr = defining.remove(&pair.output).unwrap();
        state_exprs.insert(pair.input.clone(), expr.simplify());
    }
    let halt_expr_dyn = defining.remove("halt").unwrap().simplify();
    let halt_bool = halt_expr_dyn.as_bool().ok_or_else(|| HaltsWithinError::Internal(
        format!("halts_within({fsm_name}, ..): halt's resolved expr is not Bool")))?;

    Ok((Power {
        k: 1,
        state_exprs,
        halt_aggregate: halt_bool,
    }, input_consts, pairs))
}

/// Compose F^k with itself to produce F^(2k). Pure expression
/// substitution: each state-output expr at 2k = state-output at k,
/// substituted with input consts → state-output at k. halt_aggregate
/// at 2k = halt_aggregate at k ∨ (halt_aggregate at k substituted as
/// above) — i.e. did halt fire in the first half OR the second half.
fn double<'ctx>(prev: &Power<'ctx>, input_consts: &HashMap<String, Dynamic<'ctx>>) -> Power<'ctx>
where 'ctx: 'static
{
    // Build the substitution pairs: input_const → state_expr (the
    // value of that state var after the FIRST half's k ticks).
    let mut from: Vec<Dynamic<'ctx>> = Vec::new();
    let mut to: Vec<Dynamic<'ctx>> = Vec::new();
    for (name, in_const) in input_consts {
        let Some(state_expr) = prev.state_exprs.get(name) else { continue };
        from.push(in_const.clone());
        to.push(state_expr.clone());
    }
    let pairs: Vec<(&Dynamic<'ctx>, &Dynamic<'ctx>)> =
        from.iter().zip(to.iter()).collect();

    // New state_exprs: each is prev.state_expr with the inputs
    // replaced by prev.state_exprs.
    let mut new_state: HashMap<String, Dynamic<'ctx>> = HashMap::new();
    for (name, expr) in &prev.state_exprs {
        let composed = expr.substitute(&pairs).simplify();
        new_state.insert(name.clone(), composed);
    }
    // New halt: halt at first half OR halt at second half.
    let halt_first = prev.halt_aggregate.clone();
    let halt_second_dyn = Dynamic::from_ast(&prev.halt_aggregate)
        .substitute(&pairs);
    let halt_second = halt_second_dyn.as_bool().expect("halt subst must stay Bool");
    let halt_combined = Bool::or(halt_first.get_ctx(), &[&halt_first, &halt_second]).simplify();

    Power {
        k: prev.k * 2,
        state_exprs: new_state,
        halt_aggregate: halt_combined,
    }
}

/// Compose two arbitrary powers in series: result is "apply `first`
/// then `second`". State exprs at the combined step = second.state with
/// inputs replaced by first.state. halt = first.halt OR (second.halt
/// substituted to first.state). Used by the binary-expansion
/// assembly to chain cached F^(2^k) onto the running F^accum.
fn series<'ctx>(
    first: &Power<'ctx>,
    second: &Power<'ctx>,
    input_consts: &HashMap<String, Dynamic<'ctx>>,
) -> Power<'ctx>
where 'ctx: 'static
{
    let mut from: Vec<Dynamic<'ctx>> = Vec::new();
    let mut to: Vec<Dynamic<'ctx>> = Vec::new();
    for (name, in_const) in input_consts {
        let Some(state_expr) = first.state_exprs.get(name) else { continue };
        from.push(in_const.clone());
        to.push(state_expr.clone());
    }
    let pairs: Vec<(&Dynamic<'ctx>, &Dynamic<'ctx>)> =
        from.iter().zip(to.iter()).collect();

    let mut new_state: HashMap<String, Dynamic<'ctx>> = HashMap::new();
    for (name, second_expr) in &second.state_exprs {
        let composed = second_expr.substitute(&pairs).simplify();
        new_state.insert(name.clone(), composed);
    }
    let halt_second_dyn = Dynamic::from_ast(&second.halt_aggregate)
        .substitute(&pairs);
    let halt_second = halt_second_dyn.as_bool().expect("halt subst must stay Bool");
    let halt_combined = Bool::or(
        first.halt_aggregate.get_ctx(),
        &[&first.halt_aggregate, &halt_second],
    ).simplify();

    Power {
        k: first.k + second.k,
        state_exprs: new_state,
        halt_aggregate: halt_combined,
    }
}

/// Convert a `Var` to a Dynamic — supports the scalar shapes we
/// expect for FSM state (Int, Bool, Real) and enum (Datatype).
fn var_to_dynamic<'ctx>(v: &Var<'ctx>) -> Option<Dynamic<'ctx>> {
    match v {
        Var::IntVar(i)  => Some(Dynamic::from_ast(i)),
        Var::BoolVar(b) => Some(Dynamic::from_ast(b)),
        Var::RealVar(r) => Some(Dynamic::from_ast(r)),
        Var::EnumVar { ast, .. } => Some(Dynamic::from_ast(ast)),
        _ => None,
    }
}

/// Cheap "does this AST contain another AST as a subtree" — we use
/// Dynamic equality (hash-cons) for the leaf check, then recurse.
fn mentions_dynamic<'ctx>(haystack: &Dynamic<'ctx>, needle: &Dynamic<'ctx>) -> bool {
    if haystack == needle { return true; }
    haystack.children().iter().any(|c| mentions_dynamic(c, needle))
}

// Holds the Power-power-of-2 set + the F^N expression after
// binary-expansion assembly.
struct UnrollResult<'ctx> {
    /// Largest power-of-2 actually built. Equal to 2^p where 2^p ≤ N.
    #[allow(dead_code)]
    max_power: u64,
    /// Composed F^N: state + halt aggregate over N applications.
    final_power: Power<'ctx>,
}

fn build_unrolled<'ctx>(
    fsm_name: &str,
    n: u64,
    f1: Power<'ctx>,
    input_consts: &HashMap<String, Dynamic<'ctx>>,
) -> Result<UnrollResult<'ctx>, HaltsWithinError>
where 'ctx: 'static
{
    let trace = trace_enabled();
    let t0 = Instant::now();

    let f1_nodes = power_node_count(&f1);
    if trace {
        eprintln!("[fsm_unroll] {fsm_name}: target N={n}");
        eprintln!("[fsm_unroll]   f^{:<3}  state_nodes={:<5} ratio=    —", 1, f1_nodes);
    }

    // Build cached powers up to the largest 2^p ≤ N. The first
    // PROBE_DOUBLINGS of them (up to F^8, capped at N) are the
    // affine-step probe: we classify on the last doubling ratio at
    // that depth, then either refuse (branching) or keep building.
    //
    // Why probe to F^8 rather than deciding at F^2: Z's measurement
    // showed one doubling can't separate the regimes — Fibonacci (an
    // affine body) has F^2/F^1 = 2.0 but collapses to flat by F^8,
    // while the conditional-update branching body sits at exactly 1.5
    // after one doubling. The F^8/F^4 ratio is the clean discriminant.
    // Probing to F^8 costs at most 3 doublings even on a branching
    // body (~8× F^1, still tiny), so the refuse path stays cheap.
    let probe_power = PROBE_POWER.min(largest_power_le(n));
    let mut powers: Vec<Power<'ctx>> = vec![f1];
    let mut prev_nodes = f1_nodes;
    let mut decided = false;
    while powers.last().unwrap().k * 2 <= n {
        let next = double(powers.last().unwrap(), input_consts);
        let nodes = power_node_count(&next);
        let ratio = nodes as f64 / prev_nodes.max(1) as f64;
        if trace {
            eprintln!("[fsm_unroll]   f^{:<3}  state_nodes={:<5} ratio= {:.2}", next.k, nodes, ratio);
        }
        prev_nodes = nodes;
        let reached = next.k;
        powers.push(next);

        // Once the probe depth is reached (F^8 or N's largest power if
        // N < 8), classify on the most recent doubling ratio.
        if !decided && reached >= probe_power {
            decided = true;
            let verdict = classify(ratio);
            if trace {
                eprintln!(
                    "[fsm_unroll]   detector: last-doubling ratio={ratio:.2} \
                     (probed to f^{reached}) → {verdict:?}"
                );
            }
            if verdict == Verdict::Branching {
                // Return the error; the caller (the inline walker)
                // surfaces it on stderr via the Display impl, which
                // carries the "log-unroll declined" diagnostic. We
                // don't print here to avoid a duplicate line.
                return Err(HaltsWithinError::BranchingRefused {
                    fsm: fsm_name.to_string(),
                    ratio,
                    probed_to: reached,
                    nodes,
                });
            }
        }
    }

    // Assemble F^N from the cached powers via binary expansion of N.
    // E.g. N = 1000 = 512 + 256 + 128 + 64 + 32 + 8.
    let mut composed_parts: Vec<u64> = Vec::new();
    let mut accum: Option<Power<'ctx>> = None;
    let mut remaining = n;
    for power in powers.iter().rev() {
        if power.k <= remaining {
            composed_parts.push(power.k);
            accum = Some(match accum {
                None => power.clone(),
                Some(prev) => series(&prev, power, input_consts),
            });
            remaining -= power.k;
        }
    }
    let final_power = accum.ok_or_else(|| HaltsWithinError::Internal(format!(
        "halts_within({fsm_name}, {n}): couldn't assemble F^N — no powers built (N < 1?)"
    )))?;

    if trace {
        let parts_str = composed_parts.iter()
            .map(|p| p.to_string()).collect::<Vec<_>>().join(" + ");
        eprintln!("[fsm_unroll] composing N={n} from cached powers: {parts_str}");
        eprintln!(
            "[fsm_unroll] final halt-aggregate node count: {} (O(N) disjunction; \
             collapses when initial state is pinned), time: {:?}",
            count_nodes(&[final_power.halt_aggregate.clone()]),
            t0.elapsed()
        );
    }

    Ok(UnrollResult {
        max_power: powers.last().map(|p| p.k).unwrap_or(1),
        final_power,
    })
}

/// AST node count of a Power's STATE transition — the size metric the
/// affine-step detector compares across F^1 / F^2. Counts only the
/// `state_exprs` (the `out = f(in)` vector function), NOT the halt
/// aggregate.
///
/// This is deliberate and matches Z's methodology exactly: the
/// measurement classified bodies by the node count of the composed
/// state→state function. The halt aggregate is a different beast —
/// `∃ k ∈ [1,N] : halt_k` is a disjunction over ticks that grows O(N)
/// by construction (Z3's `.simplify()` won't subsume nested intervals
/// like `count≤0 ∨ count≤1` into `count≤1`). Folding the halt
/// disjunction into the detector metric would make every body — even a
/// pure counter — look "branching" (ratio > 1.5) purely from the OR
/// clause the halt witness adds each doubling. The affine/branching
/// split lives entirely in whether the *state* collapses.
///
/// Dedup'd via Dynamic identity (hash-consed) over a walk of every
/// state-output expression. Mirrors `detector::count_nodes` but spans
/// Dynamics, not just Bools, since state exprs are Int / Real /
/// Datatype Dynamics.
fn power_node_count<'ctx>(p: &Power<'ctx>) -> usize {
    let mut seen: HashSet<Dynamic<'ctx>> = HashSet::new();
    let mut stack: Vec<Dynamic<'ctx>> = Vec::new();
    for (_n, e) in &p.state_exprs {
        stack.push(e.clone());
    }
    while let Some(d) = stack.pop() {
        if seen.insert(d.clone()) {
            for c in d.children() { stack.push(c); }
        }
    }
    seen.len()
}

/// Public entry: lower `halts_within(fsm_name, n)` into outer-solver
/// assertions, assuming the caller's `outer_env` already binds any
/// outer-scope state vars (e.g. `count = 50`) by names-match.
pub fn assert_halts_within(
    fsm_name: &str,
    n: i64,
    ctx: &'static Context,
    solver: &Solver<'static>,
    outer_env: &mut HashMap<String, Var<'static>>,
    schemas: &HashMap<String, SchemaDecl>,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
) -> Result<(), HaltsWithinError> {
    if n < 0 {
        return Err(HaltsWithinError::Internal(format!(
            "halts_within({fsm_name}, {n}): N must be non-negative"
        )));
    }
    let schema = schemas.get(fsm_name).ok_or_else(||
        HaltsWithinError::UnknownFsm(fsm_name.to_string()))?;
    if n == 0 {
        // halts_within(F, 0) asserts no ticks → halt is false. The
        // conventional reading is "UNSAT": cannot halt in zero ticks
        // unless trivially. Encode as `false`.
        solver.assert(&Bool::from_bool(ctx, false));
        return Ok(());
    }

    let (f1, input_consts, _pairs) = build_f1(
        fsm_name, schema, schemas, ctx, registry, enums,
    )?;

    let result = build_unrolled(fsm_name, n as u64, f1, &input_consts)?;

    // Bind the F^N input consts to the outer env's matching variables
    // by name. If the outer env has `count = PinnedInt(50)`, we
    // substitute the unrolled body's count_in const with the Int
    // literal 50. If the outer env has `count = IntVar(c)`, we
    // substitute count_in → c. If the outer env has no matching
    // entry, the F^N input const is left free — the existential over
    // initial states.
    let mut from: Vec<Dynamic<'static>> = Vec::new();
    let mut to: Vec<Dynamic<'static>> = Vec::new();
    for (name, in_const) in &input_consts {
        if let Some(outer_var) = outer_env.get(name) {
            let outer_dyn = match outer_var {
                Var::IntVar(i)  => Dynamic::from_ast(i),
                Var::BoolVar(b) => Dynamic::from_ast(b),
                Var::RealVar(r) => Dynamic::from_ast(r),
                Var::EnumVar { ast, .. } => Dynamic::from_ast(ast),
                Var::PinnedInt(v) => {
                    Dynamic::from_ast(&Int::from_i64(ctx, *v))
                }
                _ => continue,
            };
            from.push(in_const.clone());
            to.push(outer_dyn);
        }
    }
    let pairs: Vec<(&Dynamic<'static>, &Dynamic<'static>)> =
        from.iter().zip(to.iter()).collect();
    let halt_bound_dyn = Dynamic::from_ast(&result.final_power.halt_aggregate)
        .substitute(&pairs);
    let halt_bound = halt_bound_dyn.as_bool()
        .expect("halt remains Bool after input substitution");
    let halt_bound = halt_bound.simplify();

    solver.assert(&halt_bound);
    Ok(())
}
