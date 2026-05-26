//! Self-hosted runtime passes, run as Evident-passes-as-functions.
//!
//! Each pure runtime pass that has been moved out of Rust into
//! `stdlib/passes/*.ev` (or a stdlib claim) keeps a stable Rust
//! entry-point signature here while delegating the actual transform to an
//! Evident program. The caller in `runtime/` / `effect_loop/` never sees
//! the seam: it calls `portable::validate::enforce_external_only(&s)` /
//! `portable::subscriptions::access_sets(&s)` / … exactly as before.
//!
//! # The generic runner: [`EvidentRunner`]
//!
//! Every port repeats the same orchestration around the SHARED marshaler
//! ([`crate::translate::ast_encoder`]): hold an [`crate::EvidentRuntime`]
//! with one pass loaded, drive an FSM-with-stack to a drained-stack halt via
//! [`crate::effect_loop::run_nested`] (or `query` a stdlib claim), and handle
//! the uniform "halted in the expected `Done` variant / halted elsewhere /
//! errored" outcomes. [`EvidentRunner`] owns that orchestration: `load` a
//! pass + remember its FSM, then `run` / `run_fsm` a seed to its final state,
//! or `query` a claim through `rt`. A pair of macros ([`cached_runner`] /
//! [`guarded_runner`]) give each port a per-thread, JIT-cached runner built
//! once and reused — the `thread_local` dance the ports used to copy-paste.
//!
//! What stays per-port is exactly the genuinely per-task code: marshal the
//! Rust input into a `Value` (shared encoder), decode the FSM's output
//! (shared `decode_*` + the [`run_done_list`] / [`run_name_list`] helpers
//! here), and a small Rust "leaf" — the string-set membership / prefix-split
//! / index synthesis Evident can't (or shouldn't, in-solve) express.
//!
//! # Cost
//!
//! The runner is built **once per thread** (the cache) and reused; `run`
//! goes through `run_nested`, which JIT-caches the compiled FSM after the
//! first call — steady-state cost is a JIT call (~µs) plus marshaling, not a
//! Z3 solve. Every cut-over port is either load-time only (validate, desugar,
//! inject, generics) or shape-cached off the per-tick path (subscriptions,
//! toposort, seq_chains), so per-tick runtime is untouched.
//!
//! # The swap exception: [`pretty`]
//!
//! `pretty` keeps a full Rust reference impl ([`pretty::RustPretty`]) and the
//! [`pretty::PrettyImpl`] swap trait, because its equivalence test pins the
//! Unicode/int residuals the Evident pass can't yet reproduce. Its Evident
//! impl ([`pretty::EvidentPretty`]) routes through [`EvidentRunner`] like the
//! rest, but the swap trait stays. The sole-Evident ports carry no such
//! trait — they are free functions: marshal-in → run → decode-out → leaf.
//!
//! See `docs/self-hosting.md` for the porting checklist and the current
//! runtime gaps (recursion, Unicode-in-strings) that bound what a pass can
//! faithfully reproduce.

use std::path::Path;

use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::{decode_list, decode_str, DecodeError};

/// A transformation impl that can be swapped between a Rust and an Evident
/// backing. Now used only by [`pretty`] — the sole port that keeps a Rust
/// reference impl. `impl_name` returns a short identifier — `"rust"` /
/// `"evident"` — for tracing and test assertions.
pub trait Portable {
    fn impl_name(&self) -> &'static str;
}

// ─────────────────────────────────────────────────────────────────────
// EvidentRunner — the Rust-side "use an Evident pass as a function" dual
// ─────────────────────────────────────────────────────────────────────

/// An Evident pass loaded into a private [`EvidentRuntime`], driven as a
/// function: `run` a marshaled seed through the pass's stack-FSM to its
/// final-state `Value`, or `query` a stdlib claim through [`rt`](Self::rt).
///
/// Construct once and reuse — the runner holds the loaded + JIT-cached
/// runtime, so the pass-load cost is paid once (the [`cached_runner`] /
/// [`guarded_runner`] macros make that a per-thread, build-once cache).
pub(crate) struct EvidentRunner {
    rt: EvidentRuntime,
    /// The pass's primary FSM — what [`run`](Self::run) drives. A pass with
    /// several FSMs names its main one here and reaches the others with
    /// [`run_fsm`](Self::run_fsm); a query-only pass (toposort) leaves it
    /// empty and uses [`rt`](Self::rt).
    fsm: &'static str,
    max_steps: usize,
}

