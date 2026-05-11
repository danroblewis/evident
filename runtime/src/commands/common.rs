//! Shared helpers used by multiple `cmd_*` subcommands: usage banner,
//! generic flag parsing, runtime loading, and shared value formatting.
//!
//! Single-use helpers (e.g. JSON formatting, the SAT/UNSAT printer)
//! live in their owning command file, not here.

use std::collections::HashMap;
use std::path::Path;

use evident_runtime::{EvidentRuntime, Value};

pub fn usage() {
    eprintln!("usage:");
    eprintln!("  evident query       <files…> <schema> [--given k=v …] [--json]");
    eprintln!("  evident check       <files…>");
    eprintln!("  evident sample      <files…> <schema> [-n N] [--given k=v …] [--json]");
    eprintln!("  evident test        [path] [-v] [--no-color]");
    eprintln!("  evident effect-run  <file>           # run an effect-driven program");
    eprintln!("  evident lint        <file>");
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

pub fn format_value(v: &Value) -> String {
    match v {
        Value::Int(n)  => n.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s)  => format!("{:?}", s),
        Value::SeqInt(v)  => format!("{:?}", v),
        Value::SeqBool(v) => format!("{:?}", v),
        Value::SeqStr(v)  => format!("{:?}", v),
        Value::Enum { variant, fields, .. } => {
            if fields.is_empty() {
                variant.clone()
            } else {
                let parts: Vec<String> = fields.iter().map(format_value).collect();
                format!("{}({})", variant, parts.join(", "))
            }
        }
        // Composite / SeqComposite are placeholder Value variants that
        // aren't currently produced by the translator (sub-schema
        // expansion still emits one leaf per field). Render with Debug
        // until first-class formatting lands.
        other => format!("{:?}", other),
    }
}
