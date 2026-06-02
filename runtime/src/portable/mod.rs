//! Self-hosted runtime passes: Rust entry points delegating to Evident
//! `stdlib/passes/*.ev`. Steady-state cost is a JIT call + marshaling.

use std::path::Path;

use crate::core::Value;
use crate::runtime::EvidentRuntime;
use crate::translate::ast_decoder::{decode_list, decode_str, DecodeError};

/// Swap trait used only by [`pretty`], the sole port with a Rust reference impl.
pub trait Portable {
    fn impl_name(&self) -> &'static str;
}

/// An Evident pass in a private runtime, driven as a function. Build once;
/// [`cached_runner`] / [`guarded_runner`] give each port a per-thread cache.
pub(crate) struct EvidentRunner {
    rt: EvidentRuntime,
    fsm: &'static str,
    max_steps: usize,
}

impl EvidentRunner {
    /// Cap far above any realistic input; a non-terminating walk is a pass bug.
    pub(crate) const MAX_STEPS: usize = 5_000_000;

    /// Load `pass_relpath` (relative to stdlib) into a fresh runtime.
    pub(crate) fn load(pass_relpath: &str, fsm: &'static str) -> Result<Self, String> {
        let dir = crate::stdlib_path::stdlib_dir()
            .map_err(|e| format!("cannot locate stdlib to load `{pass_relpath}`: {e}"))?;
        Self::load_from(&dir, pass_relpath, fsm)
    }

    /// Like [`load`] but with `stdlib_dir` supplied — used by tests.
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

    /// Drive the primary FSM to a drained-stack halt over `seed`.
    pub(crate) fn run(&self, seed: Value) -> Result<Value, String> {
        self.run_fsm(self.fsm, seed)
    }

    /// Drive a named FSM to halt over `seed`.
    pub(crate) fn run_fsm(&self, fsm: &str, seed: Value) -> Result<Value, String> {
        crate::effect_loop::run_nested(&self.rt, fsm, seed, self.max_steps)
            .map_err(|e| format!("{fsm}: {e}"))
    }

    pub(crate) fn rt(&self) -> &EvidentRuntime {
        &self.rt
    }
}

/// Per-thread build-once accessor for an [`EvidentRunner`]. `!Send`/`!Sync`
/// (Z3 + Cranelift), so `thread_local` not a global. Panics if pass can't load.
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

/// Like [`cached_runner`] but returns `None` while bootstrapping (re-entrancy
/// guard for load-path ports). Pass files are trusted — no-op is correct.
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

/// Wrap a marshaled AST `Value` as a walk-FSM seed node.
pub(crate) fn work_node(enum_name: &str, variant: &str, inner: Value) -> Value {
    Value::Enum {
        enum_name: enum_name.to_string(),
        variant: variant.to_string(),
        fields: vec![inner],
    }
}

/// Run `fsm` over `seed`; return the single payload of a `<done>(payload)` halt,
/// or `None` (with eprintln) on any other halt or error.
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

/// Run `fsm` to a `<done>(List)` halt and decode the cons-list payload.
/// Returns empty (with eprintln) on any failure.
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

/// `<done>(NameList)` → `Vec<String>` of identifier strings (head-first).
pub(crate) fn run_name_list(runner: &EvidentRunner, fsm: &str, seed: Value, done: &str, ctx: &str)
    -> Vec<String>
{
    run_done_list(runner, fsm, seed, done, ctx, "NameList", "NameNil", "NameCons", decode_str)
}

pub mod desugar;
pub mod generics;
pub mod inject;
pub mod introspect;

/// `validate` — reject non-external FFI-constructing schemas; banned-name
/// decision in Rust (in-solve string-eq blows up Z3 — #18 cousin).
pub mod validate {
    use super::{run_name_list, work_node, EvidentRunner};
    use crate::core::ast::{BodyItem, Expr, Keyword, SchemaDecl};

    guarded_runner!(runner, "passes/validate.ev", "validate_walk");

