//! `desugar` — the source-level `Seq(T)`-concat flattening
//! (`desugar_seq_concat`). **Sole implementation: the self-hosted Evident
//! pass.** Session REVIVE-desugar cut this over (completing PORT-desugar's
//! groundwork now that gaps #18 + `param_count` are fixed): the canonical
//! Rust `desugar_seq_concat` gather/flatten/rewrite walk (in
//! `runtime/src/runtime/desugar.rs`) is **deleted**, and the production load
//! path flattens through [`EvidentDesugar`]. There is no Rust-pass fallback.
//!
//! [`EvidentDesugar`] owns an [`EvidentRuntime`] with
//! `stdlib/passes/desugar.ev` loaded, and runs the transform's two
//! RECURSIVE, value-carrying kernels as stack-FSMs (`run_nested`, tier-3)
//! over the SHARED marshaler (session UU):
//!   - `desugar_gather`  — body → `Assoc` cons-list of `name ↦ ⟨items⟩`
//!     bindings (canonical pass-1). No string equality (structural match),
//!     so it self-hosts cleanly.
//!   - `desugar_flatten` — an `Expr` Concat spine → an ordered chunk stream
//!     (literal items + identifier `FRef` markers), or fail (canonical
//!     `flatten`, faithfully: literal / bound-identifier operands contribute
//!     items VERBATIM).
//!
//! ## What stays in Rust, and why (the honest split)
//!
//! Two pieces stay in this shim — the SAME "Evident owns the recursion,
//! Rust owns the string leaf" division [`super::validate`] and
//! [`super::subscriptions`] ship:
//!
//!   1. **The pre-order `rewrite` tree-walk** — which `Expr` nodes to visit,
//!      where to splice the flattened `SeqLit`, recursion into subclaims.
//!      It stays in Rust this session, but **no longer for faithfulness**:
//!      as of session SEED-marshal the shared `*_to_value` SEED marshaler
//!      round-trips nested-ctor / bind `MatchPattern`s byte-identically
//!      (`bind_list_to_value` / `match_pattern_to_value` emit `BindCtor` /
//!      `PatBind` to depth, and `desugar.ev`'s `MatchBind`/`MatchPattern`
//!      enums grew to match — see `runtime/tests/seed_roundtrip.rs`). So a
//!      whole-body return is no longer byte-LOSSY. The walk stays for the
//!      same reason the `FRef` lookup does (below) — keeping the structural
//!      traversal as an in-place Rust mutation avoids an FSM solve per node;
//!      the `desugar_rewrite` cutover that deletes it is the first
//!      beneficiary of the now-symmetric marshaler. The in-place mutation
//!      (`Concat → SeqLit`, never round-tripping an untouched `match` arm)
//!      is belt-and-suspenders, no longer load-bearing.
//!   2. **The string-keyed `FRef` lookup** — resolving `FRef(name)` to its
//!      `⟨items⟩` against the gathered map. #18 (enum-payload String
//!      equality) is fixed, so this is now *correct* in an FSM — but doing
//!      the `name = key` comparison INSIDE the per-tick Z3 solve, on a
//!      flatten state that carries effect-list `EStr` literals, hits the
//!      in-solve string-theory blowup `validate` measured (minutes + GBs).
//!      So the FSM emits `FRef(name)` without comparing and the shim does
//!      the `HashMap` lookup — out of the solve, a performance call (the
//!      same reason `validate`'s `is_banned` stays in Rust).
//!
//! `unify_world_syntax`, the *other* desugar pass, stays canonical Rust
//! (`runtime/src/runtime/desugar.rs`): it rewrites identifier strings by
//! prefix-strip + format, and Evident still has no runtime
//! string-construction operator. It is a separate rewrite from
//! `desugar_seq_concat`; only the latter cuts over here.
//!
//! `desugar` is a **load-time** pass: it runs once per schema at load, never
//! on the per-tick scheduler hot path. So this cutover moves only one-time
//! load cost — steady-state per-tick runtime is untouched.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use crate::core::ast::{BinOp, BodyItem, Expr, SchemaDecl};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::{decode_expr, decode_list};
use crate::translate::ast_encoder::{body_item_list_to_value, expr_to_value};
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The trait
// ─────────────────────────────────────────────────────────────────────

