//! Thin re-export shim. Rendering logic lives in `portable::pretty::RustPretty`;
//! these free functions keep call sites (`crate::pretty::expr` / `body_item`) stable.

use crate::core::ast::{BodyItem, Expr};
use crate::portable::pretty::{PrettyImpl, RustPretty};

/// Render an expression to its readable infix form.
pub fn expr(e: &Expr) -> String {
    RustPretty.expr(e)
}

/// Render a single schema body item.
pub fn body_item(item: &BodyItem) -> String {
    RustPretty.body_item(item)
}
