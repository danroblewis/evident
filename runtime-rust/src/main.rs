//! evident-runtime CLI. Mirrors the `evident.py query` shape:
//!
//!   evident-runtime query <file.ev> <schema_name> [--given key=value …]
//!
//! Prints the model as KEY=VALUE lines for SAT, "UNSAT" otherwise.

use std::collections::HashMap;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, QueryResult, Value};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
        return ExitCode::from(2);
    }
    match args[0].as_str() {
        "query" => cmd_query(&args[1..]),
        "parse" => cmd_parse(&args[1..]),
        "help" | "--help" | "-h" => { usage(); ExitCode::SUCCESS }
        other => {
            eprintln!("unknown subcommand: {}", other);
            usage();
            ExitCode::from(2)
        }
    }
}

fn usage() {
    eprintln!("usage:");
    eprintln!("  evident-runtime query <file.ev> <schema_name> [--given key=value …]");
    eprintln!("  evident-runtime parse <file.ev>");
}

fn cmd_query(args: &[String]) -> ExitCode {
    if args.len() < 2 {
        eprintln!("query: need <file.ev> <schema_name>");
        return ExitCode::from(2);
    }
    let path = &args[0];
    let name = &args[1];
    let given = match parse_given(&args[2..]) {
        Ok(g) => g,
        Err(e) => { eprintln!("{}", e); return ExitCode::from(2); }
    };

    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => { eprintln!("read {}: {}", path, e); return ExitCode::from(1); }
    };

    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_source(&src) {
        eprintln!("parse error: {}", e);
        return ExitCode::from(1);
    }

    match rt.query(name, &given) {
        Ok(r) => print_result(&r),
        Err(e) => { eprintln!("query error: {}", e); ExitCode::from(1) }
    }
}

fn cmd_parse(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("parse: need <file.ev>");
        return ExitCode::from(2);
    }
    let path = &args[0];
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => { eprintln!("read {}: {}", path, e); return ExitCode::from(1); }
    };
    // Re-export parser through the lib; for now go via load_source
    // and dump the schema names as a quick sanity check.
    let mut rt = EvidentRuntime::new();
    match rt.load_source(&src) {
        Ok(()) => {
            // List all loaded schema names.
            for s in rt.schema_names() { println!("{}", s); }
            ExitCode::SUCCESS
        }
        Err(e) => { eprintln!("parse error: {}", e); ExitCode::from(1) }
    }
}

/// Parse `--given key=value` pairs. Value type is inferred:
///   "true"/"false" → Bool
///   parses as i64  → Int
///   else           → String
fn parse_given(args: &[String]) -> Result<HashMap<String, Value>, String> {
    let mut out = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] != "--given" {
            return Err(format!("unexpected arg: {}", args[i]));
        }
        let pair = args.get(i + 1)
            .ok_or_else(|| "--given needs key=value".to_string())?;
        let (k, v) = pair.split_once('=')
            .ok_or_else(|| format!("bad --given {:?}: need key=value", pair))?;
        let value = if v == "true" {
            Value::Bool(true)
        } else if v == "false" {
            Value::Bool(false)
        } else if let Ok(n) = v.parse::<i64>() {
            Value::Int(n)
        } else {
            Value::Str(v.to_string())
        };
        out.insert(k.to_string(), value);
        i += 2;
    }
    Ok(out)
}

fn print_result(r: &QueryResult) -> ExitCode {
    if !r.satisfied {
        println!("UNSAT");
        return ExitCode::from(1);
    }
    let mut keys: Vec<&String> = r.bindings.keys().collect();
    keys.sort();
    for k in keys {
        println!("{}={}", k, format_value(&r.bindings[k]));
    }
    ExitCode::SUCCESS
}

fn format_value(v: &Value) -> String {
    match v {
        Value::Int(n)  => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s)  => format!("{:?}", s),
        Value::SeqInt(v)  => format!("{:?}", v),
        Value::SeqBool(v) => format!("{:?}", v),
        Value::SeqStr(v)  => format!("{:?}", v),
    }
}