/// `desugar_seq_concat`'s Rust-level signature. Rewrites `s` in place,
/// flattening `Seq(T)` concatenations (`a ++ b ++ ⟨c⟩`) into single
/// `SeqLit`s where every operand resolves, recursing into subclaims.
/// [`EvidentDesugar`] is the runtime's sole impl since session
/// REVIVE-desugar; the trait remains so `runtime/tests/desugar_correctness.rs`
/// can drive the impl through a stable seam.
pub trait DesugarImpl: Portable {
    fn desugar_seq_concat(&self, s: &mut SchemaDecl);
}

// ─────────────────────────────────────────────────────────────────────
// Evident impl — gather + flatten as stack-FSMs, shared walk in Rust
// ─────────────────────────────────────────────────────────────────────

/// Pass-driven desugar. Holds an [`EvidentRuntime`] with
/// `stdlib/passes/desugar.ev` loaded; build once and reuse so the two
/// FSMs' per-tick solves are JIT-cached across calls.
pub struct EvidentDesugar {
    rt: EvidentRuntime,
}

impl EvidentDesugar {
    const GATHER_FSM:  &'static str = "desugar_gather";
    const FLATTEN_FSM: &'static str = "desugar_flatten";

    /// Max-iteration guard for the nested walks. A body / concat spine of
    /// N nodes halts in O(N) ticks; the cap is far above any realistic
    /// input so a legitimate run never hits it.
    const MAX_STEPS: usize = 5_000_000;

