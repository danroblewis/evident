//! `evident profile <files…> <schema> [--given k=v …] [--top N] [--json]`
//! `evident profile <files…> --all [--top N] [--json]`
//! — analysis report: which variables the claim is given vs solves
//! for, and a ranked bottleneck analysis of which solved-for variable,
//! if pinned, most reduces the Z3 solve cost.
//!
//! Three output shapes:
//!   * default (text)      — the human-readable single-claim report.
//!   * `--json`            — the same single-claim analysis as JSON.
//!   * `--all [--json]`    — batch mode: profile every queryable schema
//!                           *defined in* each passed file (imports
//!                           excluded) and emit one combined JSON
//!                           document. This is what `scripts/profile-all.sh`
//!                           consumes to build `docs/perf/bottlenecks.md`.
//!
//! Mirrors `query` / `sample` for file + `--given` parsing (including
//! the auto-applied desugar / inference passes), adding `--top N` for
//! the bottleneck ranking depth.

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use evident_runtime::ast::BodyItem;
use evident_runtime::{EvidentRuntime, Value};
use serde_json::json;

use super::common::{load_runtime, parse_flags, split_files_and_flags};

pub fn cmd_profile(args: &[String]) -> ExitCode {
    // Pull `--top N`, `--strict`, and `--all` out first; the rest goes
    // through the shared files + `--given` / `--json` parser (which
    // rejects unknown flags).
    let mut top: usize = 5;
    let mut strict = false;
    let mut all = false;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--top" => match args.get(i + 1).and_then(|s| s.parse::<usize>().ok()) {
                Some(n) => { top = n; i += 2; }
                None => { eprintln!("profile: --top needs a number"); return ExitCode::from(2); }
            },
            "--strict" => { strict = true; i += 1; }
            "--all" => { all = true; i += 1; }
            other => { rest.push(other.to_string()); i += 1; }
        }
    }

    let (positional, flag_args) = split_files_and_flags(&rest);
    let flags = match parse_flags(&flag_args) {
        Ok(f) => f,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
    };

    // ── Batch mode: every queryable schema defined in each file ──
    if all {
        if positional.is_empty() {
            eprintln!("profile --all: need <files…>");
            return ExitCode::from(2);
        }
        let mut rt = match load_runtime(&positional) {
            Ok(r) => r,
            Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
        };
        if !strict {
            super::desugar::auto_apply_desugar(&mut rt, &positional);
            super::infer_types::auto_apply_inferences(&mut rt, &positional);
        }
        return report_all(&rt, &positional, &flags.given, top);
    }

    // ── Single-schema mode (default text, or `--json`) ──
    if positional.len() < 2 {
        eprintln!("profile: need <files…> <schema>  (or <files…> --all)");
        return ExitCode::from(2);
    }
    let schema = positional.last().unwrap().clone();
    let files: Vec<String> = positional[..positional.len() - 1].to_vec();

    let mut rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
    };
    if !strict {
        super::desugar::auto_apply_desugar(&mut rt, &files);
        super::infer_types::auto_apply_inferences(&mut rt, &files);
    }

    if flags.json {
        let obj = profile_one(&rt, &schema, &flags.given, top);
        println!("{}", serde_json::to_string_pretty(&obj).unwrap());
        return ExitCode::SUCCESS;
    }

    report(&rt, &schema, &flags.given, top)
}

/// Batch driver: for each passed file, profile every schema *defined in
/// that file* (imports excluded) that is queryable — skipping the
/// `sat_*` / `unsat_*` test claims, generic templates, and generic-Seq
/// library helpers (the same shapes `evident check` skips). Emits one
/// combined JSON document on stdout; progress goes to stderr so the
/// stdout stream stays pure JSON for the driver to parse.
fn report_all(
    rt: &EvidentRuntime,
    files: &[String],
    given: &HashMap<String, Value>,
    top: usize,
) -> ExitCode {
    let mut files_json: Vec<serde_json::Value> = Vec::new();
    for file in files {
        let path = Path::new(file);
        let mut claims: Vec<serde_json::Value> = Vec::new();
        for idx in rt.user_claim_indices_in_file(path) {
            let Some(name) = rt.user_claim_name(idx) else { continue };
            if let Some(reason) = skip_reason(rt, &name) {
                eprintln!("  skip    {file} :: {name}  ({reason})");
                continue;
            }
            eprintln!("  profile {file} :: {name}");
            claims.push(profile_one(rt, &name, given, top));
        }
        files_json.push(json!({ "file": file, "claims": claims }));
    }
    let out = json!({ "files": files_json });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
    ExitCode::SUCCESS
}

