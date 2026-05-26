//! Symbolic-regression functionizer: GP search for a closed-form tree over Z3-sampled I/O pairs.
//! Scalar Int/Bool only; ≤4 inputs, ≤6 outputs; seeded from `EVIDENT_SYMBOLIC_SEED`.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use z3::ast::{Ast, Bool, Dynamic, Int};
use z3::{AstKind, Context, SatResult, Solver};
use z3_sys::DeclKind;

use crate::core::{EnumRegistry, Value, Z3Program, Z3Step};

/// Clamp magnitude so eval stays i128-finite without aborting.
const CLAMP: i128 = 1 << 100;
/// Clamp error diff before squaring to prevent SSE overflow across the sample set.
const DIFF_CLAMP: i128 = 1 << 40;

const MAX_INPUTS: usize = 4;
const MAX_OUTPUTS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumKind {
    Int,
    Bool,
}

impl NumKind {
    fn from_sort_name(s: &str) -> Option<NumKind> {
        match s {
            "Int" => Some(NumKind::Int),
            "Bool" => Some(NumKind::Bool),
            _ => None,
        }
    }
}

// Expression tree

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Un {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SExpr {
    Const(i64),
    Var(usize),   // index into the ordered input vector
    Unary(Un, Box<SExpr>),
    Binary(Bin, Box<SExpr>, Box<SExpr>),
    Ite(Box<SExpr>, Box<SExpr>, Box<SExpr>),
}

#[inline]
fn clamp(x: i128) -> i128 {
    x.clamp(-CLAMP, CLAMP)
}

fn apply_bin(op: Bin, a: i128, b: i128) -> i128 {
    let r = match op {
        Bin::Add => a.checked_add(b).map(clamp).unwrap_or(CLAMP),
        Bin::Sub => a.checked_sub(b).map(clamp).unwrap_or(CLAMP),
        Bin::Mul => a.checked_mul(b).map(clamp).unwrap_or(CLAMP),
        // Z3 div-by-zero yields unconstrained; we use 0 to keep the function total.
        Bin::Div => {
            if b == 0 {
                0
            } else {
                a.checked_div(b).unwrap_or(0)
            }
        }
        Bin::Mod => {
            if b == 0 {
                0
            } else {
                a.checked_rem(b).unwrap_or(0)
            }
        }
        Bin::Lt => (a < b) as i128,
        Bin::Le => (a <= b) as i128,
        Bin::Gt => (a > b) as i128,
        Bin::Ge => (a >= b) as i128,
        Bin::Eq => (a == b) as i128,
        Bin::And => ((a != 0) && (b != 0)) as i128,
        Bin::Or => ((a != 0) || (b != 0)) as i128,
    };
    clamp(r)
}

impl SExpr {
    fn eval(&self, vars: &[i128]) -> i128 {
        match self {
            SExpr::Const(c) => *c as i128,
            SExpr::Var(i) => vars.get(*i).copied().unwrap_or(0),
            SExpr::Unary(Un::Neg, a) => clamp(-a.eval(vars)),
            SExpr::Unary(Un::Not, a) => (a.eval(vars) == 0) as i128,
            SExpr::Binary(op, a, b) => apply_bin(*op, a.eval(vars), b.eval(vars)),
            SExpr::Ite(c, t, e) => {
                if c.eval(vars) != 0 {
                    t.eval(vars)
                } else {
                    e.eval(vars)
                }
            }
        }
    }

    fn size(&self) -> usize {
        match self {
            SExpr::Const(_) | SExpr::Var(_) => 1,
            SExpr::Unary(_, a) => 1 + a.size(),
            SExpr::Binary(_, a, b) => 1 + a.size() + b.size(),
            SExpr::Ite(c, t, e) => 1 + c.size() + t.size() + e.size(),
        }
    }
}

/// Render a tree with program variable names (for `EVIDENT_SYMBOLIC_ANNOUNCE`).
fn render_named(e: &SExpr, inputs: &[(String, NumKind)]) -> String {
    match e {
        SExpr::Var(i) => inputs
            .get(*i)
            .map(|(n, _)| n.clone())
            .unwrap_or_else(|| format!("x{i}")),
        SExpr::Const(c) => c.to_string(),
        SExpr::Unary(Un::Neg, a) => format!("(-{})", render_named(a, inputs)),
        SExpr::Unary(Un::Not, a) => format!("(!{})", render_named(a, inputs)),
        SExpr::Binary(op, a, b) => {
            let s = bin_symbol(*op);
            format!("({} {} {})", render_named(a, inputs), s, render_named(b, inputs))
        }
        SExpr::Ite(c, t, e) => format!(
            "(if {} then {} else {})",
            render_named(c, inputs),
            render_named(t, inputs),
            render_named(e, inputs)
        ),
    }
}

fn bin_symbol(op: Bin) -> &'static str {
    match op {
        Bin::Add => "+",
        Bin::Sub => "-",
        Bin::Mul => "*",
        Bin::Div => "/",
        Bin::Mod => "%",
        Bin::Lt => "<",
        Bin::Le => "<=",
        Bin::Gt => ">",
        Bin::Ge => ">=",
        Bin::Eq => "==",
        Bin::And => "&&",
        Bin::Or => "||",
    }
}

