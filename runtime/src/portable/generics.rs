//! Generic-type monomorphization via `stdlib/passes/generics.ev`. WALK/PARSE/
//! SUBSTITUTE run in Evident; fixed-point `HashMap` orchestration stays in Rust.

use std::collections::{HashMap, HashSet};

use super::{run_name_list, work_node, EvidentRunner};
use crate::core::ast::{BodyItem, Expr, SchemaDecl};
use crate::core::{RuntimeError, Value};
use crate::translate::ast_encoder::body_item_to_value;

guarded_runner!(runner, "passes/generics.ev", "generics_walk");

/// True if `t` looks like a generic instantiation (`<` present, ends `>`,
/// brackets balanced). Guard only — head/arg extraction is in `split_head_ev`.
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

/// Split comma-separated args at top level (commas inside `<>` / `()` skipped).
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

/// True if any type-position string in the program contains `<`; skips engine
/// build for non-generic programs.
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

/// `<` can hide in `Expr::Call` (positional generic invocation); recurse all exprs.
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

/// Strip a `Seq/Set/Bag/Map(X)` wrapper, exposing the inner type name.
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

/// Fallback head/arg split used when the Evident `split_head` query fails.
fn rust_split_head(t: &str) -> (String, String) {
    match t.find('<') {
        Some(lt) if t.ends_with('>') => (t[..lt].to_string(), t[lt + 1..t.len() - 1].to_string()),
        _ => (t.to_string(), String::new()),
    }
}

/// Collect every `(composite_name, generic_head, args_str)` tuple referenced
/// in the schema map via the `generics_walk` FSM (per body item).
fn collect_uses(
    runner: &EvidentRunner,
    schemas: &HashMap<String, SchemaDecl>,
) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
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

/// Parse one raw type-position string into `out`, recursing into wrappers and
/// top-level args. Head/arg split via `split_head_ev`; wrapping/splitting in Rust.
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

/// Split `"Edge<Rect>"` → `("Edge", "Rect")` via Evident `split_head`; falls
/// back to pure-Rust slicing on query failure.
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

/// Rewrite `Membership` type_names in a body via `subst_one_ev`; recurse into
/// subclaims. Only membership type names are touched.
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

fn subst_type_name(runner: &EvidentRunner, t: &str, params: &[String], args: &[String]) -> String {
    let mut cur = t.to_string();
    for (p, a) in params.iter().zip(args.iter()) {
        cur = subst_one_ev(runner, &cur, p, a);
    }
    cur
}

/// Substitute one param in a type-name string via Evident `subst_one`;
/// falls back to `str::replacen` on query failure.
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

/// Fixed-point monomorphization: produce concrete schemas for every generic
/// instantiation; iterate because new schemas may reference further generics.
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
                None => continue, // not a known generic; leave it
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

/// The runtime's sole monomorphization entry point; no-op for non-generic
/// programs (presence gate skips engine build).
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