impl EvidentRunner {
    /// Max-iteration guard for a nested walk. One AST node costs a small
    /// constant number of FSM ticks, so a body / expr of N nodes halts in
    /// O(N) ticks; the cap sits far above any realistic input so a legitimate
    /// walk never hits it (a non-terminating walk is a pass bug, surfaced as
    /// a loud `MaxItersExceeded`).
    pub(crate) const MAX_STEPS: usize = 5_000_000;

    /// Load `pass_relpath` (relative to the stdlib dir, e.g.
    /// `"passes/validate.ev"` or `"toposort.ev"`) into a fresh runtime,
    /// locating `stdlib/` via the one [`crate::stdlib_path::stdlib_dir`]
    /// resolver (session WW), and remember `fsm` as the primary FSM.
    pub(crate) fn load(pass_relpath: &str, fsm: &'static str) -> Result<Self, String> {
        let dir = crate::stdlib_path::stdlib_dir()
            .map_err(|e| format!("cannot locate stdlib to load `{pass_relpath}`: {e}"))?;
        Self::load_from(&dir, pass_relpath, fsm)
    }

    /// Like [`load`](Self::load) but with `stdlib_dir` supplied directly —
    /// used by tests that point at the repo's `stdlib/`.
    pub(crate) fn load_from(
        stdlib_dir: &Path,
        pass_relpath: &str,
        fsm: &'static str,
    ) -> Result<Self, String> {
        let mut rt = EvidentRuntime::new();
        rt.load_file(&stdlib_dir.join(pass_relpath))
            .map_err(|e| format!("load {pass_relpath}: {e}"))?;
        Ok(Self { rt, fsm, max_steps: Self::MAX_STEPS })
    }

    /// Drive the primary [`fsm`](Self::fsm) to a drained-stack halt over
    /// `seed`, returning its final-state `Value`.
    pub(crate) fn run(&self, seed: Value) -> Result<Value, String> {
        self.run_fsm(self.fsm, seed)
    }

    /// Drive a named FSM to halt over `seed` — for passes whose body declares
    /// several FSMs (desugar's gather/flatten, inject's build FSMs).
    pub(crate) fn run_fsm(&self, fsm: &str, seed: Value) -> Result<Value, String> {
        crate::effect_loop::run_nested(&self.rt, fsm, seed, self.max_steps)
            .map_err(|e| format!("{fsm}: {e}"))
    }

    /// The underlying runtime, for query-based ports (toposort's
    /// `ToposortRanks`, generics' `split_head` / `subst_one`).
    pub(crate) fn rt(&self) -> &EvidentRuntime {
        &self.rt
    }
}

/// Define a per-thread, build-once cached [`EvidentRunner`] accessor `$name`
/// that loads `$pass` (relative to stdlib/) with primary FSM `$fsm`. The
/// runner is `!Send`/`!Sync` (Z3 context, Cranelift module, `Rc`/`RefCell`),
/// so a `thread_local` — not a global — is the right cache: load + JIT once
/// per thread, reuse for every call. Panics with the resolver diagnostic if
/// the pass can't be located/loaded — there is no Rust fallback (these are
/// the sole impls of their passes).
macro_rules! cached_runner {
    ($name:ident, $pass:expr, $fsm:expr) => {
        fn $name() -> std::rc::Rc<$crate::portable::EvidentRunner> {
            thread_local! {
                static ENGINE: std::cell::RefCell<Option<std::rc::Rc<$crate::portable::EvidentRunner>>> =
                    const { std::cell::RefCell::new(None) };
            }
            ENGINE.with(|cell| {
                let mut slot = cell.borrow_mut();
                if slot.is_none() {
                    *slot = Some(std::rc::Rc::new(
                        $crate::portable::EvidentRunner::load($pass, $fsm)
                            .unwrap_or_else(|e| panic!("{}: {e}", $pass))));
                }
                slot.as_ref().unwrap().clone()
            })
        }
    };
}