fn render(e: &SExpr) -> String {
    match e {
        SExpr::Const(c) => c.to_string(),
        SExpr::Var(i) => format!("x{i}"),
        SExpr::Unary(Un::Neg, a) => format!("(-{})", render(a)),
        SExpr::Unary(Un::Not, a) => format!("(!{})", render(a)),
        SExpr::Binary(op, a, b) => {
            format!("({} {} {})", render(a), bin_symbol(*op), render(b))
        }
        SExpr::Ite(c, t, e) => format!("(if {} then {} else {})", render(c), render(t), render(e)),
    }
}

/// Compiled artifact: one closed-form `SExpr` per output. No Z3 ASTs; pure owned data.
struct SymbolicProgram {
    inputs: Vec<(String, NumKind)>,   // position = Var index in every tree
    outputs: Vec<(String, NumKind, SExpr)>,
}

impl super::CompiledFunction for SymbolicProgram {
    fn call(&self, given: &HashMap<String, Value>) -> Option<HashMap<String, Value>> {
        // Missing or mistyped input → None; runtime falls through to Z3.
        let mut vars: Vec<i128> = Vec::with_capacity(self.inputs.len());
        for (name, kind) in &self.inputs {
            let v = given.get(name)?;
            let n = match (kind, v) {
                (NumKind::Int, Value::Int(n)) => *n as i128,
                (NumKind::Bool, Value::Bool(b)) => *b as i128,
                _ => return None,
            };
            vars.push(n);
        }
        let mut out = HashMap::with_capacity(self.outputs.len());
        for (name, kind, expr) in &self.outputs {
            let v = expr.eval(&vars);
            let val = match kind {
                NumKind::Int => Value::Int(saturate_i64(v)),
                NumKind::Bool => Value::Bool(v != 0),
            };
            out.insert(name.clone(), val);
        }
        Some(out)
    }
}

