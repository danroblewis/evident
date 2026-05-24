//! `evident profile <files…> <schema> [--given k=v …] [--top N]`
//! — analysis report: which variables the claim is given vs solves
//! for, and a ranked bottleneck analysis of which solved-for variable,
//! if pinned, most reduces the Z3 solve cost.
//!
//! Mirrors `query` / `sample` for file + `--given` parsing (including
//! the auto-applied desugar / inference passes), adding `--top N` for
//! the bottleneck ranking depth.

use std::process::ExitCode;

use evident_runtime::EvidentRuntime;

use super::common::{load_runtime, parse_flags, split_files_and_flags};

pub fn cmd_profile(args: &[String]) -> ExitCode {
    // Pull `--top N` and `--strict` out first; the rest goes through
    // the shared files + `--given` parser (which rejects unknown flags).
    let mut top: usize = 5;
    let mut strict = false;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--top" => match args.get(i + 1).and_then(|s| s.parse::<usize>().ok()) {
                Some(n) => { top = n; i += 2; }
                None => { eprintln!("profile: --top needs a number"); return ExitCode::from(2); }
            },
            "--strict" => { strict = true; i += 1; }
            other => { rest.push(other.to_string()); i += 1; }
        }
    }

    let (files_and_schema, flag_args) = split_files_and_flags(&rest);
    if files_and_schema.len() < 2 {
        eprintln!("profile: need <files…> <schema>");
        return ExitCode::from(2);
    }
    let schema = files_and_schema.last().unwrap().clone();
    let files: Vec<String> = files_and_schema[..files_and_schema.len() - 1].to_vec();

    let flags = match parse_flags(&flag_args) {
        Ok(f) => f,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
    };

    let mut rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
    };
    if !strict {
        super::desugar::auto_apply_desugar(&mut rt, &files);
        super::infer_types::auto_apply_inferences(&mut rt, &files);
    }

    report(&rt, &schema, &flags.given, top)
}

fn report(
    rt: &EvidentRuntime,
    schema: &str,
    given: &std::collections::HashMap<String, evident_runtime::Value>,
    top: usize,
) -> ExitCode {
    let given_vars = match rt.given_vars(schema) {
        Ok(v) => v,
        Err(e) => { eprintln!("profile error: {e}"); return ExitCode::from(1); }
    };
    let given_keys: Vec<String> = given.keys().cloned().collect();
    let solved = match rt.solved_for_vars(schema, &given_keys) {
        Ok(v) => v,
        Err(e) => { eprintln!("profile error: {e}"); return ExitCode::from(1); }
    };

    println!("== Claim {schema:?} ==");
    println!();

    println!("Given (caller-supplied):");
    print_name_list(&given_vars);
    if !given.is_empty() {
        let mut pinned: Vec<&String> = given.keys().collect();
        pinned.sort();
        println!("  (--given pinned: {})", pinned.iter()
            .map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
    }
    println!();

    println!("Solved for ({} unbound):", solved.len());
    print_name_list(&solved);
    println!();

    // Bottleneck ranking — one solve per candidate leaf variable.
    let entries = match rt.bottleneck_vars(schema, given, top) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("profile: bottleneck analysis unavailable: {e}");
            return ExitCode::from(1);
        }
    };

    if entries.is_empty() {
        println!("Bottleneck analysis: no scalar candidate variables to pin.");
        println!("(The claim's solved-for variables are all composite/sequence/enum-");
        println!(" valued, which this tool doesn't pin in v1, or none survived the");
        println!(" solved-for filter.)");
        return ExitCode::SUCCESS;
    }

    println!("Bottleneck analysis (top {}):", entries.len());
    print_bottleneck_table(&entries);
    ExitCode::SUCCESS
}

/// Print a bullet list of variable names, two-space indented, wrapping
/// at ~76 columns. `(none)` when empty.
fn print_name_list(names: &[String]) {
    if names.is_empty() {
        println!("  (none)");
        return;
    }
    let mut line = String::from("  ");
    for (i, n) in names.iter().enumerate() {
        let sep = if i == 0 { "" } else { ", " };
        if line.len() + sep.len() + n.len() > 76 && line.trim().len() > 0 {
            println!("{line}");
            line = String::from("  ");
            line.push_str(n);
        } else {
            line.push_str(sep);
            line.push_str(n);
        }
    }
    if !line.trim().is_empty() {
        println!("{line}");
    }
}

fn print_bottleneck_table(entries: &[evident_runtime::BottleneckEntry]) {
    // Column widths derived from the data.
    let var_w = entries.iter().map(|e| e.var_name.len())
        .max().unwrap_or(8).max("variable".len());
    let base_w = col_w(entries.iter().map(|e| fmt_u(e.baseline_solve_us)), "baseline(μs)");
    let pin_w  = col_w(entries.iter().map(|e| fmt_u(e.pinned_solve_us)),   "pinned(μs)");
    let sav_w  = col_w(entries.iter().map(|e| fmt_i(e.savings_us)),        "savings(μs)");
    let conf_w = col_w(entries.iter().map(|e| fmt_delta(e, "conflicts")),  "Δconflicts");
    let dec_w  = col_w(entries.iter().map(|e| fmt_delta(e, "decisions")),  "Δdecisions");

    println!("  {:>4}  {:<vw$}  {:>bw$}  {:>pw$}  {:>sw$}  {:>cw$}  {:>dw$}",
        "rank", "variable", "baseline(μs)", "pinned(μs)", "savings(μs)",
        "Δconflicts", "Δdecisions",
        vw = var_w, bw = base_w, pw = pin_w, sw = sav_w, cw = conf_w, dw = dec_w);
    for (i, e) in entries.iter().enumerate() {
        println!("  {:>4}  {:<vw$}  {:>bw$}  {:>pw$}  {:>sw$}  {:>cw$}  {:>dw$}",
            i + 1, e.var_name,
            fmt_u(e.baseline_solve_us), fmt_u(e.pinned_solve_us), fmt_i(e.savings_us),
            fmt_delta(e, "conflicts"), fmt_delta(e, "decisions"),
            vw = var_w, bw = base_w, pw = pin_w, sw = sav_w, cw = conf_w, dw = dec_w);
    }
}

fn col_w(vals: impl Iterator<Item = String>, header: &str) -> usize {
    vals.map(|s| s.chars().count()).max().unwrap_or(0).max(header.chars().count())
}

fn fmt_delta(e: &evident_runtime::BottleneckEntry, key: &str) -> String {
    match e.z3_stats_delta.get(key) {
        Some(d) => fmt_i(*d as i128),
        None => "·".to_string(),
    }
}

/// Group digits into underscore-separated thousands: `4823` → `4_823`.
fn fmt_u(n: u128) -> String { group_digits(&n.to_string()) }

fn fmt_i(n: i128) -> String {
    if n < 0 {
        format!("-{}", group_digits(&n.unsigned_abs().to_string()))
    } else {
        group_digits(&n.to_string())
    }
}

fn group_digits(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    let n = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (n - i) % 3 == 0 { out.push('_'); }
        out.push(*b as char);
    }
    out
}
