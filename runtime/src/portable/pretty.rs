//! AST → readable-infix string (UNSAT diagnostics), self-hosted: [`EvidentPretty`]
//! drives the `pretty_walk` stack-FSM in `stdlib/passes/pretty.ev`. This is the
//! sole renderer — the former native `RustPretty` was retired once `pretty.ev`
//! became byte-faithful (session pretty-evident closed the int→string and
//! Unicode-glyph #16 gaps; the lone residual is `EReal`, see the pass header).

use std::path::Path;

use crate::core::ast::{BodyItem, Expr};
use crate::core::Value;
use crate::translate::ast_encoder::{body_item_to_value, expr_to_value};
use super::{work_node, EvidentRunner, Portable};

/// `pretty`'s Rust-level signature: the two public render entry points.
pub trait PrettyImpl: Portable {
    /// Render an expression to its readable infix form.
    fn expr(&self, e: &Expr) -> String;
    /// Render a single schema body item.
    fn body_item(&self, item: &BodyItem) -> String;
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
