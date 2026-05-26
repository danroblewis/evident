//! Z3 profiling: statistics aggregation (`EVIDENT_PROFILE_Z3=1`), UNSAT core extraction,
//! and axiom-profiler trace emission (`--profile-z3-trace FILE`). All off by default.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::time::Duration;
use z3::Solver;

thread_local! {
    static GLOBAL_STATS: RefCell<Z3ProfileStats> = RefCell::new(Z3ProfileStats::default());
}

#[derive(Default, Clone, Debug)]
pub struct Z3ProfileStats {
    /// Per-key Z3 statistics accumulated across all `record_check_stats` calls (f64 for sums).
    pub keys: BTreeMap<String, f64>,
    pub checks: u64,
    pub total_check_time: Duration,
    pub per_claim: BTreeMap<String, PerClaimZ3>,
}

#[derive(Default, Clone, Debug)]
pub struct PerClaimZ3 {
    pub checks: u64,
    pub total_check_time: Duration,
    pub keys: BTreeMap<String, f64>,
}

/// Enable axiom-profiler trace to `file`. MUST be called before any `Context` is created.
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
        // `proof` must be true for some QI events to fire.
        Z3_global_param_set(proof.as_ptr(),       trueval.as_ptr());
    }
    eprintln!("[profile-z3] axiom-profiler trace enabled, writing to {file}");
}

/// Record solver stats after a `check()` call, optionally keyed by claim name.
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

/// Print accumulated Z3 statistics to stderr (priority keys first, then alphabetical).
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

/// Extract the UNSAT core as formatted strings of the conflicting assertions.
pub fn extract_unsat_core<'ctx>(solver: &Solver<'ctx>) -> Vec<String> {
    let core = solver.get_unsat_core();
    core.iter().map(|b| format!("{b}")).collect()
}

/// Reset accumulated stats.
pub fn reset() {
    GLOBAL_STATS.with(|g| *g.borrow_mut() = Z3ProfileStats::default());
}

/// Snapshot the current stats.
pub fn snapshot() -> Z3ProfileStats {
    GLOBAL_STATS.with(|g| g.borrow().clone())
}
