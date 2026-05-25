//! `generics` — generic-type monomorphization. **Sole implementation: the
//! self-hosted Evident pass.** The canonical Rust pass
//! (`runtime/src/runtime/generics.rs`) is **deleted** (session
//! REVIVE-generics); the production load path computes monomorphization
//! through [`EvidentGenerics`].
//!
//! Monomorphization expands every `type Edge<T>` / `claim Toposort<T>`
//! reference into a concrete copy (`Edge<Rect>`, `Toposort<Int>`, …) before
//! translation. It runs in the load pipeline (`runtime/src/runtime/load.rs`,
//! which delegates to [`monomorphize_generics`] here). The pass decomposes
//! into four halves:
//!
//!   * **WALK** — find every type-position string that could name a generic
//!     instantiation (a `Membership`'s type_name, a `ClaimCall`'s /
//!     `Passthrough`'s name, every `Call` name in a constraint). Runs as the
//!     `generics_walk` stack-FSM in `stdlib/passes/generics.ev`.
//!   * **PARSE** — split `"Edge<Rect>"` into head + arg. Runs as the
//!     `split_head` claim (GAPC's `index_of` / `substr`).
//!   * **SUBSTITUTE** — rewrite a generic body's type_name strings, e.g.
//!     `"Seq(T)"` with `T↦Rect` → `"Seq(Rect)"`. Runs as the `subst_one`
//!     claim (GAPC's `replace`).
//!   * **CONSTRUCT + fixed-point + schema-map lookup** — the orchestration
//!     that needs the WHOLE-PROGRAM schema table (look a head up by name,
//!     dedup already-built composites, splice the substituted bodies onto
//!     clones, iterate nested generics to a fixed point). This stays in
//!     Rust here — the same whole-program-table boundary `subscriptions`
//!     keeps its access-set merge and `validate` keeps its banned-set check
//!     on. It needs no string surgery (it never depended on GAPC); it's a
//!     structural traversal over a mutable `HashMap` an FSM has no handle on.
//!
//! ## Why this cut over now (it couldn't, session PORT-generics)
//!
//! PORT-generics self-hosted only the WALK and kept PARSE + SUBSTITUTE in
//! Rust, blocked by Evident's lack of substring/tokenize ops (only `=`/`≠`/
//! `++`). Two gaps closed the loop:
//!   * **GAPC** added `index_of`/`substr`/`replace` (Z3 string theory). The
//!     character-level surgery — `split_generic_head` / `substitute_idents`
//!     — is now expressible, so `split_head` + `subst_one` run in Evident.
//!   * **GAPB** made `MakeSchemaDecl` carry `param_count`, so a generic's
//!     body round-trips through the shared marshaler losslessly.
//!
//! These are load-time string solves over short type-name strings; per-tick
//! runtime is unaffected (monomorphization runs once, at load).
//!
//! ## Documented residual (honest-fallback)
//!
//! `subst_one` uses `replace` (first-occurrence substring), faithful to the
//! token-aware canonical `substitute_idents` for the corpus surface —
//! single capitalised type params (`T`, `A`, …) appearing once per
//! type-name. A body type-name that embeds the param letter as a substring
//! (param `T`, referenced type `Tree`) or repeats the param would diverge;
//! the generic corpus has neither, and `generics_correctness.rs` pins the
//! corpus output. The fixed-point / lookup / Seq-wrapper / multi-arg split
//! (`split_top_level_args`) stay in Rust — pure orchestration that needs the
//! schema map, not the string ops GAPC added.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

use crate::core::ast::{BodyItem, Expr, SchemaDecl};
use crate::core::{RuntimeError, Value};
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::{decode_list, decode_str};
use crate::translate::ast_encoder::body_item_to_value;
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// Pure string helpers (the schema-map-independent orchestration that
// frames the Evident string ops — moved here from the deleted Rust pass).
// ─────────────────────────────────────────────────────────────────────

