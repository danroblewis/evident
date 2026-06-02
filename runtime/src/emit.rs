//! `evident emit` — translate an Evident claim to SMT-LIB the kernel can run.
//!
//! Pipeline:
//!   1. Load the source file (via the existing runtime).
//!   2. Locate the claim by name.
//!   3. Validate single-SeqLit-producer for `effects` (per
//!      docs/plans/kernel-input-spec.md).
//!   4. Build a Z3 solver via `build_cache` (asserts all body constraints,
//!      does NOT call `check()`).
//!   5. Serialize via `Z3_solver_to_string`.
//!   6. Walk the env to discover `state.*` flat fields.
//!   7. Prepend the manifest header.
//!
//! Output is one `.smt2` text blob conforming to the kernel input spec.

use std::collections::HashMap;
use std::ffi::CStr;

use crate::core::ast::{BinOp, BodyItem, Expr, SchemaDecl};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::translate::build_cache;

/// Default upper bound on `#effects` per tick (per spec).
const DEFAULT_MAX_EFFECTS: usize = 16;

#[derive(Debug)]
pub enum EmitError {
    UnknownClaim(String),
    /// effects has 0 or >1 SeqLit-shaped equality constraints.
    EffectsWriterCount { claim: String, count: usize },
    /// effects has an equality constraint but the RHS isn't a SeqLit (after `++` flattening).
    EffectsNotSeqLit { claim: String },
}

impl std::fmt::Display for EmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmitError::UnknownClaim(n) =>
                write!(f, "unknown claim `{n}`"),
            EmitError::EffectsWriterCount { claim, count } =>
                write!(f, "schema `{claim}`: `effects` has {count} SeqLit-equality \
                          constraints; exactly 1 required (sub-FSMs should write to \
                          per-concern Seq names and an assembly FSM should write \
                          `effects = ... ++ ...`)"),
            EmitError::EffectsNotSeqLit { claim } =>
                write!(f, "schema `{claim}`: `effects` is constrained but the RHS \
                          isn't a Seq literal after `++` flattening. The kernel \
                          requires a fully-pinned SeqLit."),
        }
    }
}

impl std::error::Error for EmitError {}

/// Top-level entry point. Returns the SMT-LIB blob (manifest + body asserts)
/// or an `EmitError`.
pub fn emit_kernel_smtlib(rt: &EvidentRuntime, claim_name: &str) -> Result<String, EmitError> {
    let schema = rt.get_schema(claim_name)
        .ok_or_else(|| EmitError::UnknownClaim(claim_name.to_string()))?;

    validate_effects_single_writer(schema)?;

    // Inject `last_results ∈ Seq(Result)` if the schema doesn't already
    // declare it. This forces the runtime to declare the Result datatype
    // and the last_results Array in the SMT-LIB output, so the kernel can
    // assert on them across ticks. Without this, programs that don't
    // explicitly reference `last_results` would lack the declarations.
    let mut schema_owned: SchemaDecl;
    let schema = if has_last_results_decl(schema) {
        schema
    } else {
        schema_owned = schema.clone();
        schema_owned.body.push(BodyItem::Membership {
            name: "last_results".to_string(),
            type_name: "Seq(Result)".to_string(),
            pins: crate::core::ast::Pins::None,
        });
        &schema_owned
    };

    // build_cache asserts every body constraint into a Z3 solver without
    // calling check(). The solver state is exactly what we want to serialize.
    let arith_solver: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(2);
    let given: HashMap<String, Value> = HashMap::new();
    let cached = build_cache(
        schema, rt.schemas_map(), rt.z3_context(), rt.datatypes_registry(),
        Some(rt.enums_registry()), &given, arith_solver,
    );

    let body_smt = solver_to_smtlib(&cached.solver);
    let body_smt = dedupe_datatype_accessors(&body_smt);
    let body_smt = ensure_is_first_tick_decl(&body_smt);
    let state_fields = discover_state_fields(&cached.env);

    // Decls (`_<name>` per state field) go at the top so subsequent
    // asserts that reference them parse cleanly.
    let prev_state_decls: String = state_fields.iter()
        .map(|(n, t)| format!("(declare-fun _{n} () {t})\n"))
        .collect();

    // Z3's solver-to-string elides datatypes / vars that aren't
    // constrained. The Result enum + last_results array fall into that
    // bucket in most programs (they're only USED on tick N+1 by the
    // kernel's assertions). Hand-write them in the prelude.
    let result_and_last_results = result_and_last_results_decls();

    // For literal-SeqLit bindings we add an explicit `effects__len = N`
    // assert (the runtime folds the SeqLit length to a literal during
    // translation, leaving `effects__len` free). For ternary / dynamic
    // bindings, Z3 derives the length from the body asserts naturally.
    let effects_len = effects_seqlit_length(schema);
    let trailing_asserts = match effects_len {
        Some(n) => format!("(assert (= effects__len {n}))\n"),
        None    => String::new(),
    };

    let manifest = build_manifest(&state_fields, DEFAULT_MAX_EFFECTS);

    Ok(format!("{manifest}\n{prev_state_decls}{result_and_last_results}{body_smt}{trailing_asserts}"))
}