    /// Load `passes/desugar.ev` into a fresh runtime. The pass is
    /// self-contained (declares its own cons-list AST enums matching the
    /// shared marshaler), so no other stdlib file is needed.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("passes").join("desugar.ev"))
            .map_err(|e| format!("load passes/desugar.ev: {e}"))?;
        Ok(Self { rt })
    }

    /// Pass-1: gather `name = ⟨items⟩` bindings via the `desugar_gather`
    /// FSM, then decode its `Assoc` cons-list into a Rust `name → items`
    /// map. The string-keyed map lives in Rust (string equality is
    /// unreliable in Evident — COUNTEREXAMPLES #18 — so `flatten` does the
    /// keyed lookup here, not in the FSM). On any failure the map is empty:
    /// every `flatten` ref then misses, a conservative "no bindings".
    ///
    /// Last-wins on duplicate names mirrors the canonical pass's HashMap:
    /// `desugar_gather` prepends, so the last binding sits at the cons head
    /// and `or_insert` (first-seen-wins on the head-first walk) keeps it.
    fn gather(&self, body: &[BodyItem]) -> HashMap<String, Vec<Expr>> {
        let seed = body_item_list_to_value(body);
        let assoc = match crate::effect_loop::run_nested(&self.rt, Self::GATHER_FSM, seed, Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. }) if variant == "GDone" && fields.len() == 1 =>
                fields[0].clone(),
            other => {
                eprintln!("[desugar/evident] gather returned an unexpected state: {other:?}");
                return HashMap::new();
            }
        };
        let mut map = HashMap::new();
        let mut cur = &assoc;
        // Walk the `Assoc` spine (ANil | ACons(AEntry, Assoc)); each entry is
        // MakeAEntry(name, ExprList).
        while let Value::Enum { variant, fields, .. } = cur {
            match (variant.as_str(), fields.as_slice()) {
                ("ANil", _) => break,
                ("ACons", [entry, rest]) => {
                    if let Value::Enum { variant: ev, fields: ef, .. } = entry {
                        if ev == "MakeAEntry" && ef.len() == 2 {
                            if let Value::Str(name) = &ef[0] {
                                if let Ok(items) =
                                    decode_list(&ef[1], "ExprList", "ELNil", "ELCons", decode_expr)
                                {
                                    map.entry(name.clone()).or_insert(items);
                                }
                            }
                        }
                    }
                    cur = rest;
                }
                _ => break,
            }
        }
        map
    }

    /// Resolve a `Concat` subtree `e` against the gathered `bindings`.
    /// Mirrors the canonical `flatten`: `Some(items)` when every operand
    /// resolves (a literal `⟨…⟩` or a bound identifier), `None` otherwise.
    ///
    /// The FSM returns a head-first chunk stream (`FDone(FChunks)`) — each
    /// chunk a literal item (`FLitItem`) or an unresolved identifier ref
    /// (`FRef`); `FFail` for a non-resolvable operand shape. We reverse to
    /// source order, then expand: `FLitItem` contributes itself; `FRef(n)`
    /// contributes `bindings[n]` (VERBATIM) or fails the whole flatten if
    /// `n` is unbound — exactly Rust `flatten`'s `seq_lits.get(name)`.
    fn flatten(&self, e: &Expr, bindings: &HashMap<String, Vec<Expr>>) -> Option<Vec<Expr>> {
        let seed = expr_to_value(e);
        let chunks = match crate::effect_loop::run_nested(&self.rt, Self::FLATTEN_FSM, seed, Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. }) if variant == "FDone" && fields.len() == 1 =>
                fields[0].clone(),
            Ok(Value::Enum { variant, .. }) if variant == "FFail" => return None,
            other => {
                eprintln!("[desugar/evident] flatten returned an unexpected state: {other:?}");
                return None;
            }
        };
        // Walk the head-first `FChunks` spine into source order.
        let mut rev: Vec<&Value> = Vec::new();
        let mut cur = &chunks;
        while let Value::Enum { variant, fields, .. } = cur {
            match (variant.as_str(), fields.as_slice()) {
                ("FCNil", _) => break,
                ("FCCons", [chunk, rest]) => { rev.push(chunk); cur = rest; }
                _ => return None,
            }
        }
        let mut out: Vec<Expr> = Vec::new();
        for chunk in rev.into_iter().rev() {
            let Value::Enum { variant, fields, .. } = chunk else { return None };
            match (variant.as_str(), fields.as_slice()) {
                ("FLitItem", [item]) => out.push(decode_expr(item).ok()?),
                ("FRef", [Value::Str(name)]) => out.extend(bindings.get(name)?.iter().cloned()),
                _ => return None,
            }
        }
        Some(out)
    }

    /// Pre-order rewrite of one Expr — a faithful copy of the canonical
    /// `rewrite` in `runtime/src/runtime/desugar.rs`, with `flatten`
    /// delegated to the Evident FSM (+ Rust ref-resolution). A `Concat`
    /// that fully flattens is replaced by a single `SeqLit` (no further
    /// recursion into it); everything else recurses into children, in the
    /// same order.
    fn rewrite(&self, e: &mut Expr, bindings: &HashMap<String, Vec<Expr>>) {
        if let Expr::Binary(BinOp::Concat, ..) = e {
            if let Some(items) = self.flatten(e, bindings) {
                *e = Expr::SeqLit(items);
                return;
            }
        }
        match e {
            Expr::Binary(_, l, r)
            | Expr::Range(l, r)
            | Expr::InExpr(l, r)
            | Expr::Index(l, r) => { self.rewrite(l, bindings); self.rewrite(r, bindings); }
            Expr::Ternary(c, a, b) => {
                self.rewrite(c, bindings); self.rewrite(a, bindings); self.rewrite(b, bindings);
            }
            Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es)
            | Expr::Call(_, es) => {
                for x in es { self.rewrite(x, bindings); }
            }
            Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => {
                self.rewrite(r, bindings); self.rewrite(b, bindings);
            }
            Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => {
                self.rewrite(i, bindings);
            }
            Expr::Field(recv, _) => self.rewrite(recv, bindings),
            Expr::Match(scr, arms) => {
                self.rewrite(scr, bindings);
                for a in arms { self.rewrite(&mut a.body, bindings); }
            }
            _ => {}
        }
    }
}

