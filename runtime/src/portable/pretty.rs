//! AST → readable-infix string (UNSAT diagnostics). Impls: [`RustPretty`] (canonical) and
//! [`EvidentPretty`] (drives `pretty_walk` FSM in `stdlib/passes/pretty.ev`).

use std::path::Path;

use crate::core::ast::{BinOp, BodyItem, Expr, Mapping, MatchArm, MatchPattern};
use crate::core::Value;
use crate::translate::ast_encoder::{body_item_to_value, expr_to_value};
use super::{work_node, EvidentRunner, Portable};

/// `pretty`'s Rust-level signature, mirrors the two public functions `pretty.rs` exposed.
pub trait PrettyImpl: Portable {
    /// Render an expression to its readable infix form.
    fn expr(&self, e: &Expr) -> String;
    /// Render a single schema body item.
    fn body_item(&self, item: &BodyItem) -> String;
}

/// The native renderer — total, fast, always correct; the default.
pub struct RustPretty;

impl Portable for RustPretty {
    fn impl_name(&self) -> &'static str { "rust" }
}

impl PrettyImpl for RustPretty {
    fn expr(&self, e: &Expr) -> String { render_expr(e) }
    fn body_item(&self, item: &BodyItem) -> String { render_body_item(item) }
}

/// Format a quantifier binding: single name → `x`, tuple → `(a, b, c)`.
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
            // Wrap Binary operands in parens — cheap, slightly noisy, never wrong.
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

/// Renders via the `pretty_walk` stack-FSM in `stdlib/passes/pretty.ev`.
/// Build once and reuse — per-tick solve is JIT-cached across calls.
pub struct EvidentPretty {
    runner: EvidentRunner,
}

impl EvidentPretty {
    /// Load `passes/pretty.ev`; do NOT also load `ast.ev` — duplicate enum decls would clash.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        Ok(Self { runner: EvidentRunner::load_from(stdlib_dir, "passes/pretty.ev", "pretty_walk")? })
    }

    /// Drive `pretty_walk` with a `PWork` seed; extract the String from `PDone(out)`.
    fn render(&self, seed: Value) -> String {
        match self.runner.run(seed) {
            Ok(Value::Enum { variant, fields, .. }) if variant == "PDone" && fields.len() == 1 => {
                match &fields[0] {
                    Value::Str(s) => s.clone(),
                    other => format!("<pretty-bad-out: {other:?}>"),
                }
            }
            Ok(other) => format!("<pretty-not-done: {other:?}>"),
            Err(e) => format!("<pretty-error: {e}>"),
        }
    }
}

impl Portable for EvidentPretty {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl PrettyImpl for EvidentPretty {
    fn expr(&self, e: &Expr) -> String {
        self.render(work_node("PWork", "WExpr", expr_to_value(e)))
    }

    fn body_item(&self, item: &BodyItem) -> String {
        self.render(work_node("PWork", "WBody", body_item_to_value(item)))
    }
}

/// Select impl via `EVIDENT_PRETTY_IMPL` (`rust` | `evident`); falls back to Rust on error.
pub fn default_impl() -> Box<dyn PrettyImpl> {
    if std::env::var("EVIDENT_PRETTY_IMPL").as_deref() == Ok("evident") {
        if let Ok(dir) = crate::stdlib_path::stdlib_dir() {
            if let Ok(ev) = EvidentPretty::new(&dir) {
                return Box::new(ev);
            }
        }
    }
    Box::new(RustPretty)
}