fn saturate_i64(v: i128) -> i64 {
    v.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

/// Symbolic-regression functionizer. Opt-in via `EvidentRuntime::with_functionizer`.
pub struct SymbolicFunctionizer {
    cfg: GpConfig,
}

#[derive(Clone)]
struct GpConfig {
    seed: u64,
    population: usize,
    generations: usize,
    tournament: usize,
    train_samples: usize,
    valid_samples: usize,   // wide-range; acceptance gated on exactness here too
    budget: Duration,       // wall-clock ceiling per output
}

impl Default for GpConfig {
    fn default() -> Self {
        let seed = std::env::var("EVIDENT_SYMBOLIC_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0x5EED_C0DE_u64);
        GpConfig {
            seed,
            population: 300,
            generations: 40,
            tournament: 4,
            train_samples: 40,
            valid_samples: 25,
            budget: Duration::from_secs(3),
        }
    }
}

impl Default for SymbolicFunctionizer {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolicFunctionizer {
    pub fn new() -> Self {
        SymbolicFunctionizer {
            cfg: GpConfig::default(),
        }
    }
}

impl super::Functionizer for SymbolicFunctionizer {
    fn name(&self) -> &'static str {
        "symbolic"
    }

    fn compile(
        &self,
        program: &Z3Program,
        _enums: &EnumRegistry,
        _datatypes: &crate::core::DatatypeRegistry,
    ) -> Option<Rc<dyn super::CompiledFunction>> {
        let trace = std::env::var("EVIDENT_SYMBOLIC_TRACE").is_ok();
        // Announce on stdout when `EVIDENT_SYMBOLIC_ANNOUNCE` is set; off by default.
        let announce = std::env::var("EVIDENT_SYMBOLIC_ANNOUNCE").is_ok();

        if !program.checks.is_empty() || !program.predicates.is_empty() {
            if trace {
                eprintln!("[symbolic] bail: program has checks/predicates (conditional body)");
            }
            return None;
        }
        if program.steps.is_empty() || program.steps.len() > MAX_OUTPUTS {
            return None;
        }

        let mut outputs: Vec<(String, NumKind)> = Vec::with_capacity(program.steps.len());
        let mut step_exprs: Vec<(String, NumKind, &Dynamic)> = Vec::new();
        let mut output_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        for step in &program.steps {
            let Z3Step::Scalar { var, expr } = step else {
                if trace {
                    eprintln!("[symbolic] bail: non-scalar step for {}", step.var());
                }
                return None;
            };
            let kind = NumKind::from_sort_name(&format!("{}", expr.get_sort()));
            let Some(kind) = kind else {
                if trace {
                    eprintln!(
                        "[symbolic] bail: output {var} has unsupported sort {}",
                        expr.get_sort()
                    );
                }
                return None;
            };
            outputs.push((var.clone(), kind));
            output_names.insert(var.clone());
            step_exprs.push((var.clone(), kind, expr));
        }

        let mut free: BTreeMap<String, String> = BTreeMap::new();
        for (_, _, expr) in &step_exprs {
            collect_free_consts(expr, &mut free);
        }
        let mut inputs: Vec<(String, NumKind)> = Vec::new();
        for (name, sort) in &free {
            if output_names.contains(name) {
                continue;
            }
            let Some(kind) = NumKind::from_sort_name(sort) else {
                if trace {
                    eprintln!("[symbolic] bail: input {name} has unsupported sort {sort}");
                }
                return None;
            };
            inputs.push((name.clone(), kind));
        }
        if inputs.len() > MAX_INPUTS {
            if trace {
                eprintln!("[symbolic] bail: {} inputs (> {MAX_INPUTS})", inputs.len());
            }
            return None;
        }

        let ctx: &Context = step_exprs[0].2.get_ctx();

        let mut rng = StdRng::seed_from_u64(self.cfg.seed);
        let inputs_for_sampling: Vec<(String, NumKind)> = inputs.clone();
        let outputs_for_sampling: Vec<(String, NumKind)> = outputs.clone();

        let mut sample_inputs: Vec<Vec<i128>> = Vec::new();
        let mut sample_outputs: Vec<Vec<i128>> = Vec::new();
        let total = self.cfg.train_samples + self.cfg.valid_samples;
        let mut attempts = 0usize;
        while sample_inputs.len() < total && attempts < total * 4 {
            attempts += 1;
            // Narrow range for training; wide for held-out (spurious overfits diverge on wide).
            let wide = sample_inputs.len() >= self.cfg.train_samples;
            let input_vals = random_input_vec(&mut rng, &inputs_for_sampling, wide);
            let Some(out_vals) = solve_program_at(
                program,
                ctx,
                &inputs_for_sampling,
                &outputs_for_sampling,
                &input_vals,
            ) else {
                if trace {
                    eprintln!("[symbolic] bail: program did not solve for a sampled input");
                }
                return None;
            };
            sample_inputs.push(input_vals);
            sample_outputs.push(out_vals);
        }
        if sample_inputs.len() < total {
            return None;
        }

        let n_vars = inputs.len();
        let mut fitted: Vec<(String, NumKind, SExpr)> = Vec::with_capacity(outputs.len());
        for (oi, (name, kind)) in outputs.iter().enumerate() {
            let targets: Vec<i128> = sample_outputs.iter().map(|o| o[oi]).collect();
            let expr = fit_one(&self.cfg, &mut rng, n_vars, &sample_inputs, &targets)?;
            if trace {
                eprintln!("[symbolic] {name} = {}", render(&expr));
            }
            if announce {
                println!("symbolic functionizer: rediscovered {name} = {}",
                    render_named(&expr, &inputs));
            }
            fitted.push((name.clone(), *kind, expr));
        }

        Some(Rc::new(SymbolicProgram {
            inputs,
            outputs: fitted,
        }))
    }
}

fn random_input_vec(rng: &mut StdRng, inputs: &[(String, NumKind)], wide: bool) -> Vec<i128> {
    inputs
        .iter()
        .map(|(_, kind)| match kind {
            NumKind::Bool => rng.gen_range(0..=1) as i128,
            NumKind::Int => {
                if wide {
                    rng.gen_range(-40..=40) as i128
                } else {
                    rng.gen_range(-15..=15) as i128
                }
            }
        })
        .collect()
}

/// Bind inputs to concrete values, solve, return output values in `outputs` order.
fn solve_program_at(
    program: &Z3Program,
    ctx: &Context,
    inputs: &[(String, NumKind)],
    outputs: &[(String, NumKind)],
    input_vals: &[i128],
) -> Option<Vec<i128>> {
    let solver = Solver::new(ctx);

    for ((name, kind), &val) in inputs.iter().zip(input_vals) {
        match kind {
            NumKind::Int => {
                let c = Int::new_const(ctx, name.as_str());
                solver.assert(&c._eq(&Int::from_i64(ctx, val as i64)));
            }
            NumKind::Bool => {
                let c = Bool::new_const(ctx, name.as_str());
                solver.assert(&c._eq(&Bool::from_bool(ctx, val != 0)));
            }
        }
    }

    let out_kind: HashMap<&str, NumKind> = outputs.iter().map(|(n, k)| (n.as_str(), *k)).collect();
    for step in &program.steps {
        if let Z3Step::Scalar { var, expr } = step {
            let Some(kind) = out_kind.get(var.as_str()) else {
                continue;
            };
            let c = out_const(ctx, var, *kind);
            solver.assert(&c._eq(expr));
        }
    }

    if !matches!(solver.check(), SatResult::Sat) {
        return None;
    }
    let model = solver.get_model()?;

    let mut vals = Vec::with_capacity(outputs.len());
    for (name, kind) in outputs {
        let v = match kind {
            NumKind::Int => {
                let c = Int::new_const(ctx, name.as_str());
                model.eval(&c, true)?.as_i64()? as i128
            }
            NumKind::Bool => {
                let c = Bool::new_const(ctx, name.as_str());
                model.eval(&c, true)?.as_bool()? as i128
            }
        };
        vals.push(v);
    }
    Some(vals)
}

fn out_const<'ctx>(ctx: &'ctx Context, name: &str, kind: NumKind) -> Dynamic<'ctx> {
    match kind {
        NumKind::Int => Dynamic::from_ast(&Int::new_const(ctx, name)),
        NumKind::Bool => Dynamic::from_ast(&Bool::new_const(ctx, name)),
    }
}

