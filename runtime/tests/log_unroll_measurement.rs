//! Empirical measurement: how well does Z3 collapse an FSM body
//! unrolled / composed N times? This decides whether to build an FSM
//! functionizer via **log-unrolling** (exponentiation-by-squaring
//! composition of a step with itself, simplifying between doublings)
//! or whether we need a different strategy (CEGAR).
//!
//! The question is concrete: for representative FSM body shapes, what
//! is the size ratio per doubling after simplification?
//!   * ratio ≈ 1.0  → form is flat in N: log-unroll is a huge win.
//!   * ratio ≈ 2.0  → form is linear in N: log-unroll buys nothing
//!                    over naive (asymptotically the same).
//!
//! Two complementary measurements, because "size" has two faces:
//!
//! A. **Closed-form composition** (synthetic shapes 1–5). A step that
//!    is a clean state→state vector function `out_i = f_i(in)` can be
//!    composed with itself by *substitution* — exactly what an FSM
//!    functionizer doing log-unroll would do: build S², simplify, build
//!    S⁴ = S²∘S², simplify, … The assertion count is fixed (= the state
//!    arity); the interesting metric is the **AST node count** of the
//!    composed-and-simplified transition. We report it at N = 1,2,…,64
//!    and the per-doubling ratio.
//!
//! B. **Goal-level unroll** (shape 6 — real Mario `game`). A real FSM
//!    body has guarded constraints + internal vars, so it is *not* a
//!    clean vector function; we cannot substitute-compose it. Instead
//!    we clone the simplified per-tick body N times, rename the state
//!    leaves so tick i's `world_next.*` IS tick i+1's `world.*` (the
//!    bridge), and hand the whole goal to Z3's tactic chain. We measure
//!    under two chains:
//!      * `simplify, propagate-values` — the chain the production
//!        `z3_eval::simplify_assertions` uses. It does NOT eliminate
//!        variables, so it cannot collapse a cross-tick chain.
//!      * `simplify, solve-eqs, propagate-values, simplify` — adds
//!        variable elimination, which is what a log-unroller needs to
//!        remove the bridge / internal vars. THIS is the load-bearing
//!        number for real FSMs.
//!
//! NOTE on the production pass: `simplify_assertions` deliberately
//! EXCLUDES `solve-eqs` (it would destroy the `(= var expr)` shape the
//! per-output extractor needs). So the existing pass alone can never
//! collapse an unrolled chain — log-unroll needs its own elimination
//! step. Measurement B quantifies what that step can achieve.
//!
//! Reproduce:
//!   cargo test --release --test log_unroll_measurement -- --nocapture
//! The run regenerates `docs/perf/log-unroll-feasibility.md`
//! deterministically (no wall-times in the doc).

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use evident_runtime::z3_eval::{collect_touched_names, simplify_assertions};
use evident_runtime::{EvidentRuntime, Value};
use z3::ast::{Ast, Bool, Datatype, Dynamic, Int};
use z3::{AstKind, Context, Goal, Sort, Tactic};

const POWERS: &[usize] = &[1, 2, 4, 8, 16, 32, 64];

// ───────────────────────── size metric ─────────────────────────

/// Unique AST node count across a set of expressions. Z3 ASTs are
/// hash-consed DAGs, so a recursive walk that dedups by node identity
/// (via the z3 crate's `Hash`/`Eq`, which key on `Z3_get_ast_id`)
/// counts shared subterms once — the honest "how big is this term".
fn count_nodes(exprs: &[Dynamic]) -> usize {
    let mut seen: HashSet<Dynamic> = HashSet::new();
    let mut stack: Vec<Dynamic> = exprs.to_vec();
    while let Some(d) = stack.pop() {
        if seen.insert(d.clone()) {
            for c in d.children() {
                stack.push(c);
            }
        }
    }
    seen.len()
}

fn to_dyn<'c>(ints: &[Int<'c>]) -> Vec<Dynamic<'c>> {
    ints.iter().map(|i| Dynamic::from_ast(i)).collect()
}

