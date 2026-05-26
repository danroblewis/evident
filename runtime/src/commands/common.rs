//! Shared helpers used by multiple `cmd_*` subcommands: usage banner,
//! generic flag parsing, runtime loading, and shared value formatting.
//!
//! Single-use helpers (e.g. JSON formatting, the SAT/UNSAT printer)
//! live in their owning command file, not here.

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;

use evident_runtime::ast::{BodyItem, Expr};
use evident_runtime::{EvidentRuntime, Value, stdlib_path};

pub fn usage() {
    eprintln!("usage:");
    eprintln!("  evident sample       <files…> <schema> [-n N] [--given k=v …] [--json]");
    eprintln!("  evident sample       <files…> --all [--json]   # sat-check every schema");
    eprintln!("  evident test         [path] [-v] [--no-color]");
    eprintln!("  evident effect-run   <file>           # run an effect-driven program");
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

/// Load a fresh runtime pre-seeded with `stdlib/ast.ev` + the given pass
/// files (marked as system loads), then load the user's files. Used
/// by every self-hosted pass driver (lint, desugar, infer-types).
///
/// `pass_files` are paths **relative to the stdlib directory** (e.g.
/// `"passes/desugar_passthrough.ev"`); they're resolved against the one
/// [`stdlib_path::stdlib_dir`] location, so the drivers work from any CWD.
pub fn load_runtime_with_passes(
    pass_files: &[&str],
    user_files: &[String],
) -> Result<EvidentRuntime, String> {
    let stdlib = stdlib_path::stdlib_dir()?;
    let mut rt = EvidentRuntime::new();
    let ast = stdlib.join("ast.ev");
    rt.load_file(&ast)
        .map_err(|e| format!("load {}: {e}", ast.display()))?;
    for f in pass_files {
        let p = stdlib.join(f);
        rt.load_file(&p)
            .map_err(|e| format!("load {}: {e}", p.display()))?;
    }
    rt.mark_system_loads_complete();
    for path in user_files {
        rt.load_file(Path::new(path))
            .map_err(|e| format!("load {path}: {e}"))?;
    }
    Ok(rt)
}

/// Parsed result of the shared query/sample CLI prologue.
pub struct CmdSetup {
    pub rt: EvidentRuntime,
    pub schema: String,
    pub flags: Flags,
}

/// Shared prologue for `evident sample`:
///   1. strip `--strict` (skip the auto-applied desugar pass),
///   2. split positional files + schema from flag args,
///   3. parse flags,
///   4. construct an `EvidentRuntime` from the file list,
///   5. unless `--strict`, run `auto_apply_desugar` so the user's source
///      has its canonical AST (bare-identifier → passthrough) before the
///      verb runs.
///
/// `cmd_name` is the verb word (`"sample"`) used in error messages.
/// Returns `Err(ExitCode)` for a clean caller-bubbled exit on usage /
/// load errors.
pub fn setup_query_or_sample(cmd_name: &str, args: &[String]) -> Result<CmdSetup, ExitCode> {
    let strict = args.iter().any(|a| a == "--strict");
    let stripped: Vec<String> = args.iter()
        .filter(|a| a.as_str() != "--strict")
        .cloned().collect();
    let (files_and_schema, flag_args) = split_files_and_flags(&stripped);
    if files_and_schema.len() < 2 {
        eprintln!("{cmd_name}: need <files…> <schema>");
        return Err(ExitCode::from(2));
    }
    let schema = files_and_schema.last().unwrap().clone();
    let files: Vec<String> = files_and_schema[..files_and_schema.len() - 1].to_vec();
    let flags = match parse_flags(&flag_args) {
        Ok(f) => f,
        Err(e) => { eprintln!("{e}"); return Err(ExitCode::from(2)); }
    };
    let mut rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => { eprintln!("{e}"); return Err(ExitCode::from(1)); }
    };
    if !strict {
        auto_apply_desugar(&mut rt, &files);
    }
    Ok(CmdSetup { rt, schema, flags })
}

