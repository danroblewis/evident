//! `pretty` — the first port driven by the [`super`] swap interface.
//!
//! AST → readable-infix string (for UNSAT diagnostics). Two impls:
//!   * [`RustPretty`] — the canonical native renderer. The rendering
//!     logic that used to live in `runtime/src/pretty.rs` lives here now;
//!     `pretty.rs` is a thin re-export so existing callers
//!     (`crate::pretty::expr` / `body_item`) are unchanged.
//!   * [`EvidentPretty`] — drives the `pretty_walk` stack-FSM in
//!     `stdlib/passes/pretty.ev` through an owned [`EvidentRuntime`].
//!
//! ## `EvidentPretty` routes around the recursion gap (#15)
//!
//! Session X's port could render only flat/leaf shapes because a recursive
//! claim leaves its output unconstrained (COUNTEREXAMPLES #15). This shim
//! now drives `pretty.ev`'s **ordered stack-FSM** (`pretty_walk`) exactly
//! as `portable::subscriptions` drives `subscriptions_walk`: marshal the
//! input with the ONE SHARED marshaler (`translate::ast_encoder::{expr_to_value,
//! body_item_to_value}`), wrap it as the FSM's `PWork` seed, run it to a
//! drained-stack halt via [`crate::effect_loop::run_nested`], and read the
//! rendered String out of the `PDone(out)` final state. The recursion and
//! the string assembly both live in the pass — **no Rust tree walk, no
//! per-pass encoder**.
//!
//! ## What is byte-identical to `RustPretty`
//!
//! Every shape whose rendering is pure-ASCII, **recursively** — identifiers,
//! string literals, calls + their argument lists, set/tuple literals,
//! nested field/index, `#`, ternaries, `matches` (incl. its pattern),
//! `run(...)`, and binary operators with an ASCII symbol (`= < > + - * /
//! ++`) with the same Binary-operand parenthesization. The equivalence
//! test (`runtime/tests/pretty_equivalence.rs`) asserts identity on these.
//!
//! Two residuals still diverge and are pinned in the test as known
//! boundaries (see the pass file + `docs/self-hosting.md`):
//!   * **Unicode operator glyphs (#16)** — `∈ ↦ ∀ ⇒ ∧ ¬ ≤ ⟨⟩ …` mangle
//!     through Z3 byte-string handling, so glyph-bearing shapes diverge in
//!     exactly those bytes (the pass still walks their sub-exprs).
//!   * **Numbers / Bool (no int→string in a pass; JIT bool bug #17)** —
//!     `EInt` / `EReal` / `EBool` / `BIHaltsWithin`'s count render to an
//!     ASCII sentinel.

use std::path::Path;

use crate::core::ast::{BinOp, BodyItem, Expr, Mapping, MatchArm, MatchPattern};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::translate::ast_encoder::{body_item_to_value, expr_to_value};
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
// Evident impl — drives stdlib/passes/pretty.ev's pretty_walk stack-FSM
// ─────────────────────────────────────────────────────────────────────

/// Renders via the `pretty_walk` stack-FSM in `stdlib/passes/pretty.ev`.
/// Holds its own runtime with the pass loaded; build once and reuse so
/// the FSM's per-tick solve is JIT-cached across calls.
pub struct EvidentPretty {
    rt: EvidentRuntime,
}

impl EvidentPretty {
    /// The ordered AST→String stack-FSM in `stdlib/passes/pretty.ev`.
    const WALK_FSM: &'static str = "pretty_walk";

    /// Max-iteration guard for the nested walk. Each AST node costs a
    /// small constant number of FSM ticks (one per pushed work-item), so a
    /// tree of N nodes halts in O(N) ticks; the cap sits far above any
    /// realistic diagnostic so a legitimate render never hits it (overrun
    /// would be a pass bug, surfaced as a loud `MaxItersExceeded`).
    const MAX_STEPS: usize = 5_000_000;

    /// Load `passes/pretty.ev` from `stdlib_dir` into a fresh runtime.
    /// `stdlib_dir` is the repo's `stdlib/` directory. The pass is
    /// self-contained (it declares its own cons-list copy of the AST enums
    /// matching the shared marshaler), so no other stdlib file is needed —
    /// and loading `ast.ev` too would clash (duplicate enum decls).
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("passes").join("pretty.ev"))
            .map_err(|e| format!("load passes/pretty.ev: {e}"))?;
        Ok(Self { rt })
    }

    /// Drive `pretty_walk` over a seeded `PWork` node and return the
    /// rendered String from the `PDone(out)` final state.
    fn render(&self, seed: Value) -> String {
        match crate::effect_loop::run_nested(&self.rt, Self::WALK_FSM, seed, Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. })
                if variant == "PDone" && fields.len() == 1 =>
            {
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
        // Shared marshaler: ast.rs Expr → Value::Enum (cons-list shape),
        // wrapped as the FSM's unified work node `PWork::WExpr(Expr)`.
        self.render(work_node("WExpr", expr_to_value(e)))
    }

    fn body_item(&self, item: &BodyItem) -> String {
        // The pass owns the Constraint → Expr delegation now (its
        // `WBody(BIConstraint(e)) ⇒ WExpr(e)` arm mirrors pretty.rs), so
        // unlike the pre-recursion shim there is no Rust-side special case.
        self.render(work_node("WBody", body_item_to_value(item)))
    }
}

/// Wrap an already-marshaled AST `Value` as the FSM's unified `PWork` seed.
fn work_node(variant: &str, inner: Value) -> Value {
    Value::Enum {
        enum_name: "PWork".to_string(),
        variant: variant.to_string(),
        fields: vec![inner],
    }
}

// ─────────────────────────────────────────────────────────────────────
// Selection
// ─────────────────────────────────────────────────────────────────────

/// Pick an impl by `EVIDENT_PRETTY_IMPL` (`rust` | `evident`), defaulting
/// to the Rust impl. `evident` locates `stdlib/` via the one
/// [`crate::stdlib_path::stdlib_dir`] resolver (honoring `EVIDENT_STDLIB`
/// / `EVIDENT_STDLIB_DIR`); if locating or loading fails it falls back to
/// Rust.
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
