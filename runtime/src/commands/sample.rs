//! `evident sample <files…> <schema> [-n N] [--given …] [--json]`
//! — generate up to N distinct models via a blocking-clause loop.

use std::collections::HashMap;
use std::process::ExitCode;

use evident_runtime::Value;

use super::common::{format_value, setup_query_or_sample};

pub fn cmd_sample(args: &[String]) -> ExitCode {
    let setup = match setup_query_or_sample("sample", args) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let flags = setup.flags;
    let rt = setup.rt;

    // Real blocking-clause sample loop: solver.push(), assert givens,
    // loop check + extract + assert ¬(scalar bindings), pop. Returns
    // up to `-n N` distinct models or stops at UNSAT. See
    // `EvidentRuntime::sample` for limitations (Seq/Set bindings don't
    // contribute to the blocking conjunction).
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

/// JSON serializer for `Value`. Pub so `query::print_query_result` can
/// reuse it for its `--json` output; private helper `json_str` stays
/// local.
pub fn value_as_json(v: &Value) -> String {
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
        // Composite / SeqComposite are placeholder Value variants that
        // aren't currently produced by the translator. Render with the
        // Debug form until first-class formatting lands.
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
