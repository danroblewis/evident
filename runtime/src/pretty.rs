//! Stable diagnostic-render entry points (`crate::pretty::expr` / `body_item`).
//! Rendering is self-hosted: both delegate to the `pretty_walk` stack-FSM in
//! `stdlib/passes/pretty.ev`, driven through [`EvidentPretty`]. The FSM is
//! built once per thread and reused (each render is a `run()` over the AST).

use std::cell::Cell;
use std::rc::Rc;

use crate::core::ast::{BodyItem, Expr};
use crate::portable::pretty::{EvidentPretty, PrettyImpl};

thread_local! {
    static PRETTY: std::cell::RefCell<Option<Rc<EvidentPretty>>> =
        const { std::cell::RefCell::new(None) };
    // Re-entrancy guard: a render is a Z3 solve, and a diagnostic raised
    // *inside* that solve must not recurse back into the renderer.
    static IN_FLIGHT: Cell<bool> = const { Cell::new(false) };
}

/// Render via the cached `pretty_walk` FSM, or fall back to `dbg` when the
/// pass is unavailable (no stdlib) or a render is already in flight. The
/// fallback is effectively unreachable — `pretty_walk` translates cleanly, so
/// its solve raises no diagnostics — but keeps diagnostics total + recursion-free.
fn with_pretty(f: impl FnOnce(&EvidentPretty) -> String, dbg: impl FnOnce() -> String) -> String {
    if IN_FLIGHT.with(|c| c.get()) {
        return dbg();
    }
    IN_FLIGHT.with(|c| c.set(true));
    let pretty = PRETTY.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = crate::stdlib_path::stdlib_dir().ok()
                .and_then(|dir| EvidentPretty::new(&dir).ok())
                .map(Rc::new);
        }
        slot.clone()
    });
    let out = match &pretty {
        Some(p) => f(p),
        None => dbg(),
    };
    IN_FLIGHT.with(|c| c.set(false));
    out
}

/// Render an expression to its readable infix form.
pub fn expr(e: &Expr) -> String {
    with_pretty(|p| p.expr(e), || format!("{e:?}"))
}

/// Render a single schema body item.
pub fn body_item(item: &BodyItem) -> String {
    with_pretty(|p| p.body_item(item), || format!("{item:?}"))
}
