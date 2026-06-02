//! `evident` CLI.
//!
//!   `evident check <file>`                         — load + parse. Exit 0 if accepted.
//!   `evident query <file> <claim> [--json] [--given k=v …]`
//!                                                  — run `query` on a single schema.
//!   `evident sample <file> <claim> [-n 1] [--json] [--given k=v …]`
//!                                                  — alias for query; -n is ignored (we return one model).
//!   `evident sample <file> --all --json`           — sat-check every loaded schema; emit `{name: bool}`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, Value};

fn usage() {
    eprintln!("Usage:");
    eprintln!("  evident sample <file> <claim> [-n N] [--json] [--given k=v ...]");
    eprintln!("  evident sample <file> --all [--json]");
    eprintln!("  evident emit   <file> <claim> [-o <out.smt2>]");
}

fn load(file: &str) -> Option<EvidentRuntime> {
    let mut rt = EvidentRuntime::new();
    let path = PathBuf::from(file);
    if let Err(e) = rt.load_file(&path) {
        eprintln!("load error: {e:?}");
        return None;
    }
    Some(rt)
}

fn parse_given(args: &[String]) -> HashMap<String, Value> {
    let mut given = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--given" {
            i += 1;
            while i < args.len() && !args[i].starts_with("--") && args[i] != "-n" {
                if let Some(eq) = args[i].find('=') {
                    let k = args[i][..eq].to_string();
                    let v = &args[i][eq + 1..];
                    let val = if let Ok(n) = v.parse::<i64>() {
                        Value::Int(n)
                    } else if v == "true" {
                        Value::Bool(true)
                    } else if v == "false" {
                        Value::Bool(false)
                    } else if let Ok(r) = v.parse::<f64>() {
                        Value::Real(r)
                    } else {
                        Value::Str(v.to_string())
                    };
                    given.insert(k, val);
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    given
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn value_to_json(v: &Value) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Real(r) => r.to_string(),
        Value::Str(s) => format!("{:?}", s),
        Value::SeqInt(items) => format!("[{}]",
            items.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")),
        Value::SeqBool(items) => format!("[{}]",
            items.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ")),
        Value::SeqStr(items) => format!("[{}]",
            items.iter().map(|s| format!("{:?}", s)).collect::<Vec<_>>().join(", ")),
        Value::SeqEnum(items) => format!("[{}]",
            items.iter().map(value_to_json).collect::<Vec<_>>().join(", ")),
        Value::SeqComposite(_) => "[]".to_string(),
        Value::SetInt(items) => format!("[{}]",
            items.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")),
        Value::SetBool(items) => format!("[{}]",
            items.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ")),
        Value::SetStr(items) => format!("[{}]",
            items.iter().map(|s| format!("{:?}", s)).collect::<Vec<_>>().join(", ")),
        Value::Enum { variant, fields, .. } => {
            if fields.is_empty() {
                format!("{:?}", variant)
            } else {
                let parts: Vec<String> = fields.iter().map(value_to_json).collect();
                format!("{{\"{}\":[{}]}}", variant, parts.join(", "))
            }
        }
        Value::Composite(_) => "null".to_string(),
    }
}

fn cmd_query_or_sample(args: &[String]) -> ExitCode {
    let Some(file) = args.first() else { usage(); return ExitCode::from(2); };
    let rest = &args[1..];
    let json = has_flag(rest, "--json");

    // --all: sat-check every schema, emit {name: bool}.
    if has_flag(rest, "--all") {
        let Some(rt) = load(file) else { return ExitCode::from(1); };
        let given = parse_given(rest);
        let names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
        let mut parts = Vec::new();
        for n in &names {
            // Skip generic templates (won't translate on their own).
            if let Some(s) = rt.get_schema(n) {
                if !s.type_params.is_empty() { continue; }
            }
            let sat = rt.query(n, &given).map(|r| r.satisfied).unwrap_or(false);
            parts.push(format!("\"{}\":{}", n, sat));
        }
        if json {
            println!("{{{}}}", parts.join(","));
        } else {
            for p in &parts { println!("{p}"); }
        }
        return ExitCode::SUCCESS;
    }

    // Single-claim: expect a claim name as second positional.
    let claim = rest.iter().find(|a| !a.starts_with("--") && *a != "-n"
                                       && !a.parse::<i64>().is_ok())
                    .cloned()
                    // fall back to the first non-flag
                    .or_else(|| rest.iter().find(|a| !a.starts_with("--")).cloned());
    let Some(claim) = claim else { usage(); return ExitCode::from(2); };
    let Some(rt) = load(file) else { return ExitCode::from(1); };
    let given = parse_given(rest);
    match rt.query(&claim, &given) {
        Ok(r) => {
            if json {
                if r.satisfied {
                    let parts: Vec<String> = r.bindings.iter()
                        .map(|(k, v)| format!("\"{}\":{}", k, value_to_json(v)))
                        .collect();
                    println!("[{{{}}}]", parts.join(","));
                } else {
                    println!("[]");
                }
            } else {
                println!("satisfied: {}", r.satisfied);
                for (k, v) in &r.bindings {
                    println!("  {k} = {v:?}");
                }
            }
            if r.satisfied { ExitCode::SUCCESS } else { ExitCode::from(1) }
        }
        Err(e) => {
            eprintln!("query error: {e:?}");
            if json { println!("[]"); }
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() { usage(); return ExitCode::from(2); }
    match args[0].as_str() {
        "sample" => cmd_query_or_sample(&args[1..]),
        "emit"   => cmd_emit(&args[1..]),
        "help" | "--help" | "-h" => { usage(); ExitCode::SUCCESS }
        other => { eprintln!("unknown subcommand: {other}"); usage(); ExitCode::from(2) }
    }
}

fn cmd_emit(args: &[String]) -> ExitCode {
    let Some(file) = args.first() else { usage(); return ExitCode::from(2); };
    let Some(claim) = args.get(1) else { usage(); return ExitCode::from(2); };
    let mut out_path: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "-o" && i + 1 < args.len() {
            out_path = Some(args[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }
    let Some(rt) = load(file) else { return ExitCode::from(1); };
    match evident_runtime::emit::emit_kernel_smtlib(&rt, claim) {
        Ok(s) => {
            match out_path {
                Some(p) => {
                    if let Err(e) = std::fs::write(&p, &s) {
                        eprintln!("emit: write {p}: {e}");
                        return ExitCode::from(1);
                    }
                }
                None => { print!("{s}"); }
            }
            ExitCode::SUCCESS
        }
        Err(e) => { eprintln!("emit: {e}"); ExitCode::from(1) }
    }
}