// ─────────────── A. closed-form composition (shapes 1–5) ───────────────

#[derive(Clone)]
struct Row {
    n: usize,
    pre_nodes: usize,  // composed, before simplify
    post_nodes: usize, // after simplify (the size that gets re-composed)
    assertions: usize, // = state arity (constant)
    ratio: Option<f64>,
}

struct ShapeResult {
    name: &'static str,
    note: &'static str,
    rows: Vec<Row>,
}

/// Compose a step (given as the vector of output exprs over the input
/// consts) with itself: `S∘S`. Substitute each input var with the
/// corresponding output expr (simultaneous), so `S²[i] = S[i](S[0..m])`.
fn compose<'c>(half: &[Int<'c>], inputs: &[Int<'c>]) -> Vec<Int<'c>> {
    let from = to_dyn(inputs);
    let to = to_dyn(half);
    let pairs: Vec<(&Dynamic, &Dynamic)> = from.iter().zip(to.iter()).collect();
    half.iter()
        .map(|e| {
            Dynamic::from_ast(e)
                .substitute(&pairs)
                .as_int()
                .expect("int after subst")
        })
        .collect()
}

/// Run the exp-by-squaring measurement for one synthetic shape.
fn measure_synthetic<'c>(
    ctx: &'c Context,
    name: &'static str,
    note: &'static str,
    n_vars: usize,
    step: impl Fn(&[Int<'c>]) -> Vec<Int<'c>>,
) -> ShapeResult {
    let inputs: Vec<Int> = (0..n_vars)
        .map(|i| Int::new_const(ctx, format!("s{i}")))
        .collect();

    // S¹.
    let raw1 = step(&inputs);
    let pre1 = count_nodes(&to_dyn(&raw1));
    let simp1: Vec<Int> = raw1.iter().map(|e| e.simplify()).collect();
    let post1 = count_nodes(&to_dyn(&simp1));
    let mut rows = vec![Row {
        n: 1,
        pre_nodes: pre1,
        post_nodes: post1,
        assertions: n_vars,
        ratio: None,
    }];

    let mut cur = simp1;
    let mut n = 1usize;
    while n < *POWERS.last().unwrap() {
        let composed = compose(&cur, &inputs);
        let pre = count_nodes(&to_dyn(&composed));
        let simp: Vec<Int> = composed.iter().map(|e| e.simplify()).collect();
        let post = count_nodes(&to_dyn(&simp));
        let prev_post = rows.last().unwrap().post_nodes;
        n *= 2;
        rows.push(Row {
            n,
            pre_nodes: pre,
            post_nodes: post,
            assertions: n_vars,
            ratio: Some(post as f64 / prev_post as f64),
        });
        cur = simp;
    }
    ShapeResult { name, note, rows }
}

fn synthetic_shapes(ctx: &Context) -> Vec<ShapeResult> {
    let i = |v: i64| Int::from_i64(ctx, v);
    let mut out = Vec::new();

    // 1. Pure counter: cₙ₊₁ = cₙ + 1.
    out.push(measure_synthetic(
        ctx,
        "pure counter",
        "cₙ₊₁ = cₙ + 1",
        1,
        |s| vec![Int::add(ctx, &[&s[0], &i(1)])],
    ));

    // 2. Linear recurrence: xₙ₊₁ = 3·xₙ + 7 (affine, closed-form).
    out.push(measure_synthetic(
        ctx,
        "linear recurrence",
        "xₙ₊₁ = 3·xₙ + 7",
        1,
        |s| vec![Int::add(ctx, &[&Int::mul(ctx, &[&i(3), &s[0]]), &i(7)])],
    ));

    // 3. Conditional update: xₙ₊₁ = (xₙ > 0 ? xₙ − 1 : xₙ).
    out.push(measure_synthetic(
        ctx,
        "conditional update",
        "xₙ₊₁ = (xₙ > 0 ? xₙ − 1 : xₙ)",
        1,
        |s| {
            let dec = Int::sub(ctx, &[&s[0], &i(1)]);
            vec![s[0].gt(&i(0)).ite(&dec, &s[0])]
        },
    ));

    // 4. Fibonacci: xₙ₊₁ = yₙ ; yₙ₊₁ = xₙ + yₙ (linear system).
    out.push(measure_synthetic(
        ctx,
        "Fibonacci",
        "x' = y ; y' = x + y",
        2,
        |s| vec![s[1].clone(), Int::add(ctx, &[&s[0], &s[1]])],
    ));

    // 5. 3-state machine + counter: state cycles 0→1→2→0, acc++ on state 2.
    out.push(measure_synthetic(
        ctx,
        "3-state machine",
        "s' = cycle(s); acc' = (s=2 ? acc+1 : acc)",
        2,
        |s| {
            let state = &s[0];
            let acc = &s[1];
            let next_state = state
                .gt(&i(1))
                .ite(&i(0), &Int::add(ctx, &[state, &i(1)])); // 0→1→2→0
            let next_acc = state.gt(&i(1)).ite(&Int::add(ctx, &[acc, &i(1)]), acc);
            vec![next_state, next_acc]
        },
    ));

    out
}

// ─────────────── B. goal-level unroll (shape 6 — Mario) ───────────────

struct MarioRow {
    n: usize,
    pre_size: u32,
    pre_exprs: u32,
    // simplify + propagate-values (production pass — no elimination)
    prop_size: u32,
    prop_exprs: u32,
    // simplify + solve-eqs + propagate-values + simplify (log-unroll)
    elim_size: u32,
    elim_exprs: u32,
    elim_ratio: Option<f64>, // elim_exprs(n) / elim_exprs(n/2)
}

struct MarioResult {
    raw_tick: usize,
    simp_tick: usize,
    rows: Vec<MarioRow>,
}

/// Collect a representative `Dynamic` for every 0-arity uninterpreted
/// constant in `assertions` whose name is in `touched`. Keyed by name,
/// carries the sort (so we can mint same-sorted renamed copies).
fn collect_const_dyns<'a>(
    assertions: &[Bool<'a>],
    touched: &HashSet<String>,
) -> HashMap<String, Dynamic<'a>> {
    fn rec<'a>(
        d: &Dynamic<'a>,
        touched: &HashSet<String>,
        out: &mut HashMap<String, Dynamic<'a>>,
    ) {
        if d.kind() == AstKind::App && d.num_children() == 0 {
            if let Ok(decl) = d.safe_decl() {
                let name = decl.name();
                if touched.contains(&name) {
                    out.entry(name).or_insert_with(|| d.clone());
                }
            }
            return;
        }
        for c in d.children() {
            rec(&c, touched, out);
        }
    }
    let mut out = HashMap::new();
    for a in assertions {
        rec(&Dynamic::from_ast(a), touched, &mut out);
    }
    out
}

