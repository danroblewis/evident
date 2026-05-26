//! `generics` — generic-type monomorphization. **Sole implementation: the
//! self-hosted Evident pass** (`stdlib/passes/generics.ev`). The canonical
//! Rust pass is deleted (session REVIVE-generics); the production load path
//! computes monomorphization through here.
//!
//! Monomorphization expands every `type Edge<T>` / `claim Toposort<T>`
//! reference into a concrete copy before translation, in four halves:
//!   * **WALK** — find every type-position string that could name a generic
//!     instantiation. Runs as the `generics_walk` stack-FSM.
//!   * **PARSE** — split `"Edge<Rect>"` into head + arg. The `split_head`
//!     claim (GAPC `index_of` / `substr`).
//!   * **SUBSTITUTE** — rewrite a generic body's type_name strings. The
//!     `subst_one` claim (GAPC `replace`).
//!   * **CONSTRUCT + fixed-point + schema-map lookup** — orchestration that
//!     needs the WHOLE-PROGRAM schema table (look a head up by name, dedup
//!     built composites, splice substituted bodies onto clones, iterate to a
//!     fixed point). Stays in Rust — a structural traversal over a mutable
//!     `HashMap` an FSM has no handle on, needing no string surgery.
//!
//! These are load-time string solves over short type-name strings; per-tick
//! runtime is unaffected (monomorphization runs once, at load).

use std::collections::{HashMap, HashSet};

use super::{run_name_list, work_node, EvidentRunner};
use crate::core::ast::{BodyItem, Expr, SchemaDecl};
use crate::core::{RuntimeError, Value};
use crate::translate::ast_encoder::body_item_to_value;

guarded_runner!(runner, "passes/generics.ev", "generics_walk");

// ─────────────────────────────────────────────────────────────────────
// Pure string helpers (the schema-map-independent orchestration that frames
// the Evident string ops — moved from the deleted Rust pass).
// ─────────────────────────────────────────────────────────────────────

