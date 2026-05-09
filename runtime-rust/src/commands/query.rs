//! `evident query <files…> <schema> [--given …] [--json] [--explain]`
//! — single SAT/UNSAT decision against the named schema.

use std::collections::HashMap;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, Value};

use super::common::{
    format_value, load_runtime, parse_flags, print_query_result, split_files_and_flags,
};

pub fn cmd_query(args: &[String]) -> ExitCode {
    // Strip --infer-types before the standard flag parser sees it.
    let infer_types = args.iter().any(|a| a == "--infer-types");
    let stripped: Vec<String> = args.iter()
        .filter(|a| a.as_str() != "--infer-types").cloned().collect();
    let (files_and_schema, flag_args) = split_files_and_flags(&stripped);
    if files_and_schema.len() < 2 {
        eprintln!("query: need <files…> <schema>");
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

    // --infer-types: run the self-hosted inference pipeline (in a
    // separate runtime, so the inference passes don't pollute this
    // query's schema list), then graft each unambiguous inference
    // into this runtime's claim bodies as a Membership. From here
    // on the query proceeds normally — Z3 sees the user's body
    // augmented with the inferred declarations.
    if infer_types {
        match super::infer_types::collect_inferences(&files) {
            Ok(all) => {
                let unambiguous = super::infer_types::unambiguous_inferences(&all);
                let mut applied = 0;
                for inf in &unambiguous {
                    match rt.add_membership_to_claim(
                        &inf.claim_name, &inf.var, &inf.type_name,
                    ) {
                        Ok(true)  => { applied += 1; }
                        Ok(false) => { /* already declared, skip */ }
                        Err(e)    => eprintln!("warning: couldn't add Membership: {e}"),
                    }
                }
                if applied > 0 {
                    eprintln!("--infer-types: added {applied} inferred Membership(s)");
                }
            }
            Err(e) => {
                eprintln!("warning: --infer-types pipeline failed: {e}");
                eprintln!("(continuing without inferences)");
            }
        }
    }

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
