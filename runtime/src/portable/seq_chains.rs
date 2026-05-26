//! `seq_chains` — body `Seq(Effect)` ordering-chain extraction for the
//! multi-FSM scheduler's dispatch-ordering path. **Sole implementation: the
//! self-hosted Evident pass.**
//!
//! Session PORT-seqchains cut chain extraction over to Evident-only: the
//! canonical Rust walk (the old `crate::effect_loop::seq_chains::
//! extract_seq_effect_chains` + its `node_name` matcher) is **deleted**, and
//! [`collect_dispatchable_effects`](crate::effect_loop) now derives ordering
//! edges through [`extract_seq_effect_chains`] here. There is no Rust-walk
//! fallback.
//!
//! [`EvidentSeqChains`] owns an [`EvidentRuntime`] with
//! `stdlib/passes/seq_chains.ev` loaded. The walk runs in Evident as an
//! FSM-with-stack (`seq_chains_walk`): this shim marshals the claim body into
//! a poppable `BodyItemList` via the SHARED marshaler
//! ([`crate::translate::ast_encoder::body_item_list_to_value`]), drives the FSM
//! to a drained-list halt via [`crate::effect_loop::run_nested`], and decodes
//! the collected chains. **No Rust-side tree walk, no bespoke encoder** — the
//! traversal lives in the pass.
//!
//! ## What stays in Rust, and why
//!
//! The FSM owns the body traversal and emits each recognized `Seq(Effect)`
//! literal's RAW element `Expr`s. It does NOT resolve them to node names:
//! [`node_name`] does set-membership string-equality keying (the in-solve Z3
//! string-theory blow-up VALIDATE-recursive measured — the #18 cousin) AND
//! builds synthetic names like `hat_effs[0]` / `plat_effs[0].effs[0]` with
//! `format!`, which needs int→string (a gap). So [`node_name`] + the
//! all-elements-resolve gate stay here, run off the decoded walk output.
//!
//! ## Per-tick cost: cached, and strictly cheaper than the old Rust walk
//!
//! Chain extraction sits on the per-tick scheduler path
//! (`collect.rs` runs it for every Mode-2 dispatch). The Evident walk depends
//! ONLY on the static claim body, so [`extract_seq_effect_chains`] runs it
//! **once per claim body** and caches the raw `Expr` chains keyed by the body's
//! identity (see [`RAW_CACHE`]). The per-tick work is then a cache lookup plus
//! the [`node_name`] resolution that already lived in Rust — strictly *less*
//! per-tick work than the old code, which re-walked the body in Rust every
//! tick. The Evident walk's one-time cost is amortized over the run.
//!
//! ## No bootstrap cycle
//!
//! `seq_chains_walk` reads no `world.X` (its state is the plain `SC` stack
//! machine), and it is driven by [`crate::effect_loop::run_nested`] — the
//! tier-3 blocking interpreter, which never calls back into the scheduler's
//! dispatch path. The pass that computes dispatch ordering does not itself
//! need dispatch ordering.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;

use crate::core::ast::{BodyItem, Expr};
use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::{decode_expr, decode_list};
use crate::translate::ast_encoder::body_item_list_to_value;
use super::Portable;

// ─────────────────────────────────────────────────────────────────────
// The trait
// ─────────────────────────────────────────────────────────────────────

/// `seq_chains`' Rust-level signature, independent of which impl backs it.
/// Kept for uniformity with the rest of the [`super`] swap-interface family
/// (`pretty`, `validate`, `subscriptions`); seq_chains now has a single impl.
pub trait SeqChainsImpl: Portable {
    /// Walk one claim body and return the RAW element `Expr`s of each
    /// recognized `Seq(Effect)` literal constraint, in body order.
    fn raw_chains(&self, body: &[BodyItem]) -> Vec<Vec<Expr>>;
}

// ─────────────────────────────────────────────────────────────────────
// Evident impl — runs stdlib/passes/seq_chains.ev as a stack-FSM
// ─────────────────────────────────────────────────────────────────────

/// Runs the extraction by marshaling the claim body with the shared marshaler
/// and driving the `seq_chains_walk` FSM to halt. Holds its own runtime with
/// the pass loaded; build once and reuse so the FSM's per-tick solve is
/// JIT-cached across calls.
pub struct EvidentSeqChains {
    rt: EvidentRuntime,
}

impl EvidentSeqChains {
    /// The walk FSM in `stdlib/passes/seq_chains.ev`.
    const WALK_FSM: &'static str = "seq_chains_walk";