impl Portable for EvidentDesugar {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl DesugarImpl for EvidentDesugar {
    fn desugar_seq_concat(&self, s: &mut SchemaDecl) {
        if s.external { return; }
        // Pass 1: gather the seq-lit bindings (Evident FSM → Rust map).
        let bindings = self.gather(&s.body);
        // Pass 2: rewrite Constraint exprs + ClaimCall mapping values — the
        // exact two body-item shapes the canonical pass-2 touches.
        for item in s.body.iter_mut() {
            match item {
                BodyItem::Constraint(e) => self.rewrite(e, &bindings),
                BodyItem::ClaimCall { mappings, .. } => {
                    for m in mappings.iter_mut() { self.rewrite(&mut m.value, &bindings); }
                }
                _ => {}
            }
        }
        // Recurse into subclaims — held as Rust `SchemaDecl`s (never
        // round-tripped through the marshaler), so their `external` flag and
        // any nested-ctor `MatchPattern`s survive intact. This in-place,
        // never-round-trip-an-untouched-node discipline is what keeps the
        // rewrite lossless despite the marshaler's `MatchPattern` gap.
        for item in s.body.iter_mut() {
            if let BodyItem::SubclaimDecl(sub) = item {
                self.desugar_seq_concat(sub);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Production entry point — a per-thread cached engine + bootstrap guard
// ─────────────────────────────────────────────────────────────────────

thread_local! {
    /// One [`EvidentDesugar`] engine per thread, built lazily on the first
    /// [`desugar_seq_concat`] call. `EvidentRuntime` is `!Send`/`!Sync` (Z3
    /// context, Cranelift module, `Rc`/`RefCell` interior), so a
    /// thread-local — not a global — is the right cache: load + JIT-compile
    /// the pass once per thread, reuse for every schema.
    static ENGINE: std::cell::RefCell<Option<Rc<EvidentDesugar>>> =
        const { std::cell::RefCell::new(None) };

    /// Re-entrancy guard. Set while the engine's private runtime is loading
    /// `desugar.ev`: that load runs this same desugar pass over the pass's
    /// own schemas, which would re-enter here mid-build (and re-borrow
    /// [`ENGINE`]). While set, [`desugar_seq_concat`] is a no-op — the pass
    /// file is trusted, hand-verified stdlib that contains no `++` Seq concat
    /// to flatten, so skipping it leaves the schemas byte-identical.
    static BOOTSTRAPPING: Cell<bool> = const { Cell::new(false) };
}

/// Flatten `Seq(T)` concatenations in `s` (in place) via the self-hosted
/// Evident `desugar_gather` / `desugar_flatten` pass. **This is the
/// runtime's sole `desugar_seq_concat` entry point** —
/// `runtime::desugar::desugar_seq_concat` (on the load path) delegates here.
///
/// Builds and caches a per-thread [`EvidentDesugar`] engine on first use
/// (see [`ENGINE`]). The engine locates `stdlib/` via the one
/// [`crate::stdlib_path::stdlib_dir`] resolver (session WW).
///
/// # Panics
///
/// If `stdlib/passes/desugar.ev` cannot be located or loaded. There is no
/// Rust-pass fallback (this session), so an unloadable pass is a hard error
/// — the same robust resolution the rest of the runtime relies on. The error
/// names every checked path and the `EVIDENT_STDLIB` override.
pub fn desugar_seq_concat(s: &mut SchemaDecl) {
    // Re-entrancy break: while building the engine (loading the trusted
    // desugar pass), skip flattening — see [`BOOTSTRAPPING`].
    if BOOTSTRAPPING.with(|b| b.get()) {
        return;
    }
    // Fast path: a schema that contains no `++` Concat ANYWHERE (this level
    // or any nested subclaim) is a byte-identical no-op — `gather`'s bindings
    // are consumed only by `flatten`, which only fires on a Concat subtree.
    // So skipping the engine entirely for Concat-free schemas (the
    // overwhelming majority — most `++` in real code is String concat, not
    // Seq) is exact, and keeps `desugar` a cheap load-time pass instead of
    // paying an FSM solve on every schema loaded. See [`schema_has_seq_concat`].
    if !schema_has_seq_concat(s) {
        return;
    }
    let engine = ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            // The build loads desugar.ev; that load re-enters this function
            // for each pass schema. Guard the window so the re-entry
            // short-circuits before touching `ENGINE` again.
            BOOTSTRAPPING.with(|b| b.set(true));
            let built = build_engine();
            BOOTSTRAPPING.with(|b| b.set(false));
            *slot = Some(Rc::new(built));
        }
        slot.as_ref().unwrap().clone()
    });
    engine.desugar_seq_concat(s);
}

/// Locate `stdlib/` and load the desugar pass into a fresh engine. Panics
/// with the resolver's path-list diagnostic on failure — see
/// [`desugar_seq_concat`].
fn build_engine() -> EvidentDesugar {
    let dir = crate::stdlib_path::stdlib_dir().unwrap_or_else(|e| panic!(
        "desugar: cannot locate stdlib to load the desugar pass \
         (the sole impl since session REVIVE-desugar): {e}"));
    EvidentDesugar::new(&dir).unwrap_or_else(|e| panic!(
        "desugar: failed to load passes/desugar.ev from {}: {e}",
        dir.display()))
}

// ─────────────────────────────────────────────────────────────────────
// Concat-free fast path
// ─────────────────────────────────────────────────────────────────────

/// Does `s` contain a `Seq`-concat (`Expr::Binary(Concat, …)`) ANYWHERE the
/// pass would rewrite it — a `Constraint` expr, a `ClaimCall` mapping value,
/// or a nested subclaim's body? A cheap, pure-Rust structural scan. When it
/// returns `false`, [`desugar_seq_concat`] short-circuits: with no Concat to
/// flatten, the gather/flatten engine would leave the schema byte-identical,
/// so the whole FSM round-trip is skipped. This keeps `desugar` a near-free
/// load-time pass on the common case (real source uses `++` mostly for
/// String concat, which is left alone, or not at all).
fn schema_has_seq_concat(s: &SchemaDecl) -> bool {
    s.body.iter().any(|item| match item {
        BodyItem::Constraint(e) => expr_has_concat(e),
        BodyItem::ClaimCall { mappings, .. } => mappings.iter().any(|m| expr_has_concat(&m.value)),
        BodyItem::SubclaimDecl(sub) => schema_has_seq_concat(sub),
        _ => false,
    })
}

/// True if `e` contains a `Concat` binary anywhere in its tree. Visits the
/// same nodes as [`EvidentDesugar::rewrite`] so the guard and the rewrite
/// agree on reachability.
fn expr_has_concat(e: &Expr) -> bool {
    match e {
        Expr::Binary(BinOp::Concat, ..) => true,
        Expr::Binary(_, l, r)
        | Expr::Range(l, r)
        | Expr::InExpr(l, r)
        | Expr::Index(l, r) => expr_has_concat(l) || expr_has_concat(r),
        Expr::Ternary(c, a, b) => expr_has_concat(c) || expr_has_concat(a) || expr_has_concat(b),
        Expr::SetLit(es) | Expr::SeqLit(es) | Expr::Tuple(es) | Expr::Call(_, es) =>
            es.iter().any(expr_has_concat),
        Expr::Forall(_, r, b) | Expr::Exists(_, r, b) => expr_has_concat(r) || expr_has_concat(b),
        Expr::Cardinality(i) | Expr::Not(i) | Expr::Matches(i, _) => expr_has_concat(i),
        Expr::Field(recv, _) => expr_has_concat(recv),
        Expr::Match(scr, arms) =>
            expr_has_concat(scr) || arms.iter().any(|a| expr_has_concat(&a.body)),
        _ => false,
    }
}