fn collect_free_consts(d: &Dynamic, out: &mut BTreeMap<String, String>) {
    if d.kind() == AstKind::App {
        if let Ok(decl) = d.safe_decl() {
            if decl.kind() == DeclKind::UNINTERPRETED && d.num_children() == 0 {
                out.insert(decl.name(), format!("{}", d.get_sort()));
                return;
            }
        }
        for c in d.children() {
            collect_free_consts(&c, out);
        }
    }
}

/// Summed squared error; 0 means exact reproduction.
fn sse(expr: &SExpr, samples: &[Vec<i128>], targets: &[i128]) -> i128 {
    let mut total: i128 = 0;
    for (vars, &t) in samples.iter().zip(targets) {
        let d = (expr.eval(vars) - t).clamp(-DIFF_CLAMP, DIFF_CLAMP);
        total = total.saturating_add(d * d);
    }
    total
}

/// Fitness: error dominates; tree size is a tiebreak (Occam pressure).
fn score(expr: &SExpr, samples: &[Vec<i128>], targets: &[i128]) -> i128 {
    let e = sse(expr, samples, targets);
    e.saturating_mul(1000).saturating_add(expr.size() as i128)
}

fn accept_if_exact(expr: SExpr, samples: &[Vec<i128>], targets: &[i128]) -> Option<SExpr> {
    if sse(&expr, samples, targets) == 0 {
        Some(expr)
    } else {
        None
    }
}

