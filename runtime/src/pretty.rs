//! AST → readable-infix string. Used for diagnostics on UNSAT (so the
//! user sees `state.dots[i].pos_x = state.dots[i].pos_x` instead of a
//! deeply-nested `Binary(Eq, Field(Index(Identifier("state.dots"),
//! Identifier("i")), "pos_x"), …)` tree).
//!
//! Not a precise round-trip pretty-printer — operator spacing matches
//! source style and Unicode operators (∈, ∀, ⇒, …) are restored, but
//! nothing here is parsed back. If a future feature needs accurate
//! re-parse, write a separate one.
//!
//! This module is now a **thin wrapper** over the swap interface in
//! `runtime/src/portable/pretty.rs` — `pretty` is the first transform
//! ported to the portable Rust⇄Evident pattern (see
//! `docs/self-hosting.md`). The native rendering logic lives in
//! `portable::pretty::RustPretty`; these free functions keep the
//! original call sites (`crate::pretty::expr` / `body_item`) working
//! against the default (Rust) impl.

use crate::core::ast::{BodyItem, Expr};
use crate::portable::pretty::{PrettyImpl, RustPretty};

/// Render an expression to its readable infix form (Rust impl).
pub fn expr(e: &Expr) -> String {
    RustPretty.expr(e)
}

/// Render a single schema body item (Rust impl).
pub fn body_item(item: &BodyItem) -> String {
    RustPretty.body_item(item)
}
