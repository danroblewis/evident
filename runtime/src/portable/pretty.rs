//! `pretty` — the first port driven by the [`super`] swap interface.
//!
//! AST → readable-infix string (for UNSAT diagnostics). Two impls:
//!   * [`RustPretty`] — the canonical native renderer. The rendering
//!     logic that used to live in `runtime/src/pretty.rs` lives here now;
//!     `pretty.rs` is a thin re-export so existing callers
//!     (`crate::pretty::expr` / `body_item`) are unchanged.
//!   * [`EvidentPretty`] — calls `stdlib/passes/pretty.ev` through an
//!     owned [`EvidentRuntime`].
//!
//! `EvidentPretty` is faithful (byte-identical to `RustPretty`) only on
//! the ASCII, non-recursive subset — see the pass file and
//! `docs/self-hosting.md`. Unsupported shapes return ASCII sentinels;
//! the equivalence test (`runtime/tests/pretty_equivalence.rs`) asserts
//! identity on exactly the faithful shapes.

use std::collections::HashMap;
use std::path::Path;

use crate::core::ast::{BinOp, BodyItem, Expr, Mapping, MatchArm, MatchPattern, Pins};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The trait
// ─────────────────────────────────────────────────────────────────────

/// `pretty`'s Rust-level signature, independent of which impl backs it.
/// Mirrors the two public functions the original `pretty.rs` exposed.
pub trait PrettyImpl: Portable {
    /// Render an expression to its readable infix form.
    fn expr(&self, e: &Expr) -> String;
    /// Render a single schema body item.
    fn body_item(&self, item: &BodyItem) -> String;
}

// ─────────────────────────────────────────────────────────────────────
// Rust impl — the canonical renderer
// ─────────────────────────────────────────────────────────────────────

/// The native renderer. Total, fast, always correct — the default.
pub struct RustPretty;

impl Portable for RustPretty {
    fn impl_name(&self) -> &'static str { "rust" }
}

impl PrettyImpl for RustPretty {
    fn expr(&self, e: &Expr) -> String { render_expr(e) }
    fn body_item(&self, item: &BodyItem) -> String { render_body_item(item) }
}

/// Render a quantifier binding: a single name as `x`, a tuple as `(a, b, c)`.
fn fmt_binding(vs: &[String]) -> String {
    if vs.len() == 1 { vs[0].clone() } else { format!("({})", vs.join(", ")) }
}

