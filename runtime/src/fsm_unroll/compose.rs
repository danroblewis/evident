//! Exponentiation-by-squaring FSM body composer for `halts_within(F, N)`.
//! Builds F^N via Z3 substitution; asserts cumulative halt. Trace: `EVIDENT_FSM_UNROLL_TRACE=1`.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use z3::ast::{Ast, Bool, Dynamic, Int};
use z3::{AstKind, Context};
use z3_sys::DeclKind;

use crate::core::ast::{BodyItem, Keyword, SchemaDecl};
use crate::core::{DatatypeRegistry, EnumRegistry, Value, Var, Z3Program, Z3Step};
use crate::z3_eval::simplify_assertions;

use super::detector::{classify, count_nodes, Verdict, PROBE_POWER};

#[allow(dead_code)] // retained for the §6.2 BMC discharge of F(seed, fsm_state)
fn largest_power_le(n: u64) -> u64 {
    if n == 0 { return 1; }
    let mut p = 1u64;
    while p * 2 <= n { p *= 2; }
    p
}

/// Why `assert_halts_within` refused; caller asserts `false` on the outer solver.
#[derive(Debug, Clone)]
pub enum HaltsWithinError {
    UnknownFsm(String),
    /// `fsm` keyword is the sole FSM signal; `claim`/`type`/`schema` targets are rejected.
    NotFsm { fsm: String, keyword: String },
    NoStatePair(String),
    NoHaltVar(String),
    /// Body doesn't shape as `out = expr` for each state-output + halt.
    NotFunctionShape { fsm: String, missing: Vec<String> },
    /// Node-count ratio > 1.5 at probe depth — branching body, log-unroll declined.
    BranchingRefused { fsm: String, ratio: f64, probed_to: u64, nodes: usize },
    Internal(String),
}

impl std::fmt::Display for HaltsWithinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HaltsWithinError::UnknownFsm(name) =>
                write!(f, "halts_within: unknown FSM claim {name:?}"),
            HaltsWithinError::NotFsm { fsm, keyword } =>
                write!(f, "halts_within's target `{fsm}` must be declared `fsm`, not \
                          `{keyword}` — the `fsm` keyword is the sole signal that a \
                          schema is an FSM (no shape-detection). Relabel `{keyword} \
                          {fsm}` to `fsm {fsm}`."),
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

#[derive(Debug, Clone)]
struct StatePair {
    input: String,
    output: String,
    #[allow(dead_code)]
    type_name: String,
}

fn keyword_word(kw: &Keyword) -> &'static str {
    match kw {
        Keyword::Schema   => "schema",
        Keyword::Claim    => "claim",
        Keyword::Type     => "type",
        Keyword::Subclaim => "subclaim",
        Keyword::Fsm      => "fsm",
    }
}

fn trace_enabled() -> bool {
    std::env::var("EVIDENT_FSM_UNROLL_TRACE")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

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

fn split_equality<'ctx>(b: &Bool<'ctx>) -> Option<(Dynamic<'ctx>, Dynamic<'ctx>)> {
    if b.kind() != AstKind::App { return None; }
    let decl = b.safe_decl().ok()?;
    if decl.kind() != DeclKind::EQ { return None; }
    let children = b.children();
    if children.len() != 2 { return None; }
    Some((children[0].clone(), children[1].clone()))
}

fn ast_app_name<'ctx>(a: &Dynamic<'ctx>) -> Option<String> {
    if a.kind() != AstKind::App { return None; }
    if a.num_children() != 0 { return None; }
    let decl = a.safe_decl().ok()?;
    Some(decl.name())
}

fn mentions_name<'ctx>(a: &Dynamic<'ctx>, name: &str) -> bool {
    if a.kind() == AstKind::App && a.num_children() == 0 {
        if let Ok(decl) = a.safe_decl() {
            if decl.name() == name { return true; }
        }
    }
    a.children().iter().any(|c| mentions_name(c, name))
}