/// Hand-written Result datatype + last_results array decl. Mirrors the
/// shape stdlib/kernel.ev defines for `enum Result`.
///
/// We always emit these, even for programs that don't touch last_results,
/// because the kernel always wires it through (asserting length 0 on
/// tick 0 and the prior tick's results on subsequent ticks). The runtime's
/// auto-generated decls would skip them when unconstrained, hence this
/// hand-write.
fn result_and_last_results_decls() -> String {
    "(declare-datatypes ((Result 0)) ((\
      (NoResult) \
      (IntResult (IntResult__f0 Int)) \
      (StringResult (StringResult__f0 String)) \
      (RealResult (RealResult__f0 Real)) \
      (EofResult) \
      (ErrorResult (ErrorResult__f0 String)))))\n\
     (declare-fun last_results () (Array Int Result))\n".to_string()
}

/// Pull the integer length of the SeqLit constraint on `effects`. The validator
/// already ensured there's exactly one such constraint.
fn effects_seqlit_length(schema: &SchemaDecl) -> Option<usize> {
    for item in &schema.body {
        if let BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, rhs)) = item {
            if let Expr::Identifier(name) = lhs.as_ref() {
                if name == "effects" {
                    if let Expr::SeqLit(items) = rhs.as_ref() {
                        return Some(items.len());
                    }
                }
            }
        }
    }
    None
}

fn has_last_results_decl(schema: &SchemaDecl) -> bool {
    schema.body.iter().any(|item| matches!(
        item, BodyItem::Membership { name, .. } if name == "last_results"
    ))
}

/// Z3's solver-to-string emits accessors like `(f0 Int)` repeated across variants
/// of one datatype. Z3 itself tolerates that internally, but `Z3_parse_smtlib2_string`
/// rejects it ("repeated accessor identifier"). We post-process each
/// `(declare-datatypes ...)` block to prefix accessor names with their variant.
///
/// Input:  `((Foo (f0 Int) (f1 String)) (Bar (f0 Bool)))`
/// Output: `((Foo (Foo__f0 Int) (Foo__f1 String)) (Bar (Bar__f0 Bool)))`
///
/// Accessors are NOT referenced by the body asserts (we use constructors only),
/// so renaming them is invisible to downstream consumers.
fn dedupe_datatype_accessors(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 256);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Look for `(declare-datatypes`.
        if let Some(end) = find_declare_datatypes(s, i) {
            // Rewrite this block; copy through up to `i`, then process block.
            // `i` is currently the start of `(declare-datatypes` minus possibly some prefix.
            // We just append [i, dt_start) to out and process the block.
            let (dt_start, dt_end) = (
                s[i..].find("(declare-datatypes").map(|x| i + x).unwrap_or(i),
                end,
            );
            out.push_str(&s[i..dt_start]);
            let block = &s[dt_start..dt_end];
            out.push_str(&rewrite_datatype_accessors(block));
            i = dt_end;
        } else {
            out.push_str(&s[i..]);
            break;
        }
    }
    out
}

