//! `desugar` — the source-level `Seq(T)`-concat flattening
//! (`desugar_seq_concat`), behind the [`super`] swap interface.
//!
//! Two interchangeable backings of the same rewrite:
//!
//!   * [`RustDesugar`] — wraps the canonical
//!     [`crate::runtime::desugar::desugar_seq_concat`] verbatim (the pass
//!     `runtime/src/runtime/load.rs` runs in production). The default, and
//!     the oracle the equivalence test compares against.
//!   * [`EvidentDesugar`] — owns an [`EvidentRuntime`] with
//!     `stdlib/passes/desugar.ev` loaded, and runs the transform's two
//!     RECURSIVE, value-carrying kernels as stack-FSMs (`run_nested`,
//!     tier-3) over the SHARED marshaler (session UU):
//!       - `desugar_gather`  — body → `Assoc` cons-list of
//!         `name ↦ ⟨items⟩` bindings (canonical pass-1).
//!       - `desugar_flatten` — `(Expr, Assoc)` → flattened `ExprList`, or
//!         fail (canonical `flatten`, faithfully: literal / bound-identifier
//!         operands contribute items VERBATIM).
//!     The structural pre-order `rewrite` tree-walk stays in Rust — the
//!     SAME shared-walk + Evident-decision division [`super::validate`]
//!     uses. Because the walk is identical and the kernels reproduce the
//!     canonical logic, `EvidentDesugar` is byte-identical to `RustDesugar`.
//!
//! ## Why this is NOT cut over (the honest fallback)
//!
//! `desugar_seq_concat` is one of two passes the canonical desugar runs.
//! The other, `unify_world_syntax`, rewrites identifier strings by
//! prefix-strip (`_world.X` → `world.X`, `world.X` → `world_next.X`) — and
//! Evident has **no substring/prefix/format operator**, so it cannot be
//! self-hosted at all (the same "no substring op" wall `subscriptions` hit,
//! but here it's the WHOLE transform, not a classify step). And returning
//! the entire rewritten body through the shared marshaler is byte-LOSSY in
//! general: `SchemaDecl.param_count` has no marshaler slot (subclaims
//! round-trip to 0) and nested constructor sub-patterns collapse to
//! `BindWildcard`. So a full all-Evident desugar can't be the production
//! load path. We self-host the expressible kernels, equivalence-prove them
//! against the canonical pass, keep the canonical in `load.rs`, and document
//! the gaps (`docs/self-hosting.md`, `examples/COUNTEREXAMPLES.md`).
//!
//! `desugar` is a **load-time** pass: it runs once per schema at load, never
//! on the per-tick scheduler hot path. So even an Evident-backed desugar
//! would leave steady-state runtime untouched — only one-time load cost
//! moves. (Not that it matters: production stays on the Rust pass.)

use std::collections::HashMap;
use std::path::Path;

use crate::core::ast::{BinOp, BodyItem, Expr, SchemaDecl};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::{decode_expr, decode_list};
use crate::translate::ast_encoder::{body_item_list_to_value, expr_to_value};
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The trait
// ─────────────────────────────────────────────────────────────────────

/// `desugar_seq_concat`'s Rust-level signature, independent of which impl
/// backs it. Rewrites `s` in place, flattening `Seq(T)` concatenations
/// (`a ++ b ++ ⟨c⟩`) into single `SeqLit`s where every operand resolves,
/// recursing into subclaims — exactly as
/// [`crate::runtime::desugar::desugar_seq_concat`].
pub trait DesugarImpl: Portable {
    fn desugar_seq_concat(&self, s: &mut SchemaDecl);
}

// ─────────────────────────────────────────────────────────────────────
// Rust impl — wraps the canonical pass verbatim (the production path)
// ─────────────────────────────────────────────────────────────────────

/// Native desugar. Delegates straight to the canonical
/// `runtime::desugar::desugar_seq_concat` — so `RustDesugar` IS the
/// production behavior, and the equivalence test compares the Evident impl
/// against the real thing, not a re-implementation that could drift.
pub struct RustDesugar;

impl Portable for RustDesugar {
    fn impl_name(&self) -> &'static str { "rust" }
}

impl DesugarImpl for RustDesugar {
    fn desugar_seq_concat(&self, s: &mut SchemaDecl) {
        crate::runtime::desugar::desugar_seq_concat(s);
    }
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
        // round-tripped through the marshaler), so their `param_count` /
        // `external` survive. This is what keeps the body-level return
        // lossless despite the marshaler's documented `param_count` gap.
        for item in s.body.iter_mut() {
            if let BodyItem::SubclaimDecl(sub) = item {
                self.desugar_seq_concat(sub);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// Selection
// ─────────────────────────────────────────────────────────────────────

/// Pick an impl by `EVIDENT_DESUGAR_IMPL` (`rust` | `evident`), defaulting
/// to the Rust impl (the production path — desugar is NOT cut over). The
/// `evident` choice locates `stdlib/` via the one
/// [`crate::stdlib_path::stdlib_dir`] resolver; if locating or loading
/// fails it falls back to Rust.
pub fn default_impl() -> Box<dyn DesugarImpl> {
    if std::env::var("EVIDENT_DESUGAR_IMPL").as_deref() == Ok("evident") {
        if let Ok(dir) = crate::stdlib_path::stdlib_dir() {
            if let Ok(ev) = EvidentDesugar::new(&dir) {
                return Box::new(ev);
            }
        }
    }
    Box::new(RustDesugar)
}