fn fit_one(
    cfg: &GpConfig,
    rng: &mut StdRng,
    n_vars: usize,
    samples: &[Vec<i128>],
    targets: &[i128],
) -> Option<SExpr> {
    if let Some(e) = try_analytic(n_vars, samples, targets) {
        return Some(e);
    }
    let bank = seed_bank(n_vars, targets);
    for cand in &bank {
        if sse(cand, samples, targets) == 0 {
            return Some(cand.clone());
        }
    }
    let start = Instant::now();
    let mut population: Vec<SExpr> = bank;
    while population.len() < cfg.population {
        population.push(random_tree(rng, 3, n_vars));
    }

    for _gen in 0..cfg.generations {
        // Rank by score (lower is better).
        population.sort_by_key(|e| score(e, samples, targets));
        if let Some(best) = population.first() {
            if let Some(found) = accept_if_exact(best.clone(), samples, targets) {
                return Some(found);
            }
        }
        if start.elapsed() > cfg.budget {
            break;
        }

        let elite = (cfg.population / 20).max(2);
        let mut next: Vec<SExpr> = population.iter().take(elite).cloned().collect();
        while next.len() < cfg.population {
            let a = tournament(rng, &population, cfg.tournament, samples, targets);
            let mut child = if rng.gen_bool(0.85) {
                let b = tournament(rng, &population, cfg.tournament, samples, targets);
                crossover(rng, a, b)
            } else {
                a.clone()
            };
            if rng.gen_bool(0.25) {
                child = mutate(rng, &child, n_vars);
            }
            if child.size() > 60 {  // discard bloated trees
                child = random_tree(rng, 3, n_vars);
            }
            next.push(child);
        }
        population = next;
    }

    population.sort_by_key(|e| score(e, samples, targets));
    accept_if_exact(population.into_iter().next()?, samples, targets)
}

