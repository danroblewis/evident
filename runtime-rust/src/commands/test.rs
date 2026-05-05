//! `evident test [path]` — discover and run `claim sat_*` /
//! `claim unsat_*` claims in `test_*.ev` files. Exits 1 on any failure.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use evident_runtime::EvidentRuntime;

pub fn cmd_test(args: &[String]) -> ExitCode {
    let path: PathBuf = match args.first().map(String::as_str) {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from("."),
    };
    let mut files = Vec::new();
    if path.is_file() {
        files.push(path.clone());
    } else if path.is_dir() {
        collect_test_files(&path, &mut files);
    } else {
        eprintln!("test: not a file or directory: {}", path.display());
        return ExitCode::from(2);
    }
    if files.is_empty() {
        eprintln!("test: no test_*.ev files found under {}", path.display());
        return ExitCode::from(0);
    }

    let mut total_pass = 0usize;
    let mut total_fail = 0usize;
    let mut total_skip = 0usize;
    let empty = HashMap::new();
    for f in &files {
        let mut rt = EvidentRuntime::new();
        if let Err(e) = rt.load_file(f) {
            eprintln!("{}: load error: {e}", f.display());
            total_fail += 1;
            continue;
        }
        let mut names: Vec<String> = rt.schema_names()
            .filter(|n| n.starts_with("sat_") || n.starts_with("unsat_"))
            .map(|s| s.to_string()).collect();
        names.sort();
        if names.is_empty() {
            total_skip += 1;
            continue;
        }
        println!("{}:", f.display());
        for name in &names {
            let want_sat = name.starts_with("sat_");
            match rt.query(name, &empty) {
                Ok(r) if r.satisfied == want_sat => {
                    println!("  PASS  {}", name);
                    total_pass += 1;
                }
                Ok(r) => {
                    println!("  FAIL  {}  (expected {} got {})",
                        name,
                        if want_sat { "SAT" } else { "UNSAT" },
                        if r.satisfied { "SAT" } else { "UNSAT" });
                    total_fail += 1;
                }
                Err(e) => {
                    println!("  ERROR {}  ({e})", name);
                    total_fail += 1;
                }
            }
        }
    }
    println!();
    println!("{} passed, {} failed, {} files skipped (no sat_/unsat_ claims)",
             total_pass, total_fail, total_skip);
    if total_fail > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

fn collect_test_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_test_files(&p, out);
        } else if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
            if name.starts_with("test_") && name.ends_with(".ev") {
                out.push(p);
            }
        }
    }
}
