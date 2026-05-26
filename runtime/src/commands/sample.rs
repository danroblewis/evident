//! `evident sample` — generate distinct models via blocking-clause loop, or
//! `--all` to sat-check every schema (ignores `-n`/`--given`).

use std::collections::HashMap;
use std::process::ExitCode;

use evident_runtime::ast::BodyItem;
use evident_runtime::{EvidentRuntime, Value};

use super::common::{format_value, load_runtime, setup_query_or_sample, split_files_and_flags};

pub fn cmd_sample(args: &[String]) -> ExitCode {
    if args.iter().any(|a| a == "--all") {
        return cmd_sample_all(args);
    }
    let setup = match setup_query_or_sample("sample", args) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let flags = setup.flags;
    let rt = setup.rt;

    let samples: Vec<HashMap<String, Value>> = match rt.sample(&setup.schema, &flags.given, flags.n_samples) {
        Ok(s) => s,
        Err(e) => { eprintln!("sample error: {e}"); return ExitCode::from(1); }
    };

    if flags.json {
        print!("[");
        for (i, s) in samples.iter().enumerate() {
            if i > 0 { print!(", "); }
            let mut keys: Vec<&String> = s.keys().collect(); keys.sort();
            let parts: Vec<_> = keys.iter()
                .map(|k| format!("\"{}\": {}", k, value_as_json(&s[*k])))
                .collect();
            print!("{{{}}}", parts.join(", "));
        }
        println!("]");
    } else {
        for (i, s) in samples.iter().enumerate() {
            println!("--- sample {} ---", i + 1);
            let mut keys: Vec<&String> = s.keys().collect(); keys.sort();
            for k in keys {
                println!("{k}={}", format_value(&s[k]));
            }
        }
    }
    ExitCode::SUCCESS
}

/// Sat-check every loaded schema. `--json` → `{"schema": bool}` object (skipped schemas omitted).
/// Exit 1 on load/usage error only; UNSAT is data, not a failure.
fn cmd_sample_all(args: &[String]) -> ExitCode {
    let stripped: Vec<String> = args.iter()
        .filter(|a| a.as_str() != "--all")
        .cloned().collect();
    let (files, flag_args) = split_files_and_flags(&stripped);
    let json = flag_args.iter().any(|a| a == "--json");
    if files.is_empty() {
        eprintln!("sample --all: need at least one file");
        return ExitCode::from(2);
    }
    let mut rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
    };
    super::common::auto_apply_desugar(&mut rt, &files);

    let mut names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
    names.sort();
    let empty = HashMap::new();

    let mut results: Vec<(String, bool)> = Vec::new();
    for name in &names {
        if has_generic_seq_param(&rt, name) {
            if !json { println!("SKIP   {name}  (generic Seq param — library helper)"); }
            continue;
        }
        if is_generic_template(&rt, name) {
            if !json { println!("SKIP   {name}  (generic template — monomorphic copies queried separately)"); }
            continue;
        }
        let satisfied = matches!(rt.query(name, &empty), Ok(r) if r.satisfied);
        if !json {
            println!("{}  {name}", if satisfied { "SAT  " } else { "UNSAT" });
        }
        results.push((name.clone(), satisfied));
    }

    if json {
        let parts: Vec<String> = results.iter()
            .map(|(n, sat)| format!("{}: {}", json_str(n), sat))
            .collect();
        println!("{{{}}}", parts.join(", "));
    }
    ExitCode::SUCCESS
}

/// `s ∈ Seq` (bare, no element type) is only valid at a names-match call site;
/// standalone evaluation drops constraints. Skip — it's a library helper, not a test.
fn has_generic_seq_param(rt: &EvidentRuntime, name: &str) -> bool {
    let Some(decl) = rt.get_schema(name) else { return false };
    decl.body.iter().any(|item| matches!(item,
        BodyItem::Membership { type_name, .. } if type_name == "Seq"))
}

/// Generic declarations are templates; monomorphic copies are evaluated instead.
fn is_generic_template(rt: &EvidentRuntime, name: &str) -> bool {
    let Some(decl) = rt.get_schema(name) else { return false };
    !decl.type_params.is_empty()
}

/// JSON serializer for `Value`.
fn value_as_json(v: &Value) -> String {
    match v {
        Value::Int(n)  => n.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s)  => json_str(s),
        Value::SeqInt(v) => {
            let parts: Vec<_> = v.iter().map(|n| n.to_string()).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::SeqBool(v) => {
            let parts: Vec<_> = v.iter().map(|b| b.to_string()).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::SeqStr(v) => {
            let parts: Vec<_> = v.iter().map(|s| json_str(s)).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Enum { variant, fields, .. } => {
            if fields.is_empty() {
                json_str(variant)
            } else {
                let parts: Vec<String> = fields.iter().map(value_as_json).collect();
                format!("{{\"variant\": {}, \"fields\": [{}]}}",
                        json_str(variant), parts.join(", "))
            }
        }
        // Composite/SeqComposite not yet produced by translator; Debug until first-class formatting.
        other => json_str(&format!("{:?}", other)),
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c    => out.push(c),
        }
    }
    out.push('"');
    out
}