pub(crate) fn render_expr(e: &Expr) -> String {
    match e {
        Expr::Identifier(n) => n.clone(),
        Expr::Int(n)        => n.to_string(),
        Expr::Real(f)       => f.to_string(),
        Expr::Bool(b)       => b.to_string(),
        Expr::Str(s)        => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        Expr::SetLit(items) => format!("{{{}}}", items.iter().map(render_expr).collect::<Vec<_>>().join(", ")),
        Expr::SeqLit(items) => format!("⟨{}⟩",   items.iter().map(render_expr).collect::<Vec<_>>().join(", ")),
        Expr::Tuple(items)  => format!("({})",   items.iter().map(render_expr).collect::<Vec<_>>().join(", ")),
        Expr::Range(lo, hi) => format!("{{{}..{}}}", render_expr(lo), render_expr(hi)),
        Expr::InExpr(lhs, rhs) => format!("{} ∈ {}", render_expr(lhs), render_expr(rhs)),
        Expr::Forall(vs, range, body) =>
            format!("∀ {} ∈ {} : {}", fmt_binding(vs), render_expr(range), render_expr(body)),
        Expr::Exists(vs, range, body) =>
            format!("∃ {} ∈ {} : {}", fmt_binding(vs), render_expr(range), render_expr(body)),
        Expr::Call(name, args) =>
            format!("{}({})", name, args.iter().map(render_expr).collect::<Vec<_>>().join(", ")),
        Expr::Cardinality(inner) => format!("#{}", render_expr(inner)),
        Expr::Index(seq, idx)    => format!("{}[{}]", render_expr(seq), render_expr(idx)),
        Expr::Field(receiver, f) => format!("{}.{}", render_expr(receiver), f),
        Expr::Not(inner)         => format!("¬({})", render_expr(inner)),
        Expr::Ternary(c, a, b)   => format!("({} ? {} : {})", render_expr(c), render_expr(a), render_expr(b)),
        Expr::Matches(e, pat) => format!("({} matches {})", render_expr(e), fmt_pattern(pat)),
        Expr::RunFsm { fsm, init } => format!("run({}, {})", fsm, render_expr(init)),
        Expr::Match(scr, arms)   => {
            let arms_s: Vec<String> = arms.iter().map(|a: &MatchArm| {
                format!("{} ⇒ {}", fmt_pattern(&a.pattern), render_expr(&a.body))
            }).collect();
            format!("match {} {{ {} }}", render_expr(scr), arms_s.join(" | "))
        }
        Expr::Binary(op, lhs, rhs) => {
            let l = render_expr(lhs);
            let r = render_expr(rhs);
            // Wrap any Binary operand in parens — cheap, slightly noisy,
            // never wrong. A precedence-aware printer is overkill for
            // diagnostics.
            let l = if matches!(lhs.as_ref(), Expr::Binary(..)) { format!("({})", l) } else { l };
            let r = if matches!(rhs.as_ref(), Expr::Binary(..)) { format!("({})", r) } else { r };
            format!("{} {} {}", l, binop_sym(op), r)
        }
    }
}

fn fmt_pattern(pat: &MatchPattern) -> String {
    match pat {
        MatchPattern::Wildcard => "_".to_string(),
        MatchPattern::Bind(n) => n.clone(),
        MatchPattern::Ctor { name, binds } => {
            if binds.is_empty() { name.clone() }
            else {
                let bs: Vec<String> = binds.iter().map(fmt_pattern).collect();
                format!("{}({})", name, bs.join(", "))
            }
        }
    }
}

fn binop_sym(op: &BinOp) -> &'static str {
    match op {
        BinOp::Eq      => "=",
        BinOp::Neq     => "≠",
        BinOp::Lt      => "<",
        BinOp::Le      => "≤",
        BinOp::Gt      => ">",
        BinOp::Ge      => "≥",
        BinOp::And     => "∧",
        BinOp::Or      => "∨",
        BinOp::Implies => "⇒",
        BinOp::Add     => "+",
        BinOp::Sub     => "-",
        BinOp::Mul     => "*",
        BinOp::Div     => "/",
        BinOp::Concat  => "++",
    }
}

pub(crate) fn render_body_item(item: &BodyItem) -> String {
    match item {
        BodyItem::Membership { name, type_name, .. } => format!("{} ∈ {}", name, type_name),
        BodyItem::Passthrough(c) => format!("..{}", c),
        BodyItem::SubclaimDecl(s) => format!("subclaim {} (…)", s.name),
        BodyItem::ClaimCall { name, mappings } => {
            if mappings.is_empty() {
                name.clone()
            } else {
                format!("{} ({})", name, mappings.iter().map(mapping).collect::<Vec<_>>().join(", "))
            }
        }
        BodyItem::Constraint(e) => render_expr(e),
        BodyItem::HaltsWithin { fsm_name, n } => format!("halts_within({fsm_name}, {n})"),
    }
}

fn mapping(m: &Mapping) -> String {
    format!("{} ↦ {}", m.slot, render_expr(&m.value))
}

// ─────────────────────────────────────────────────────────────────────
// Evident impl — calls stdlib/passes/pretty.ev
// ─────────────────────────────────────────────────────────────────────

/// Renders via `stdlib/passes/pretty.ev`. Holds its own runtime with the
/// pass loaded; build once and reuse so the pass is compiled/cached
/// across calls.
pub struct EvidentPretty {
    rt: EvidentRuntime,
}