/// Like [`cached_runner`] but with a re-entrancy guard, for ports that run
/// **on the load path**: building the runner loads the pass file, and that
/// load re-runs this same production hook over the pass's own schemas —
/// re-entering the port mid-build. The generated accessor returns `None`
/// while bootstrapping (set during the build), so the caller short-circuits
/// to its no-op verdict; once built, the guard is clear and every call gets a
/// real runner. The pass files are trusted, hand-verified stdlib that the
/// no-op verdict is correct for (they construct AST enum *values*, never
/// trigger the thing the pass checks for).
macro_rules! guarded_runner {
    ($name:ident, $pass:expr, $fsm:expr) => {
        fn $name() -> Option<std::rc::Rc<$crate::portable::EvidentRunner>> {
            thread_local! {
                static ENGINE: std::cell::RefCell<Option<std::rc::Rc<$crate::portable::EvidentRunner>>> =
                    const { std::cell::RefCell::new(None) };
                static BOOTSTRAPPING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
            }
            if BOOTSTRAPPING.with(|b| b.get()) {
                return None;
            }
            Some(ENGINE.with(|cell| {
                let mut slot = cell.borrow_mut();
                if slot.is_none() {
                    // The build loads the pass; that load re-enters this hook
                    // for each pass schema. Guard the window so the re-entry
                    // short-circuits before touching `ENGINE` again.
                    BOOTSTRAPPING.with(|b| b.set(true));
                    let built = $crate::portable::EvidentRunner::load($pass, $fsm)
                        .unwrap_or_else(|e| panic!("{}: {e}", $pass));
                    BOOTSTRAPPING.with(|b| b.set(false));
                    *slot = Some(std::rc::Rc::new(built));
                }
                slot.as_ref().unwrap().clone()
            }))
        }
    };
}

// ─────────────────────────────────────────────────────────────────────
// Shared marshaling-out helpers
// ─────────────────────────────────────────────────────────────────────

/// Wrap an already-marshaled AST `Value` as a unified walk node — the seed
/// shape every walk-FSM expects (`Work::WExpr(Expr)`, `PWork::WBody(...)`,
/// …). `run_nested`'s coerce then seeds it into the FSM's `Seed` constructor.
pub(crate) fn work_node(enum_name: &str, variant: &str, inner: Value) -> Value {
    Value::Enum {
        enum_name: enum_name.to_string(),
        variant: variant.to_string(),
        fields: vec![inner],
    }
}

/// Run `fsm` over `seed` and return the single payload of a `<done>(payload)`
/// halt. Logs + returns `None` on any non-`<done>` halt or run error — the
/// uniform handling every port used to hand-roll.
pub(crate) fn run_done_payload(
    runner: &EvidentRunner,
    fsm: &str,
    seed: Value,
    done: &str,
    ctx: &str,
) -> Option<Value> {
    match runner.run_fsm(fsm, seed) {
        Ok(Value::Enum { variant, fields, .. }) if variant == done && fields.len() == 1 => {
            Some(fields[0].clone())
        }
        Ok(other) => {
            eprintln!("[{ctx}] {fsm} returned a non-{done} state: {other:?}");
            None
        }
        Err(e) => {
            eprintln!("[{ctx}] {fsm} failed: {e}");
            None
        }
    }
}

/// Run `fsm` to a `<done>(List)` halt and decode the single payload cons-list
/// with `(list_enum, nil, cons, elem)`. Logs + returns empty on any
/// non-`<done>` halt, run error, or decode failure. The general decode-out
/// helper for every "FSM accumulates a cons-list" port.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_done_list<T>(
    runner: &EvidentRunner,
    fsm: &str,
    seed: Value,
    done: &str,
    ctx: &str,
    list_enum: &'static str,
    nil: &str,
    cons: &str,
    elem: impl Fn(&Value) -> Result<T, DecodeError>,
) -> Vec<T> {
    let Some(payload) = run_done_payload(runner, fsm, seed, done, ctx) else {
        return Vec::new();
    };
    decode_list(&payload, list_enum, nil, cons, elem).unwrap_or_else(|e| {
        eprintln!("[{ctx}] decode of {list_enum} failed: {e}");
        Vec::new()
    })
}