/// The per-tick rename rule. `world.X` at tick i → snapshot i; the
/// `world_next.X` it produces → snapshot i+1 (= next tick's input —
/// the bridge). Everything else (internal vars, Level constants,
/// is_first_tick) gets a per-tick fresh name.
fn renamed<'a>(name: &str, tick: usize, ctx: &'a Context, orig: &Dynamic<'a>) -> Dynamic<'a> {
    let new_name = if let Some(suffix) = name.strip_prefix("world_next.") {
        format!("snap{}__{suffix}", tick + 1)
    } else if let Some(suffix) = name.strip_prefix("world.") {
        format!("snap{tick}__{suffix}")
    } else {
        format!("{name}@t{tick}")
    };
    fresh_const_like(ctx, &new_name, orig)
}

/// Mint a fresh constant named `name` with the SAME sort as `orig`
/// (so Z3 substitution `orig ↦ fresh` type-checks). Dispatched by sort
/// kind — the z3 crate's per-type `new_const` each assert their kind.
/// Covers every sort Mario's state leaves use: Int, Bool, and Array
/// (the `Seq` arrays for coins/enemies). The array element sort is
/// recovered via `select` (whose result's `get_sort` keeps the full
/// 'a lifetime, unlike `Sort::array_range`).
fn fresh_const_like<'a>(ctx: &'a Context, name: &str, orig: &Dynamic<'a>) -> Dynamic<'a> {
    use z3::SortKind;
    let sort = orig.get_sort();
    match sort.kind() {
        SortKind::Int => Dynamic::from_ast(&Int::new_const(ctx, name)),
        SortKind::Bool => Dynamic::from_ast(&z3::ast::Bool::new_const(ctx, name)),
        SortKind::Real => Dynamic::from_ast(&z3::ast::Real::new_const(ctx, name)),
        SortKind::Datatype => Dynamic::from_ast(&Datatype::new_const(ctx, name, &sort)),
        SortKind::Array => {
            let arr = orig.as_array().expect("array dynamic");
            let elem = arr.select(&Int::from_i64(ctx, 0));
            let range = elem.get_sort();
            let dom = Sort::int(ctx);
            Dynamic::from_ast(&z3::ast::Array::new_const(ctx, name, &dom, &range))
        }
        other => panic!("fresh_const_like: unhandled sort kind {other:?} for {name}"),
    }
}

