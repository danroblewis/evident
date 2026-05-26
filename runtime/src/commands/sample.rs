//! `evident sample <files…> <schema> [-n N] [--given …] [--json]`
//! — generate up to N distinct models via a blocking-clause loop.
//!
//! `evident sample <files…> --all [--json]` — batch sat-check every
//! schema in the loaded file(s) (subsumes the former `evident check`):
//! "got ≥1 model" → SAT, "UNSAT / no model" → UNSAT. `--all` ignores
//! `-n` / `--given` (it's a sat decision, not a model enumerator).

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

/// `evident sample <files…> --all [--json]` — sat-check every loaded
/// schema. This subsumes the former `evident check`: it reuses the
/// exact all-schemas iteration (including the generic-Seq-param and
/// generic-template skips) so the schema SET is identical.
///
/// Output:
///   `--json` → a single JSON object `{"<schema>": <bool>, …}` that
///              `conftest.check()` parses into `{schema: satisfied}`.
///              Skipped (generic) schemas are omitted — they're neither
///              SAT nor UNSAT.
///   text     → one readable `SAT  / UNSAT / SKIP <schema>` line each.
///
/// Exit code: 1 on a load/usage error, 0 otherwise. (It's a report of
/// sat-ness, not a pass/fail gate — an UNSAT schema is data, not an
/// error.)
fn cmd_sample_all(args: &[String]) -> ExitCode {
    // Strip the mode/output flags; everything before the first `-…`
    // is a file path. `--all` ignores `-n` / `--given`.
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

    // Collect (name, satisfied) for the JSON object; print text inline.
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

/// Generic-Seq parameters (`s ∈ Seq` with no element type) only have a
/// meaningful element sort at the call site via names-match. Standalone
/// evaluation would emit "unknown type Seq for s" and drop downstream
/// constraints. Detect and skip — the claim is a library helper, not a
/// top-level test. (Copied from the former `check` so `--all`'s schema
/// set is identical.)
fn has_generic_seq_param(rt: &EvidentRuntime, name: &str) -> bool {
    let Some(decl) = rt.get_schema(name) else { return false };
    decl.body.iter().any(|item| matches!(item,
        BodyItem::Membership { type_name, .. } if type_name == "Seq"))
}

/// Generic declarations (`type Edge<T>`, `claim Toposort<T>`) are
/// templates — their bodies contain type variables that resolve only at
/// monomorphization. Skip — the monomorphic copies get evaluated
/// instead.
fn is_generic_template(rt: &EvidentRuntime, name: &str) -> bool {
    let Some(decl) = rt.get_schema(name) else { return false };
    !decl.type_params.is_empty()
}

/// JSON serializer for `Value`, used by `sample`'s `--json` output.
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
