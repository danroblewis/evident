//! Z3 profiling: aggregated statistics across solves + UNSAT
//! core extraction + axiom-profiler trace emission.
//!
//! Three orthogonal capabilities, activated independently:
//!
//! 1. **Statistics aggregation** (`--profile-z3` / env
//!    `EVIDENT_PROFILE_Z3=1`). After each `Solver::check()`
//!    call in the runtime, capture the solver's per-key
//!    statistics (conflicts, decisions, propagations, etc.)
//!    and accumulate globally. Print a summary at end-of-run.
//!
//! 2. **UNSAT core** (`--profile-z3-unsat-cores`). Track
//!    individual assertions; on UNSAT, ask Z3 which subset
//!    actually caused the conflict.
//!
//! 3. **Axiom-profiler trace** (`--profile-z3-trace FILE`).
//!    Z3 emits a per-quantifier-instantiation log readable by
//!    Z3's `axiom_profiler` Python tool. Visualizes which
//!    quantifiers fire most often.
//!
//! All three are off by default. The runtime checks the env
//! vars `EVIDENT_PROFILE_Z3*` at the relevant call sites and
//! routes through this module.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::Duration;
use z3::Solver;

thread_local! {
    static GLOBAL_STATS: RefCell<Z3ProfileStats> = RefCell::new(Z3ProfileStats::default());
}

#[derive(Default, Clone, Debug)]
pub struct Z3ProfileStats {
    /// Per-key Z3 statistic, accumulated across every
    /// `record_check_stats` call. Z3's statistic dictionary
    /// is a flat map of `String → i64 | f64`. We treat all
    /// values as f64 for accumulation.
    pub keys: BTreeMap<String, f64>,
    /// Total number of `check()` calls observed.
    pub checks: u64,
    /// Total wall time across observed check() calls.
    pub total_check_time: Duration,
    /// Per-claim breakdown — when the caller provides a
    /// `claim_name`, we accumulate into a sub-map for
    /// finer-grained analysis.
    pub per_claim: BTreeMap<String, PerClaimZ3>,
}

#[derive(Default, Clone, Debug)]
pub struct PerClaimZ3 {
    pub checks: u64,
    pub total_check_time: Duration,
    pub keys: BTreeMap<String, f64>,
}

/// Enable Z3's axiom-profiler trace logging to `file`. Must be
/// called BEFORE any `Context` is created — Z3 reads these
/// params at context construction. Subsequent solves emit
/// quantifier-instantiation events to the trace file. Use Z3's
/// `axiom_profiler` Python tool to visualize.
pub fn enable_trace(file: &str) {
    use std::ffi::CString;
    use z3_sys::Z3_global_param_set;
    let trace = CString::new("trace").unwrap();
    let trueval = CString::new("true").unwrap();
    let trace_file = CString::new("trace_file_name").unwrap();
    let file_c = CString::new(file).unwrap();
    let proof = CString::new("proof").unwrap();
    unsafe {
        Z3_global_param_set(trace.as_ptr(),       trueval.as_ptr());
        Z3_global_param_set(trace_file.as_ptr(),  file_c.as_ptr());
        // Quantifier-instantiation events are tagged "smt"
        // internally; the `proof` global has to be true for
        // some events to fire.
        Z3_global_param_set(proof.as_ptr(),       trueval.as_ptr());
    }
    eprintln!("[profile-z3] axiom-profiler trace enabled, writing to {file}");
}

/// Record stats from a `Solver` after a `check()` call.
/// `claim_name` is optional; when provided, the stats also
/// accumulate into a per-claim sub-aggregate.
pub fn record_check_stats<'ctx>(
    solver: &Solver<'ctx>,
    claim_name: Option<&str>,
    duration: Duration,
) {
    if std::env::var("EVIDENT_PROFILE_Z3").map(|s| s == "1").unwrap_or(false) {
        let stats = solver.get_statistics();
        GLOBAL_STATS.with(|g| {
            let mut g = g.borrow_mut();
            g.checks += 1;
            g.total_check_time += duration;
            for entry in stats.entries() {
                let v = stat_value_f64(&entry.value);
                *g.keys.entry(entry.key.to_string()).or_default() += v;
            }
            if let Some(name) = claim_name {
                let per = g.per_claim.entry(name.to_string()).or_default();
                per.checks += 1;
                per.total_check_time += duration;
                for entry in stats.entries() {
                    let v = stat_value_f64(&entry.value);
                    *per.keys.entry(entry.key.to_string()).or_default() += v;
                }
            }
        });
    }
}