fn find_declare_datatypes(s: &str, from: usize) -> Option<usize> {
    let start = s[from..].find("(declare-datatypes")?;
    let abs_start = from + start;
    // Match parens.
    let mut depth = 0i32;
    let bytes = s.as_bytes();
    let mut j = abs_start;
    while j < bytes.len() {
        let c = bytes[j] as char;
        if c == '(' { depth += 1; }
        else if c == ')' {
            depth -= 1;
            if depth == 0 { return Some(j + 1); }
        }
        j += 1;
    }
    None
}

/// Rewrite accessor names inside one `(declare-datatypes ...)` block so they
/// are globally unique within the datatype.
///
/// Z3 emits accessors like `(f0 Int)` repeated across variants. We rename
/// them to `<variant>__<orig>` so `Z3_parse_smtlib2_string` accepts them.
///
/// Approach: parenthesis-balanced scan with paren-stack. When we see
/// `(<Ident> (`, we treat the Ident as a *variant* name and rename each
/// subsequent `(<accessor> <sort>)` pair at the next inner depth.
fn rewrite_datatype_accessors(block: &str) -> String {
    // Tokenize the block into raw s-exp tokens (paren or atom).
    let toks = sexpr_tokens(block);
    let (tree, _) = parse_sexpr(&toks, 0);
    rewrite_tree(&tree)
}

#[derive(Debug, Clone)]
enum Sexpr { Atom(String), List(Vec<Sexpr>) }

fn sexpr_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' | ')' => {
                if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
                cur.clear();
                out.push(c.to_string());
            }
            ' ' | '\t' | '\n' | '\r' => {
                if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() { out.push(cur.trim().to_string()); }
    out
}

fn parse_sexpr(toks: &[String], start: usize) -> (Sexpr, usize) {
    if toks[start] != "(" {
        return (Sexpr::Atom(toks[start].clone()), start + 1);
    }
    let mut i = start + 1;
    let mut items: Vec<Sexpr> = Vec::new();
    while i < toks.len() && toks[i] != ")" {
        let (e, next) = parse_sexpr(toks, i);
        items.push(e);
        i = next;
    }
    (Sexpr::List(items), i + 1)
}

fn rewrite_tree(e: &Sexpr) -> String {
    let mut out = String::new();
    let new_tree = rename_in_dt(e.clone());
    emit_sexpr(&new_tree, &mut out);
    out
}

/// For a `(declare-datatypes ((Name 0)...) ( ((Variant1 (acc T) ...) (Variant2 ...)) ... ))`
/// form, walk into the variants and rename accessors. Other forms passed
/// through. The input here IS the outer `(declare-datatypes ...)` list.
fn rename_in_dt(e: Sexpr) -> Sexpr {
    match e {
        Sexpr::Atom(_) => e,
        Sexpr::List(items) => {
            // Match (declare-datatypes <sortdecls> <vardecls>)
            if items.len() == 3 {
                if let Sexpr::Atom(head) = &items[0] {
                    if head == "declare-datatypes" {
                        // items[2] is a list of per-sort variant blocks.
                        // Each variant block is a list of variant decls.
                        let var_blocks = match &items[2] {
                            Sexpr::List(xs) => xs.clone(),
                            _ => return Sexpr::List(items),
                        };
                        let renamed: Vec<Sexpr> = var_blocks.into_iter()
                            .map(rename_variant_block).collect();
                        return Sexpr::List(vec![
                            items[0].clone(),
                            items[1].clone(),
                            Sexpr::List(renamed),
                        ]);
                    }
                }
            }
            // Generic recursion for any other list.
            Sexpr::List(items.into_iter().map(rename_in_dt).collect())
        }
    }
}

/// A variant block is `( (Var1 (acc T)...) (Var2 ...) ... )`. Rename accessors.
fn rename_variant_block(e: Sexpr) -> Sexpr {
    let Sexpr::List(variants) = e else { return e };
    let renamed: Vec<Sexpr> = variants.into_iter().map(rename_one_variant).collect();
    Sexpr::List(renamed)
}