    fn keyword_label(kw: &Keyword) -> &'static str {
        match kw {
            Keyword::Fsm => "fsm",
            Keyword::Type => "type",
            Keyword::Claim => "claim",
            Keyword::Schema => "schema",
            Keyword::Subclaim => "subclaim",
        }
    }

    pub(crate) fn error_msg(kind: &str, name: &str, call: &str) -> String {
        format!(
            "{kind} `{name}` constructs `{call}(...)` but isn't \
             declared `external`. Either mark this declaration \
             `external claim` / `external type`, or move the \
             FFI into an `external claim` helper and call it \
             from here."
        )
    }

    fn is_banned(name: &str) -> bool {
        matches!(name, "FFICall" | "FFIOpen" | "FFILookup" | "LibCall")
    }

    /// First banned FFI call (pre-order) in `e`, or `None`. FSM returns
    /// head-first; reversed to recover pre-order.
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

    /// The runtime's sole validate entry point.
    pub fn enforce_external_only(s: &SchemaDecl) -> Result<(), String> {
        let Some(runner) = runner() else { return Ok(()) };
        check(&runner, s)
    }
}

/// `subscriptions` — world-access-set inference. Per-item walk keeps state
/// small; `world.`/`world_next.` classification stays in Rust (substr op).
pub mod subscriptions {
    use super::{run_name_list, work_node};
    use crate::core::ast::SchemaDecl;
    use crate::subscriptions::AccessSets;
    use crate::translate::ast_encoder::body_item_to_value;

    cached_runner!(runner, "passes/subscriptions.ev", "subscriptions_walk");

    fn first_segment(s: &str) -> &str {
        s.split('.').next().unwrap_or(s)
    }

    fn classify(name: &str, sets: &mut AccessSets) {
        if let Some(field) = name.strip_prefix("world_next.") {
            sets.writes.insert(first_segment(field).to_string());
        } else if let Some(field) = name.strip_prefix("world.") {
            sets.reads.insert(first_segment(field).to_string());
        }
    }

    /// World access sets for one claim. The runtime's sole subscriptions entry
    /// point; the scheduler calls it to wake FSMs and scope writer snapshots.
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

/// `toposort` — effect-dispatch ordering; cyclic graph → UNSAT → `None`.
pub mod toposort {
    use crate::core::Value;
    use std::collections::HashMap;

    cached_runner!(runner, "toposort.ev", "");

    /// Order `nodes` respecting `edges`; `None` on cycle or decode failure.
    pub fn toposort(nodes: &[String], edges: &[(String, String)]) -> Option<Vec<String>> {
        let n = nodes.len();
        if n == 0 {
            return Some(Vec::new());
        }
        let runner = runner();

        let idx: HashMap<&str, i64> = nodes.iter().enumerate()
            .map(|(i, name)| (name.as_str(), i as i64)).collect();

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
            return None;
        }
        let Some(Value::SeqInt(pos)) = r.bindings.get("pos") else { return None };
        if pos.len() != n {
            return None;
        }

        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&k| pos[k]);
        Some(order.into_iter().map(|k| nodes[k].clone()).collect())
    }
}

/// `seq_chains` — `Seq(Effect)` chain extraction. `node_name` in Rust
/// (string-keying blows up Z3 in-solve). Chains cached per body identity.
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
        // Keyed by (data-ptr, len); stable because body Vecs are never mutated.
        static RAW_CACHE: RefCell<HashMap<usize, (usize, Rc<Vec<Vec<Expr>>>)>> =
            RefCell::new(HashMap::new());
    }

    /// Raw element `Expr`s of each `Seq(Effect)` literal in body order.
    pub fn raw_chains(body: &[BodyItem]) -> Vec<Vec<Expr>> {
        // FSM accumulates newest-first; reverse to recover body order.
        let mut chains = run_done_list(
            &runner(), "seq_chains_walk", body_item_list_to_value(body),
            "SCDone", "seq_chains/evident",
            "ChainList", "ChNil", "ChCons",
            |chain| decode_list(chain, "ExprList", "ELNil", "ELCons", decode_expr),
        );
        chains.reverse();
        chains
    }

    /// Resolve a SeqLit element to its dispatch node name: `Identifier(name)`,
    /// `Index(Identifier, Int)` → `name[i]`, or nested field form.
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

    /// Ordering chains for one claim body; emits a chain only when every
    /// element resolves. Called per tick on the Mode-2 dispatch path.
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

    /// Drop the raw-chain cache. For tests that reuse body allocations.
    pub fn reset_cache() {
        RAW_CACHE.with(|c| c.borrow_mut().clear());
    }
}