/// Apply a named tactic chain to a set of assertions; return
/// (num formulas, num exprs) summed over residual subgoals.
fn apply_chain<'a>(ctx: &'a Context, assertions: &[Bool<'a>], tactics: &[&str]) -> (u32, u32) {
    let goal = Goal::new(ctx, false, false, false);
    for a in assertions {
        goal.assert(a);
    }
    let mut chain = Tactic::new(ctx, tactics[0]);
    for t in &tactics[1..] {
        chain = chain.and_then(&Tactic::new(ctx, t));
    }
    let res = chain.apply(&goal, None).expect("tactic apply");
    let mut size = 0;
    let mut exprs = 0;
    for sub in res.list_subgoals() {
        size += sub.get_size();
        exprs += sub.get_num_expr();
    }
    (size, exprs)
}

fn measure_mario(max_n: usize) -> MarioResult {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../examples/test_21_mario/main.ev"))
        .expect("load mario");
    let ctx = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums = rt.enums_registry();
    let schemas = rt.schemas_map();
    let empty_given: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::translate::build_cache(
        rt.get_schema("game").expect("game schema"),
        schemas,
        ctx,
        datatypes,
        Some(enums),
        &empty_given,
        2,
    );
    let raw_body: Vec<Bool> = cached.solver.get_assertions();
    let raw_tick = raw_body.len();
    let simp_body = simplify_assertions(ctx, &raw_body).formulas;
    let simp_tick = simp_body.len();

    // Constant table: name → representative Dynamic (with sort).
    let mut touched: HashSet<String> = HashSet::new();
    for a in &simp_body {
        collect_touched_names(a, &mut touched);
    }
    let consts = collect_const_dyns(&simp_body, &touched);

    let mut rows = Vec::new();
    for &n in POWERS {
        if n > max_n {
            break;
        }
        // Build the N-tick unrolled goal.
        let mut unrolled: Vec<Bool> = Vec::with_capacity(simp_tick * n);
        for tick in 0..n {
            // Build this tick's substitution.
            let mut owned: Vec<(Dynamic, Dynamic)> = Vec::with_capacity(consts.len());
            for (name, orig) in &consts {
                let repl = renamed(name, tick, ctx, orig);
                owned.push((orig.clone(), repl));
            }
            let pairs: Vec<(&Dynamic, &Dynamic)> =
                owned.iter().map(|(a, b)| (a, b)).collect();
            for a in &simp_body {
                unrolled.push(a.substitute(&pairs));
            }
            // Pin is_first_tick = false at every tick (steady-state step).
            if let Some(orig) = consts.get("is_first_tick") {
                let repl = renamed("is_first_tick", tick, ctx, orig);
                if let Some(b) = repl.as_bool() {
                    unrolled.push(b.not());
                }
            }
        }

        // Pre-simplify size.
        let goal = Goal::new(ctx, false, false, false);
        for a in &unrolled {
            goal.assert(a);
        }
        let pre_size = goal.get_size();
        let pre_exprs = goal.get_num_expr();

        // Production chain (no elimination) vs log-unroll chain (with solve-eqs).
        let (prop_size, prop_exprs) =
            apply_chain(ctx, &unrolled, &["simplify", "propagate-values"]);
        let (elim_size, elim_exprs) = apply_chain(
            ctx,
            &unrolled,
            &["simplify", "solve-eqs", "propagate-values", "simplify"],
        );

        let elim_ratio = rows
            .last()
            .map(|r: &MarioRow| elim_exprs as f64 / r.elim_exprs.max(1) as f64);

        rows.push(MarioRow {
            n,
            pre_size,
            pre_exprs,
            prop_size,
            prop_exprs,
            elim_size,
            elim_exprs,
            elim_ratio,
        });
    }

    MarioResult {
        raw_tick,
        simp_tick,
        rows,
    }
}

