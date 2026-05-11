//! `evident query <files…> <schema> [--given …] [--json] [--explain]`
//! — single SAT/UNSAT decision against the named schema.

use std::collections::HashMap;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, QueryResult, Value};

use super::common::{format_value, setup_query_or_sample};
use super::sample::value_as_json;

pub fn cmd_query(args: &[String]) -> ExitCode {
    let setup = match setup_query_or_sample("query", args) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let r = match setup.rt.query(&setup.schema, &setup.flags.given) {
        Ok(r) => r,
        Err(e) => { eprintln!("query error: {e}"); return ExitCode::from(1); }
    };
    if !r.satisfied && setup.flags.explain {
        explain_unsat(&setup.rt, &setup.schema, &setup.flags.given);
    }
    print_query_result(&r, setup.flags.json)
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

/// Print a `QueryResult` in either text (`k=v` lines) or JSON form.
/// Returns the exit code: 0 for SAT, 1 for UNSAT.
fn print_query_result(r: &QueryResult, json: bool) -> ExitCode {
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
