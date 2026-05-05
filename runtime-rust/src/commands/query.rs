//! `evident query <files…> <schema> [--given …] [--json] [--explain]`
//! — single SAT/UNSAT decision against the named schema.

use std::collections::HashMap;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, Value};

use super::common::{
    format_value, load_runtime, parse_flags, print_query_result, split_files_and_flags,
};

pub fn cmd_query(args: &[String]) -> ExitCode {
    let (files_and_schema, flag_args) = split_files_and_flags(args);
    if files_and_schema.len() < 2 {
        eprintln!("query: need <files…> <schema>");
        return ExitCode::from(2);
    }
    // Last positional is the schema name; the rest are files.
    let schema = files_and_schema.last().unwrap().clone();
    let files: Vec<String> = files_and_schema[..files_and_schema.len() - 1].to_vec();
    let flags = match parse_flags(&flag_args) {
        Ok(f) => f,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
    };
    let rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
    };
    let r = match rt.query(&schema, &flags.given) {
        Ok(r) => r,
        Err(e) => { eprintln!("query error: {e}"); return ExitCode::from(1); }
    };
    if !r.satisfied && flags.explain {
        explain_unsat(&rt, &schema, &flags.given);
    }
    print_query_result(&r, flags.json)
}

/// On UNSAT, dump the schema body items + given values to stderr so
/// the user can see what was asserted and start narrowing the conflict.
///
/// Future work: track each translated assertion with a Z3 unsat-core
/// name (`assert_and_track`) so we can print the actual minimal
/// conflicting subset. Today, the body dump is the minimum useful
/// step — combined with the per-item dropped-constraint warnings
/// (which pinpoint translation failures), it's enough to find most
/// conflicts.
fn explain_unsat(rt: &EvidentRuntime, schema_name: &str, given: &HashMap<String, Value>) {
    let Some(schema) = rt.get_schema(schema_name) else { return };
    eprintln!();
    eprintln!("--- explain UNSAT for schema {schema_name} ---");
    if !given.is_empty() {
        let mut keys: Vec<&String> = given.keys().collect();
        keys.sort();
        eprintln!("given values:");
        for k in keys { eprintln!("  {k} = {}", format_value(&given[k])); }
    }
    eprintln!("schema body has {} items:", schema.body.len());
    for (i, item) in schema.body.iter().enumerate() {
        eprintln!("  [{i}] {}", evident_runtime::pretty::body_item(item));
    }
    eprintln!("--- end explain ---");
    eprintln!("(hint: comment out body items to narrow the conflict; or");
    eprintln!(" check that no `--given` value contradicts a body equality.)");
}
