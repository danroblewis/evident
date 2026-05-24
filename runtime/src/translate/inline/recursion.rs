//! Inlining-recursion bookkeeping: a per-claim depth counter that
//! bounds how deep self-passthrough / recursive-claim expansion goes,
//! plus helper-local Z3-const isolation when entering a ClaimCall.
//!
//! `visited` is a per-claim depth counter that bounds inlining
//! recursion. Each entry maps a claim name to how many frames of
//! it are currently on the inlining stack. A frame can re-enter the
//! same claim up to `MAX_INLINE_DEPTH` times — enough to walk a
//! recursive AST (transpilers, list emitters, etc.) but bounded so
//! pathological self-passthrough cycles don't OOM. Without unrolling
//! at all, the transpiler-as-recursive-claims pattern doesn't work
//! (Z3 invents arbitrary string values for un-asserted `tail_out`
//! bindings). The depth bound is overridable via
//! `EVIDENT_MAX_INLINE_DEPTH` for ASTs deeper than the default.

use std::collections::HashMap;

use crate::core::ast::*;
use crate::core::Var;

/// Default cap — large enough for any realistic shader/transpiler AST,
/// small enough that a self-passthrough loop trips it before the
/// translation context blows out.
const DEFAULT_MAX_INLINE_DEPTH: usize = 64;

fn max_inline_depth() -> usize {
    std::env::var("EVIDENT_MAX_INLINE_DEPTH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_MAX_INLINE_DEPTH)
}

/// Try to enter a frame of `name` on the inlining stack. Returns
/// `Some(depth)` (the post-increment count) on success, `None` if
/// we'd exceed the depth cap. `depth > 1` ⇒ this is a recursive
/// frame; callers use that to force fresh per-call declarations
/// for body-internal Memberships, otherwise the env-clone would
/// shadow them with outer-scope vars and recursive claims would
/// self-reference (e.g. `out = "x " ++ tail_out` where `tail_out`
/// is the SAME Z3 const as the outer call's `tail_out`).
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

/// Counterpart to `try_enter` — call after the inlined body has been
/// translated. Removes the entry entirely when its count hits zero
/// so subsequent same-name lookups don't see stale state.
pub(super) fn exit_frame(visited: &mut HashMap<String, usize>, name: &str) {
    if let Some(cnt) = visited.get_mut(name) {
        *cnt -= 1;
        if *cnt == 0 { visited.remove(name); }
    }
}

/// Pre-isolate helper-local Z3 consts: when entering a ClaimCall, any
/// caller-scope vars whose names match the called claim's body
/// Memberships PAST `param_count` (i.e. the helper's internal locals,
/// not its first-line input/output slots) are removed from the cloned
/// inner env. This prevents recursive helper invocations from
/// accidentally sharing locals via the env-clone chain — without it,
/// nested `emit_ternary(...)` would reuse the OUTER `emit_ternary`'s
/// `cnd`/`thn`/`els` Z3 consts, collapsing distinct AST values to one
/// const and going UNSAT.
///
/// Slot params (the leading body Memberships up to `param_count`) are
/// PRESERVED in the clone so the helper's body can reach the
/// outer-supplied values via names-match composition.
pub(super) fn isolate_helper_locals(
    body: &[BodyItem],
    inner: &mut HashMap<String, Var<'static>>,
    param_count: usize,
) {
    // When the claim has no first-line params (param_count == 0), we
    // can't tell input slots from helper-locals — fall back to the
    // legacy names-match behavior: keep everything in the cloned env
    // so body Memberships that match outer scope re-use those Z3
    // consts. Helpers that NEED isolation (transpiler-style recursive
    // claims) must use first-line params to declare which body
    // Memberships are inputs.
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