/// Fast analytic paths: constant, or single-variable affine `a·x + b`.
fn try_analytic(n_vars: usize, samples: &[Vec<i128>], targets: &[i128]) -> Option<SExpr> {
    if let Some(&first) = targets.first() {
        if targets.iter().all(|&t| t == first) {
            if let Ok(c) = i64::try_from(first) {
                return accept_if_exact(SExpr::Const(c), samples, targets);
            }
        }
    }

    if n_vars == 1 {
        let mut pair: Option<(usize, usize)> = None;
        'outer: for i in 0..samples.len() {
            for j in (i + 1)..samples.len() {
                if samples[i][0] != samples[j][0] {
                    pair = Some((i, j));
                    break 'outer;
                }
            }
        }
        if let Some((i, j)) = pair {
            let dx = samples[j][0] - samples[i][0];
            let dy = targets[j] - targets[i];
            if dx != 0 && dy % dx == 0 {
                let a = dy / dx;
                let b = targets[i] - a * samples[i][0];
                if let (Ok(a), Ok(b)) = (i64::try_from(a), i64::try_from(b)) {
                    let expr = SExpr::Binary(
                        Bin::Add,
                        Box::new(SExpr::Binary(
                            Bin::Mul,
                            Box::new(SExpr::Const(a)),
                            Box::new(SExpr::Var(0)),
                        )),
                        Box::new(SExpr::Const(b)),
                    );
                    return accept_if_exact(expr, samples, targets);
                }
            }
        }
    }
    None
}

/// Seed the initial population with structured candidates: constants, affine, products, comparisons.
fn seed_bank(n_vars: usize, targets: &[i128]) -> Vec<SExpr> {
    let mut bank: Vec<SExpr> = Vec::new();
    let median = {
        let mut t: Vec<i128> = targets.to_vec();
        t.sort_unstable();
        i64::try_from(t.get(t.len() / 2).copied().unwrap_or(0)).unwrap_or(0)
    };
    for c in [0, 1, -1, median] {
        bank.push(SExpr::Const(c));
    }
    let coeffs = [-3i64, -2, -1, 1, 2, 3, 5, 10];
    for i in 0..n_vars {
        bank.push(SExpr::Var(i));
        bank.push(SExpr::Unary(Un::Neg, Box::new(SExpr::Var(i))));
        bank.push(SExpr::Binary(
            Bin::Mul,
            Box::new(SExpr::Var(i)),
            Box::new(SExpr::Var(i)),
        ));
        for &a in &coeffs {
            for b in -10i64..=10 {
                bank.push(SExpr::Binary(
                    Bin::Add,
                    Box::new(SExpr::Binary(
                        Bin::Mul,
                        Box::new(SExpr::Const(a)),
                        Box::new(SExpr::Var(i)),
                    )),
                    Box::new(SExpr::Const(b)),
                ));
            }
        }
        for c in -2i64..=5 {
            for op in [Bin::Lt, Bin::Le, Bin::Gt, Bin::Ge, Bin::Eq] {
                bank.push(SExpr::Binary(
                    op,
                    Box::new(SExpr::Var(i)),
                    Box::new(SExpr::Const(c)),
                ));
            }
        }
        for j in (i + 1)..n_vars {
            for op in [Bin::Add, Bin::Sub, Bin::Mul] {
                bank.push(SExpr::Binary(
                    op,
                    Box::new(SExpr::Var(i)),
                    Box::new(SExpr::Var(j)),
                ));
            }
        }
    }
    bank
}

// GP operators

fn random_terminal(rng: &mut StdRng, n_vars: usize) -> SExpr {
    if n_vars > 0 && rng.gen_bool(0.6) {
        SExpr::Var(rng.gen_range(0..n_vars))
    } else {
        SExpr::Const(rng.gen_range(-5..=10))
    }
}

fn random_bin(rng: &mut StdRng) -> Bin {
    const OPS: [Bin; 12] = [
        Bin::Add,
        Bin::Sub,
        Bin::Mul,
        Bin::Div,
        Bin::Mod,
        Bin::Lt,
        Bin::Le,
        Bin::Gt,
        Bin::Ge,
        Bin::Eq,
        Bin::And,
        Bin::Or,
    ];
    OPS[rng.gen_range(0..OPS.len())]
}