// ───────────────────────── reporting ─────────────────────────

fn fmt_ratio(r: Option<f64>) -> String {
    match r {
        Some(v) => format!("{v:.2}×"),
        None => "—".to_string(),
    }
}

/// Classify a shape by its asymptotic (tail) per-doubling ratio.
fn verdict(tail_ratio: f64) -> &'static str {
    if tail_ratio < 1.15 {
        "**flat** — collapses to closed form; log-unroll is a huge win"
    } else if tail_ratio < 1.6 {
        "**sub-linear** — log-unroll helps"
    } else {
        "**linear** (~2×) — log-unroll buys nothing over naive"
    }
}

fn build_report(synth: &[ShapeResult], mario: &MarioResult) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# Log-unrolling Feasibility Measurement");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "_Generated by `runtime/tests/log_unroll_measurement.rs`. \
         Re-run with `cargo test --release --test log_unroll_measurement \
         -- --nocapture`._"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "## The question");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Composing an FSM step with itself N times naively gives \
         `O(N × body)` constraints. Log-unrolling (exponentiation by \
         squaring, simplifying between each doubling) is only worth \
         building if simplification keeps the composed form small. The \
         decision metric is the **size ratio per doubling** of the \
         simplified form:"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "| ratio/doubling | meaning |");
    let _ = writeln!(s, "|---|---|");
    let _ = writeln!(s, "| ≈ 1.0 | flat in N — log-unroll is a huge win (tractable even at N≈10⁴) |");
    let _ = writeln!(s, "| ≈ 1.5 | sub-linear — log-unroll helps |");
    let _ = writeln!(s, "| ≈ 2.0 | linear in N — log-unroll is asymptotically no better than naive |");
    let _ = writeln!(s);
    let _ = writeln!(s, "## Methodology");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Two measurements, because \"size\" has two faces. Full rationale \
         in the test's module doc."
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "**A. Closed-form composition (shapes 1–5).** A clean state→state \
         vector function `out = f(in)` is composed with itself by Z3 \
         *substitution* — literally what a log-unroller does: build S², \
         simplify (Z3's term simplifier `Z3_simplify`), build S⁴ = S²∘S², \
         simplify, … The assertion count is fixed at the state arity; the \
         metric is the **unique AST-node count** of the composed, \
         simplified transition."
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "**B. Goal-level unroll (shape 6 — real Mario `game`).** A real FSM \
         body has guarded constraints and internal vars, so it is not a \
         clean vector function and cannot be substitution-composed. We \
         clone the simplified per-tick body N times, rename state leaves so \
         tick i's `world_next.*` IS tick i+1's `world.*` (the bridge), pin \
         `is_first_tick = false` (steady state), and apply two Z3 tactic \
         chains: the production `simplify, propagate-values` (no variable \
         elimination) and a log-unroll chain that adds `solve-eqs`."
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "> **The production `z3_eval::simplify_assertions` deliberately \
         excludes `solve-eqs`** (it would destroy the `(= var expr)` shape \
         the per-output extractor needs). So the existing pass alone can \
         never collapse a cross-tick chain — a log-unroller needs its own \
         elimination step. Chain B quantifies what that step achieves."
    );
    let _ = writeln!(s);

    // ── Section A results ──
    let _ = writeln!(s, "## Results — A. closed-form composition (AST node count, post-simplify)");
    let _ = writeln!(s);
    let mut header = String::from("| shape |");
    for p in POWERS {
        let _ = write!(header, " n={p} |");
    }
    let _ = write!(header, " tail ratio | verdict |");
    let _ = writeln!(s, "{header}");
    let mut sep = String::from("|---|");
    for _ in POWERS {
        sep.push_str("---|");
    }
    sep.push_str("---|---|");
    let _ = writeln!(s, "{sep}");
    for sh in synth {
        let mut line = format!("| {} |", sh.name);
        for p in POWERS {
            let cell = sh
                .rows
                .iter()
                .find(|r| r.n == *p)
                .map(|r| r.post_nodes.to_string())
                .unwrap_or_else(|| "—".into());
            let _ = write!(line, " {cell} |");
        }
        let tail = sh.rows.last().and_then(|r| r.ratio).unwrap_or(1.0);
        let _ = write!(line, " {} | {} |", fmt_ratio(Some(tail)), verdict(tail));
        let _ = writeln!(s, "{line}");
    }
    let _ = writeln!(s);
    let _ = writeln!(s, "Per-shape detail (pre = composed before simplify, post = after):");
    let _ = writeln!(s);
    for sh in synth {
        let _ = writeln!(s, "**{}** — `{}` (assertions = {} = state arity, constant)\n",
            sh.name, sh.note, sh.rows[0].assertions);
        let _ = writeln!(s, "| n | pre-nodes | post-nodes | ratio/doubling |");
        let _ = writeln!(s, "|---|---|---|---|");
        for r in &sh.rows {
            let _ = writeln!(
                s,
                "| {} | {} | {} | {} |",
                r.n,
                r.pre_nodes,
                r.post_nodes,
                fmt_ratio(r.ratio)
            );
        }
        let _ = writeln!(s);
    }

    // ── Section B results ──
    let _ = writeln!(s, "## Results — B. real Mario `game` body, goal-level unroll");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "One tick: **{} raw assertions → {} after `simplify` + \
         `propagate-values`** (the production simplify *grows* the count: \
         it splits record/conjunction equalities into per-field pieces).",
        mario.raw_tick, mario.simp_tick
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Unrolling N ticks and applying each chain. `exprs` = \
         `Z3_goal_num_exprs` (formulas + subformulas + terms); the \
         per-doubling ratio is on the **elimination** chain's `exprs`."
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "| N | pre (formulas / exprs) | simplify+propagate (formulas / exprs) \
         | +solve-eqs (formulas / exprs) | elim ratio/doubling |"
    );
    let _ = writeln!(s, "|---|---|---|---|---|");
    for r in &mario.rows {
        let _ = writeln!(
            s,
            "| {} | {} / {} | {} / {} | {} / {} | {} |",
            r.n,
            r.pre_size,
            r.pre_exprs,
            r.prop_size,
            r.prop_exprs,
            r.elim_size,
            r.elim_exprs,
            fmt_ratio(r.elim_ratio)
        );
    }
    let _ = writeln!(s);
    let mario_tail = mario.rows.last().and_then(|r| r.elim_ratio).unwrap_or(2.0);
    let _ = writeln!(s, "Mario tail ratio (elimination chain): **{}** → {}",
        fmt_ratio(Some(mario_tail)), verdict(mario_tail));
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Even with `solve-eqs` eliminating the bridge + internal vars, \
         each Mario tick is a tree of `ite`s over the input state \
         (positions, collisions, branches). Composing two ticks nests \
         tick-i+1's branches *inside* tick-i's — the branch structure \
         multiplies and does not fold, because the conditions are \
         data-dependent on the symbolic state. (Note: under the \
         elimination chain, final-tick outputs defined by clean \
         equalities are substituted away, so absolute counts undercount \
         the relation slightly; the per-doubling *ratio* is unaffected \
         for N ≥ 2.)"
    );
    let _ = writeln!(s);

    // ── Conclusion ──
    let _ = writeln!(s, "## Conclusion");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "**Build log-unroll, but only for affine / linear-recurrence FSM \
         bodies — not for branching game-logic FSMs.**"
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "The measured split is clean:");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "- **Affine / linear systems** (pure counter, linear recurrence, \
         Fibonacci) collapse to a **flat closed form** — node count is \
         constant in N (ratio ≈ 1.0). For these, log-unroll computes S^N \
         in `O(log N)` simplify steps and the result is `O(1)` sized. A \
         counter at N = 10,000 is still `cₙ = c₀ + 10000`. This is the \
         huge win the technique promises."
    );
    let _ = writeln!(
        s,
        "- **Data-dependent branching** (conditional update, 3-state \
         machine, and real Mario) does **not** collapse: each composition \
         nests the next step's `ite` tree inside the previous one, and the \
         branch conditions depend on the symbolic state, so the simplifier \
         cannot fold them. Ratio trends toward ~2× — log-unroll is \
         asymptotically no better than naive unrolling here."
    );
    let _ = writeln!(s);
    let _ = writeln!(s, "### Who benefits");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Log-unroll pays off exactly where the per-tick state update is \
         **affine in the carried state** (next = A·state + b, possibly \
         with per-tick constant inputs). In this repo:"
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "- **`game_clock` / `frame` / `tick` counters** — every FSM that \
         carries `frame ∈ Int = (is_first_tick ? 0 : _frame + 1)` (Mario's \
         `display`/`keyboard`, most `examples/test_*` demos). The frame \
         counter is a pure counter; its N-step value is closed-form. A \
         log-unroller could answer \"what is `frame` after N ticks\" \
         without unrolling."
    );
    let _ = writeln!(
        s,
        "- **Fixed-velocity / accumulator state** — projectile or camera \
         tracks with `pos' = pos + vel` and constant `vel`, score/coin \
         accumulators that add a per-tick constant. These are affine and \
         fold to closed form."
    );
    let _ = writeln!(
        s,
        "- **stdlib counters & deterministic seeds** — anything stepping a \
         linear-congruential-style `x' = a·x + c` (RNG seeds) is a linear \
         recurrence and collapses."
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "Log-unroll does **not** help the bodies that dominate this repo's \
         interesting demos — the platformer/physics FSMs (`test_21_mario` \
         `game`, the dot-physics and collision demos) — because their \
         updates branch on the state (collisions, on-ground tests, \
         clamping, stomp detection). For those, a per-tick functionizer \
         (the existing Cranelift JIT path) or CEGAR (functionize-as-oracle) \
         is the right tool; log-unroll would just reproduce the naive \
         blowup."
    );
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "**Recommendation:** if/when an FSM functionizer is built, gate \
         log-unroll behind an *affine-step detector* — attempt closed-form \
         composition, measure the node-count ratio across the first 2–3 \
         doublings, and fall back to per-tick JIT / CEGAR the moment the \
         ratio exceeds ~1.5. The detector is cheap (a few `Z3_simplify` \
         calls) and the synthetic measurements here show the two regimes \
         separate sharply enough that 2–3 doublings classify a body \
         reliably."
    );
    let _ = writeln!(s);
    s
}

