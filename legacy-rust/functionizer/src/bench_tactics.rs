//! Tactic-chain benchmark — in-process. Loads Mario once, runs the
//! FSM scheduler N ticks per trial under each `EVIDENT_TACTICS` setting,
//! reports wall time. Picks the chain that wins on our actual workload.
//!
//! Notes:
//!   - In-process so process-startup noise is excluded. We are measuring
//!     solve+dispatch cost.
//!   - Mario opens an SDL window, so to keep this comparable we run with
//!     EVIDENT_HEADLESS=1 if the runtime supports it; otherwise we'll
//!     just compare relative times.
//!   - 3 trials per chain; report median + best.

use evident_runtime::EvidentRuntime;
use evident_runtime::effect_dispatch::DispatchContext;
use evident_runtime::effect_loop::{run_with_ctx, LoopOpts};
use std::path::Path;
use std::time::Instant;

const CHAINS: &[(&str, &str)] = &[
    ("baseline (off)",      "off"),
    ("simplify",            "simplify"),
    ("propagate-values",    "propagate-values"),
    ("solve-eqs",           "solve-eqs"),
    ("der",                 "der"),
    ("standard",            "standard"),
    ("aggressive",          "aggressive"),
    ("simp+solve-eqs",      "simplify,solve-eqs"),
    ("simp+ctx-simplify",   "simplify,ctx-solver-simplify"),
    ("simp+elim-uncnstr",   "simplify,elim-uncnstr"),
    ("simp+der",            "simplify,der"),
    ("full+elim-pred",      "simplify,propagate-values,solve-eqs,elim-uncnstr,elim-predicates"),
    ("simp+prop+der",       "simplify,propagate-values,der"),
];

/// Returns (ms, completed_steps). `completed_steps` lets the caller
/// detect tactics that produce wrong UNSAT (which short-circuits the
/// FSM at step 0 — a fake "fast" result).
fn run_trial(path: &Path, ticks: usize) -> (f64, usize) {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_file(path).unwrap();

    let t0 = Instant::now();
    let opts = LoopOpts { max_steps: ticks };
    let mut ctx = DispatchContext::new();
    let result = run_with_ctx(&rt, &opts, &mut ctx);
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let steps = result.map(|r| r.steps).unwrap_or(0);
    (ms, steps)
}

fn main() {
    let ticks: usize = std::env::args().nth(1)
        .and_then(|s| s.parse().ok()).unwrap_or(20);
    let path_arg = std::env::args().nth(2)
        .unwrap_or_else(|| "../examples/test_22_prev_record.ev".to_string());
    let path = Path::new(&path_arg);
    println!("\nTactic-chain bench: {} ticks, file={}\n",
        ticks, path.display());
    println!("  {:<28} {:>10}  {:>10}  {:>11}",
        "chain", "median ms", "best ms", "vs baseline");
    println!("  {}", "─".repeat(72));

    // Run baseline first to establish expected step count + time.
    std::env::set_var("EVIDENT_TACTICS", "off");
    std::env::set_var("EVIDENT_LENIENT", "1");
    let (_warm, _) = run_trial(path, ticks);
    let baseline_runs: Vec<(f64, usize)> = (0..3).map(|_| run_trial(path, ticks)).collect();
    let baseline_steps = baseline_runs[0].1;
    let mut baseline_times: Vec<f64> = baseline_runs.iter().map(|(t, _)| *t).collect();
    baseline_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let baseline_median = baseline_times[1];
    println!("  {:<28} {:>8.1}ms  {:>8.1}ms  {:>11}  (expected {} steps)",
        "baseline (off)", baseline_median, baseline_times[0], "—", baseline_steps);

    for (label, spec) in CHAINS {
        if *spec == "off" { continue; }   // already ran
        std::env::set_var("EVIDENT_TACTICS", spec);
        let _ = run_trial(path, ticks);   // warm-up
        let runs: Vec<(f64, usize)> = (0..3).map(|_| run_trial(path, ticks)).collect();
        let mut times: Vec<f64> = runs.iter().map(|(t, _)| *t).collect();
        times.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = times[1];
        let best = times[0];
        let steps = runs[0].1;
        let correct = steps == baseline_steps;
        let mark = if correct {
            format!("{:.2}×", baseline_median / median)
        } else {
            format!("BROKEN ({}/{})", steps, baseline_steps)
        };
        println!("  {:<28} {:>8.1}ms  {:>8.1}ms  {:>11}",
            label, median, best, mark);
    }
    println!();
}
