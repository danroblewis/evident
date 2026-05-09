//! `evident sample <files…> <schema> [-n N] [--given …] [--json]`
//! — generate up to N distinct models via a blocking-clause loop.

use std::collections::HashMap;
use std::process::ExitCode;

use evident_runtime::Value;

use super::common::{
    format_value, load_runtime, parse_flags, split_files_and_flags, value_as_json,
};

pub fn cmd_sample(args: &[String]) -> ExitCode {
    let strict = args.iter().any(|a| a == "--strict");
    let stripped: Vec<String> = args.iter()
        .filter(|a| a.as_str() != "--strict")
        .cloned().collect();
    let (files_and_schema, flag_args) = split_files_and_flags(&stripped);
    if files_and_schema.len() < 2 {
        eprintln!("sample: need <files…> <schema>");
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

    // Real blocking-clause sample loop: solver.push(), assert givens,
    // loop check + extract + assert ¬(scalar bindings), pop. Returns
    // up to `-n N` distinct models or stops at UNSAT. See
    // `EvidentRuntime::sample` for limitations (Seq/Set bindings don't
    // contribute to the blocking conjunction).
    let samples: Vec<HashMap<String, Value>> = match rt.sample(&schema, &flags.given, flags.n_samples) {
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