// ── Self-hosted desugar pass (bare-identifier → passthrough) ──────
//
// Relocated here from the former `commands/desugar.rs` (the `evident
// desugar` report-only command was removed). The pass itself is still
// applied automatically by `sample` (via `setup_query_or_sample`),
// `test`, and `effect-run` before they run — it's load-bearing, not a
// command. The Evident-side rule lives in
// `stdlib/passes/desugar_passthrough.ev`.

// Relative to the resolved stdlib directory (see `load_runtime_with_passes`).
const DESUGAR_PASSTHROUGH: &str = "passes/desugar_passthrough.ev";
const PASSTHROUGH_RULE:    &str = "is_passthrough_at_index";

/// One detected rewrite: in `claim_name`, replace `body[body_idx]` with
/// `BodyItem::Passthrough(target_name)`.
#[derive(Debug, Clone)]
pub struct Rewrite {
    pub claim_name:  String,
    pub body_idx:    usize,
    pub target_name: String,
}

/// Find every (claim, body_idx, name) triple where the body item is a
/// bare-identifier constraint AND the identifier names a known schema.
/// Spins up its own runtime so the caller's state isn't touched.
pub fn collect_passthrough_rewrites(user_files: &[String])
    -> Result<Vec<Rewrite>, String>
{
    let rt = load_runtime_with_passes(&[DESUGAR_PASSTHROUGH], user_files)?;

    // Set of every claim name the user (transitively) loaded — the
    // filter for "is target_name a known schema".
    let known: std::collections::HashSet<String> =
        rt.schema_names().map(|s| s.to_string()).collect();

    let mut out: Vec<Rewrite> = Vec::new();
    let mut indices: std::collections::BTreeSet<usize> =
        std::collections::BTreeSet::new();
    for f in user_files {
        for i in rt.user_claim_indices_in_file(Path::new(f)) {
            indices.insert(i);
        }
    }
    for claim_idx in indices {
        let claim_name = rt.user_claim_name(claim_idx).unwrap_or_default();
        let body_len = rt.user_claim_body_len(claim_idx).unwrap_or(0);
        for body_idx in 0..body_len {
            let mut given = HashMap::new();
            given.insert("target_idx".to_string(), Value::Int(body_idx as i64));
            let r = rt.query_with_nth_claim_body_only_given(
                PASSTHROUGH_RULE, "body", claim_idx, given,
            );
            let Ok(Some(qr)) = r else { continue };
            if !qr.satisfied { continue; }
            let Some(Value::Str(name)) = qr.bindings.get("target_name") else { continue };
            if !known.contains(name) { continue; }
            out.push(Rewrite { claim_name: claim_name.clone(), body_idx, target_name: name.clone() });
        }
    }
    Ok(out)
}

/// Apply every detected rewrite to `rt`. Quiet on success; prints one
/// stderr warning if the pipeline fails (non-fatal — caller continues
/// without rewrites). Returns the number of body items rewritten.
pub fn auto_apply_desugar(rt: &mut EvidentRuntime, user_files: &[String]) -> usize {
    let rewrites = match collect_passthrough_rewrites(user_files) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("warning: desugar pipeline failed: {e}");
            return 0;
        }
    };
    let mut applied = 0usize;
    for r in &rewrites {
        let new_item = BodyItem::Passthrough(r.target_name.clone());
        // Sanity check: only rewrite if the body item still matches the
        // expected shape (defends against running twice).
        let still_matches = rt.get_schema(&r.claim_name)
            .and_then(|s| s.body.get(r.body_idx))
            .map(|item| matches!(item,
                BodyItem::Constraint(Expr::Identifier(n)) if n == &r.target_name))
            .unwrap_or(false);
        if !still_matches { continue; }
        if let Ok(true) = rt.replace_body_item_in_claim(&r.claim_name, r.body_idx, new_item) {
            applied += 1;
        }
    }
    applied
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