impl EvidentPretty {
    /// Pass-claim name for a `BodyItem`.
    const BODY_ITEM_CLAIM: &'static str = "Pretty";
    /// Pass-claim name for an `Expr`.
    const EXPR_CLAIM: &'static str = "PrettyExpr";

    /// Load `ast.ev` + `passes/pretty.ev` from `stdlib_dir` into a fresh
    /// runtime. `stdlib_dir` is the repo's `stdlib/` directory.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("ast.ev"))
            .map_err(|e| format!("load ast.ev: {e}"))?;
        rt.load_file(&stdlib_dir.join("passes").join("pretty.ev"))
            .map_err(|e| format!("load passes/pretty.ev: {e}"))?;
        Ok(Self { rt })
    }

    /// Run a single-output pass claim with `var` pinned to `val`, return
    /// the `out` String binding.
    fn render(&self, claim: &str, var: &str, val: Value) -> String {
        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert(var.to_string(), val);
        match self.rt.query(claim, &given) {
            Ok(qr) if qr.satisfied => match qr.bindings.get("out") {
                Some(Value::Str(s)) => s.clone(),
                other => format!("<pretty-bad-out: {other:?}>"),
            },
            Ok(_)  => "<pretty-unsat>".to_string(),
            Err(e) => format!("<pretty-error: {e}>"),
        }
    }
}

impl Portable for EvidentPretty {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl PrettyImpl for EvidentPretty {
    fn expr(&self, e: &Expr) -> String {
        self.render(Self::EXPR_CLAIM, "e", encode_expr(e))
    }