/// F^k: per-state-var next expressions + cumulative halt, all in F^1 input consts.
#[derive(Clone)]
struct Power<'ctx> {
    k: u64,
    /// State after k ticks (keyed by input name). Used to compose doublings — NOT halted state.
    state_exprs: HashMap<String, Dynamic<'ctx>>,
    /// True iff halt fired at any tick in 1..=k.
    halt_aggregate: Bool<'ctx>,
    /// State at the first halting tick (keyed by input name); meaningful only when halt_aggregate holds.
    halted_state: HashMap<String, Dynamic<'ctx>>,
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
    // `fsm` is the sole FSM signal — rejects claim/type/schema targets for both
    // `halts_within` and `collapse_run` (both share this resolution point).
    if !matches!(schema.keyword, Keyword::Fsm) {
        return Err(HaltsWithinError::NotFsm {
            fsm: fsm_name.to_string(),
            keyword: keyword_word(&schema.keyword).to_string(),
        });
    }
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

    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = crate::translate::build_cache(
        schema, schemas, ctx, registry, enums, &empty_given, 0,
    );

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

    // Resolve forward refs (e.g. halt = count_next ≤ 0; count_next = count - 1)
    // via fixed-point substitution bounded by output count; non-cyclic graphs converge.
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
    let n_outputs = output_consts_vec.len() + 1;
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

    let mut state_exprs: HashMap<String, Dynamic<'static>> = HashMap::new();
    for pair in &pairs {
        let expr = defining.remove(&pair.output).unwrap();
        state_exprs.insert(pair.input.clone(), expr.simplify());
    }
    let halt_expr_dyn = defining.remove("halt").unwrap().simplify();
    let halt_bool = halt_expr_dyn.as_bool().ok_or_else(|| HaltsWithinError::Internal(
        format!("halts_within({fsm_name}, ..): halt's resolved expr is not Bool")))?;

    // F^1's halted state = input (run_nested returns the input at the halting tick).
    let halted_state: HashMap<String, Dynamic<'static>> = input_consts.clone();

    Ok((Power {
        k: 1,
        state_exprs,
        halt_aggregate: halt_bool,
        halted_state,
    }, input_consts, pairs))
}

/// Pure-substitution doubling: F^k → F^(2k). halt = first_half ∨ second_half.
fn double<'ctx>(prev: &Power<'ctx>, input_consts: &HashMap<String, Dynamic<'ctx>>) -> Power<'ctx>
where 'ctx: 'static
{
    let mut from: Vec<Dynamic<'ctx>> = Vec::new();
    let mut to: Vec<Dynamic<'ctx>> = Vec::new();
    for (name, in_const) in input_consts {
        let Some(state_expr) = prev.state_exprs.get(name) else { continue };
        from.push(in_const.clone());
        to.push(state_expr.clone());
    }
    let pairs: Vec<(&Dynamic<'ctx>, &Dynamic<'ctx>)> =
        from.iter().zip(to.iter()).collect();

    let mut new_state: HashMap<String, Dynamic<'ctx>> = HashMap::new();
    for (name, expr) in &prev.state_exprs {
        let composed = expr.substitute(&pairs).simplify();
        new_state.insert(name.clone(), composed);
    }
    let halt_first = prev.halt_aggregate.clone();
    let halt_second_dyn = Dynamic::from_ast(&prev.halt_aggregate)
        .substitute(&pairs);
    let halt_second = halt_second_dyn.as_bool().expect("halt subst must stay Bool");
    let halt_combined = Bool::or(halt_first.get_ctx(), &[&halt_first, &halt_second]).simplify();

    // Halted state: "first halt wins" — if first half halted, return its halted value;
    // otherwise continue into second half (seeded from first half's end state).
    let mut new_halted: HashMap<String, Dynamic<'ctx>> = HashMap::new();
    for (name, first_halted) in &prev.halted_state {
        let second_halted = prev.halted_state.get(name)
            .map(|h| h.substitute(&pairs))
            .unwrap_or_else(|| first_halted.clone());
        let chosen = halt_first.ite(first_halted, &second_halted).simplify();
        new_halted.insert(name.clone(), chosen);
    }

    Power {
        k: prev.k * 2,
        state_exprs: new_state,
        halt_aggregate: halt_combined,
        halted_state: new_halted,
    }
}

