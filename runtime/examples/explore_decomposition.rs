//! Walk every loadable `.ev` file under `examples/`, `stdlib/`, and
//! `packages/`, run `analyze_decomposition` on each top-level claim,
//! report how many independent sub-models each claim decomposes into.
//!
//! The point: we expect Evident programs to be composed of mostly-separate
//! pieces; the runtime should recover those pieces structurally. This
//! tool measures whether that intuition matches reality across our
//! actual codebase.
//!
//! Run:  cargo run --release --example explore_decomposition

use evident_runtime::EvidentRuntime;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn collect_ev_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_ev_files(&p, out);
        } else if p.extension().map(|e| e == "ev").unwrap_or(false) {
            out.push(p);
        }
    }
}

#[derive(Default)]
struct Summary {
    total_claims: usize,
    no_free_vars: usize,                 // claims with no free vars after given
    components_histogram: Vec<usize>,    // index = component count, value = how many claims
    biggest_component_sizes: Vec<usize>, // for the few largest claims
    top_decomposable: Vec<(String, usize, usize)>, // (claim, components, total vars)
}

fn main() {
    let workspace_root = std::env::var("EVIDENT_ROOT")
        .unwrap_or_else(|_| "..".to_string());
    let mut files = Vec::new();
    for sub in ["examples", "stdlib", "packages"] {
        let p = Path::new(&workspace_root).join(sub);
        if p.exists() {
            collect_ev_files(&p, &mut files);
        }
    }
    files.sort();

    println!("Decomposition exploration — {} .ev files\n", files.len());

    let mut summary = Summary::default();
    let mut failed_loads: Vec<(PathBuf, String)> = Vec::new();
    let mut failed_analyses: Vec<(String, String)> = Vec::new();

    // For each file: load it (in a fresh runtime), analyze each schema.
    for path in &files {
        let mut rt = EvidentRuntime::new();
        let load = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.load_file(path)
        }));
        match load {
            Ok(Ok(())) => {}
            Ok(Err(e)) => { failed_loads.push((path.clone(), format!("{e:?}"))); continue; }
            Err(_)     => { failed_loads.push((path.clone(), "panic".into())); continue; }
        }

        let schema_names: Vec<String> = rt.schema_names()
            // Skip generic templates (have angle brackets but no monomorphization).
            // The monomorphized copies appear as separate entries.
            .filter(|n| !is_generic_template(n))
            // Skip sat_* / unsat_* convention tests — they're FSM-style with
            // explicit pins, not the structural target of decomposition.
            .filter(|n| !n.starts_with("sat_") && !n.starts_with("unsat_"))
            .map(|s| s.to_string()).collect();

        for name in schema_names {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                rt.analyze_decomposition(&name, &HashMap::new())
            }));
            let comps = match r {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => { failed_analyses.push((name, format!("{e:?}"))); continue; }
                Err(_)     => { failed_analyses.push((name, "panic".into())); continue; }
            };
            summary.total_claims += 1;
            let total_vars: usize = comps.iter().map(|c| c.vars.len()).sum();
            if total_vars == 0 {
                summary.no_free_vars += 1;
                continue;
            }
            let n_comp = comps.len();
            while summary.components_histogram.len() <= n_comp {
                summary.components_histogram.push(0);
            }
            summary.components_histogram[n_comp] += 1;
            if let Some(biggest) = comps.iter().map(|c| c.vars.len()).max() {
                summary.biggest_component_sizes.push(biggest);
            }
            // Keep a sample of the top decomposable claims.
            if n_comp >= 3 {
                summary.top_decomposable.push((name.clone(), n_comp, total_vars));
            }
        }
    }

    // ── Report ──
    println!("Loaded {} files, analyzed {} claims",
        files.len() - failed_loads.len(), summary.total_claims);
    if !failed_loads.is_empty() {
        println!("  ({} files failed to load; {} analyses errored)",
            failed_loads.len(), failed_analyses.len());
    }
    println!();

    println!("Components per claim (histogram):");
    println!("  {:>3}  {:>5}  {}", "#comp", "count", "");
    println!("  {}", "─".repeat(30));
    println!("  {:>3}  {:>5}  {}", "0 vars", summary.no_free_vars,
        "(no free vars — fully pinned by body)");
    for (n, count) in summary.components_histogram.iter().enumerate() {
        if *count == 0 { continue; }
        let bar = "█".repeat((*count).min(40));
        println!("  {:>3}  {:>5}  {}", n, count, bar);
    }
    println!();

    if !summary.biggest_component_sizes.is_empty() {
        let mut sizes = summary.biggest_component_sizes.clone();
        sizes.sort_unstable();
        let p50 = sizes[sizes.len() / 2];
        let p90 = sizes[(sizes.len() * 9) / 10];
        let max = *sizes.last().unwrap();
        println!("Biggest component (per claim): p50={p50} p90={p90} max={max}");
    }

    summary.top_decomposable.sort_by(|a, b| b.1.cmp(&a.1));
    if !summary.top_decomposable.is_empty() {
        println!("\nTop 20 most-decomposable claims:");
        println!("  {:>3}  {:>5}  {}", "#comp", "vars", "claim");
        println!("  {}", "─".repeat(70));
        for (name, n, v) in summary.top_decomposable.iter().take(20) {
            println!("  {:>3}  {:>5}  {}", n, v, name);
        }
    }

    // Re-run on a curated set of "interesting" claims and print
    // component-size breakdowns. This is the view that actually shows
    // which programs have non-trivial separable structure.
    let interesting: &[(&str, &str)] = &[
        ("examples/test_21_mario/main.ev",         "display"),
        ("examples/test_21_mario/main.ev",         "level_gen"),
        ("examples/test_21_mario/main.ev",         "game"),
        ("examples/test_21_mario/main.ev",         "keyboard"),
        ("stdlib/passes/literal_types.ev",         "infer_int_from_single_assignment"),
        ("stdlib/passes/propagation.ev",           "propagate_int"),
        ("stdlib/toposort.ev",                     "Toposort<Int>"),
        ("stdlib/combinatorics.ev",                "Permutation<Int>"),
    ];
    println!("\nPer-claim component-size breakdown (curated):");
    for (rel_path, claim_name) in interesting {
        let path = Path::new(&workspace_root).join(rel_path);
        if !path.exists() { continue; }
        let mut rt = EvidentRuntime::new();
        if rt.load_file(&path).is_err() { continue; }
        let Ok(comps) = rt.analyze_decomposition(claim_name, &HashMap::new()) else { continue };
        let mut sizes: Vec<usize> = comps.iter().map(|c| c.vars.len()).collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        let singletons = sizes.iter().filter(|&&s| s == 1).count();
        let multi: Vec<usize> = sizes.iter().filter(|&&s| s > 1).copied().collect();
        println!("\n  {} :: {}", rel_path, claim_name);
        println!("    {} total components, {} singletons", sizes.len(), singletons);
        if !multi.is_empty() {
            print!("    multi-var components (top 10 sizes): ");
            for s in multi.iter().take(10) { print!("{s} "); }
            println!();
            // Show the biggest component's variable names.
            if let Some(biggest) = comps.iter().max_by_key(|c| c.vars.len()) {
                if biggest.vars.len() > 1 {
                    let preview: Vec<&String> = biggest.vars.iter().take(8).collect();
                    println!("    biggest component ({} vars): {:?}{}",
                        biggest.vars.len(), preview,
                        if biggest.vars.len() > 8 { ", ..." } else { "" });
                }
            }
        } else {
            println!("    fully separable — no multi-variable components");
        }
    }

    if !failed_loads.is_empty() && std::env::var("VERBOSE").is_ok() {
        println!("\n── Load failures ──");
        for (p, e) in &failed_loads {
            println!("  {}: {}", p.display(), e);
        }
    }
    if !failed_analyses.is_empty() && std::env::var("VERBOSE").is_ok() {
        println!("\n── Analysis failures ──");
        for (n, e) in &failed_analyses {
            println!("  {}: {}", n, e);
        }
    }
    println!();
}

/// Heuristic: a name like `Foo<T>` (with non-monomorphized type params)
/// is a generic template; `Foo<Int>` is a concrete instantiation.
fn is_generic_template(name: &str) -> bool {
    let Some(open) = name.find('<') else { return false; };
    let Some(close) = name.rfind('>') else { return false; };
    let inside = &name[open + 1..close];
    inside.split(',').map(|s| s.trim()).all(|s| {
        // Single-uppercase-letter type vars are templates; concrete types
        // like Int, String, Bool, or user-record names are not.
        s.len() == 1 && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
    })
}