/// The Some-condition of the canonical `split_generic_head`: `t` is a
/// generic instantiation — it contains `<`, ends with `>`, and the angle
/// brackets are balanced. This is the None-determination (the cheap Rust
/// guard); the head/arg EXTRACTION is the Evident `split_head` claim. Kept
/// here so the surgery only fires on strings that actually name a generic.
fn is_generic_head(t: &str) -> bool {
    let bytes = t.as_bytes();
    if !bytes.iter().any(|&b| b == b'<') { return false; }
    if !t.ends_with('>') { return false; }
    let mut depth: i32 = 0;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'<' => depth += 1,
            b'>' => {
                depth -= 1;
                if depth == 0 && i != bytes.len() - 1 { return false; }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Split a comma-separated arg list at the TOP level — commas inside nested
/// `<...>` / `(...)` are not splits. `"Pair<Int, String>, Bool"` →
/// `["Pair<Int, String>", "Bool"]`. The corpus uses single non-nested args
/// (so this is the identity on one element), but the recursion is kept for
/// faithfulness to the canonical pass.
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
    if !tail.is_empty() { out.push(tail.to_string()); }
    out
}

/// Cheap presence gate: does any type-position string in the program name a
/// generic instantiation (contain `<`)? If not, monomorphization is a
/// guaranteed no-op — no `Head<…>` use means nothing to expand AND no
/// "isn't declared generic" error to raise (that error case also requires a
/// `<`). So the load path skips building the Evident engine and running the
/// walk entirely, keeping non-generic loads (≈ every program — Mario, the
/// whole test suite) at the Rust baseline. Without this gate the per-load
/// Evident walk costs ~1–3s on a real program for zero benefit.
///
/// This is a presence GATE (a boolean "any generics?"), distinct from the
/// self-hosted COLLECTION: any program that *does* mention a generic still
/// runs the full `generics_walk` + `split_head` + `subst_one` in Evident to
/// compute and expand the actual use set. It mirrors a toolchain skipping an
/// optional pass when there's provably nothing to do — `<` is the necessary
/// syntactic marker of every type-position generic use.
fn program_has_generic_use(schemas: &HashMap<String, SchemaDecl>) -> bool {
    schemas.values().any(|s| body_mentions_generic(&s.body))
}

fn body_mentions_generic(body: &[BodyItem]) -> bool {
    body.iter().any(|item| match item {
        BodyItem::Membership { type_name, .. } => type_name.contains('<'),
        BodyItem::Passthrough(n) => n.contains('<'),
        BodyItem::ClaimCall { name, mappings } =>
            name.contains('<') || mappings.iter().any(|m| expr_mentions_generic(&m.value)),
        BodyItem::Constraint(e) => expr_mentions_generic(e),
        BodyItem::SubclaimDecl(s) => body_mentions_generic(&s.body),
        BodyItem::HaltsWithin { .. } => false,
    })
}

/// `<` can hide in a positional generic invocation (`Edge<Int>(a, b)`) that
/// parses as an `Expr::Call` inside a constraint, so the gate recurses
/// every expression for a generic Call name — the same four type-position
/// kinds `generics_walk` collects.
fn expr_mentions_generic(e: &Expr) -> bool {
    match e {
        Expr::Call(name, args) => name.contains('<') || args.iter().any(expr_mentions_generic),
        Expr::Binary(_, a, b) | Expr::Range(a, b) | Expr::InExpr(a, b) | Expr::Index(a, b) =>
            expr_mentions_generic(a) || expr_mentions_generic(b),
        Expr::Ternary(a, b, c) =>
            expr_mentions_generic(a) || expr_mentions_generic(b) || expr_mentions_generic(c),
        Expr::SetLit(xs) | Expr::SeqLit(xs) | Expr::Tuple(xs) => xs.iter().any(expr_mentions_generic),
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => expr_mentions_generic(r) || expr_mentions_generic(b),
        Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => expr_mentions_generic(i),
        Expr::Field(b, _) => expr_mentions_generic(b),
        Expr::Match(scr, arms) => expr_mentions_generic(scr) || arms.iter().any(|a| expr_mentions_generic(&a.body)),
        Expr::RunFsm { init, .. } => expr_mentions_generic(init),
        Expr::Identifier(_) | Expr::Int(_) | Expr::Real(_) | Expr::Bool(_) | Expr::Str(_) => false,
    }
}

/// If `t` is `"Seq(X)"`, `"Set(X)"`, `"Bag(X)"`, or `"Map(X)"`, return
/// `Some(X)`. Otherwise `None`. Lets `collect_from_type_name` reach the
/// generic inside a container (`Seq(Edge<T>)` → `Edge<T>`).
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

// ─────────────────────────────────────────────────────────────────────
// The Evident-backed monomorphizer
// ─────────────────────────────────────────────────────────────────────

/// Pass-driven monomorphizer. Holds an [`EvidentRuntime`] with
/// `stdlib/passes/generics.ev` loaded — a self-contained pass (it declares
/// its own cons-list copy of the AST enums matching the shared marshaler),
/// so no other stdlib file is needed. Build once and reuse across the whole
/// load (and across loads, per-thread) so the FSM's per-tick solve and the
/// `split_head`/`subst_one` claims stay JIT-cached.
pub struct EvidentGenerics {
    rt: EvidentRuntime,
}

impl EvidentGenerics {
    /// The whole-walk FSM in `stdlib/passes/generics.ev`.
    const WALK_FSM: &'static str = "generics_walk";

    /// Max-iteration guard for the nested walk. One AST node costs a small
    /// constant number of FSM ticks, so a body of N nodes halts in O(N)
    /// ticks; the cap sits far above any realistic claim (a runaway walk
    /// would be a pass bug, surfaced as a loud `MaxItersExceeded`).
    const MAX_STEPS: usize = 5_000_000;

    /// Load `passes/generics.ev` from `stdlib_dir` into a fresh runtime.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("passes").join("generics.ev"))
            .map_err(|e| format!("load passes/generics.ev: {e}"))?;
        Ok(Self { rt })
    }

    // ── WALK ──────────────────────────────────────────────────────────

    /// Drive `generics_walk` over one seeded `Work` node and return the RAW
    /// type-position strings it reaches (head-first). The FSM returns
    /// `GWDone(NameList)` — a cons-list of unparsed type-name strings.
    fn walk_item_raw(&self, seed: &Value, claim_name: &str) -> Vec<String> {
        match crate::effect_loop::run_nested(&self.rt, Self::WALK_FSM, seed.clone(), Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. }) if variant == "GWDone" && fields.len() == 1 => {
                match decode_list(&fields[0], "NameList", "NameNil", "NameCons", decode_str) {
                    Ok(names) => names,
                    Err(e) => {
                        eprintln!("[generics/evident] decode of `{claim_name}` result failed: {e}");
                        Vec::new()
                    }
                }
            }
            Ok(other) => {
                eprintln!("[generics/evident] walk of `{claim_name}` returned a \
                    non-GWDone state: {other:?}");
                Vec::new()
            }
            Err(e) => {
                eprintln!("[generics/evident] walk of `{claim_name}` failed: {e}");
                Vec::new()
            }
        }
    }

    /// Collect every `(composite_name, generic_head, args_str)` tuple
    /// referenced anywhere in the schema map. The WALK runs in Evident
    /// (`generics_walk`, per top-level body item — every recursion, into
    /// sub-exprs AND subclaim bodies, happens inside the FSM); each raw
    /// string is parsed through `collect_from_type_name` with one shared
    /// `seen`. Mirrors the canonical `collect_generic_uses`.
    fn collect_uses(&self, schemas: &HashMap<String, SchemaDecl>) -> Vec<(String, String, String)> {
        let mut out: Vec<(String, String, String)> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        // Sorted keys for run-to-run reproducibility (the result SET is
        // order-independent; the canonical iterated HashMap order).
        let mut keys: Vec<&String> = schemas.keys().collect();
        keys.sort();
        for k in keys {
            let s = &schemas[k];
            for item in &s.body {
                let seed = work_node("WBody", body_item_to_value(item));
                for raw in self.walk_item_raw(&seed, &s.name) {
                    self.collect_from_type_name(&raw, &mut out, &mut seen);
                }
            }
        }
        out
    }

    // ── PARSE ─────────────────────────────────────────────────────────

    /// Parse one raw type-position string, recording it (and any generic
    /// nested inside a `Seq(...)` wrapper or a top-level arg) into `out`.
    /// Mirrors the canonical `collect_from_type_name`. The head/arg split is
    /// the Evident `split_head` claim; the Seq-wrapper recursion and the
    /// top-level arg split are pure Rust string orchestration.
    fn collect_from_type_name(
        &self,
        t: &str,
        out: &mut Vec<(String, String, String)>,
        seen: &mut HashSet<String>,
    ) {
        if is_generic_head(t) {
            let (head, args) = self.split_head_ev(t);
            if seen.insert(t.to_string()) {
                out.push((t.to_string(), head, args.clone()));
            }
            for arg in split_top_level_args(&args) {
                self.collect_from_type_name(&arg, out, seen);
            }
            return;
        }
        if let Some(inner) = strip_seq_wrapper(t) {
            self.collect_from_type_name(inner, out, seen);
        }
    }

    /// PARSE `"Edge<Rect>"` → `("Edge", "Rect")` via the Evident `split_head`
    /// claim (GAPC `index_of`/`substr`). Only called for strings
    /// [`is_generic_head`] already accepts. On a query failure, falls back to
    /// pure-Rust slicing so a transient engine error can't drop a generic use.
    fn split_head_ev(&self, t: &str) -> (String, String) {
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("g".to_string(), Value::Str(t.to_string()));
        match self.rt.query("split_head", &given) {
            Ok(r) if r.satisfied => {
                let head = match r.bindings.get("head") { Some(Value::Str(s)) => s.clone(), _ => String::new() };
                let arg  = match r.bindings.get("arg")  { Some(Value::Str(s)) => s.clone(), _ => String::new() };
                if !head.is_empty() { return (head, arg); }
                rust_split_head(t)
            }
            other => {
                if let Err(e) = other { eprintln!("[generics/evident] split_head(`{t}`) failed: {e}"); }
                rust_split_head(t)
            }
        }
    }

    // ── SUBSTITUTE ────────────────────────────────────────────────────

    /// Apply the type-param substitution to every `type_name` in a body,
    /// recursing into subclaim bodies. Mirrors the canonical
    /// `substitute_type_params_in_body`: ONLY `Membership` type_names are
    /// rewritten (and subclaims recursed) — `ClaimCall`/`Passthrough`/`Call`
    /// names are left as-is, which is why a names-matched `Permutation<T>`
    /// call inside `Toposort<Int>` stays generic-bodied and resolves by
    /// names-match at inline time. The per-string rewrite is the Evident
    /// `subst_one` claim; this traversal is the structural splice (the
    /// CONSTRUCT orchestration that needs no string ops).
    fn apply_substitution_to_body(&self, body: &mut [BodyItem], params: &[String], args: &[String]) {
        for item in body.iter_mut() {
            match item {
                BodyItem::Membership { type_name, .. } => {
                    *type_name = self.subst_type_name(type_name, params, args);
                }
                BodyItem::SubclaimDecl(sub) => {
                    self.apply_substitution_to_body(&mut sub.body, params, args);
                }
                _ => {}
            }
        }
    }

    /// Thread every `(param ↦ arg)` substitution through one type-name
    /// string. For the corpus this is one pass (single type param); a
    /// multi-param generic chains the rewrites left-to-right.
    fn subst_type_name(&self, t: &str, params: &[String], args: &[String]) -> String {
        let mut cur = t.to_string();
        for (p, a) in params.iter().zip(args.iter()) {
            cur = self.subst_one_ev(&cur, p, a);
        }
        cur
    }

    /// SUBSTITUTE one param in a type-name string via the Evident `subst_one`
    /// claim (GAPC `replace`). On a query failure, falls back to pure-Rust
    /// substring replace so a transient engine error can't silently drop a
    /// substitution.
    fn subst_one_ev(&self, t: &str, param: &str, arg: &str) -> String {
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("t".to_string(),     Value::Str(t.to_string()));
        given.insert("param".to_string(), Value::Str(param.to_string()));
        given.insert("arg".to_string(),   Value::Str(arg.to_string()));
        match self.rt.query("subst_one", &given) {
            Ok(r) if r.satisfied => match r.bindings.get("out") {
                Some(Value::Str(s)) => s.clone(),
                _ => t.replacen(param, arg, 1),
            },
            other => {
                if let Err(e) = other { eprintln!("[generics/evident] subst_one(`{t}`, `{param}`) failed: {e}"); }
                t.replacen(param, arg, 1)
            }
        }
    }

    // ── CONSTRUCT + fixed point ───────────────────────────────────────

    /// Monomorphize to a fixed point: produce concrete `SchemaDecl`s for
    /// every generic instantiation referenced in the program. After this,
    /// every type_name containing `<` resolves to a real schema in the map.
    ///
    /// Iterates because monomorphized schemas can themselves reference
    /// generics (`Toposort<T>`'s `edges ∈ Seq(Edge<T>)` becomes
    /// `Seq(Edge<Int>)` after substitution — a new instantiation). Byte-for-
    /// byte the canonical fixed-point loop (same error wording), with the
    /// collector and the body substitution backed by Evident.
    pub fn monomorphize(
        &self,
        schemas: &mut HashMap<String, SchemaDecl>,
        schema_order: &mut Vec<String>,
    ) -> Result<(), RuntimeError> {
        for _iteration in 0..50 {
            let needed = self.collect_uses(schemas);
            let mut produced = 0;
            for (composite_name, generic_head, args_str) in needed {
                if schemas.contains_key(&composite_name) { continue; }
                let generic = match schemas.get(&generic_head) {
                    Some(g) => g,
                    None => continue,  // not a generic we know about; leave it
                };
                if generic.type_params.is_empty() {
                    return Err(RuntimeError::Parse(format!(
                        "type `{}` referenced with type arguments `<{}>` but \
                         isn't declared as generic",
                        generic_head, args_str)));
                }
                let args = split_top_level_args(&args_str);
                if args.len() != generic.type_params.len() {
                    return Err(RuntimeError::Parse(format!(
                        "type `{}` expects {} type argument(s), got {}: `{}`",
                        generic_head, generic.type_params.len(), args.len(),
                        composite_name)));
                }
                let params = generic.type_params.clone();
                let mut mono = generic.clone();
                mono.name = composite_name.clone();
                mono.type_params = Vec::new();
                self.apply_substitution_to_body(&mut mono.body, &params, &args);
                schemas.insert(composite_name.clone(), mono);
                schema_order.push(composite_name);
                produced += 1;
            }
            if produced == 0 { return Ok(()); }
        }
        Err(RuntimeError::Parse(
            "monomorphize_generics: didn't converge after 50 iterations (cycle?)".to_string()))
    }
}