/// Series composition of two arbitrary powers; used for binary-expansion assembly.
#[allow(dead_code)] // retained for the §6.2 BMC discharge of F(seed, fsm_state)
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

    let halt_first = first.halt_aggregate.clone();
    let mut new_halted: HashMap<String, Dynamic<'ctx>> = HashMap::new();
    for (name, first_halted) in &first.halted_state {
        let second_halted = second.halted_state.get(name)
            .map(|h| h.substitute(&pairs))
            .unwrap_or_else(|| first_halted.clone());
        let chosen = halt_first.ite(first_halted, &second_halted).simplify();
        new_halted.insert(name.clone(), chosen);
    }

    Power {
        k: first.k + second.k,
        state_exprs: new_state,
        halt_aggregate: halt_combined,
        halted_state: new_halted,
    }
}

fn var_to_dynamic<'ctx>(v: &Var<'ctx>) -> Option<Dynamic<'ctx>> {
    match v {
        Var::IntVar(i)  => Some(Dynamic::from_ast(i)),
        Var::BoolVar(b) => Some(Dynamic::from_ast(b)),
        Var::RealVar(r) => Some(Dynamic::from_ast(r)),
        Var::EnumVar { ast, .. } => Some(Dynamic::from_ast(ast)),
        _ => None,
    }
}

fn mentions_dynamic<'ctx>(haystack: &Dynamic<'ctx>, needle: &Dynamic<'ctx>) -> bool {
    if haystack == needle { return true; }
    haystack.children().iter().any(|c| mentions_dynamic(c, needle))
}

#[allow(dead_code)] // retained for the §6.2 BMC discharge of F(seed, fsm_state)
struct UnrollResult<'ctx> {
    max_power: u64,
    final_power: Power<'ctx>,
}

#[allow(dead_code)] // retained for the §6.2 BMC discharge of F(seed, fsm_state)
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

    // Build cached powers; probe to F^8 (3 doublings) before classifying affine vs branching.
    // One doubling misclassifies Fibonacci (affine but ratio=2.0 at F^2/F^1).
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
                return Err(HaltsWithinError::BranchingRefused {
                    fsm: fsm_name.to_string(),
                    ratio,
                    probed_to: reached,
                    nodes,
                });
            }
        }
    }

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

/// Node count of state exprs only — halt_aggregate grows O(N) regardless of body shape.
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

// The `halts_within(F, N)` surface (and its `assert_halts_within` lowering)
// was removed: halting is implicit in the embed constraint `F(seed, fsm_state)`.
// The closed-form unroller below (`build_f1`/`double`/`series`/`build_unrolled`)
// is retained — `collapse_run` (tier-1 JIT) uses `build_f1`/`double`, and the
// N-fold halt-aggregate assembly is reused by the §6.2 BMC discharge path.

// Tier-1 nested-run: reads halted-state expression (not halt Bool) into a Z3Program
// for JIT. See `docs/design/nested-fsm-strategies.md` §7 step 3.

/// Closed-form `run(F, init)` result: a one-step Z3Program for the JIT.
pub struct TierOneRun {
    /// One Scalar step: `output_name := halted-state expr`. JIT-compilable.
    pub program: Z3Program<'static>,
    pub input_name: String,
    pub output_name: String,
    pub k: u64,
    pub nodes: usize,
}

/// Refuse if halted-state tree exceeds this; JITing a non-collapsing ite-tree buys nothing.
const MAX_COLLAPSED_NODES: usize = 4096;

/// Refuse if init doesn't provably halt within this many ticks; fall through to tier 3.
const DEFAULT_MAX_UNROLL: u64 = 1 << 20; // ~1.05M ticks

fn halt_fires_for_init(
    power: &Power<'static>,
    input_consts: &HashMap<String, Dynamic<'static>>,
    input_name: &str,
    init: i64,
    ctx: &'static Context,
) -> bool {
    let Some(in_const) = input_consts.get(input_name) else { return false };
    let init_dyn = Dynamic::from_ast(&Int::from_i64(ctx, init));
    let subst = Dynamic::from_ast(&power.halt_aggregate)
        .substitute(&[(in_const, &init_dyn)]);
    matches!(subst.simplify().as_bool().and_then(|b| b.as_bool()), Some(true))
}