fn random_tree(rng: &mut StdRng, depth: u32, n_vars: usize) -> SExpr {
    if depth == 0 || rng.gen_bool(0.3) {
        return random_terminal(rng, n_vars);
    }
    match rng.gen_range(0..10) {
        0 => {
            let u = if rng.gen_bool(0.5) { Un::Neg } else { Un::Not };
            SExpr::Unary(u, Box::new(random_tree(rng, depth - 1, n_vars)))
        }
        1 => SExpr::Ite(
            Box::new(random_tree(rng, depth - 1, n_vars)),
            Box::new(random_tree(rng, depth - 1, n_vars)),
            Box::new(random_tree(rng, depth - 1, n_vars)),
        ),
        _ => SExpr::Binary(
            random_bin(rng),
            Box::new(random_tree(rng, depth - 1, n_vars)),
            Box::new(random_tree(rng, depth - 1, n_vars)),
        ),
    }
}

fn tournament<'a>(
    rng: &mut StdRng,
    pop: &'a [SExpr],
    k: usize,
    samples: &[Vec<i128>],
    targets: &[i128],
) -> &'a SExpr {
    let mut best = &pop[rng.gen_range(0..pop.len())];
    let mut best_score = score(best, samples, targets);
    for _ in 1..k {
        let cand = &pop[rng.gen_range(0..pop.len())];
        let s = score(cand, samples, targets);
        if s < best_score {
            best = cand;
            best_score = s;
        }
    }
    best
}

fn subtree_at(e: &SExpr, n: usize, counter: &mut usize) -> Option<SExpr> {
    let here = *counter;
    *counter += 1;
    if here == n {
        return Some(e.clone());
    }
    match e {
        SExpr::Const(_) | SExpr::Var(_) => None,
        SExpr::Unary(_, a) => subtree_at(a, n, counter),
        SExpr::Binary(_, a, b) => {
            subtree_at(a, n, counter).or_else(|| subtree_at(b, n, counter))
        }
        SExpr::Ite(c, t, f) => subtree_at(c, n, counter)
            .or_else(|| subtree_at(t, n, counter))
            .or_else(|| subtree_at(f, n, counter)),
    }
}

fn replace_at(e: &SExpr, n: usize, repl: &SExpr, counter: &mut usize) -> SExpr {
    let here = *counter;
    *counter += 1;
    if here == n {
        return repl.clone();
    }
    match e {
        SExpr::Const(_) | SExpr::Var(_) => e.clone(),
        SExpr::Unary(op, a) => SExpr::Unary(*op, Box::new(replace_at(a, n, repl, counter))),
        SExpr::Binary(op, a, b) => {
            let na = replace_at(a, n, repl, counter);
            let nb = replace_at(b, n, repl, counter);
            SExpr::Binary(*op, Box::new(na), Box::new(nb))
        }
        SExpr::Ite(c, t, f) => {
            let nc = replace_at(c, n, repl, counter);
            let nt = replace_at(t, n, repl, counter);
            let nf = replace_at(f, n, repl, counter);
            SExpr::Ite(Box::new(nc), Box::new(nt), Box::new(nf))
        }
    }
}

fn crossover(rng: &mut StdRng, a: &SExpr, b: &SExpr) -> SExpr {
    let a_size = a.size();
    let b_size = b.size();
    let cut_a = rng.gen_range(0..a_size);
    let cut_b = rng.gen_range(0..b_size);
    let donor = {
        let mut c = 0;
        subtree_at(b, cut_b, &mut c).unwrap_or_else(|| b.clone())
    };
    let mut c = 0;
    replace_at(a, cut_a, &donor, &mut c)
}