    /// Max-iteration guard for the nested walk. One body item costs a small
    /// constant number of FSM ticks, so a body of N items halts in O(N)
    /// ticks; the cap is set far above any realistic claim so a legitimate
    /// walk never hits it (a non-terminating walk would be a pass bug,
    /// surfaced as a loud `MaxItersExceeded`).
    const MAX_STEPS: usize = 5_000_000;

    /// Load `passes/seq_chains.ev` from `stdlib_dir` into a fresh runtime.
    /// `stdlib_dir` is the repo's `stdlib/` directory. The pass is
    /// self-contained (it declares its own cons-list copy of the AST enums
    /// matching the shared marshaler), so no other stdlib file is needed.
    pub fn new(stdlib_dir: &Path) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join("passes").join("seq_chains.ev"))
            .map_err(|e| format!("load passes/seq_chains.ev: {e}"))?;
        Ok(Self { rt })
    }
}

impl Portable for EvidentSeqChains {
    fn impl_name(&self) -> &'static str { "evident" }
}

impl SeqChainsImpl for EvidentSeqChains {
    fn raw_chains(&self, body: &[BodyItem]) -> Vec<Vec<Expr>> {
        // Shared marshaler: the whole body → a poppable BodyItemList cons-list
        // (the FSM's seed). `run_nested`'s coerce seeds it into `SCSeed`.
        let seed = body_item_list_to_value(body);
        match crate::effect_loop::run_nested(&self.rt, Self::WALK_FSM, seed, Self::MAX_STEPS) {
            Ok(Value::Enum { variant, fields, .. }) if variant == "SCDone" && fields.len() == 1 => {
                // ChainList (newest-first) of ExprList; shared cons-list reader
                // for both levels. Reverse to recover body order — matching the
                // deleted Rust walk, which pushed chains in body order.
                match decode_list(&fields[0], "ChainList", "ChNil", "ChCons",
                        |chain| decode_list(chain, "ExprList", "ELNil", "ELCons", decode_expr)) {
                    Ok(mut chains) => { chains.reverse(); chains }
                    Err(e) => {
                        eprintln!("[seq_chains/evident] decode of walk result failed: {e}");
                        Vec::new()
                    }
                }
            }
            Ok(other) => {
                eprintln!("[seq_chains/evident] walk returned a non-SCDone state: {other:?}");
                Vec::new()
            }
            Err(e) => {
                eprintln!("[seq_chains/evident] walk failed: {e}");
                Vec::new()
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────
// node_name — the one piece Evident can't express (no substring / int→string)
// ─────────────────────────────────────────────────────────────────────

/// Resolve a SeqLit element `Expr` to its dispatch node name, or `None` if it
/// isn't a known node. Recognizes (verbatim from the deleted Rust walk):
///   * `Identifier(name)` where `name` is a bare Effect binding.
///   * `Index(Identifier(name), Int(i))` where `name[i]` names a synthetic
///     `Seq(Effect)` element (e.g. `hat_effs[0]`).
///   * `Index(Field(Index(Identifier(outer), Int(i)), field), Int(j))` where
///     `outer[i].field[j]` names a synthetic `Seq(Composite-with-Seq-Effect-
///     field)` element (e.g. `plat_effs[0].effs[0]`).
///
/// Stays in Rust because both halves are string-keyed: the synthetic-name
/// construction needs int→string (`format!`), and the `set.contains` check is
/// the string-equality keying that blows up Z3 string theory in-solve.
fn node_name(e: &Expr, set: &HashSet<&String>) -> Option<String> {
    match e {
        Expr::Identifier(n) if set.contains(n) => Some(n.clone()),
        Expr::Index(seq, idx) => match seq.as_ref() {
            Expr::Identifier(name) => {
                if let Expr::Int(i) = idx.as_ref() {
                    let syn = format!("{}[{}]", name, i);
                    if set.contains(&syn) { return Some(syn); }
                }
                None
            }
            Expr::Field(inner_seq, field) => {
                let Expr::Index(outer_seq, outer_idx) = inner_seq.as_ref() else {
                    return None;
                };
                let Expr::Identifier(outer_name) = outer_seq.as_ref() else {
                    return None;
                };
                let (Expr::Int(i), Expr::Int(j)) = (outer_idx.as_ref(), idx.as_ref())
                    else { return None };
                let syn = format!("{}[{}].{}[{}]", outer_name, i, field, j);
                if set.contains(&syn) { Some(syn) } else { None }
            }
            _ => None,
        },
        _ => None,
    }
}

// ─────────────────────────────────────────────────────────────────────
// Production entry point — a per-thread cached engine + per-body chain cache
// ─────────────────────────────────────────────────────────────────────

thread_local! {
    /// One [`EvidentSeqChains`] engine per thread, built lazily on first use.
    /// `EvidentRuntime` is `!Send`/`!Sync` (Z3 context, Cranelift module,
    /// `Rc`/`RefCell` interior), so a thread-local — not a global — is the
    /// right cache: the scheduler runs single-threaded, so it pays the
    /// pass-load + JIT-compile cost exactly once.
    static ENGINE: RefCell<Option<Rc<EvidentSeqChains>>> = const { RefCell::new(None) };

    /// Raw chains cached by the body's identity (data-ptr + len). The Evident
    /// walk depends ONLY on the static claim body, so it runs once per body and
    /// the per-tick `effect_node_set` resolution happens off this cache.
    ///
    /// A claim body's `Vec<BodyItem>` is owned by its runtime's schema table
    /// and never mutated for the runtime's lifetime, so `(as_ptr, len)` is a
    /// stable, O(1), exact key within a scheduler run — the production reality
    /// (one runtime per `effect-run` process). The only theoretical staleness
    /// is a freed body's pointer being reused by a *different* body of the same
    /// length in the *same thread* with this cache still live; impossible in
    /// the single-runtime production path, and tests build distinct bodies.
    static RAW_CACHE: RefCell<HashMap<usize, (usize, Rc<Vec<Vec<Expr>>>)>> =
        RefCell::new(HashMap::new());
}

/// The per-thread engine, built (pass loaded + JIT primed) on first call.
fn engine() -> Rc<EvidentSeqChains> {
    ENGINE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(Rc::new(build_engine()));
        }
        slot.as_ref().unwrap().clone()
    })
}

/// Locate `stdlib/` and load the seq_chains pass into a fresh engine. Panics
/// with the resolver's path-list diagnostic on failure — there is no Rust-walk
/// fallback (session PORT-seqchains), matching the rest of the self-hosted
/// load path (session WW resolver).
fn build_engine() -> EvidentSeqChains {
    let dir = crate::stdlib_path::stdlib_dir().unwrap_or_else(|e| panic!(
        "seq_chains: cannot locate stdlib to load the seq_chains pass \
         (the sole impl since session PORT-seqchains): {e}"));
    EvidentSeqChains::new(&dir).unwrap_or_else(|e| panic!(
        "seq_chains: failed to load passes/seq_chains.ev from {}: {e}",
        dir.display()))
}

/// Raw `Expr` chains for `body`, computed by the Evident walk once and cached
/// by body identity (see [`RAW_CACHE`]). Cheap clone of an `Rc` on a hit.
fn cached_raw_chains(body: &[BodyItem]) -> Rc<Vec<Vec<Expr>>> {
    let key = body.as_ptr() as usize;
    let len = body.len();
    if let Some(hit) = RAW_CACHE.with(|c| {
        c.borrow().get(&key).filter(|(l, _)| *l == len).map(|(_, v)| v.clone())
    }) {
        return hit;
    }
    let chains = Rc::new(engine().raw_chains(body));
    RAW_CACHE.with(|c| c.borrow_mut().insert(key, (len, chains.clone())));
    chains
}

/// Ordering chains for one claim body — the runtime's sole chain-extraction
/// entry point. Walks the body for `Seq(Effect)` literal constraints (in
/// Evident, cached per body), resolves each element to its dispatch node name
/// via [`node_name`], and emits a chain only when *every* element resolves (a
/// chain with one unresolved element isn't a clean ordering chain — drop it).
///
/// Same signature and semantics as the deleted Rust walk; `collect.rs` calls
/// it per tick on the Mode-2 dispatch path.
pub fn extract_seq_effect_chains(
    body: &[BodyItem],
    effect_node_set: &HashSet<&String>,
) -> Vec<Vec<String>> {
    let raw = cached_raw_chains(body);
    let mut chains: Vec<Vec<String>> = Vec::new();
    for chain in raw.iter() {
        let names: Vec<String> = chain.iter()
            .filter_map(|e| node_name(e, effect_node_set))
            .collect();
        if names.len() != chain.len() { continue; }
        chains.push(names);
    }
    chains
}

/// Drop the per-body raw-chain cache. Never needed in the single-runtime
/// production path (one program per `effect-run` process); provided for tests
/// — which reuse body allocations across cases — and for any long-lived process
/// that reloads distinct programs and wants to forgo body-pointer reuse
/// assumptions. A no-op for correctness in production; only forgoes the cache.
pub fn reset_cache() {
    RAW_CACHE.with(|c| c.borrow_mut().clear());
}