/// The most common decode-out: a `<done>(NameList)` halt → `Vec<String>` of
/// raw identifier strings (head-first, as the FSM accumulated them). Used by
/// validate, subscriptions, generics, and inject's collection walk.
pub(crate) fn run_name_list(runner: &EvidentRunner, fsm: &str, seed: Value, done: &str, ctx: &str)
    -> Vec<String>
{
    run_done_list(runner, fsm, seed, done, ctx, "NameList", "NameNil", "NameCons", decode_str)
}

// ─────────────────────────────────────────────────────────────────────
// The substantial ports keep their own files; pretty keeps its swap.
// ─────────────────────────────────────────────────────────────────────

pub mod desugar;
pub mod generics;
pub mod inject;
pub mod pretty;

// ─────────────────────────────────────────────────────────────────────
// validate — load-time external-only check
// ─────────────────────────────────────────────────────────────────────

/// `validate` — load-time external-only check. **Sole implementation: the
/// self-hosted Evident stack-FSM walk** (`stdlib/passes/validate.ev`,
/// `validate_walk`).
///
/// Reject non-`external` schemas that construct FFI effects (`FFICall`,
/// `FFIOpen`, `FFILookup`, `LibCall`). The WHOLE Expr-tree walk runs in
/// Evident as an FSM-with-stack; this shim marshals each `Constraint`'s
/// `Expr` (shared `expr_to_value`), drives the FSM to a drained-stack halt,
/// and collects the `ECall` names it reached. The banned-set decision stays
/// in Rust ([`is_banned`]): deciding `nm ∈ {FFICall, …}` is a string equality
/// that, done INSIDE the per-tick Z3 solve, blows up Z3's string theory on a
/// string-heavy walk state (the in-solve cousin of gap #18). A per-thread,
/// guarded runner avoids the bootstrap cycle (loading `validate.ev` re-runs
/// the validate hook over its own schemas).
pub mod validate {
    use super::{run_name_list, work_node, EvidentRunner};
    use crate::core::ast::{BodyItem, Expr, Keyword, SchemaDecl};

    guarded_runner!(runner, "passes/validate.ev", "validate_walk");

    /// `kind` label used in the diagnostic — must match the wording the
    /// canonical impl shipped.
    fn keyword_label(kw: &Keyword) -> &'static str {
        match kw {
            Keyword::Fsm => "fsm",
            Keyword::Type => "type",
            Keyword::Claim => "claim",
            Keyword::Schema => "schema",
            Keyword::Subclaim => "subclaim",
        }
    }

    /// Format the diagnostic, byte-for-byte the canonical wording.
    pub(crate) fn error_msg(kind: &str, name: &str, call: &str) -> String {
        format!(
            "{kind} `{name}` constructs `{call}(...)` but isn't \
             declared `external`. Either mark this declaration \
             `external claim` / `external type`, or move the \
             FFI into an `external claim` helper and call it \
             from here."
        )
    }

    /// The leaf decision the FSM defers to Rust: is `name` one of the four
    /// banned FFI-construction primitives? A 4-element set membership, kept
    /// out of the per-tick Z3 solve.
    fn is_banned(name: &str) -> bool {
        matches!(name, "FFICall" | "FFIOpen" | "FFILookup" | "LibCall")
    }

    /// First banned FFI call (pre-order) constructed by `e`, or `None`. The
    /// FSM returns `SVDone(NameList)` head-first (reverse pre-order); reversing
    /// recovers pre-order so the first banned name matches the canonical walk.
    fn find_banned(runner: &EvidentRunner, e: &Expr) -> Option<String> {
        let seed = work_node("Work", "WExpr", crate::translate::ast_encoder::expr_to_value(e));
        let names = run_name_list(runner, "validate_walk", seed, "SVDone", "validate/evident");
        names.iter().rev().find(|n| is_banned(n)).cloned()
    }

    fn check(runner: &EvidentRunner, s: &SchemaDecl) -> Result<(), String> {
        if s.external {
            return Ok(());
        }
        for item in &s.body {
            if let BodyItem::Constraint(e) = item {
                if let Some(call) = find_banned(runner, e) {
                    return Err(error_msg(keyword_label(&s.keyword), &s.name, &call));
                }
            }
        }
        Ok(())
    }

    /// Enforce the external-only rule on one schema via the self-hosted
    /// `validate_walk` pass. **The runtime's sole validate entry point** —
    /// `runtime::validate::enforce_external_only` (on the load path) delegates
    /// here. During the engine build the guarded runner short-circuits to
    /// `Ok(())` (the trusted pass file constructs `Expr` values, never calls
    /// FFI).
    pub fn enforce_external_only(s: &SchemaDecl) -> Result<(), String> {
        let Some(runner) = runner() else { return Ok(()) };
        check(&runner, s)
    }
}

