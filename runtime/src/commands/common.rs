//! Shared helpers for `cmd_*` subcommands: flag parsing, runtime loading, value formatting.
//! Single-use helpers live in their owning command file.

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
    eprintln!("  evident effect-run-smtlib <fixture.json>  # run an SMT-LIB-driven program");
}

/// Split positional file paths (before first `-…` flag) from flag args.
pub fn split_files_and_flags(args: &[String]) -> (Vec<String>, Vec<String>) {
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() && !args[i].starts_with('-') {
        files.push(args[i].clone());
        i += 1;
    }
    (files, args[i..].to_vec())
}

pub struct Flags {
    pub given: HashMap<String, Value>,
    pub json: bool,
    pub n_samples: usize,
    /// When UNSAT, retry per-constraint to identify conflicting body items.
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
        // load_file resolves `import "..."` relative to the file itself.
        rt.load_file(Path::new(f)).map_err(|e| format!("{f}: {e}"))?;
    }
    Ok(rt)
}

/// Load a fresh runtime with `stdlib/ast.ev` + `pass_files` (system loads), then the user files.
/// `pass_files` are relative to the stdlib directory.
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

/// Shared prologue for query commands: strips `--strict`, parses flags, loads runtime,
/// and runs `auto_apply_desugar` unless `--strict`. Returns `Err(ExitCode)` on usage/load errors.
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

// Self-hosted desugar pass (bare-identifier → passthrough). Auto-applied by
// sample/test/effect-run. Rule lives in `stdlib/passes/desugar_passthrough.ev`.
const DESUGAR_PASSTHROUGH: &str = "passes/desugar_passthrough.ev";
const PASSTHROUGH_RULE:    &str = "is_passthrough_at_index";

/// One detected passthrough rewrite: replace `body[body_idx]` with `Passthrough(target_name)`.
#[derive(Debug, Clone)]
pub struct Rewrite {
    pub claim_name:  String,
    pub body_idx:    usize,
    pub target_name: String,
}

/// Find every (claim, body_idx, name) triple where the body item is a bare-identifier
/// naming a known schema. Uses its own runtime to avoid touching the caller's state.
pub fn collect_passthrough_rewrites(user_files: &[String])
    -> Result<Vec<Rewrite>, String>
{
    let rt = load_runtime_with_passes(&[DESUGAR_PASSTHROUGH], user_files)?;

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

/// Apply detected rewrites to `rt`. Warns on pipeline failure (non-fatal). Returns rewrite count.
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
        // Only rewrite if item still matches — defends against running twice.
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
        // Composite/SeqComposite not yet produced by translator; Debug until first-class formatting.
        other => format!("{:?}", other),
    }
}