impl Portable for EvidentGenerics {
    fn impl_name(&self) -> &'static str { "evident" }
}

/// Wrap an already-marshaled AST `Value` as the FSM's unified `Work` node.
fn work_node(variant: &str, inner: Value) -> Value {
    Value::Enum {
        enum_name: "Work".to_string(),
        variant: variant.to_string(),
        fields: vec![inner],
    }
}

/// Pure-Rust head/arg slice, used only as a fallback if the Evident
/// `split_head` query fails. Equivalent extraction to the claim: head =
/// before the first `<`, arg = the balanced `<…>` body (last char is `>`,
/// guaranteed by [`is_generic_head`]).
fn rust_split_head(t: &str) -> (String, String) {
    match t.find('<') {
        Some(lt) if t.ends_with('>') => (t[..lt].to_string(), t[lt + 1..t.len() - 1].to_string()),
        _ => (t.to_string(), String::new()),
    }
}

// ─────────────────────────────────────────────────────────────────────
// Production entry point — a per-thread cached engine + bootstrap guard
// ─────────────────────────────────────────────────────────────────────

thread_local! {
    /// One [`EvidentGenerics`] engine per thread, built lazily on the first
    /// [`monomorphize_generics`] call. `EvidentRuntime` is `!Send`/`!Sync`
    /// (Z3 context, Cranelift module, `Rc`/`RefCell`), so a thread-local —
    /// not a global — is the right cache: load + JIT-compile the pass once
    /// per thread, reuse for every load.
    static ENGINE: RefCell<Option<Rc<EvidentGenerics>>> =
        const { RefCell::new(None) };

    /// Re-entrancy guard. Set while the engine's private runtime is loading
    /// `generics.ev`: that load runs the production monomorphize hook over
    /// the pass's own schemas, re-entering here mid-build (and re-borrowing
    /// [`ENGINE`]). While set, [`monomorphize_generics`] is a no-op — the
    /// pass file declares cons-list AST enums and helper claims but uses no
    /// `<…>` generics, so leaving its schema map unchanged is correct.
    static BOOTSTRAPPING: Cell<bool> = const { Cell::new(false) };
}