// ───────────────────────── stdout table ─────────────────────────

fn print_synthetic(synth: &[ShapeResult]) {
    println!("\n=== A. closed-form composition — post-simplify AST node count ===");
    print!("{:<22}", "shape");
    for p in POWERS {
        print!("{:>7}", format!("n={p}"));
    }
    println!("{:>9}", "tail");
    for sh in synth {
        print!("{:<22}", sh.name);
        for p in POWERS {
            let cell = sh
                .rows
                .iter()
                .find(|r| r.n == *p)
                .map(|r| r.post_nodes)
                .unwrap_or(0);
            print!("{cell:>7}");
        }
        let tail = sh.rows.last().and_then(|r| r.ratio).unwrap_or(1.0);
        println!("{:>9}", format!("{tail:.2}x"));
    }
}

fn print_mario(mario: &MarioResult) {
    println!(
        "\n=== B. Mario `game` — 1 tick: {} raw → {} simplified ===",
        mario.raw_tick, mario.simp_tick
    );
    println!(
        "{:>4} {:>16} {:>20} {:>18} {:>10}",
        "N", "pre(sz/expr)", "simp+prop(sz/expr)", "+solve-eqs(sz/exp)", "ratio"
    );
    for r in &mario.rows {
        println!(
            "{:>4} {:>16} {:>20} {:>18} {:>10}",
            r.n,
            format!("{}/{}", r.pre_size, r.pre_exprs),
            format!("{}/{}", r.prop_size, r.prop_exprs),
            format!("{}/{}", r.elim_size, r.elim_exprs),
            fmt_ratio(r.elim_ratio),
        );
    }
}