/// Closed-form tier-1 `run(fsm_name, init)`. Returns `Ok(None)` — fall through to tier 2/3 — for
/// non-single/non-Int state pairs, branching bodies, non-halting init, or non-collapsing carry.
pub fn collapse_run(
    fsm_name: &str,
    init: i64,
    ctx: &'static Context,
    schemas: &HashMap<String, SchemaDecl>,
    registry: &DatatypeRegistry,
    enums: Option<&EnumRegistry>,
    max_unroll: Option<u64>,
) -> Result<Option<TierOneRun>, HaltsWithinError> {
    let trace = trace_enabled();
    let max_unroll = max_unroll.unwrap_or(DEFAULT_MAX_UNROLL).max(PROBE_POWER);

    let schema = schemas.get(fsm_name).ok_or_else(||
        HaltsWithinError::UnknownFsm(fsm_name.to_string()))?;

    let (f1, input_consts, pairs) =
        build_f1(fsm_name, schema, schemas, ctx, registry, enums)?;

    if pairs.len() != 1 {
        if trace {
            eprintln!("[tier1] {fsm_name}: {} state pairs — tier-1 v1 handles one; \
                       falling through", pairs.len());
        }
        return Ok(None);
    }
    if pairs[0].type_name != "Int" {
        if trace {
            eprintln!("[tier1] {fsm_name}: state type {:?} — tier-1 v1 handles Int; \
                       falling through", pairs[0].type_name);
        }
        return Ok(None);
    }
    let input_name = pairs[0].input.clone();

    let f1_nodes = power_node_count(&f1);
    let mut prev_nodes = f1_nodes;
    let mut decided = false;
    let mut powers: Vec<Power<'static>> = vec![f1];

    loop {
        let cur = powers.last().unwrap();
        if decided
            && halt_fires_for_init(cur, &input_consts, &input_name, init, ctx)
        {
            break;
        }
        if cur.k >= max_unroll {
            if trace {
                eprintln!("[tier1] {fsm_name}: init={init} did not provably halt \
                           within F^{} (cap) — falling through to tier 3", cur.k);
            }
            return Ok(None);
        }
        let next = double(cur, &input_consts);
        let nodes = power_node_count(&next);
        let ratio = nodes as f64 / prev_nodes.max(1) as f64;
        prev_nodes = nodes;
        let reached = next.k;
        powers.push(next);

        if !decided && reached >= PROBE_POWER {
            decided = true;
            let verdict = classify(ratio);
            if trace {
                eprintln!("[tier1] {fsm_name}: affine probe — last-doubling ratio \
                           {ratio:.2} at F^{reached} → {verdict:?}");
            }
            if verdict == Verdict::Branching {
                return Ok(None); // symbolic stack never collapses; fall through
            }
        }
    }

    let halting_power = powers.last().unwrap();
    let halted = halting_power.halted_state.get(&input_name).ok_or_else(||
        HaltsWithinError::Internal(format!(
            "collapse_run({fsm_name}, {init}): no halted-state expr for {input_name:?}")))?;
    let halted = halted.simplify();

    let nodes = count_dynamic_nodes(&halted);
    if nodes > MAX_COLLAPSED_NODES {
        if trace {
            eprintln!("[tier1] {fsm_name}: halted-state carry has {nodes} nodes \
                       (> {MAX_COLLAPSED_NODES}) — not collapsing, falling through");
        }
        return Ok(None);
    }

    let output_name = format!("{input_name}__tier1_final");
    let program = Z3Program {
        steps: vec![Z3Step::Scalar { var: output_name.clone(), expr: halted }],
        checks: Vec::new(),
        predicates: Vec::new(),
        label: Some(format!("tier1:{fsm_name}")),
    };

    if trace {
        eprintln!("[tier1] {fsm_name}: collapsed run(init={init}) → closed form at \
                   F^{} ({nodes} nodes), output `{output_name}`",
                   halting_power.k);
    }

    Ok(Some(TierOneRun {
        program,
        input_name,
        output_name,
        k: halting_power.k,
        nodes,
    }))
}

fn count_dynamic_nodes(d: &Dynamic<'_>) -> usize {
    let mut seen: HashSet<Dynamic<'_>> = HashSet::new();
    let mut stack: Vec<Dynamic<'_>> = vec![d.clone()];
    while let Some(node) = stack.pop() {
        if seen.insert(node.clone()) {
            for c in node.children() { stack.push(c); }
        }
    }
    seen.len()
}