/// A single variant is `(Variant (acc1 T1) (acc2 T2) ...)`. Prefix each
/// accessor with `Variant__`.
fn rename_one_variant(e: Sexpr) -> Sexpr {
    let Sexpr::List(parts) = e else { return e };
    let mut out: Vec<Sexpr> = Vec::with_capacity(parts.len());
    let mut variant_name: Option<String> = None;
    for (i, item) in parts.into_iter().enumerate() {
        if i == 0 {
            if let Sexpr::Atom(name) = &item {
                variant_name = Some(name.clone());
            }
            out.push(item);
            continue;
        }
        if let (Some(vname), Sexpr::List(pair)) = (&variant_name, &item) {
            // Pair should be (accessor sort) — rename accessor.
            if pair.len() >= 1 {
                if let Sexpr::Atom(acc) = &pair[0] {
                    let mut new_pair = pair.clone();
                    new_pair[0] = Sexpr::Atom(format!("{vname}__{acc}"));
                    out.push(Sexpr::List(new_pair));
                    continue;
                }
            }
        }
        out.push(item);
    }
    Sexpr::List(out)
}

fn emit_sexpr(e: &Sexpr, out: &mut String) {
    match e {
        Sexpr::Atom(s) => out.push_str(s),
        Sexpr::List(items) => {
            out.push('(');
            for (i, item) in items.iter().enumerate() {
                if i > 0 { out.push(' '); }
                emit_sexpr(item, out);
            }
            out.push(')');
        }
    }
}

/// Ensure `is_first_tick` is declared at the top of the SMT-LIB. The runtime
/// auto-injects it as a free Bool only when an FSM body references `_var`;
/// if not, the kernel's "always assert is_first_tick" approach would fail.
/// Idempotent: only adds the decl if not already present.
fn ensure_is_first_tick_decl(s: &str) -> String {
    if s.contains("is_first_tick") { return s.to_string(); }
    format!("(declare-fun is_first_tick () Bool)\n{s}")
}

// ── Validation: single SeqLit-producer for `effects` ───────────────

/// Count constraints binding `effects`. Single-writer enforcement: at most
/// one UNCONDITIONAL `effects = <expr>` constraint. Constraints guarded by
/// `cond ⇒ effects = <expr>` are skipped — they're per-tick dispatch and
/// the user is responsible for keeping the guards mutually exclusive.
///
/// Multi-writer conjunction (unconditional `effects = ⟨a⟩ ∧ effects = ⟨b⟩`)
/// is still rejected as a UNSAT trap.
fn validate_effects_single_writer(schema: &SchemaDecl) -> Result<(), EmitError> {
    let mut unguarded = 0;
    let mut any_writer = 0;
    for item in &schema.body {
        if let BodyItem::Constraint(e) = item {
            if let Expr::Binary(BinOp::Eq, lhs, _) = e {
                if let Expr::Identifier(name) = lhs.as_ref() {
                    if name == "effects" {
                        unguarded += 1;
                        any_writer += 1;
                    }
                }
            } else if let Expr::Binary(BinOp::Implies, _, consequent) = e {
                if let Expr::Binary(BinOp::Eq, lhs, _) = consequent.as_ref() {
                    if let Expr::Identifier(name) = lhs.as_ref() {
                        if name == "effects" {
                            any_writer += 1;
                        }
                    }
                }
            }
        }
    }
    if unguarded > 1 || any_writer == 0 {
        return Err(EmitError::EffectsWriterCount {
            claim: schema.name.clone(),
            count: any_writer,
        });
    }
    Ok(())
}

// ── Solver → SMT-LIB string ────────────────────────────────────────

