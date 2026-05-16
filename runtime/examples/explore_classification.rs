//! Walk the codebase and classify each component of each claim by
//! the 2-copy uniqueness check. Reports the fraction of components
//! that are function-shaped — the population the function-izer
//! could target.
//!
//! Run:  cargo run --release --example explore_classification

use evident_runtime::EvidentRuntime;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn collect_ev_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() { collect_ev_files(&p, out); }
        else if p.extension().map(|e| e == "ev").unwrap_or(false) { out.push(p); }
    }
}

fn is_generic_template(name: &str) -> bool {
    let Some(open) = name.find('<') else { return false };
    let Some(close) = name.rfind('>') else { return false };
    let inside = &name[open + 1..close];
    inside.split(',').map(|s| s.trim()).all(|s|
        s.len() == 1 && s.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
}

fn main() {
    std::env::set_var("EVIDENT_LENIENT", "1");

    let workspace_root = std::env::var("EVIDENT_ROOT")
        .unwrap_or_else(|_| "..".to_string());
    let mut files = Vec::new();
    for sub in ["examples", "stdlib"] {
        let p = Path::new(&workspace_root).join(sub);
        if p.exists() { collect_ev_files(&p, &mut files); }
    }
    files.sort();

    let mut total_claims = 0;
    let mut total_components = 0;
    let mut functional_components = 0;
    let mut singleton_components = 0;
    let mut multi_var_functional = 0;
    let mut multi_var_total = 0;
    let mut top: Vec<(String, usize, usize, usize)> = Vec::new();
    let mut failed = 0;

    for path in &files {
        let mut rt = EvidentRuntime::new();
        let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.load_file(path)
        }));
        if !matches!(loaded, Ok(Ok(()))) { continue; }

        let names: Vec<String> = rt.schema_names()
            .filter(|n| !is_generic_template(n))
            .filter(|n| !n.starts_with("sat_") && !n.starts_with("unsat_"))
            .map(|s| s.to_string()).collect();

        for name in names {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                rt.classify_components(&name, &HashMap::new())
            }));
            let comps = match r {
                Ok(Ok(c)) => c,
                _ => { failed += 1; continue; }
            };
            total_claims += 1;
            let n_comp = comps.len();
            let n_func = comps.iter().filter(|c| c.functional).count();
            let n_singleton = comps.iter().filter(|c| c.component.vars.len() == 1).count();
            let n_multi_func = comps.iter()
                .filter(|c| c.functional && c.component.vars.len() > 1).count();
            let n_multi = comps.iter()
                .filter(|c| c.component.vars.len() > 1).count();

            total_components += n_comp;
            functional_components += n_func;
            singleton_components += n_singleton;
            multi_var_functional += n_multi_func;
            multi_var_total += n_multi;

            if n_multi > 0 {
                top.push((name, n_comp, n_func, n_multi));
            }
        }
    }

    println!("\nClassification across {} loaded files\n", files.len());
    println!("  total claims analyzed:        {}", total_claims);
    println!("  total components:             {}", total_components);
    println!("    of which singletons:        {}  ({:.0}%)",
        singleton_components,
        100.0 * singleton_components as f64 / total_components.max(1) as f64);
    println!("    of which multi-var:         {}",
        multi_var_total);
    println!();
    println!("  components classified functional: {}  ({:.0}% of total)",
        functional_components,
        100.0 * functional_components as f64 / total_components.max(1) as f64);
    println!("    of those, multi-var:        {}  ({:.0}% of multi-var)",
        multi_var_functional,
        100.0 * multi_var_functional as f64 / multi_var_total.max(1) as f64);
    println!();
    if failed > 0 {
        println!("  ({} claims failed analysis)", failed);
    }

    // Show claims with the most multi-var components.
    top.sort_by(|a, b| b.3.cmp(&a.3));
    println!("\nTop claims with multi-var components (compile candidates):");
    println!("  {:>3}  {:>3}  {:>3}  {}", "ncomp", "func", "mvar", "claim");
    println!("  {}", "─".repeat(74));
    for (name, n_comp, n_func, n_mvar) in top.iter().take(20) {
        println!("  {:>3}  {:>3}  {:>3}  {}", n_comp, n_func, n_mvar, name);
    }
    println!();
}
