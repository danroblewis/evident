//! Survey which claims in the codebase pass the function-izer gate.
//! Helps prioritize what to expand next.

use evident_runtime::functionize::{is_pure_assignment_body, is_pure_assignment_body_full};
use evident_runtime::EvidentRuntime;
use std::fs;
use std::path::{Path, PathBuf};

fn collect_ev(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root) else { return };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() { collect_ev(&p, out); }
        else if p.extension().map(|e| e == "ev").unwrap_or(false) { out.push(p); }
    }
}

fn is_generic_template(name: &str) -> bool {
    let Some(open) = name.find('<') else { return false };
    let Some(close) = name.rfind('>') else { return false };
    let inside = &name[open + 1..close];
    inside.split(',').all(|s| s.trim().len() == 1
        && s.trim().chars().next().is_some_and(|c| c.is_ascii_uppercase()))
}

fn main() {
    std::env::set_var("EVIDENT_LENIENT", "1");
    let root = std::env::var("EVIDENT_ROOT").unwrap_or_else(|_| "..".to_string());
    let mut files = Vec::new();
    for sub in ["examples", "stdlib"] {
        let p = Path::new(&root).join(sub);
        if p.exists() { collect_ev(&p, &mut files); }
    }
    files.sort();

    let mut total = 0;
    let mut bare = 0;       // gate with no predicates (primitives only)
    let mut with_full = 0;  // gate WITH enum + simple-record predicates

    for file in &files {
        let mut rt = EvidentRuntime::new();
        let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rt.load_file(file)
        }));
        if !matches!(loaded, Ok(Ok(()))) { continue; }

        // Build the same predicates rt.query uses.
        let enums_set: std::collections::HashSet<String> =
            rt.schema_names().filter(|n| {
                rt.get_schema(n).map(|s|
                    matches!(s.keyword, evident_runtime::ast::Keyword::Schema)
                ).unwrap_or(false)
            }).map(|s| s.to_string()).collect();
        let _ = enums_set; // (kept for future expansion)

        // Collect all simple-record type names + enum names from this runtime.
        let mut enum_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut record_names: std::collections::HashSet<String> = std::collections::HashSet::new();
        for n in rt.schema_names() {
            let Some(s) = rt.get_schema(n) else { continue };
            if matches!(s.keyword, evident_runtime::ast::Keyword::Type) {
                // Check if all members are primitives.
                let mut all_prim = true;
                for item in &s.body {
                    if let evident_runtime::ast::BodyItem::Membership { type_name, .. } = item {
                        if !matches!(type_name.as_str(), "Int"|"Real"|"Bool"|"String") {
                            all_prim = false; break;
                        }
                    }
                }
                if all_prim { record_names.insert(n.to_string()); }
            }
        }
        // Enums come from rt.enums (which is private). Approximate via
        // schemas that are not type/claim/fsm/schema (none — there's
        // no Keyword::Enum). We have no public API to enumerate enums.
        // For this probe, fall back to checking via type-name string.
        let _ = enum_names;

        let names: Vec<String> = rt.schema_names()
            .filter(|n| !is_generic_template(n))
            .filter(|n| !n.starts_with("sat_") && !n.starts_with("unsat_"))
            .map(|s| s.to_string()).collect();

        let is_record = |t: &str| -> bool { record_names.contains(t) };
        // Without access to enum registry from here, assume any
        // type_name that's not a known record AND not primitive AND
        // exists as a schema_name might be an enum. Conservative.
        let is_enum = |t: &str| -> bool {
            rt.get_schema(t).is_some() && !record_names.contains(t) &&
                !matches!(t, "Int"|"Real"|"Bool"|"String")
        };

        for n in names {
            let Some(sch) = rt.get_schema(&n) else { continue };
            total += 1;
            if is_pure_assignment_body(sch) { bare += 1; }
            if is_pure_assignment_body_full(sch, &is_enum, &is_record) { with_full += 1; }
        }
    }

    println!("Function-izer gate coverage:");
    println!("  bare (primitives only):  {}/{} ({:.0}%)",
        bare, total, 100.0 * bare as f64 / total.max(1) as f64);
    println!("  full (enums + records):  {}/{} ({:.0}%)",
        with_full, total, 100.0 * with_full as f64 / total.max(1) as f64);
}