/// Monomorphize generic instantiations via the self-hosted Evident
/// `generics.ev` pass. **This is the runtime's sole monomorphization entry
/// point** — `runtime/src/runtime/load.rs` calls it after each schema batch.
///
/// Builds and caches a per-thread [`EvidentGenerics`] engine on first use
/// (see [`ENGINE`]). The engine locates `stdlib/` via the one
/// [`crate::stdlib_path::stdlib_dir`] resolver (session WW).
///
/// # Panics
///
/// If `stdlib/passes/generics.ev` cannot be located or loaded. There is no
/// Rust-pass fallback (this session) — an unloadable pass is a hard error,
/// the same robust resolution the rest of the runtime relies on.
pub fn monomorphize_generics(
    schemas: &mut HashMap<String, SchemaDecl>,
    schema_order: &mut Vec<String>,
) -> Result<(), RuntimeError> {
    // Re-entrancy break: while building the engine (loading the pass), skip
    // monomorphization — see [`BOOTSTRAPPING`].
    if BOOTSTRAPPING.with(|b| b.get()) {
        return Ok(());
    }
    // Presence gate: no generic use anywhere → guaranteed no-op. Skip the
    // engine build + Evident walk entirely so non-generic programs load at
    // the Rust baseline. See [`program_has_generic_use`].
    if !program_has_generic_use(schemas) {
        return Ok(());
    }
    let engine = ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            BOOTSTRAPPING.with(|b| b.set(true));
            let built = build_engine();
            BOOTSTRAPPING.with(|b| b.set(false));
            *slot = Some(Rc::new(built));
        }
        slot.as_ref().unwrap().clone()
    });
    engine.monomorphize(schemas, schema_order)
}

/// Locate `stdlib/` and load the generics pass into a fresh engine. Panics
/// with the resolver's path-list diagnostic on failure.
fn build_engine() -> EvidentGenerics {
    let dir = crate::stdlib_path::stdlib_dir().unwrap_or_else(|e| panic!(
        "generics: cannot locate stdlib to load the monomorphization pass \
         (the sole impl since session REVIVE-generics): {e}"));
    EvidentGenerics::new(&dir).unwrap_or_else(|e| panic!(
        "generics: failed to load passes/generics.ev from {}: {e}",
        dir.display()))
}