fn stat_value_f64(v: &z3::StatisticsValue) -> f64 {
    use z3::StatisticsValue;
    match v {
        StatisticsValue::UInt(n)   => *n as f64,
        StatisticsValue::Double(d) => *d,
    }
}

/// Print the accumulated Z3 statistics summary to stderr. The
/// most actionable keys are listed first (with explanations);
/// the rest follow in alphabetical order.
pub fn print_summary() {
    GLOBAL_STATS.with(|g| {
        let g = g.borrow();
        eprintln!("[profile-z3] ── summary ─────────────────────────────");
        eprintln!("[profile-z3] check() calls: {}", g.checks);
        eprintln!("[profile-z3] total time:    {:>7.2}ms ({:>5.1}ms/check)",
            g.total_check_time.as_secs_f64() * 1000.0,
            if g.checks > 0 {
                g.total_check_time.as_secs_f64() * 1000.0 / g.checks as f64
            } else { 0.0 });
        eprintln!();
        eprintln!("[profile-z3] aggregate solver statistics (sum across checks):");
        // Promote the most actionable keys to the top.
        let priority = ["conflicts", "decisions", "propagations",
                        "restarts", "memory", "max memory",
                        "binary propagations", "literals deleted",
                        "del.clauses", "added eqs", "datatype splits"];
        let mut printed: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for key in &priority {
            if let Some(v) = g.keys.get(*key) {
                eprintln!("[profile-z3]   {:<24} {}", key, format_stat(*v));
                printed.insert(*key);
            }
        }
        // Then everything else.
        for (k, v) in &g.keys {
            if printed.contains(k.as_str()) { continue; }
            eprintln!("[profile-z3]   {:<24} {}", k, format_stat(*v));
        }
        if !g.per_claim.is_empty() {
            eprintln!();
            eprintln!("[profile-z3] per-claim breakdown:");
            for (claim, per) in &g.per_claim {
                eprintln!("[profile-z3]   {} ({} checks, {:.1}ms total)",
                    claim, per.checks, per.total_check_time.as_secs_f64() * 1000.0);
                for key in &priority {
                    if let Some(v) = per.keys.get(*key) {
                        eprintln!("[profile-z3]     {:<22} {}", key, format_stat(*v));
                    }
                }
            }
        }
        eprintln!();
        eprintln!("[profile-z3] key interpretation:");
        eprintln!("[profile-z3]   conflicts    — clauses that contradicted; high count = solver");
        eprintln!("[profile-z3]                  thrashing on a hard constraint.");
        eprintln!("[profile-z3]   decisions    — branching choices; proportional to search depth.");
        eprintln!("[profile-z3]   propagations — boolean implications derived; high count = many");
        eprintln!("[profile-z3]                  forced moves, usually cheap.");
        eprintln!("[profile-z3]   restarts     — solver gave up and started over; high count =");
        eprintln!("[profile-z3]                  bad heuristic match for the formula.");
    });
}

fn format_stat(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e18 {
        format!("{:>14}", v as i64)
    } else {
        format!("{:>14.2}", v)
    }
}

/// Extract the UNSAT core from a solver. Each Bool in the
/// returned vec is one of the original assertions that
/// contributed to the conflict. Z3 returns these by their
/// tracking variable; we format them for human inspection.
pub fn extract_unsat_core<'ctx>(solver: &Solver<'ctx>) -> Vec<String> {
    let core = solver.get_unsat_core();
    core.iter().map(|b| format!("{b}")).collect()
}

/// Reset accumulated stats. Useful for fine-grained tests.
pub fn reset() {
    GLOBAL_STATS.with(|g| *g.borrow_mut() = Z3ProfileStats::default());
}

/// Snapshot the current stats (for tests / programmatic use).
pub fn snapshot() -> Z3ProfileStats {
    GLOBAL_STATS.with(|g| g.borrow().clone())
}