// ─────────────────────────────────────────────────────────────────────
// subscriptions — static world-access-set inference
// ─────────────────────────────────────────────────────────────────────

/// `subscriptions` — static world-access-set inference for the multi-FSM
/// scheduler. **Sole implementation: the self-hosted Evident pass**
/// (`stdlib/passes/subscriptions.ev`, `subscriptions_walk`).
///
/// The WHOLE walk (into sub-exprs AND subclaim bodies) runs in Evident; this
/// shim drives it per top-level body item (keeping the per-tick marshaled
/// state small — the O(N) vs O(N²) difference on Mario's `game`) and folds
/// the reachable identifiers in. The `world.`/`world_next.` classification
/// stays in Rust ([`classify`]): it needs `strip_prefix` / first-segment, and
/// Evident has no substring operator. No bootstrap guard is needed —
/// `subscriptions_walk` reads no `world.X`, so the analysis never needs
/// subscriptions for itself, and it runs off the load path (the scheduler
/// calls it, and `run_nested` never re-enters the scheduler).
pub mod subscriptions {
    use super::{run_name_list, work_node};
    use crate::core::ast::SchemaDecl;
    use crate::subscriptions::AccessSets;
    use crate::translate::ast_encoder::body_item_to_value;

    cached_runner!(runner, "passes/subscriptions.ev", "subscriptions_walk");

    /// First dotted segment of `s` (`player.pos.x` → `player`).
    fn first_segment(s: &str) -> &str {
        s.split('.').next().unwrap_or(s)
    }

    /// Classify one raw dotted identifier into the read/write sets: a
    /// `world_next.X…` access writes the top-level field `X`, a `world.X…`
    /// access reads it, anything else contributes nothing.
    fn classify(name: &str, sets: &mut AccessSets) {
        if let Some(field) = name.strip_prefix("world_next.") {
            sets.writes.insert(first_segment(field).to_string());
        } else if let Some(field) = name.strip_prefix("world.") {
            sets.reads.insert(first_segment(field).to_string());
        }
    }

    /// World access sets for one claim, computed by the self-hosted
    /// `subscriptions_walk` pass. **The runtime's sole subscriptions entry
    /// point** — the scheduler ([`crate::effect_loop`]) calls it to wake FSMs
    /// on read-set deltas and scope multi-writer snapshots. reads/writes is a
    /// set union over body items, so per-item-then-union is identical to
    /// walking the whole body in one pass.
    pub fn access_sets(claim: &SchemaDecl) -> AccessSets {
        let runner = runner();
        let mut sets = AccessSets::default();
        for item in &claim.body {
            let seed = work_node("Work", "WBody", body_item_to_value(item));
            for name in run_name_list(&runner, "subscriptions_walk", seed, "SWDone",
                                      &format!("subscriptions/evident `{}`", claim.name))
            {
                classify(&name, &mut sets);
            }
        }
        sets
    }
}

// ─────────────────────────────────────────────────────────────────────
// toposort — effect-dispatch ordering (integer-rank ToposortRanks claim)
// ─────────────────────────────────────────────────────────────────────

