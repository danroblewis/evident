//! Inlining-recursion bookkeeping: per-claim depth counter bounding self-passthrough expansion,
//! plus helper-local Z3-const isolation when entering a ClaimCall.

use std::collections::HashMap;

use crate::core::ast::*;
use crate::core::Var;

/// Depth cap: large enough for recursive AST walks, small enough to trip on runaway self-passthrough loops.
const DEFAULT_MAX_INLINE_DEPTH: usize = 64;

fn max_inline_depth() -> usize {
    std::env::var("EVIDENT_MAX_INLINE_DEPTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_INLINE_DEPTH)
}

/// Enter a frame for `name`; returns `Some(depth)` or `None` if cap exceeded.
/// `depth > 1` means recursive — callers must force fresh Z3 consts to avoid self-reference.
pub(super) fn try_enter(visited: &mut HashMap<String, usize>, name: &str) -> Option<usize> {
    let max = max_inline_depth();
    let cnt = visited.entry(name.to_string()).or_insert(0);
    if *cnt >= max {
        None
    } else {
        *cnt += 1;
        Some(*cnt)
    }
}

/// Pop a `try_enter` frame; removes the entry when count hits zero.
pub(super) fn exit_frame(visited: &mut HashMap<String, usize>, name: &str) {
    if let Some(cnt) = visited.get_mut(name) {
        *cnt -= 1;
        if *cnt == 0 { visited.remove(name); }
    }
}

/// Strip helper-internal locals from the cloned env on ClaimCall entry.
/// Prevents recursive invocations from sharing locals (e.g. `emit_ternary`'s `cnd`/`thn`/`els`), which collapses distinct AST values and causes UNSAT.
pub(super) fn isolate_helper_locals(
    body: &[BodyItem],
    inner: &mut HashMap<String, Var<'static>>,
    param_count: usize,
) {
    // param_count == 0: can't distinguish input slots from locals; fall back to names-match (keep all).
    if param_count == 0 { return; }
    for (i, item) in body.iter().enumerate() {
        if i < param_count { continue; } // input/output slot — keep.
        if let BodyItem::Membership { name, .. } = item {
            inner.remove(name);
            let prefix = format!("{}.", name);
            let dotted: Vec<String> = inner.keys()
                .filter(|k| k.starts_with(&prefix)).cloned().collect();
            for k in dotted { inner.remove(&k); }
        }
    }
}