/// Why a schema should be skipped by batch profiling, or `None` if it's
/// a legitimate profiling target. Mirrors `commands/check.rs`'s skip
/// policy (test claims + generic templates + generic-Seq helpers).
fn skip_reason(rt: &EvidentRuntime, name: &str) -> Option<&'static str> {
    if name.starts_with("sat_") || name.starts_with("unsat_") {
        return Some("test claim");
    }
    let decl = rt.get_schema(name)?;
    if !decl.type_params.is_empty() {
        return Some("generic template — monomorphic copies queried separately");
    }
    if decl.body.iter().any(|item|
        matches!(item, BodyItem::Membership { type_name, .. } if type_name == "Seq"))
    {
        return Some("generic Seq param — library helper");
    }
    None
}

/// Build the per-claim JSON object: given / solved-for var lists, the
/// cold full-solve wall-clock (`query_us` — the claim's total baseline
/// solve cost), and the ranked bottleneck table. `status` is one of
/// `ok` / `no_candidates` / `unsat` / `error`.
fn profile_one(
    rt: &EvidentRuntime,
    schema: &str,
    given: &HashMap<String, Value>,
    top: usize,
) -> serde_json::Value {
    let given_keys: Vec<String> = given.keys().cloned().collect();
    let given_vars = rt.given_vars(schema).unwrap_or_default();
    let solved = rt.solved_for_vars(schema, &given_keys).unwrap_or_default();

    // Total baseline: one cold full solve (translate + build + check +
    // extract). This is the claim's end-to-end solve cost and is
    // available for every claim, even ones with no scalar candidates.
    let t0 = Instant::now();
    let qres = rt.query(schema, given);
    let query_us = t0.elapsed().as_micros();

    let base = json!({
        "schema": schema,
        "given": given_vars,
        "solved_for": solved,
    });
    let mut merge = |status: &str, message: serde_json::Value,
                     query_us: serde_json::Value,
                     bottlenecks: Vec<serde_json::Value>| -> serde_json::Value {
        let mut o = base.clone();
        let m = o.as_object_mut().unwrap();
        m.insert("status".into(), json!(status));
        m.insert("message".into(), message);
        m.insert("query_us".into(), query_us);
        m.insert("bottlenecks".into(), json!(bottlenecks));
        o
    };

    match qres {
        Err(e) => return merge("error", json!(e.to_string()),
                               serde_json::Value::Null, vec![]),
        Ok(r) if !r.satisfied =>
            return merge("unsat", serde_json::Value::Null,
                         json!(query_us as u64), vec![]),
        Ok(_) => {}
    }

    match rt.bottleneck_vars(schema, given, top) {
        Ok(entries) if entries.is_empty() =>
            merge("no_candidates", serde_json::Value::Null,
                  json!(query_us as u64), vec![]),
        Ok(entries) => {
            let rows: Vec<serde_json::Value> = entries.iter().enumerate()
                .map(|(rank, e)| json!({
                    "rank": rank + 1,
                    "var": e.var_name,
                    "baseline_us": e.baseline_solve_us as u64,
                    "pinned_us": e.pinned_solve_us as u64,
                    "savings_us": e.savings_us as i64,
                    "conflicts_delta": e.z3_stats_delta.get("conflicts").copied(),
                    "decisions_delta": e.z3_stats_delta.get("decisions").copied(),
                }))
                .collect();
            merge("ok", serde_json::Value::Null, json!(query_us as u64), rows)
        }
        // SAT baseline but the bottleneck pass couldn't run (rare —
        // e.g. a lenient-only cache that re-solves UNSAT). Keep the
        // claim's query_us; record the reason.
        Err(e) => merge("ok", json!(e.to_string()), json!(query_us as u64), vec![]),
    }
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