// ───────────────────────── the test ─────────────────────────

#[test]
fn log_unroll_measurement() {
    let t0 = Instant::now();

    // A — synthetic shapes (own context).
    let cfg = z3::Config::new();
    let ctx = Context::new(&cfg);
    let synth = synthetic_shapes(&ctx);
    print_synthetic(&synth);

    // B — Mario (runtime's leaked 'static context). N up to 8: each
    // tick is ~264 assertions, so N=8 is ~2k assertions through
    // solve-eqs — a couple seconds, the realistic ceiling for the
    // measurement to stay inside the normal test budget.
    let mario = measure_mario(8);
    print_mario(&mario);

    println!("\n(measurement wall time: {:?})", t0.elapsed());

    // Write the reproducible markdown report.
    let report = build_report(&synth, &mario);
    let doc = Path::new("../docs/perf/log-unroll-feasibility.md");
    if let Some(parent) = doc.parent() {
        std::fs::create_dir_all(parent).expect("mkdir docs/perf");
    }
    std::fs::write(doc, &report).expect("write report");
    println!("wrote {} ({} bytes)", doc.display(), report.len());

    // ── Sanity assertions (loose — this is measurement, not a unit) ──

    // The counter must be flat: node count at n=64 equals node count at
    // n=1 (cₙ = c₀ + const is always the same shape).
    let counter = synth.iter().find(|s| s.name == "pure counter").unwrap();
    let c1 = counter.rows.iter().find(|r| r.n == 1).unwrap().post_nodes;
    let c64 = counter.rows.iter().find(|r| r.n == 64).unwrap().post_nodes;
    assert_eq!(
        c1, c64,
        "pure counter should be flat under composition (n=1 nodes {c1} vs n=64 {c64})"
    );

    // The branching shapes must grow (tail ratio > counter's).
    let three = synth.iter().find(|s| s.name == "3-state machine").unwrap();
    let three_tail = three.rows.last().and_then(|r| r.ratio).unwrap();
    assert!(
        three_tail > 1.1,
        "3-state machine should grow under composition, tail ratio = {three_tail}"
    );

    // Mario must have produced at least the N=1,2 rows.
    assert!(mario.rows.len() >= 2, "mario probe should measure ≥ 2 powers");
    assert!(mario.simp_tick > 0, "mario body should be non-empty");
}
