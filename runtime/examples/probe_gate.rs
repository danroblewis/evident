//! Survey which claims in the codebase pass the function-izer gate.
//! Helps prioritize what to expand next.

use evident_runtime::functionize::is_pure_assignment_body;
use evident_runtime::EvidentRuntime;
use std::fs;
use std::path::{Path, PathBuf};

fn collect_ev(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() { collect_ev(&p, out); }
        else if p.extension().map(|e| e == "ev").unwrap_or(false) { out.push(p); }
    }
}

fn is_generic_template(name: &str) -> bool {
    let Some(open) = name.find('<') else { return false };
    let Some(close) = name.rfind('>') else { return false };
    let inside = &name[open + 1..close];
    inside.split(',').all(|s| s.trim().len() == 1
        && s.trim().chars().next().is_some_and(|c| c.is_ascii_uppercase()))
}

fn main() {
    std::env::set_var("EVIDENT_LENIENT", "1");
    let root = std::env::var("EVIDENT_ROOT").unwrap_or_else(|_| "..".to_string());
    let mut files = Vec::new();
    for sub in ["examples", "stdlib"] {
        let p = Path::new(&root).join(sub);
        if p.exists() { collect_ev(&p, &mut files); }
    }
    files.sort();

    let mut total = 0;
    let mut passed = 0;
    let mut passed_names: Vec<String> = Vec::new();

    for file in &files {
        let mut rt = EvidentRuntime::new();
        let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.load_file(file)
        }));
        if !matches!(loaded, Ok(Ok(()))) { continue; }
        let names: Vec<String> = rt.schema_names()
            .filter(|n| !is_generic_template(n))
            .filter(|n| !n.starts_with("sat_") && !n.starts_with("unsat_"))
            .map(|s| s.to_string()).collect();
        for n in names {
            let Some(sch) = rt.get_schema(&n) else { continue };
            total += 1;
            if is_pure_assignment_body(sch) {
                passed += 1;
                passed_names.push(n);
            }
        }
    }

    println!("Function-izer gate coverage: {}/{} claims pass ({:.0}%)",
        passed, total, 100.0 * passed as f64 / total.max(1) as f64);
    println!("\nClaims passing the gate:");
    for name in &passed_names {
        println!("  {}", name);
    }
}