/// Calls raw `Z3_solver_to_string` via z3-sys. The `z3` crate's `Solver`
/// doesn't expose a `to_smt2` method, so we drop to the C API.
fn solver_to_smtlib(solver: &z3::Solver<'_>) -> String {
    use z3_sys::*;
    unsafe {
        let ctx_ptr = solver_context_ptr(solver);
        let solver_ptr = solver_inner_ptr(solver);
        let cstr_ptr = Z3_solver_to_string(ctx_ptr, solver_ptr);
        if cstr_ptr.is_null() {
            return String::from(";; <Z3_solver_to_string returned null>\n");
        }
        CStr::from_ptr(cstr_ptr).to_string_lossy().into_owned()
    }
}

/// Reach into the `z3` crate's `Solver` to get the raw `Z3_solver`.
/// The crate doesn't expose this directly, so we transmute via a layout-
/// compatible shim. This is fragile but matches what `runtime/src/z3_ctx.rs`
/// already does for `Context` access.
unsafe fn solver_inner_ptr(solver: &z3::Solver<'_>) -> z3_sys::Z3_solver {
    // The `z3::Solver` struct layout (z3-0.12.1): { ctx: &Context, z3_slv: Z3_solver }.
    // We rely on this layout matching the shim below. If z3 crate changes layout,
    // this breaks — caught immediately by a panic or segv when serializing.
    #[repr(C)]
    struct SolverShim<'ctx> {
        _ctx: &'ctx z3::Context,
        z3_slv: z3_sys::Z3_solver,
    }
    let shim: &SolverShim = std::mem::transmute(solver);
    shim.z3_slv
}

/// Same shim trick for getting `Z3_context` out of a `Solver`'s context.
unsafe fn solver_context_ptr(solver: &z3::Solver<'_>) -> z3_sys::Z3_context {
    #[repr(C)]
    struct ContextShim {
        z3_ctx: z3_sys::Z3_context,
    }
    #[repr(C)]
    struct SolverShim<'ctx> {
        ctx: &'ctx ContextShim,
        _z3_slv: z3_sys::Z3_solver,
    }
    let shim: &SolverShim = std::mem::transmute(solver);
    shim.ctx.z3_ctx
}

// ── State-field discovery from env ─────────────────────────────────

/// State fields are every top-level claim-body Membership EXCEPT the two
/// reserved names `effects` and `last_results`. The runtime stores them in
/// `env` keyed by the source identifier; we emit them in the manifest
/// verbatim (no `state.` prefix — the spec calls them "state fields" by
/// role, not by name).
fn discover_state_fields(env: &HashMap<String, crate::core::Var<'static>>) -> Vec<(String, String)> {
    use crate::core::Var;
    let mut fields: Vec<(String, String)> = Vec::new();
    for (name, var) in env {
        if name == "effects" || name == "last_results" { continue; }
        // Skip dotted-prefix internal leaves and auto-injected helpers.
        if name.contains('.') { continue; }
        if name == "is_first_tick" { continue; }
        let ty = match var {
            Var::IntVar(_)     => "Int",
            Var::BoolVar(_)    => "Bool",
            Var::RealVar(_)    => "Real",
            Var::StrVar(_)     => "String",
            Var::PinnedInt(_)  => "Int",
            Var::EnumVar { enum_name, .. } => enum_name.as_str(),
            Var::SeqVar { .. } | Var::SetVar { .. } |
            Var::DatatypeSeqVar { .. } | Var::DatatypeSetVar { .. } => continue,
            Var::EnumValue { .. } | Var::EnumCtor { .. } => continue,
        };
        fields.push((name.clone(), ty.to_string()));
    }
    fields.sort_by(|a, b| a.0.cmp(&b.0));
    fields
}

// ── Manifest header ────────────────────────────────────────────────

fn build_manifest(state_fields: &[(String, String)], max_effects: usize) -> String {
    let fields_str = if state_fields.is_empty() {
        String::new()
    } else {
        state_fields.iter()
            .map(|(n, t)| format!("{n}:{t}"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    format!(
        ";; manifest: state-fields = {fields_str}\n\
         ;; manifest: effects-name = effects\n\
         ;; manifest: effect-enum-name = Effect\n\
         ;; manifest: result-enum-name = Result\n\
         ;; manifest: max-effects = {max_effects}\n"
    )
}