fn mutate(rng: &mut StdRng, e: &SExpr, n_vars: usize) -> SExpr {
    let size = e.size();
    let cut = rng.gen_range(0..size);
    let fresh = random_tree(rng, 2, n_vars);
    let mut c = 0;
    replace_at(e, cut, &fresh, &mut c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EvidentRuntime;
    use std::collections::HashMap;

    fn rt_with_symbolic(src: &str) -> EvidentRuntime {
        let mut rt = EvidentRuntime::with_functionizer(Box::new(SymbolicFunctionizer::new()));
        rt.load_source(src).expect("load");
        rt
    }

    fn given_int(name: &str, v: i64) -> HashMap<String, Value> {
        let mut g = HashMap::new();
        g.insert(name.to_string(), Value::Int(v));
        g
    }

    /// Discovers `3·x + 5`; `compiled` stat confirms symbolic ran (not Z3 fallback).
    #[test]
    fn discovers_linear_polynomial() {
        let rt = rt_with_symbolic(
            "claim poly\n    input ∈ Int\n    output ∈ Int = 3 * input + 5\n",
        );

        for x in [-7i64, 0, 1, 4, 13, 100] {
            let r = rt.query("poly", &given_int("input", x)).expect("query");
            assert!(r.satisfied, "poly should be SAT for input={x}");
            assert_eq!(
                r.bindings.get("output"),
                Some(&Value::Int(3 * x + 5)),
                "output mismatch for input={x}"
            );
        }

        let stats = rt.functionize_stats();
        let poly = stats.claims.get("poly").expect("stats for poly");
        assert!(
            poly.compiled >= 1,
            "symbolic functionizer should have compiled `poly` (compiled={})",
            poly.compiled
        );
    }

    #[test]
    fn discovers_quadratic() {
        let rt = rt_with_symbolic("claim sq\n    input ∈ Int\n    output ∈ Int = input * input\n");
        for x in [-9i64, -1, 0, 2, 6, 30] {
            let r = rt.query("sq", &given_int("input", x)).expect("query");
            assert!(r.satisfied);
            assert_eq!(r.bindings.get("output"), Some(&Value::Int(x * x)));
        }
        let stats = rt.functionize_stats();
        assert!(stats.claims.get("sq").map(|s| s.compiled).unwrap_or(0) >= 1);
    }

    #[test]
    fn discovers_multi_output() {
        let rt = rt_with_symbolic(
            "claim two\n    a ∈ Int\n    b ∈ Int\n    \
             sum ∈ Int = a + b\n    diff ∈ Int = a - b\n",
        );
        let mut g = HashMap::new();
        g.insert("a".to_string(), Value::Int(12));
        g.insert("b".to_string(), Value::Int(5));
        let r = rt.query("two", &g).expect("query");
        assert_eq!(r.bindings.get("sum"), Some(&Value::Int(17)));
        assert_eq!(r.bindings.get("diff"), Some(&Value::Int(7)));
    }

    /// String output triggers `compile → None`; runtime falls through to Z3 and still answers.
    #[test]
    fn falls_back_on_unsupported_output() {
        let rt = rt_with_symbolic(
            "claim greet\n    n ∈ Int\n    msg ∈ String = \"hi\"\n    doubled ∈ Int = n * 2\n",
        );
        let r = rt.query("greet", &given_int("n", 21)).expect("query");
        assert!(r.satisfied);
        assert_eq!(r.bindings.get("doubled"), Some(&Value::Int(42)));
        assert_eq!(r.bindings.get("msg"), Some(&Value::Str("hi".to_string())));
    }

    #[test]
    fn sexpr_eval_basics() {
        // 3*x + 5
        let e = SExpr::Binary(
            Bin::Add,
            Box::new(SExpr::Binary(
                Bin::Mul,
                Box::new(SExpr::Const(3)),
                Box::new(SExpr::Var(0)),
            )),
            Box::new(SExpr::Const(5)),
        );
        assert_eq!(e.eval(&[7]), 26);
        assert_eq!(e.eval(&[0]), 5);
        let d = SExpr::Binary(Bin::Div, Box::new(SExpr::Const(4)), Box::new(SExpr::Const(0)));
        // div-by-zero returns 0
        assert_eq!(d.eval(&[]), 0);
    }
}