/// The Some-condition of the canonical `split_generic_head`: `t` contains `<`,
/// ends with `>`, and the angle brackets are balanced. The cheap Rust guard;
/// the head/arg EXTRACTION is the Evident `split_head` claim.
fn is_generic_head(t: &str) -> bool {
    let bytes = t.as_bytes();
    if !bytes.iter().any(|&b| b == b'<') {
        return false;
    }
    if !t.ends_with('>') {
        return false;
    }
    let mut depth: i32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 && i != bytes.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Split a comma-separated arg list at the TOP level — commas inside nested
/// `<...>` / `(...)` are not splits. `"Pair<Int, String>, Bool"` →
/// `["Pair<Int, String>", "Bool"]`.
fn split_top_level_args(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let bytes = args.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' | b'(' => depth += 1,
            b'>' | b')' => depth -= 1,
            b',' if depth == 0 => {
                out.push(args[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = args[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

/// Cheap presence gate: does any type-position string in the program name a
/// generic instantiation (contain `<`)? If not, monomorphization is a
/// guaranteed no-op, so the load path skips building the engine — keeping
/// non-generic loads (≈ every program) at the Rust baseline.
fn program_has_generic_use(schemas: &HashMap<String, SchemaDecl>) -> bool {
    schemas.values().any(|s| body_mentions_generic(&s.body))
}

fn body_mentions_generic(body: &[BodyItem]) -> bool {
    body.iter().any(|item| match item {
        BodyItem::Membership { type_name, .. } => type_name.contains('<'),
        BodyItem::Passthrough(n) => n.contains('<'),
        BodyItem::ClaimCall { name, mappings } => {
            name.contains('<') || mappings.iter().any(|m| expr_mentions_generic(&m.value))
        }
        BodyItem::Constraint(e) => expr_mentions_generic(e),
        BodyItem::SubclaimDecl(s) => body_mentions_generic(&s.body),
        BodyItem::HaltsWithin { .. } => false,
    })
}

/// `<` can hide in a positional generic invocation (`Edge<Int>(a, b)`) that
/// parses as an `Expr::Call`, so the gate recurses every expression.
fn expr_mentions_generic(e: &Expr) -> bool {
    match e {
        Expr::Call(name, args) => name.contains('<') || args.iter().any(expr_mentions_generic),
        Expr::Binary(_, a, b) | Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) => {
            expr_mentions_generic(a) || expr_mentions_generic(b)
        }
        Expr::Ternary(a, b, c) => {
            expr_mentions_generic(a) || expr_mentions_generic(b) || expr_mentions_generic(c)
        }
        Expr::SetLit(xs) | Expr::SeqLit(xs) | Expr::Tuple(xs) => xs.iter().any(expr_mentions_generic),
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
            expr_mentions_generic(r) || expr_mentions_generic(b)
        }
        Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => expr_mentions_generic(i),
        Expr::Field(b, _) => expr_mentions_generic(b),
        Expr::Match(scr, arms) => {
            expr_mentions_generic(scr) || arms.iter().any(|a| expr_mentions_generic(&a.body))
        }
        Expr::RunFsm { init, .. } => expr_mentions_generic(init),
        Expr::Identifier(_) | Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => false,
    }
}

/// If `t` is `"Seq(X)"`, `"Set(X)"`, `"Bag(X)"`, or `"Map(X)"`, return
/// `Some(X)`. Lets `collect_from_type_name` reach the generic inside a
/// container (`Seq(Edge<T>)` → `Edge<T>`).
fn strip_seq_wrapper(t: &str) -> Option<&str> {
    for prefix in &["Seq(", "Set(", "Bag(", "Map("] {
        if let Some(rest) = t.strip_prefix(prefix) {
            if let Some(inner) = rest.strip_suffix(')') {
                return Some(inner);
            }
        }
    }
    None
}

/// Pure-Rust head/arg slice, the fallback if the Evident `split_head` query
/// fails. Equivalent extraction to the claim.
fn rust_split_head(t: &str) -> (String, String) {
    match t.find('<') {
        Some(lt) if t.ends_with('>') => (t[..lt].to_string(), t[lt + 1..t.len() - 1].to_string()),
        _ => (t.to_string(), String::new()),
    }
}

// ─────────────────────────────────────────────────────────────────────
// WALK / PARSE / SUBSTITUTE (Evident) + CONSTRUCT (Rust orchestration)
// ─────────────────────────────────────────────────────────────────────

/// Collect every `(composite_name, generic_head, args_str)` tuple referenced
/// anywhere in the schema map. The WALK runs in Evident (`generics_walk`, per
/// top-level body item — every recursion happens inside the FSM); each raw
/// string is parsed through [`collect_from_type_name`] with one shared `seen`.
fn collect_uses(
    runner: &EvidentRunner,
    schemas: &HashMap<String, SchemaDecl>,
) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    // Sorted keys for run-to-run reproducibility (the result SET is
    // order-independent).
    let mut keys: Vec<&String> = schemas.keys().collect();
    keys.sort();
    for k in keys {
        let s = &schemas[k];
        for item in &s.body {
            let seed = work_node("Work", "WBody", body_item_to_value(item));
            let raws = run_name_list(runner, "generics_walk", seed, "GWDone",
                                     &format!("generics/evident `{}`", s.name));
            for raw in raws {
                collect_from_type_name(runner, &raw, &mut out, &mut seen);
            }
        }
    }
    out
}

/// Parse one raw type-position string, recording it (and any generic nested
/// inside a `Seq(...)` wrapper or a top-level arg) into `out`. The head/arg
/// split is the Evident `split_head` claim; the Seq-wrapper recursion and the
/// top-level arg split are pure Rust string orchestration.
fn collect_from_type_name(
    runner: &EvidentRunner,
    t: &str,
    out: &mut Vec<(String, String, String)>,
    seen: &mut HashSet<String>,
) {
    if is_generic_head(t) {
        let (head, args) = split_head_ev(runner, t);
        if seen.insert(t.to_string()) {
            out.push((t.to_string(), head, args.clone()));
        }
        for arg in split_top_level_args(&args) {
            collect_from_type_name(runner, &arg, out, seen);
        }
        return;
    }
    if let Some(inner) = strip_seq_wrapper(t) {
        collect_from_type_name(runner, inner, out, seen);
    }
}

/// PARSE `"Edge<Rect>"` → `("Edge", "Rect")` via the Evident `split_head`
/// claim. Only called for strings [`is_generic_head`] accepts. Falls back to
/// pure-Rust slicing on a query failure so a transient error can't drop a use.
fn split_head_ev(runner: &EvidentRunner, t: &str) -> (String, String) {
    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("g".to_string(), Value::Str(t.to_string()));
    match runner.rt().query("split_head", &given) {
        Ok(r) if r.satisfied => {
            let head = match r.bindings.get("head") {
                Some(Value::Str(s)) => s.clone(),
                _ => String::new(),
            };
            let arg = match r.bindings.get("arg") {
                Some(Value::Str(s)) => s.clone(),
                _ => String::new(),
            };
            if !head.is_empty() {
                return (head, arg);
            }
            rust_split_head(t)
        }
        other => {
            if let Err(e) = other {
                eprintln!("[generics/evident] split_head(`{t}`) failed: {e}");
            }
            rust_split_head(t)
        }
    }
}

/// Apply the type-param substitution to every `type_name` in a body, recursing
/// into subclaim bodies. Mirrors the canonical
/// `substitute_type_params_in_body`: ONLY `Membership` type_names are
/// rewritten (and subclaims recursed). The per-string rewrite is the Evident
/// `subst_one` claim; this traversal is the structural splice.
fn apply_substitution_to_body(
    runner: &EvidentRunner,
    body: &mut [BodyItem],
    params: &[String],
    args: &[String],
) {
    for item in body.iter_mut() {
        match item {
            BodyItem::Membership { type_name, .. } => {
                *type_name = subst_type_name(runner, type_name, params, args);
            }
            BodyItem::SubclaimDecl(sub) => {
                apply_substitution_to_body(runner, &mut sub.body, params, args);
            }
            _ => {}
        }
    }
}

/// Thread every `(param ↦ arg)` substitution through one type-name string.
fn subst_type_name(runner: &EvidentRunner, t: &str, params: &[String], args: &[String]) -> String {
    let mut cur = t.to_string();
    for (p, a) in params.iter().zip(args.iter()) {
        cur = subst_one_ev(runner, &cur, p, a);
    }
    cur
}

/// SUBSTITUTE one param in a type-name string via the Evident `subst_one`
/// claim (GAPC `replace`). Falls back to pure-Rust replace on a query failure.
fn subst_one_ev(runner: &EvidentRunner, t: &str, param: &str, arg: &str) -> String {
    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("t".to_string(), Value::Str(t.to_string()));
    given.insert("param".to_string(), Value::Str(param.to_string()));
    given.insert("arg".to_string(), Value::Str(arg.to_string()));
    match runner.rt().query("subst_one", &given) {
        Ok(r) if r.satisfied => match r.bindings.get("out") {
            Some(Value::Str(s)) => s.clone(),
            _ => t.replacen(param, arg, 1),
        },
        other => {
            if let Err(e) = other {
                eprintln!("[generics/evident] subst_one(`{t}`, `{param}`) failed: {e}");
            }
            t.replacen(param, arg, 1)
        }
    }
}

/// Monomorphize to a fixed point: produce concrete `SchemaDecl`s for every
/// generic instantiation referenced in the program. Iterates because
/// monomorphized schemas can themselves reference generics. Byte-for-byte the
/// canonical fixed-point loop (same error wording), with the collector and the
/// body substitution backed by Evident.
fn monomorphize(
    runner: &EvidentRunner,
    schemas: &mut HashMap<String, SchemaDecl>,
    schema_order: &mut Vec<String>,
) -> Result<(), RuntimeError> {
    for _iteration in 0..50 {
        let needed = collect_uses(runner, schemas);
        let mut produced = 0;
        for (composite_name, generic_head, args_str) in needed {
            if schemas.contains_key(&composite_name) {
                continue;
            }
            let generic = match schemas.get(&generic_head) {
                Some(g) => g,
                None => continue, // not a generic we know about; leave it
            };
            if generic.type_params.is_empty() {
                return Err(RuntimeError::Parse(format!(
                    "type `{}` referenced with type arguments `<{}>` but \
                     isn't declared as generic",
                    generic_head, args_str
                )));
            }
            let args = split_top_level_args(&args_str);
            if args.len() != generic.type_params.len() {
                return Err(RuntimeError::Parse(format!(
                    "type `{}` expects {} type argument(s), got {}: `{}`",
                    generic_head, generic.type_params.len(), args.len(), composite_name
                )));
            }
            let params = generic.type_params.clone();
            let mut mono = generic.clone();
            mono.name = composite_name.clone();
            mono.type_params = Vec::new();
            apply_substitution_to_body(runner, &mut mono.body, &params, &args);
            schemas.insert(composite_name.clone(), mono);
            schema_order.push(composite_name);
            produced += 1;
        }
        if produced == 0 {
            return Ok(());
        }
    }
    Err(RuntimeError::Parse(
        "monomorphize_generics: didn't converge after 50 iterations (cycle?)".to_string(),
    ))
}

// ─────────────────────────────────────────────────────────────────────
// Production entry point
// ─────────────────────────────────────────────────────────────────────

/// Monomorphize generic instantiations via the self-hosted `generics.ev` pass.
/// **The runtime's sole monomorphization entry point** —
/// `runtime/src/runtime/load.rs` calls it after each schema batch.
///
/// The presence gate skips the engine build for non-generic programs; the
/// guarded runner short-circuits the bootstrap re-entry (the pass file uses no
/// `<…>` generics, so leaving its schema map unchanged is correct).
pub fn monomorphize_generics(
    schemas: &mut HashMap<String, SchemaDecl>,
    schema_order: &mut Vec<String>,
) -> Result<(), RuntimeError> {
    if !program_has_generic_use(schemas) {
        return Ok(());
    }
    let Some(runner) = runner() else { return Ok(()) };
    monomorphize(&runner, schemas, schema_order)
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ast::{Keyword, Pins};

    fn schema(keyword: Keyword, name: &str, type_params: Vec<&str>, body: Vec<BodyItem>, param_count: usize) -> SchemaDecl {
        SchemaDecl {
            keyword,
            name: name.to_string(),
            type_params: type_params.into_iter().map(|s| s.to_string()).collect(),
            body,
            param_count,
            external: false,
        }
    }
    fn member(name: &str, type_name: &str) -> BodyItem {
        BodyItem::Membership { name: name.to_string(), type_name: type_name.to_string(), pins: Pins::None }
    }

    /// The pure-Rust orchestration helpers behave as the canonical pass did —
    /// the part that does NOT need the Evident engine, pinned directly.
    #[test]
    fn string_helpers_match_canonical() {
        assert!(is_generic_head("Edge<Int>"));
        assert!(is_generic_head("Pair<Int, String>"));
        assert!(!is_generic_head("Edge"));
        assert!(!is_generic_head("Seq(Int)"));
        assert_eq!(split_top_level_args("Pair<Int, String>, Bool"),
                   vec!["Pair<Int, String>".to_string(), "Bool".to_string()]);
        assert_eq!(strip_seq_wrapper("Seq(Edge<T>)"), Some("Edge<T>"));
        assert_eq!(rust_split_head("Edge<Rect>"), ("Edge".to_string(), "Rect".to_string()));

        let mut schemas: HashMap<String, SchemaDecl> = HashMap::new();
        schemas.insert("user".into(), schema(Keyword::Claim, "user", vec![], vec![member("e", "Edge<Int>")], 0));
        assert!(program_has_generic_use(&schemas));
        let mut plain: HashMap<String, SchemaDecl> = HashMap::new();
        plain.insert("p".into(), schema(Keyword::Type, "p", vec![], vec![member("x", "Int")], 1));
        assert!(!program_has_generic_use(&plain));
    }
}