/// `toposort` — effect-dispatch ordering. **Sole implementation: the
/// self-hosted Evident `ToposortRanks` claim** (`stdlib/toposort.ev`).
///
/// The dispatcher's node names are arbitrary identities, so this shim maps
/// them to `0..n-1` and queries the integer-rank `ToposortRanks` claim (edges
/// index a rank array directly, O(1) per edge — ~19ms on a Mario-scale graph
/// vs 13–42s for the depth-n `position_of` of the generic `Toposort<T>`). The
/// string↔int mapping and the rank→order inversion are pure Rust leaf
/// marshaling; the integers never escape this module. Cycle recovery (a
/// cyclic graph is UNSAT → `None`) is the caller's policy, not the algorithm,
/// so it stays in `effect_loop::toposort::cycle_recovery`. No bootstrap guard:
/// toposort runs only on the dispatch path, which nothing in the load path
/// re-enters.
pub mod toposort {
    use crate::core::Value;
    use std::collections::HashMap;

    cached_runner!(runner, "toposort.ev", "");

    /// Order `nodes` so every `(from, to)` edge has `from` earlier than `to`.
    /// `Some(ordering)` on an acyclic graph; `None` if the graph has a cycle
    /// (UNSAT) or the result can't be decoded. **The runtime's sole
    /// effect-ordering entry point** — `effect_loop::collect` calls it once
    /// per unique effect-graph shape (cached thereafter).
    pub fn toposort(nodes: &[String], edges: &[(String, String)]) -> Option<Vec<String>> {
        let n = nodes.len();
        if n == 0 {
            return Some(Vec::new());
        }
        let runner = runner();

        // Leaf marshaling: each node name → its contiguous index. Names are
        // unique by construction, so position IS a stable identity.
        let idx: HashMap<&str, i64> = nodes.iter().enumerate()
            .map(|(i, name)| (name.as_str(), i as i64)).collect();

        // Edges become Edge<Int> over the node indices; skip strays.
        let edge_vals: Vec<HashMap<String, Value>> = edges.iter()
            .filter_map(|(f, t)| {
                let (fi, ti) = (*idx.get(f.as_str())?, *idx.get(t.as_str())?);
                let mut m = HashMap::new();
                m.insert("from".into(), Value::Int(fi));
                m.insert("to".into(), Value::Int(ti));
                Some(m)
            }).collect();

        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert("n".into(), Value::Int(n as i64));
        given.insert("edges".into(), Value::SeqComposite(edge_vals));

        let r = runner.rt().query("ToposortRanks", &given).ok()?;
        if !r.satisfied {
            return None; // cyclic graph → UNSAT
        }
        let Some(Value::SeqInt(pos)) = r.bindings.get("pos") else { return None };
        if pos.len() != n {
            return None;
        }

        // `pos[k]` is node k's output rank; invert to an ordering by sorting
        // the node indices on ascending rank.
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&k| pos[k]);
        Some(order.into_iter().map(|k| nodes[k].clone()).collect())
    }
}

// ─────────────────────────────────────────────────────────────────────
// seq_chains — body Seq(Effect) ordering-chain extraction
// ─────────────────────────────────────────────────────────────────────

/// `seq_chains` — body `Seq(Effect)` ordering-chain extraction for the
/// dispatch-ordering path. **Sole implementation: the self-hosted Evident
/// pass** (`stdlib/passes/seq_chains.ev`, `seq_chains_walk`).
///
/// The FSM walks the body and emits each recognized `Seq(Effect)` literal's
/// RAW element `Expr`s; [`node_name`] resolves them to dispatch node names in
/// Rust (the set-membership string keying blows up Z3 string theory in-solve,
/// and the synthetic `name[i]` names need int→string `format!`). The Evident
/// walk depends only on the static body, so [`extract_seq_effect_chains`]
/// caches the raw chains by body identity ([`RAW_CACHE`]) — strictly *less*
/// per-tick work than the deleted Rust walk, which re-walked every tick.
pub mod seq_chains {
    use super::run_done_list;
    use crate::core::ast::{BodyItem, Expr};
    use crate::translate::ast_decoder::{decode_expr, decode_list};
    use crate::translate::ast_encoder::body_item_list_to_value;
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;

    cached_runner!(runner, "passes/seq_chains.ev", "seq_chains_walk");

