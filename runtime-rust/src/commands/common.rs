//! Shared helpers used by multiple `cmd_*` subcommands: usage banner,
//! generic flag parsing, runtime loading, value formatting (text +
//! JSON), and the SAT/UNSAT printer used by both `query` and `sample`.

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, QueryResult, Value};

pub fn usage() {
    eprintln!("usage:");
    eprintln!("  evident query   <files…> <schema> [--given k=v …] [--json]");
    eprintln!("  evident check   <files…>");
    eprintln!("  evident sample  <files…> <schema> [-n N] [--given k=v …] [--json]");
    eprintln!("  evident test    [path]");
    eprintln!("  evident execute <file> [--width N] [--height N] [--title S]");
    eprintln!("                         [--host H] [--port P] [--quiet | --explain]");
    eprintln!("  evident parse   <file>");
    eprintln!();
    eprintln!("execute flags (mirror evident.py where applicable):");
    eprintln!("  --width  N   SDL window width  (default 800; used by SDL plugin)");
    eprintln!("  --height N   SDL window height (default 600; used by SDL plugin)");
    eprintln!("  --title  S   SDL window title  (default \"Evident\")");
    eprintln!("  --host   H   TCP listen host   (default 127.0.0.1; reserved for TCP plugin)");
    eprintln!("  --port   P   TCP listen port   (default 8080;       reserved for TCP plugin)");
    eprintln!("  --quiet              suppress per-step UNSAT warnings (default: warn loud)");
    eprintln!("  --explain            on UNSAT, dump per-step `given` + schema body to stderr");
    eprintln!("  --initial-state PATH  JSON file: top-level keys → first-frame `given`");
    eprintln!();
    eprintln!("not yet implemented (use evident.py):");
    eprintln!("  evident batch|repl …");
}

/// Split positional file paths from flag arguments. Files are everything
/// before the first `--…` flag. Returns `(files, flags)`.
pub fn split_files_and_flags(args: &[String]) -> (Vec<String>, Vec<String>) {
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() && !args[i].starts_with('-') {
        files.push(args[i].clone());
        i += 1;
    }
    (files, args[i..].to_vec())
}

/// Parse `--given k=v k2=v2 …` (consecutive k=v args after `--given`)
/// and `--json`. Unknown flags trigger an error.
pub struct Flags {
    pub given: HashMap<String, Value>,
    pub json: bool,
    pub n_samples: usize,
    /// `--explain`: when a query returns UNSAT, run a per-constraint
    /// retry to identify which body items make the schema unsatisfiable.
    pub explain: bool,
}

impl Default for Flags {
    fn default() -> Self {
        Flags { given: HashMap::new(), json: false, n_samples: 5, explain: false }
    }
}

pub fn parse_flags(flags: &[String]) -> Result<Flags, String> {
    let mut out = Flags::default();
    let mut i = 0;
    while i < flags.len() {
        match flags[i].as_str() {
            "--given" => {
                i += 1;
                while i < flags.len() && !flags[i].starts_with('-') {
                    let pair = &flags[i];
                    let (k, v) = pair.split_once('=')
                        .ok_or_else(|| format!("bad --given {pair:?}: need key=value"))?;
                    out.given.insert(k.to_string(), infer_value(v));
                    i += 1;
                }
            }
            "--json" => { out.json = true; i += 1; }
            "--explain" => { out.explain = true; i += 1; }
            "-n" => {
                i += 1;
                let n = flags.get(i)
                    .ok_or_else(|| "-n needs a number".to_string())?
                    .parse::<usize>()
                    .map_err(|e| format!("bad -n: {e}"))?;
                out.n_samples = n;
                i += 1;
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(out)
}

pub fn infer_value(v: &str) -> Value {
    if v == "true" { Value::Bool(true) }
    else if v == "false" { Value::Bool(false) }
    else if let Ok(n) = v.parse::<i64>() { Value::Int(n) }
    else { Value::Str(v.to_string()) }
}

pub fn load_runtime(files: &[String]) -> Result<EvidentRuntime, String> {
    let mut rt = EvidentRuntime::new();
    for f in files {
        // Use load_file so any `import "..."` statements inside the
        // file resolve relative to the file itself.
        rt.load_file(Path::new(f)).map_err(|e| format!("{f}: {e}"))?;
    }
    Ok(rt)
}

pub fn print_query_result(r: &QueryResult, json: bool) -> ExitCode {
    if json {
        // Minimal JSON: {"satisfied": true/false, "bindings": {…}}
        if !r.satisfied {
            println!("{{\"satisfied\": false}}");
            return ExitCode::from(1);
        }
        let mut keys: Vec<&String> = r.bindings.keys().collect();
        keys.sort();
        let mut parts = Vec::new();
        for k in keys {
            parts.push(format!("\"{}\": {}", k, value_as_json(&r.bindings[k])));
        }
        println!("{{\"satisfied\": true, \"bindings\": {{{}}}}}", parts.join(", "));
        return ExitCode::SUCCESS;
    }
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

pub fn format_value(v: &Value) -> String {
    match v {
        Value::Int(n)  => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s)  => format!("{:?}", s),
        Value::SeqInt(v)  => format!("{:?}", v),
        Value::SeqBool(v) => format!("{:?}", v),
        Value::SeqStr(v)  => format!("{:?}", v),
        // Composite / SeqComposite are placeholder Value variants that
        // aren't currently produced by the translator (sub-schema
        // expansion still emits one leaf per field). Render with Debug
        // until first-class formatting lands.
        other => format!("{:?}", other),
    }
}

pub fn value_as_json(v: &Value) -> String {
    match v {
        Value::Int(n)  => n.to_string(),
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
        // See note in format_value: not yet produced by the translator.
        other => json_str(&format!("{:?}", other)),
    }
}

pub fn json_str(s: &str) -> String {
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