    fn body_item(&self, item: &BodyItem) -> String {
        // Mirror pretty.rs: a Constraint body item renders exactly as its
        // inner expression. The pass can't call PrettyExpr inline, so the
        // shim does the delegation (keeps each Evident claim flat).
        match item {
            BodyItem::Constraint(e) => self.expr(e),
            _ => self.render(Self::BODY_ITEM_CLAIM, "item", encode_body_item(item)),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Marshaling: Rust AST → Value::Enum tree (matches stdlib/ast.ev)
// ─────────────────────────────────────────────────────────────────────
//
// A self-contained mirror of the private `*_to_value` family in
// translate/encode_ast.rs. Each list field becomes a named Cons/Nil
// enum, exactly as that module emits. Kept local so the port owns its
// marshaling; unify with encode_ast.rs once its surface is public.

fn ev(enum_name: &str, variant: &str, fields: Vec<Value>) -> Value {
    Value::Enum { enum_name: enum_name.to_string(), variant: variant.to_string(), fields }
}

pub(crate) fn encode_body_item(bi: &BodyItem) -> Value {
    match bi {
        BodyItem::Membership { name, type_name, pins } =>
            ev("BodyItem", "BIMembership",
               vec![Value::Str(name.clone()), Value::Str(type_name.clone()), encode_pins(pins)]),
        BodyItem::Passthrough(name) =>
            ev("BodyItem", "BIPassthrough", vec![Value::Str(name.clone())]),
        BodyItem::ClaimCall { name, mappings } =>
            ev("BodyItem", "BIClaimCall",
               vec![Value::Str(name.clone()), encode_mapping_list(mappings)]),
        BodyItem::Constraint(e) =>
            ev("BodyItem", "BIConstraint", vec![encode_expr(e)]),
        BodyItem::SubclaimDecl(s) =>
            ev("BodyItem", "BISubclaim", vec![encode_schema_decl(s)]),
        BodyItem::HaltsWithin { fsm_name, n } =>
            ev("BodyItem", "BIHaltsWithin",
               vec![Value::Str(fsm_name.clone()), Value::Int(*n)]),
    }
}

fn encode_schema_decl(s: &crate::core::ast::SchemaDecl) -> Value {
    ev("SchemaDecl", "MakeSchemaDecl",
       vec![encode_keyword(&s.keyword), Value::Str(s.name.clone()), encode_body_item_list(&s.body)])
}

fn encode_keyword(kw: &crate::core::ast::Keyword) -> Value {
    use crate::core::ast::Keyword::*;
    let v = match kw { Schema => "KSchema", Claim => "KClaim", Type => "KType", Subclaim => "KSubclaim", Fsm => "KFsm" };
    ev("Keyword", v, vec![])
}

fn encode_pins(p: &Pins) -> Value {
    match p {
        Pins::None => ev("Pins", "PNone", vec![]),
        Pins::Named(maps) => ev("Pins", "PNamed", vec![encode_mapping_list(maps)]),
        Pins::Positional(args) => ev("Pins", "PPositional", vec![encode_expr_list(args)]),
    }
}

fn encode_mapping(m: &Mapping) -> Value {
    ev("Mapping", "MakeMapping", vec![Value::Str(m.slot.clone()), encode_expr(&m.value)])
}

pub(crate) fn encode_expr(e: &Expr) -> Value {
    match e {
        Expr::Identifier(s) => ev("Expr", "EIdentifier", vec![Value::Str(s.clone())]),
        Expr::Int(n)        => ev("Expr", "EInt",        vec![Value::Int(*n)]),
        Expr::Real(f)       => ev("Expr", "EReal",       vec![Value::Real(*f)]),
        Expr::Bool(b)       => ev("Expr", "EBool",       vec![Value::Bool(*b)]),
        Expr::Str(s)        => ev("Expr", "EStr",        vec![Value::Str(s.clone())]),
        Expr::SetLit(items) => ev("Expr", "ESetLit",     vec![encode_expr_list(items)]),
        Expr::SeqLit(items) => ev("Expr", "ESeqLit",     vec![encode_expr_list(items)]),
        Expr::Tuple(items)  => ev("Expr", "ETuple",      vec![encode_expr_list(items)]),
        Expr::Range(lo, hi) => ev("Expr", "ERange",      vec![encode_expr(lo), encode_expr(hi)]),
        Expr::InExpr(l, r)  => ev("Expr", "EInExpr",     vec![encode_expr(l), encode_expr(r)]),
        Expr::Forall(vs, range, body) =>
            ev("Expr", "EForall", vec![encode_string_list(vs), encode_expr(range), encode_expr(body)]),
        Expr::Exists(vs, range, body) =>
            ev("Expr", "EExists", vec![encode_string_list(vs), encode_expr(range), encode_expr(body)]),
        Expr::Call(name, args) =>
            ev("Expr", "ECall", vec![Value::Str(name.clone()), encode_expr_list(args)]),
        Expr::Cardinality(inner) => ev("Expr", "ECardinality", vec![encode_expr(inner)]),
        Expr::Index(seq, idx)    => ev("Expr", "EIndex", vec![encode_expr(seq), encode_expr(idx)]),
        Expr::Field(base, name)  => ev("Expr", "EField", vec![encode_expr(base), Value::Str(name.clone())]),
        Expr::Binary(op, l, r)   => ev("Expr", "EBinary", vec![encode_binop(op), encode_expr(l), encode_expr(r)]),
        Expr::Not(inner)         => ev("Expr", "ENot", vec![encode_expr(inner)]),
        Expr::Ternary(c, a, b)   => ev("Expr", "ETernary", vec![encode_expr(c), encode_expr(a), encode_expr(b)]),
        Expr::Match(scr, arms)   => ev("Expr", "EMatch", vec![encode_expr(scr), encode_match_arm_list(arms)]),
        Expr::Matches(e, pat)    => ev("Expr", "EMatches", vec![encode_expr(e), encode_match_pattern(pat)]),
        Expr::RunFsm { fsm, init } => ev("Expr", "ERunFsm", vec![Value::Str(fsm.clone()), encode_expr(init)]),
    }
}

fn encode_binop(op: &BinOp) -> Value {
    let v = match op {
        BinOp::Eq => "OpEq", BinOp::Neq => "OpNeq", BinOp::Lt => "OpLt", BinOp::Le => "OpLe",
        BinOp::Gt => "OpGt", BinOp::Ge => "OpGe", BinOp::And => "OpAnd", BinOp::Or => "OpOr",
        BinOp::Implies => "OpImplies", BinOp::Add => "OpAdd", BinOp::Sub => "OpSub",
        BinOp::Mul => "OpMul", BinOp::Div => "OpDiv", BinOp::Concat => "OpConcat",
    };
    ev("BinOp", v, vec![])
}

fn encode_match_arm(a: &MatchArm) -> Value {
    ev("MatchArm", "MakeMatchArm", vec![encode_match_pattern(&a.pattern), encode_expr(&a.body)])
}

fn encode_match_pattern(p: &MatchPattern) -> Value {
    match p {
        // A top-level bind has no `PatBind` in stdlib/ast.ev; the
        // self-hosting corpus never produces one. Treat as wildcard.
        MatchPattern::Wildcard | MatchPattern::Bind(_) =>
            ev("MatchPattern", "PatWildcard", vec![]),
        MatchPattern::Ctor { name, binds } =>
            ev("MatchPattern", "PatCtor", vec![Value::Str(name.clone()), encode_bind_list(binds)]),
    }
}

// ── list builders → named Cons/Nil enums ──

fn cons_list(enum_name: &str, cons: &str, nil: &str, items: impl DoubleEndedIterator<Item = Value>) -> Value {
    let mut acc = ev(enum_name, nil, vec![]);
    for head in items.rev() {
        acc = ev(enum_name, cons, vec![head, acc]);
    }
    acc
}

fn encode_body_item_list(items: &[BodyItem]) -> Value {
    cons_list("BodyItemList", "BILCons", "BILNil", items.iter().map(encode_body_item))
}
fn encode_mapping_list(items: &[Mapping]) -> Value {
    cons_list("MappingList", "MLCons", "MLNil", items.iter().map(encode_mapping))
}
fn encode_expr_list(items: &[Expr]) -> Value {
    cons_list("ExprList", "ELCons", "ELNil", items.iter().map(encode_expr))
}
fn encode_string_list(items: &[String]) -> Value {
    cons_list("StringList", "SLCons", "SLNil", items.iter().map(|s| Value::Str(s.clone())))
}
fn encode_match_arm_list(items: &[MatchArm]) -> Value {
    cons_list("MatchArmList", "MALCons", "MALNil", items.iter().map(encode_match_arm))
}
fn encode_bind_list(binds: &[MatchPattern]) -> Value {
    cons_list("BindList", "BLCons", "BLNil", binds.iter().map(|b| match b {
        MatchPattern::Bind(n) => ev("MatchBind", "BindName", vec![Value::Str(n.clone())]),
        // Wildcard, and (lossily) any nested constructor sub-pattern,
        // which the flat `MatchBind` can't represent — never hit by the
        // self-hosting corpus.
        _ => ev("MatchBind", "BindWildcard", vec![]),
    }))
}

// ─────────────────────────────────────────────────────────────────────
// Selection
// ─────────────────────────────────────────────────────────────────────

/// Pick an impl by `EVIDENT_PRETTY_IMPL` (`rust` | `evident`), defaulting
/// to the Rust impl. `evident` requires `EVIDENT_STDLIB_DIR` to point at
/// the repo `stdlib/`; if loading fails it falls back to Rust.
pub fn default_impl() -> Box<dyn PrettyImpl> {
    if std::env::var("EVIDENT_PRETTY_IMPL").as_deref() == Ok("evident") {
        if let Ok(dir) = std::env::var("EVIDENT_STDLIB_DIR") {
            if let Ok(ev) = EvidentPretty::new(Path::new(&dir)) {
                return Box::new(ev);
            }
        }
    }
    Box::new(RustPretty)
}