    thread_local! {
        /// Raw chains cached by the body's identity (data-ptr + len). The
        /// Evident walk depends ONLY on the static body, so it runs once per
        /// body and the per-tick `node_name` resolution happens off this
        /// cache. A claim body's `Vec<BodyItem>` is owned by its runtime's
        /// schema table and never mutated, so `(as_ptr, len)` is a stable,
        /// O(1), exact key within a scheduler run (one program per process).
        static RAW_CACHE: RefCell<HashMap<usize, (usize, Rc<Vec<Vec<Expr>>>)>> =
            RefCell::new(HashMap::new());
    }

    /// The RAW element `Expr`s of each recognized `Seq(Effect)` literal
    /// constraint, in body order — the FSM's output before `node_name`
    /// resolution. Exposed for tests; production goes through the cache.
    pub fn raw_chains(body: &[BodyItem]) -> Vec<Vec<Expr>> {
        // ChainList (newest-first) of ExprList; reverse to recover body order
        // (the deleted Rust walk pushed chains in body order).
        let mut chains = run_done_list(
            &runner(), "seq_chains_walk", body_item_list_to_value(body),
            "SCDone", "seq_chains/evident",
            "ChainList", "ChNil", "ChCons",
            |chain| decode_list(chain, "ExprList", "ELNil", "ELCons", decode_expr),
        );
        chains.reverse();
        chains
    }

    /// Resolve a SeqLit element `Expr` to its dispatch node name, or `None` if
    /// it isn't a known node. Verbatim from the deleted Rust walk:
    ///   * `Identifier(name)` for a bare Effect binding.
    ///   * `Index(Identifier(name), Int(i))` for a synthetic `name[i]`.
    ///   * `Index(Field(Index(Identifier(outer), Int(i)), field), Int(j))` for
    ///     a synthetic `outer[i].field[j]`.
    fn node_name(e: &Expr, set: &HashSet<&String>) -> Option<String> {
        match e {
            Expr::Identifier(n) if set.contains(n) => Some(n.clone()),
            Expr::Index(seq, idx) => match seq.as_ref() {
                Expr::Identifier(name) => {
                    if let Expr::Int(i) = idx.as_ref() {
                        let syn = format!("{}[{}]", name, i);
                        if set.contains(&syn) {
                            return Some(syn);
                        }
                    }
                    None
                }
                Expr::Field(inner_seq, field) => {
                    let Expr::Index(outer_seq, outer_idx) = inner_seq.as_ref() else { return None };
                    let Expr::Identifier(outer_name) = outer_seq.as_ref() else { return None };
                    let (Expr::Int(i), Expr::Int(j)) = (outer_idx.as_ref(), idx.as_ref()) else {
                        return None;
                    };
                    let syn = format!("{}[{}].{}[{}]", outer_name, i, field, j);
                    if set.contains(&syn) { Some(syn) } else { None }
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Raw `Expr` chains for `body`, computed by the Evident walk once and
    /// cached by body identity. Cheap `Rc` clone on a hit.
    fn cached_raw_chains(body: &[BodyItem]) -> Rc<Vec<Vec<Expr>>> {
        let key = body.as_ptr() as usize;
        let len = body.len();
        if let Some(hit) = RAW_CACHE.with(|c| {
            c.borrow().get(&key).filter(|(l, _)| *l == len).map(|(_, v)| v.clone())
        }) {
            return hit;
        }
        let chains = Rc::new(raw_chains(body));
        RAW_CACHE.with(|c| c.borrow_mut().insert(key, (len, chains.clone())));
        chains
    }

    /// Ordering chains for one claim body — the runtime's sole chain-extraction
    /// entry point. Walks the body for `Seq(Effect)` literals (in Evident,
    /// cached per body), resolves each element via [`node_name`], and emits a
    /// chain only when *every* element resolves. `collect.rs` calls it per
    /// tick on the Mode-2 dispatch path.
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
            if names.len() != chain.len() {
                continue;
            }
            chains.push(names);
        }
        chains
    }

    /// Drop the per-body raw-chain cache. Never needed in the single-runtime
    /// production path; provided for tests (which reuse body allocations) and
    /// long-lived processes that reload distinct programs.
    pub fn reset_cache() {
        RAW_CACHE.with(|c| c.borrow_mut().clear());
    }
}
