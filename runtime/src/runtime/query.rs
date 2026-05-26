//! `query`, `query_cached`, and the per-component Z3-AST functionizer fast path.
//! Body is decomposed into independent components; each compiled separately; uncompilable
//! components go to a scoped cached Z3 solver. Arrangement cached as `ClaimPlan`.

use super::autotune::SolveHistory;
use crate::core::{CachedSchema, CompiledFunction, QueryResult, RuntimeError, Var, Z3Step};
use super::lenient::LenientGuard;
use super::{EvidentRuntime, Value};
use crate::translate::{build_cache, run_cached, structural_signature};
use crate::z3_eval::{collect_touched_names, extract_program_partial,
                     recompose_record_seqs, simplify_assertions};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::Instant;
use z3::ast::{Ast, Bool};
use z3::{Context, Params, SatResult, Solver, Tactic};
use z3_sys::DeclKind;

/// Returns true if the component has a defining constraint (equality, guarded implication, etc.)
/// beyond bare type-bound comparisons (`>=`, `>`, `<=`, `<`).
fn component_has_defining_assertion(assertions: &[Bool<'static>]) -> bool {
    !assertions.iter().all(|a| {
        a.safe_decl().ok()
            .map(|d| matches!(d.kind(),
                DeclKind::GE | DeclKind::GT | DeclKind::LE | DeclKind::LT))
            .unwrap_or(false)
    })
}

/// Per-claim execution plan: compiled components + scoped slow solver for the rest.
/// Cached per `(claim, given-keys)`; run by `EvidentRuntime::execute_plan`.
pub(crate) struct ClaimPlan {
    /// One compiled artifact per JIT-able component; each produces disjoint outputs.
    pub(super) compiled: Vec<Rc<dyn CompiledFunction>>,
    /// One scoped Z3 solve per uncompiled component; disjoint var sets → union cleanly.
    pub(super) slow: Vec<SlowPart>,
    /// When true, slow parts own private Z3 contexts and solve on separate threads.
    pub(super) slow_parallel: bool,
    /// Statically-resolved (`PinnedInt`) vars; injected into every result.
    pub(super) pinned_ints: Vec<(String, Value)>,
}

/// One uncompiled component's scoped Z3 solve. In the parallel case each part owns a private
/// `'static` context so parts can `check()` concurrently; sequential parts share main ctx.
pub(crate) struct SlowPart {
    /// Solver carrying only this component's assertions + env for given-pinning / extraction.
    cached: CachedSchema<'static>,
    /// The Z3 context all objects in this part live in (private or main).
    ctx: &'static Context,
    /// Output var names this solve is responsible for.
    outputs: Vec<String>,
    /// Per-worker enum registry (parallel path only); `None` on sequential or primitive-only parts.
    enums: Option<crate::core::EnumRegistry>,
}

// SAFETY: Parallel parts each own a private Z3 context touched by exactly one thread
// (1:1 part↔thread pairing). Sequential parts never leave the calling thread.
// `enums: RefCell` on a parallel part is never borrowed concurrently for the same reason.
unsafe impl Send for SlowPart {}
unsafe impl Sync for SlowPart {}

/// Max cached `(given-values → result)` entries per claim; FIFO eviction.
const VALUE_CACHE_CAP: usize = 100;

/// Cached `(input, result)` pair; `input` retained to detect hash collisions on hit.
pub(crate) struct ValueCacheSlot {
    input: HashMap<String, Value>,
    satisfied: bool,
    bindings: HashMap<String, Value>,
}

/// Per-claim cross-tick value cache keyed by `hash(given-values)`; FIFO, capped at `VALUE_CACHE_CAP`.
#[derive(Default)]
pub(crate) struct ClaimValueCache {
    entries: HashMap<u64, ValueCacheSlot>,
    order: VecDeque<u64>, // insertion order for FIFO eviction
}

/// Returns false when `EVIDENT_VALUE_CACHE=0`; memoized (hot path).
fn value_cache_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("EVIDENT_VALUE_CACHE").map(|s| s != "0").unwrap_or(true)
    })
}

/// Hash a `given` map deterministically (keys sorted; verified on hit to catch collisions).
fn hash_given_values(given: &HashMap<String, Value>) -> u64 {
    let mut keys: Vec<&String> = given.keys().collect();
    keys.sort_unstable();
    let mut h = DefaultHasher::new();
    keys.len().hash(&mut h);
    for k in keys {
        k.hash(&mut h);
        hash_value(&given[k], &mut h);
    }
    h.finish()
}

/// Feed a `Value` into a hasher. Each variant writes a discriminant tag first;
/// reals hash their bit pattern so `NaN`/`-0.0` are deterministic.
fn hash_value<H: Hasher>(v: &Value, h: &mut H) {
    match v {
        Value::Int(i)   => { 0u8.hash(h); i.hash(h); }
        Value::Real(f)  => { 1u8.hash(h); f.to_bits().hash(h); }
        Value::Bool(b)  => { 2u8.hash(h); b.hash(h); }
        Value::Str(s)   => { 3u8.hash(h); s.hash(h); }
        Value::SeqInt(xs)  => { 4u8.hash(h); xs.hash(h); }
        Value::SeqBool(xs) => { 5u8.hash(h); xs.hash(h); }
        Value::SeqStr(xs)  => { 6u8.hash(h); xs.hash(h); }
        Value::Composite(m) => { 7u8.hash(h); hash_value_map(m, h); }
        Value::SeqComposite(ms) => {
            8u8.hash(h); ms.len().hash(h);
            for m in ms { hash_value_map(m, h); }
        }
        Value::SeqEnum(es) => {
            9u8.hash(h); es.len().hash(h);
            for e in es { hash_value(e, h); }
        }
        Value::SetInt(xs)  => { 10u8.hash(h); xs.hash(h); }
        Value::SetBool(xs) => { 11u8.hash(h); xs.hash(h); }
        Value::SetStr(xs)  => { 12u8.hash(h); xs.hash(h); }
        Value::Enum { enum_name, variant, fields } => {
            13u8.hash(h);
            enum_name.hash(h);
            variant.hash(h);
            fields.len().hash(h);
            for f in fields { hash_value(f, h); }
        }
    }
}

fn hash_value_map<H: Hasher>(m: &HashMap<String, Value>, h: &mut H) {
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort_unstable();
    keys.len().hash(h);
    for k in keys {
        k.hash(h);
        hash_value(&m[k], h);
    }
}

enum ComponentOutcome {
    Compiled(Rc<dyn CompiledFunction>),
    /// Couldn't compile; safe to solve in the scoped slow part.
    Slow,
    /// Gap-fill refused: output has no safe definition → fall through to non-lenient `evaluate`.
    Bail,
}

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind { parent: (0..n).collect(), rank: vec![0; n] }
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.parent[r] != r { r = self.parent[r]; }
        let mut y = x;
        while self.parent[y] != r {
            let next = self.parent[y];
            self.parent[y] = r;
            y = next;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb { return; }
        if self.rank[ra] < self.rank[rb] {
            self.parent[ra] = rb;
        } else if self.rank[ra] > self.rank[rb] {
            self.parent[rb] = ra;
        } else {
            self.parent[rb] = ra;
            self.rank[ra] += 1;
        }
    }
}

/// Decompose simplified assertions into independent components. Connectivity nodes = outputs +
/// intermediates (NOT broadcast=givens/constants). Intermediates must be nodes: Mario's
/// `draw_rect__rect_buf__callN` bridges `mario.rects` and `mario_effs`; missing it
/// left rect x/y free → invisible sprite. Returns (comp_vars, comp_assert_idx, global_idx).
fn decompose_simplified(
    simplified: &[Bool<'static>],
    outputs: &[String],
    broadcast: &HashSet<String>,
) -> (Vec<Vec<String>>, Vec<Vec<usize>>, Vec<usize>) {
    // Outputs interned first (indices 0..n, deterministic ordering); intermediates discovered later.
    let mut node_of: HashMap<String, usize> = HashMap::with_capacity(outputs.len());
    for o in outputs {
        let n = node_of.len();
        node_of.entry(o.clone()).or_insert(n);
    }
    // Fold `s__len`/`s__arr` back to base name so length pins join the same component as elements.
    let mut per_assert: Vec<Vec<usize>> = Vec::with_capacity(simplified.len());
    for a in simplified {
        let mut touched: HashSet<String> = HashSet::new();
        collect_touched_names(a, &mut touched);
        let mut idxs: Vec<usize> = Vec::with_capacity(touched.len());
        for raw in &touched {
            let base = raw.strip_suffix("__len")
                .or_else(|| raw.strip_suffix("__arr"))
                .unwrap_or(raw.as_str());
            if broadcast.contains(base) { continue; }
            let id = match node_of.get(base) {
                Some(&i) => i,
                None => {
                    let i = node_of.len();
                    node_of.insert(base.to_string(), i);
                    i
                }
            };
            idxs.push(id);
        }
        idxs.sort_unstable();
        idxs.dedup();
        per_assert.push(idxs);
    }
    let mut uf = UnionFind::new(node_of.len());
    for idxs in &per_assert {
        for w in idxs.windows(2) { uf.union(w[0], w[1]); }
    }
    let mut root_to_comp: HashMap<usize, usize> = HashMap::new();
    let mut comp_vars: Vec<Vec<String>> = Vec::new();
    for (i, o) in outputs.iter().enumerate() {
        let r = uf.find(i);
        let comp = *root_to_comp.entry(r).or_insert_with(|| {
            comp_vars.push(Vec::new());
            comp_vars.len() - 1
        });
        comp_vars[comp].push(o.clone());
    }
    // Assertions with no component var (broadcast-only) go to global; so do pure-intermediate islands.
    let mut comp_assertions: Vec<Vec<usize>> = vec![Vec::new(); comp_vars.len()];
    let mut global: Vec<usize> = Vec::new();
    for (ai, idxs) in per_assert.iter().enumerate() {
        match idxs.first() {
            Some(&first) => match root_to_comp.get(&uf.find(first)) {
                Some(&comp) => comp_assertions[comp].push(ai),
                None => global.push(ai),
            },
            None => global.push(ai),
        }
    }
    (comp_vars, comp_assertions, global)
}

/// Build a tuned solver (tactic chain from `EVIDENT_TACTICS`, `smt.arith.solver` param).
fn build_tuned_solver(ctx: &'static Context, arith_solver: u32) -> Solver<'static> {
    let chain = std::env::var("EVIDENT_TACTICS").ok();
    let chain_spec = chain.as_deref().unwrap_or("solve-eqs");
    let solver = match chain_spec {
        "" | "off" => Solver::new(ctx),
        spec => {
            let mut names: Vec<&str> = match spec {
                "simplify"   => vec!["simplify"],
                "standard"   => vec!["simplify", "propagate-values", "solve-eqs"],
                "aggressive" => vec!["simplify", "propagate-values", "solve-eqs",
                                     "elim-uncnstr", "propagate-ineqs"],
                custom => custom.split(',').map(|s| s.trim()).collect(),
            };
            if !names.last().map(|n| *n == "smt").unwrap_or(false) { names.push("smt"); }
            let mut t = Tactic::new(ctx, names[0]);
            for n in &names[1..] { t = t.and_then(&Tactic::new(ctx, n)); }
            t.solver()
        }
    };
    if arith_solver != 0 {
        let mut params = Params::new(ctx);
        params.set_u32("smt.arith.solver", arith_solver);
        solver.set_params(&params);
    }
    solver
}

/// Solve one slow part with `given` pinned; returns output bindings or `None` if UNSAT.
/// Enum givens are pinned in a push frame, popped before return so the solver is reusable.
fn solve_one_part(
    part: &SlowPart,
    given: &HashMap<String, Value>,
    enums: Option<&crate::core::EnumRegistry>,
) -> Option<HashMap<String, Value>> {
    let ctx = part.ctx;
    part.cached.solver.push();
    let mut scalar_given: HashMap<String, Value> = HashMap::with_capacity(given.len());
    for (n, v) in given {
        match (part.cached.env.get(n), v) {
            (Some(Var::EnumVar { ast, .. }), Value::Enum { .. }) => {
                if let Some(dt) = enums.and_then(|e|
                    crate::translate::value_enum_to_datatype(v, ctx, e))
                {
                    part.cached.solver.assert(&ast._eq(&dt));
                }
            }
            _ => { scalar_given.insert(n.clone(), v.clone()); }
        }
    }
    let r = run_cached(&part.cached, &scalar_given, ctx, enums);
    part.cached.solver.pop(1);
    if !r.satisfied { return None; }
    let mut out: HashMap<String, Value> = HashMap::with_capacity(part.outputs.len());
    for vn in &part.outputs {
        if let Some(v) = r.bindings.get(vn) {
            out.insert(vn.clone(), v.clone());
        }
    }
    Some(out)
}

/// Global mutex serializing Z3 context creation and datatype replay (historically racy).
fn z3_setup_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::Mutex;
    static SETUP_LOCK: Mutex<()> = Mutex::new(());
    SETUP_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Mint a fresh `'static` Z3 context (leaked, same as main ctx); serialized via `z3_setup_lock`.
fn new_leaked_context() -> &'static Context {
    let _guard = z3_setup_lock();
    let cfg = z3::Config::new();
    Box::leak(Box::new(Context::new(&cfg)))
}

/// True when this part's env entries can be reproduced in a private worker context.
/// Excluded (→ sequential): record-element `DatatypeSeqVar` (main-ctx `FieldKind::Nested`
/// sorts cause cross-context panic), `DatatypeSetVar`, `EnumValue`, `EnumCtor`.
fn env_subset_translatable(
    env: &HashMap<String, Var<'static>>,
    outputs: &[String],
    given: &HashMap<String, Value>,
) -> bool {
    outputs.iter().chain(given.keys()).all(|name| {
        match env.get(name) {
            None => true,
            Some(Var::IntVar(_)) | Some(Var::RealVar(_)) | Some(Var::BoolVar(_))
            | Some(Var::StrVar(_)) | Some(Var::SeqVar { .. }) | Some(Var::SetVar { .. })
            | Some(Var::PinnedInt(_)) | Some(Var::EnumVar { .. }) => true,
            // Enum-element Seq (no nested record sorts) is translatable;
            // record-element Seq is not (see doc comment).
            Some(Var::DatatypeSeqVar { fields, .. }) => fields.is_empty(),
            Some(Var::DatatypeSetVar { .. }) | Some(Var::EnumValue { .. })
            | Some(Var::EnumCtor { .. }) => false,
        }
    })
}

/// Translate a `Var` into worker context `dst`. EnumVar / enum-element DatatypeSeqVar rebind
/// their DatatypeSort from the replayed worker registry. Returns `None` for unsupported variants.
fn translate_var(
    var: &Var<'static>,
    dst: &'static Context,
    worker_enums: Option<&crate::core::EnumRegistry>,
) -> Option<Var<'static>> {
    Some(match var {
        Var::IntVar(i)  => Var::IntVar(i.translate(dst)),
        Var::RealVar(r) => Var::RealVar(r.translate(dst)),
        Var::BoolVar(b) => Var::BoolVar(b.translate(dst)),
        Var::StrVar(s)  => Var::StrVar(s.translate(dst)),
        Var::PinnedInt(v) => Var::PinnedInt(*v),
        Var::SeqVar { arr, len, elem } => Var::SeqVar {
            arr: arr.translate(dst), len: len.translate(dst), elem: *elem,
        },
        Var::SetVar { set, elem, candidates } => Var::SetVar {
            set: set.translate(dst), elem: *elem, candidates: candidates.clone(),
        },
        Var::EnumVar { ast, enum_name, .. } => {
            let dt = worker_enums?.by_name.borrow().get(enum_name).map(|(d, _)| *d)?;
            Var::EnumVar {
                ast: ast.translate(dst),
                enum_name: enum_name.clone(),
                dt,
            }
        }
        // Enum-element Seq only (fields empty); record-element Seq → None (main-ctx bound).
        Var::DatatypeSeqVar { arr, len, type_name, fields, .. } if fields.is_empty() => {
            let dt = worker_enums?.by_name.borrow().get(type_name).map(|(d, _)| *d)?;
            Var::DatatypeSeqVar {
                arr: arr.translate(dst),
                len: len.translate(dst),
                type_name: type_name.clone(),
                dt,
                fields: Vec::new(),
            }
        }
        Var::DatatypeSeqVar { .. } | Var::DatatypeSetVar { .. }
        | Var::EnumValue { .. } | Var::EnumCtor { .. } => return None,
    })
}

impl EvidentRuntime {
    /// Functionizer fast path with a cross-tick value cache. Skips the JIT call entirely on
    /// byte-identical inputs; cache cleared on reload so stale values are impossible.
    pub(super) fn try_functionize_z3(&self, name: &str, schema: &crate::core::ast::SchemaDecl,
                          given: &HashMap<String, Value>) -> Option<QueryResult>
    {
        let vhash = if value_cache_enabled() { Some(hash_given_values(given)) } else { None };
        if let Some(h) = vhash {
            if let Some(result) = self.value_cache_get(name, h, given) {
                self.functionize_stats.borrow_mut()
                    .claims.entry(name.to_string()).or_default().value_cache_hits += 1;
                return Some(result);
            }
        }
        let result = self.functionize_z3_uncached(name, schema, given);
        if let (Some(h), Some(r)) = (vhash, &result) { // only memoize fast-path successes
            self.value_cache_put(name, h, given, r);
        }
        result
    }

    /// Read from the value cache; verifies input on hit to catch hash collisions.
    fn value_cache_get(&self, name: &str, hash: u64, given: &HashMap<String, Value>)
        -> Option<QueryResult>
    {
        let cache = self.value_cache.borrow();
        let slot = cache.get(name)?.entries.get(&hash)?;
        if slot.input == *given {
            Some(QueryResult { satisfied: slot.satisfied, bindings: slot.bindings.clone() })
        } else {
            None
        }
    }

    fn value_cache_put(&self, name: &str, hash: u64, given: &HashMap<String, Value>,
                       result: &QueryResult) {
        let mut cache = self.value_cache.borrow_mut();
        let claim = cache.entry(name.to_string()).or_default();
        if !claim.entries.contains_key(&hash) {
            claim.order.push_back(hash);
            while claim.order.len() > VALUE_CACHE_CAP {
                if let Some(old) = claim.order.pop_front() {
                    claim.entries.remove(&old);
                }
            }
        }
        claim.entries.insert(hash, ValueCacheSlot {
            input: given.clone(),
            satisfied: result.satisfied,
            bindings: result.bindings.clone(),
        });
    }

    /// Per-component functionizer: decomposes, compiles what it can, slow-solves the rest.
    /// Returns `Some(QueryResult)` on success or `None` to fall through to a full Z3 solve.
    fn functionize_z3_uncached(&self, name: &str, schema: &crate::core::ast::SchemaDecl,
                          given: &HashMap<String, Value>) -> Option<QueryResult>
    {
        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (name.to_string(), given_keys.clone());

        if let Some(entry) = self.fn_cache.borrow().get(&cache_key).cloned() { // cache hit
            let Some(plan) = entry else { return None };
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().cache_hits += 1;
            return self.execute_plan(&plan, given);
        }

        // Cache miss: build CachedSchema, simplify, decompose, compile/slow-solve per component.
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        if crate::z3_eval::has_known_translator_gap(&schema.body) {
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }
        // LENIENT so untranslatable body items become warnings; pass empty given so
        // the extracted program is generic (baking given values would break other ticks).
        let _lenient_guard = LenientGuard::enable();
        let empty_given: HashMap<String, Value> = HashMap::new();
        let cached = crate::translate::build_cache(
            schema, &self.schemas, self.z3_ctx, &self.datatypes,
            Some(&self.enums), &empty_given, arith);
        drop(_lenient_guard);
        // Z3 ASTs are reference-counted by the `'static` Context; transmute lifetime is sound.
        let assertions_local = cached.solver.get_assertions();
        let assertions: Vec<z3::ast::Bool<'static>> = unsafe {
            std::mem::transmute::<Vec<z3::ast::Bool<'_>>, Vec<z3::ast::Bool<'static>>>(
                assertions_local)
        };
        let simplify_result = simplify_assertions(self.z3_ctx, &assertions);
        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.analyses += 1;
            per.simplified_total += simplify_result.formulas.len() as u32;
        }
        if simplify_result.unsat {
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().decided_unsat += 1;
            return Some(QueryResult { satisfied: false, bindings: HashMap::new() });
        }
        let simplified = &simplify_result.formulas;

        // Outputs = env vars that appear in the simplified body, excluding givens and constants.
        // Env entries absent from the body are unconstrained (Z3 picks freely) — not outputs.
        let mut touched: std::collections::HashSet<String> = std::collections::HashSet::new();
        for a in simplified {
            crate::z3_eval::collect_touched_names(a, &mut touched);
        }
        let outputs: Vec<String> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter(|(_, v)| !matches!(v,
                crate::translate::Var::EnumValue { .. }
                | crate::translate::Var::EnumCtor { .. }
                | crate::translate::Var::PinnedInt(_)))
            .filter(|(name, _)| touched.contains(name.as_str()))
            .map(|(n, _)| n.clone())
            .collect();
        let pinned_steps: Vec<crate::core::Z3Step<'static>> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter_map(|(n, v)| match v {
                crate::translate::Var::PinnedInt(i) => Some(crate::core::Z3Step::Scalar {
                    var:  n.clone(),
                    expr: z3::ast::Dynamic::from_ast(&z3::ast::Int::from_i64(self.z3_ctx, *i)),
                }),
                _ => None,
            })
            .collect();
        let pinned_ints: Vec<(String, Value)> = cached.env.iter()
            .filter(|(name, _)| !given.contains_key(name.as_str()))
            .filter_map(|(n, v)| match v {
                crate::translate::Var::PinnedInt(i) => Some((n.clone(), Value::Int(*i))),
                _ => None,
            })
            .collect();

        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
            eprintln!("[fz/z3] {}: simplified body has {} assertions, outputs = {:?}",
                name, simplified.len(), outputs);
            for a in simplified {
                eprintln!("    {a}");
            }
        }
        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
            eprintln!("[fz/z3] {} extract pass: {} outputs, {} simplified",
                name, outputs.len(), simplified.len());
            if name == "keyboard" || std::env::var("EVIDENT_FZ_DUMP_BODY").is_ok() {
                eprintln!("[fz/z3] {name} outputs: {outputs:?}");
                for a in simplified {
                    eprintln!("  {a}");
                }
            }
        }
        if outputs.is_empty() {
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None; // no constrained outputs → fall through to slow path
        }
        // Broadcast = givens + constants (PinnedInt/enum literals); NOT connectivity nodes.
        let mut broadcast: HashSet<String> = given.keys().cloned().collect();
        for (n, v) in &cached.env {
            if matches!(v, Var::PinnedInt(_) | Var::EnumValue { .. } | Var::EnumCtor { .. }) {
                broadcast.insert(n.clone());
            }
        }
        let (comp_vars, comp_assert_idx, global_idx) =
            decompose_simplified(simplified, &outputs, &broadcast);
        let n_components = comp_vars.len();

        let mut compiled: Vec<Rc<dyn CompiledFunction>> = Vec::new();
        let mut slow_components: Vec<(Vec<String>, Vec<usize>)> = Vec::new();
        let mut n_compiled = 0u32;
        let mut bail = false;
        for (ci, cvars) in comp_vars.iter().enumerate() {
            let casserts: Vec<Bool<'static>> =
                comp_assert_idx[ci].iter().map(|&i| simplified[i].clone()).collect();
            match self.compile_one_component(name, cvars, &casserts, &cached, given, &pinned_steps) {
                ComponentOutcome::Compiled(c) => { compiled.push(c); n_compiled += 1; }
                ComponentOutcome::Slow => {
                    slow_components.push((cvars.clone(), comp_assert_idx[ci].clone()));
                }
                ComponentOutcome::Bail => { bail = true; break; }
            }
        }

        if bail { // gap-fill refused: cache body for slow path, fall through to non-lenient evaluate
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().last_extract_ok = Some(false);
            let cached_static: CachedSchema<'static> = cached;
            self.slow_path_cache.borrow_mut()
                .insert(cache_key.clone(), Rc::new(cached_static));
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }

        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.last_extract_ok = Some(true);
            per.components += n_components as u32;
            per.components_compiled += n_compiled;
            if n_compiled > 0 { per.compiled += 1; }
        }
        if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok()
            || std::env::var("EVIDENT_FUNCTIONIZE_STATS").is_ok()
        {
            eprintln!("[fz/stats] {}: components={} compiled={} slow_components={} simplified={}",
                name, n_components, n_compiled, slow_components.len(), simplified.len());
        }

        // One slow part per uncompiled component; global (broadcast-only) assertions replicated.
        // Parallel when ≥2 parts and all vars are context-translatable; sequential otherwise.
        let global_assertions: Vec<Bool<'static>> =
            global_idx.iter().map(|&i| simplified[i].clone()).collect();
        let component_assertions: Vec<Vec<Bool<'static>>> = slow_components.iter()
            .map(|(_, comp_idx)| comp_idx.iter().map(|&i| simplified[i].clone()).collect())
            .collect();
        let can_parallel = self.slow_parallel_enabled.get()
            && slow_components.len() >= 2
            && slow_components.iter().all(|(outs, _)|
                env_subset_translatable(&cached.env, outs, given));
        let (slow, slow_parallel) = if can_parallel {
            let parts = self.build_parallel_slow(
                &cached.env, &slow_components, &component_assertions,
                &global_assertions, given, arith);
            (parts, true)
        } else {
            let parts = self.build_sequential_slow(
                &cached.env, &slow_components, &component_assertions,
                &global_assertions, arith);
            (parts, false)
        };
        if (std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok()
            || std::env::var("EVIDENT_PLAN_TIMING").is_ok())
            && !slow.is_empty()
        {
            eprintln!("[plan] {} slow part(s) for {} (parallel={})",
                slow.len(), name, slow_parallel);
        }

        let plan = Rc::new(ClaimPlan { compiled, slow, slow_parallel, pinned_ints });
        self.fn_cache.borrow_mut().insert(cache_key, Some(plan.clone()));
        self.execute_plan(&plan, given)
    }

    /// Compile one component: extract → recompose → gap-fill → JIT. Returns Compiled/Slow/Bail.
    fn compile_one_component(
        &self,
        name: &str,
        comp_outputs: &[String],
        comp_assertions: &[Bool<'static>],
        cached: &CachedSchema<'static>,
        given: &HashMap<String, Value>,
        pinned_steps: &[Z3Step<'static>],
    ) -> ComponentOutcome {
        let _ = name;
        if comp_outputs.is_empty() { return ComponentOutcome::Slow; }
        // Z3 Set = characteristic function (Array→Bool); JIT would misread it. Use slow path.
        for v in comp_outputs {
            if matches!(cached.env.get(v),
                Some(Var::SetVar { .. }) | Some(Var::DatatypeSetVar { .. }))
            {
                return ComponentOutcome::Slow;
            }
        }
        let comp_out_vec: Vec<String> = comp_outputs.to_vec();
        let Some((mut program, mut missing)) =
            extract_program_partial(comp_assertions, &comp_out_vec)
        else {
            return ComponentOutcome::Slow; // extraction cycle → slow solve
        };
        // Recompose record-element Seq outputs (simplify breaks whole-element ctor into per-field).
        if !missing.is_empty() {
            recompose_record_seqs(
                comp_assertions, &mut missing, &mut program, &self.datatypes, self.z3_ctx);
        }
        if !missing.is_empty() {
            // Scoped unsafe-free check: would baking a model value be
            // unsafe for any var this component touches? It is, when the
            // var is neither given, a computed output, nor a constant —
            // its empty-given model value would be Z3's free choice,
            // wrong on later ticks.
            let mut touched: HashSet<String> = HashSet::new();
            for a in comp_assertions { collect_touched_names(a, &mut touched); }
            let output_set: HashSet<&str> = comp_out_vec.iter().map(|s| s.as_str()).collect();
            let missing_set: HashSet<&str> = missing.iter().map(|s| s.as_str()).collect();
            let mut unsafe_free = false;
            for n in &touched {
                let in_given = given.contains_key(n);
                let is_covered = output_set.contains(n.as_str())
                    && !missing_set.contains(n.as_str());
                let in_env = cached.env.contains_key(n);
                let is_const = cached.env.get(n).map(|v| matches!(v,
                    Var::PinnedInt(_) | Var::EnumValue { .. } | Var::EnumCtor { .. }))
                    .unwrap_or(false);
                if in_env && !in_given && !is_covered && !is_const {
                    unsafe_free = true;
                    break;
                }
            }
            if unsafe_free {
                // Has a defining constraint → slow solve recovers it; only type bounds → bail.
                if component_has_defining_assertion(comp_assertions) {
                    return ComponentOutcome::Slow;
                }
                return ComponentOutcome::Bail;
            }
            // Safe to gap-fill: missing outputs are constants (e.g. Mario's `platforms`).
            if !matches!(cached.solver.check(), SatResult::Sat) {
                return ComponentOutcome::Bail;
            }
            let Some(model) = cached.solver.get_model() else {
                return ComponentOutcome::Bail;
            };
            let mut prebaked: Vec<Z3Step<'static>> = Vec::with_capacity(missing.len());
            for var_name in &missing {
                let Some(var) = cached.env.get(var_name) else {
                    return ComponentOutcome::Bail;
                };
                let mut tmp: HashMap<String, Value> = HashMap::new();
                crate::translate::extract_binding(
                    var_name, var, &model, self.z3_ctx, &mut tmp, Some(&self.enums));
                let Some(value) = tmp.remove(var_name) else {
                    return ComponentOutcome::Bail;
                };
                prebaked.push(Z3Step::PreBaked { var: var_name.clone(), value });
            }
            let mut all = prebaked;
            all.append(&mut program.steps);
            program.steps = all;
        }
        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.steps_total      += program.steps.len() as u32;
            per.checks_total     += program.checks.len() as u32;
            per.predicates_total += program.predicates.len() as u32;
        }
        let mut all = pinned_steps.to_vec();
        all.append(&mut program.steps);
        program.steps = all;
        program.label = Some(name.to_string());
        match self.functionizer.compile(&program, &self.enums, &self.datatypes) {
            Some(c) => ComponentOutcome::Compiled(c),
            None => ComponentOutcome::Slow, // codegen refused (e.g. Guarded step)
        }
    }

    /// Execute a `ClaimPlan`: run compiled components then solve slow parts. Returns `None`
    /// to fall through to a full Z3 solve if any component bails or slow part is UNSAT.
    fn execute_plan(&self, plan: &ClaimPlan, given: &HashMap<String, Value>)
        -> Option<QueryResult>
    {
        let mut out: HashMap<String, Value> = HashMap::new();
        for (k, v) in &plan.pinned_ints { // pinned ints sit in no component; inject first
            if !given.contains_key(k) { out.insert(k.clone(), v.clone()); }
        }
        for c in &plan.compiled {
            let bindings = c.call(given)?;
            for (k, v) in bindings {
                if !given.contains_key(&k) { out.insert(k, v); }
            }
        }
        let slow_bindings = self.solve_slow_parts(&plan.slow, plan.slow_parallel, given)?;
        for (k, v) in slow_bindings {
            if !given.contains_key(&k) { out.insert(k, v); }
        }
        for (k, v) in given { out.insert(k.clone(), v.clone()); }
        Some(QueryResult { satisfied: true, bindings: out })
    }

    /// Solve slow parts and merge; returns `None` if any part is UNSAT.
    /// Parallel when ≥2 parts and plan was built parallel; sequential otherwise.
    fn solve_slow_parts(&self, parts: &[SlowPart], parallel: bool,
                        given: &HashMap<String, Value>)
        -> Option<HashMap<String, Value>>
    {
        if parts.is_empty() { return Some(HashMap::new()); }
        let timing = std::env::var("EVIDENT_PLAN_TIMING").is_ok();

        if parallel && parts.len() >= 2 {
            // Enum-bearing parts use their own replayed EnumRegistry (private ctx, !Sync RefCell).
            let results: Vec<Option<HashMap<String, Value>>> =
                std::thread::scope(|scope| {
                    let handles: Vec<_> = parts.iter().map(|part| {
                        scope.spawn(move || {
                            let t0 = Instant::now();
                            let r = solve_one_part(part, given, part.enums.as_ref());
                            if timing {
                                eprintln!("[plan] ‖ slow part outputs={} in {:.2}ms",
                                    part.outputs.len(),
                                    t0.elapsed().as_secs_f64() * 1000.0);
                            }
                            r
                        })
                    }).collect();
                    handles.into_iter()
                        .map(|h| h.join().unwrap_or(None)) // worker panic → None → full Z3 solve
                        .collect()
                });
            let mut merged: HashMap<String, Value> = HashMap::new();
            for r in results {
                merged.extend(r?);
            }
            return Some(merged);
        }

        let mut merged: HashMap<String, Value> = HashMap::new();
        for part in parts {
            let t0 = Instant::now();
            let part_out = solve_one_part(part, given, Some(&self.enums))?;
            if timing {
                eprintln!("[plan] slow part outputs={} in {:.2}ms",
                    part.outputs.len(), t0.elapsed().as_secs_f64() * 1000.0);
            }
            merged.extend(part_out);
        }
        Some(merged)
    }

    /// Build slow parts sharing the main context; solved sequentially by `solve_slow_parts`.
    fn build_sequential_slow(
        &self,
        env: &HashMap<String, Var<'static>>,
        components: &[(Vec<String>, Vec<usize>)],
        component_assertions: &[Vec<Bool<'static>>],
        global_assertions: &[Bool<'static>],
        arith: u32,
    ) -> Vec<SlowPart> {
        let ctx = self.z3_ctx;
        components.iter().enumerate().map(|(ci, (outputs, _))| {
            let solver = build_tuned_solver(ctx, arith);
            for a in &component_assertions[ci] { solver.assert(a); }
            for a in global_assertions { solver.assert(a); }
            SlowPart {
                cached: CachedSchema { env: env.clone(), solver, arith_solver: arith },
                ctx,
                outputs: outputs.clone(),
                enums: None, // sequential: solve_slow_parts passes &self.enums directly
            }
        }).collect()
    }

    /// Build parallel slow parts: each gets a private context. Enum-bearing components get
    /// a replayed EnumRegistry (replay before assertion translation so sorts unify).
    fn build_parallel_slow(
        &self,
        env: &HashMap<String, Var<'static>>,
        components: &[(Vec<String>, Vec<usize>)],
        component_assertions: &[Vec<Bool<'static>>],
        global_assertions: &[Bool<'static>],
        given: &HashMap<String, Value>,
        arith: u32,
    ) -> Vec<SlowPart> {
        components.iter().enumerate().map(|(ci, (outputs, _))| {
            let needs_enums = outputs.iter().chain(given.keys()).any(|name|
                matches!(env.get(name),
                    Some(Var::EnumVar { .. }) | Some(Var::DatatypeSeqVar { .. })));
            let (ctx, wenums) = { // z3_setup_lock prevents concurrent context/type construction
                let _guard = z3_setup_lock();
                let cfg = z3::Config::new();
                let ctx: &'static Context = Box::leak(Box::new(Context::new(&cfg)));
                let wenums = if needs_enums { Some(self.replay_enums_into(ctx)) } else { None };
                (ctx, wenums)
            };
            let solver = build_tuned_solver(ctx, arith);
            for a in &component_assertions[ci] { solver.assert(&a.translate(ctx)); }
            for a in global_assertions { solver.assert(&a.translate(ctx)); }
            let mut part_env: HashMap<String, Var<'static>> = HashMap::new();
            for name in outputs.iter().chain(given.keys()) {
                if part_env.contains_key(name) { continue; }
                if let Some(var) = env.get(name) {
                    // translatable precondition guarantees Some(_).
                    if let Some(tv) = translate_var(var, ctx, wenums.as_ref()) {
                        part_env.insert(name.clone(), tv);
                    }
                }
            }
            SlowPart {
                cached: CachedSchema { env: part_env, solver, arith_solver: arith },
                ctx,
                outputs: outputs.clone(),
                enums: wenums, // None for primitive-only parts
            }
        }).collect()
    }

    /// Replay enum definitions into `ctx`; called under `z3_setup_lock` by `build_parallel_slow`.
    /// Errors silently leave the registry partial → caller falls back to main-context solve.
    fn replay_enums_into(&self, ctx: &'static Context) -> crate::core::EnumRegistry {
        let wenums = crate::core::EnumRegistry::new();
        let _ = super::register_enums::register_enums(&self.program.enums, ctx, &wenums);
        wenums
    }

    /// Tier-1 accelerator: collapse `run(fsm, init)` via affine unroll + JIT.
    /// Returns `Ok(None)` on any refusal (branching, non-Int state, codegen refusal) — clean fall-through.
    pub fn tier1_run(&self, fsm: &str, init: &Value)
        -> Result<Option<Value>, RuntimeError>
    {
        let Value::Int(init_i) = init else { return Ok(None) }; // v1: Int state only
        let max_unroll = std::env::var("EVIDENT_TIER1_MAX_UNROLL").ok()
            .and_then(|s| s.parse::<u64>().ok());
        let tier1 = crate::fsm_unroll::collapse_run(
            fsm, *init_i, self.z3_ctx, &self.schemas, &self.datatypes,
            Some(&self.enums), max_unroll)
            .map_err(|e| RuntimeError::Parse(e.to_string()))?;
        let Some(t1) = tier1 else { return Ok(None) }; // detector / cap refused

        let trace = std::env::var("EVIDENT_FUNCTIONIZE_STATS").is_ok()
            || std::env::var("EVIDENT_FSM_UNROLL_TRACE").is_ok();

        let Some(compiled) =
            self.functionizer.compile(&t1.program, &self.enums, &self.datatypes)
        else {
            if trace {
                eprintln!("[tier1] {fsm}: functionizer refused the collapsed \
                           program — falling through to tier 3");
            }
            return Ok(None);
        };
        if trace {
            eprintln!("[tier1] {fsm}: comp=1/1 fn✓ — collapsed F^{} program \
                       ({} nodes) JIT'd via {}",
                       t1.k, t1.nodes, self.functionizer.name());
        }

        let mut given: HashMap<String, Value> = HashMap::new();
        given.insert(t1.input_name.clone(), Value::Int(*init_i));
        let Some(bindings) = compiled.call(&given) else { return Ok(None) }; // guard bail → fall-through
        Ok(bindings.get(&t1.output_name).cloned())
    }

    /// Evaluate the named schema; `given` pre-binds variables.
    pub fn query(&self, name: &str, given: &HashMap<String, Value>) -> Result<QueryResult, RuntimeError> {
        let base = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let resolved = self.resolve_runs(base, given)?;
        let schema = resolved.as_ref().unwrap_or(base);
        // Skip JIT for run-containing bodies: plan keys on given-KEYS but run literals
        // depend on given-VALUES, so a cached plan could return stale values.
        let functionize_on = resolved.is_none()
            && std::env::var("EVIDENT_FUNCTIONIZE").map(|s| s != "0").unwrap_or(true);
        if functionize_on {
            if let Some(result) = self.try_functionize_z3(name, schema, given) {
                if std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok() {
                    eprintln!("[fz/z3] HIT {}", name);
                }
                return Ok(result);
            }
        }

        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Like `query` but reuses a cached Z3 solver (push/pop per call). Cache is rebuilt when
    /// structural givens (quantifier bounds) change; non-structural changes re-assert in place.
    pub fn query_cached(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<QueryResult, RuntimeError>
    {
        let base = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let schema = match self.resolve_runs(base, given)? { // resolve run() before caching
            Some(rewritten) => rewritten,
            None => base.clone(), // cheap: SchemaDecl is small + Arc-friendly clones
        };
        let cur_sig = structural_signature(&schema.body, given);

        let arith_solver = { // auto-tuner picks current config
            let mut hist = self.solve_history.borrow_mut();
            hist.entry(name.to_string()).or_insert_with(SolveHistory::new)
                .current_config()
        };

        let mut cache = self.cache.borrow_mut();
        let needs_rebuild = match cache.get(name) {
            Some((cached, cached_sig)) =>
                cached_sig != &cur_sig || cached.arith_solver != arith_solver,
            None => true,
        };
        if needs_rebuild {
            if cache.contains_key(name) {
                *self.cache_rebuilds.borrow_mut() += 1;
            }
            let names = crate::translate::structural_names(&schema.body);
            let structural_given: HashMap<String, Value> = given.iter()
                .filter(|(k, _)| names.contains(k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let new_cached = build_cache(
                &schema, &self.schemas, self.z3_ctx, &self.datatypes,
                Some(&self.enums), &structural_given, arith_solver);
            cache.insert(name.to_string(), (new_cached, cur_sig));
        }
        let entry = cache.get(name).unwrap();

        let t0 = Instant::now();
        let r = run_cached(&entry.0, given, self.z3_ctx, Some(&self.enums));
        let dt = t0.elapsed();
        drop(cache); // release before potential invalidation below
        if let Some(_new_cfg) = self.solve_history.borrow_mut()
            .get_mut(name).and_then(|h| h.record(dt))
        {
            self.cache.borrow_mut().remove(name);
        }
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
}

#[cfg(test)]
mod value_hash_tests {
    use super::*;

    fn map(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn equal_maps_hash_equal_regardless_of_insertion_order() {
        let a = map(&[("x", Value::Int(1)), ("y", Value::Str("hi".into()))]);
        let mut b: HashMap<String, Value> = HashMap::new();
        b.insert("y".into(), Value::Str("hi".into()));
        b.insert("x".into(), Value::Int(1));
        assert_eq!(hash_given_values(&a), hash_given_values(&b));
    }

    #[test]
    fn distinct_values_hash_distinct() {
        let a = map(&[("x", Value::Int(1))]);
        let b = map(&[("x", Value::Int(2))]);
        assert_ne!(hash_given_values(&a), hash_given_values(&b));
    }

    #[test]
    fn enum_and_seq_values_hash_deterministically() {
        // Exercises the SeqEnum / Enum arms (Mario's `last_results`,
        // `state`): the hash is stable across calls for the same value
        // and changes when the payload changes.
        let state = Value::Enum {
            enum_name: "GameState".into(),
            variant: "Playing".into(),
            fields: vec![Value::Int(7), Value::Bool(true)],
        };
        let results = Value::SeqEnum(vec![
            Value::Enum { enum_name: "Result".into(), variant: "IntResult".into(),
                          fields: vec![Value::Int(3)] },
        ]);
        let g = map(&[("state", state.clone()), ("last_results", results.clone())]);
        assert_eq!(hash_given_values(&g), hash_given_values(&g.clone()));

        // Flip one nested field — the hash must move.
        let state2 = Value::Enum {
            enum_name: "GameState".into(),
            variant: "Playing".into(),
            fields: vec![Value::Int(8), Value::Bool(true)],
        };
        let g2 = map(&[("state", state2), ("last_results", results)]);
        assert_ne!(hash_given_values(&g), hash_given_values(&g2));
    }

    #[test]
    fn variant_tag_distinguishes_same_payload() {
        // Two enums with identical payload but different variant must
        // not collide (the discriminant + variant name guard this).
        let a = map(&[("e", Value::Enum {
            enum_name: "E".into(), variant: "A".into(), fields: vec![Value::Int(0)] })]);
        let b = map(&[("e", Value::Enum {
            enum_name: "E".into(), variant: "B".into(), fields: vec![Value::Int(0)] })]);
        assert_ne!(hash_given_values(&a), hash_given_values(&b));
    }
}

#[cfg(test)]
mod decompose_tests {
    use super::*;
    use z3::ast::Int;

    fn leaked_ctx() -> &'static Context {
        let cfg = z3::Config::new();
        Box::leak(Box::new(Context::new(&cfg)))
    }

    fn name_set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    /// The Mario regression. An intermediate `t` is *defined* from
    /// output `a` (`t = a`) and *consumed* to produce output `b`
    /// (`b = t`) — no single assertion mentions both `a` and `b`. The
    /// old outputs-only union-find split them into two components, so
    /// the component that owned `b` (and went to the scoped slow solve)
    /// never saw `t`'s definition — exactly how Mario's rect-buffer
    /// position (`draw_rect__rect_buf__callN` defined from `mario.rects`,
    /// consumed by the `mario_effs` LibCall) was left free → invisible.
    /// With intermediates as connectivity nodes, `a`, `t`, `b` stay in
    /// one component.
    #[test]
    fn intermediate_bridges_outputs_into_one_component() {
        let ctx = leaked_ctx();
        let a = Int::new_const(ctx, "a");
        let b = Int::new_const(ctx, "b");
        let t = Int::new_const(ctx, "t"); // intermediate: not an output
        let asserts: Vec<Bool<'static>> = vec![t._eq(&a), b._eq(&t)];
        let outputs = vec!["a".to_string(), "b".to_string()];
        let (comp_vars, _, global) =
            decompose_simplified(&asserts, &outputs, &HashSet::new());
        assert_eq!(comp_vars.len(), 1,
            "a and b must share one component (bridged by intermediate t)");
        let comp: HashSet<&str> = comp_vars[0].iter().map(|s| s.as_str()).collect();
        assert!(comp.contains("a") && comp.contains("b"));
        assert!(global.is_empty());
    }

    /// The test_29 perf protection. A *broadcast* variable (given or
    /// statically-known constant) is pinned into every part, so it must
    /// NOT be a connectivity node. Two outputs that share only a
    /// broadcast var stay in SEPARATE components — otherwise every
    /// independent chain reading a shared given / constant (test_29's
    /// `tick`, a `LEVEL_W`) would collapse into one slow solve.
    #[test]
    fn broadcast_var_does_not_bridge_components() {
        let ctx = leaked_ctx();
        let a = Int::new_const(ctx, "a");
        let b = Int::new_const(ctx, "b");
        let g = Int::new_const(ctx, "g"); // broadcast (given / constant)
        let asserts: Vec<Bool<'static>> = vec![a._eq(&g), b._eq(&g)];
        let outputs = vec!["a".to_string(), "b".to_string()];
        let (comp_vars, _, _) =
            decompose_simplified(&asserts, &outputs, &name_set(&["g"]));
        assert_eq!(comp_vars.len(), 2,
            "a and b must stay independent — g is broadcast, not a node");
    }

    /// Genuinely-independent outputs (distinct intermediates, no shared
    /// var) stay in separate components — the fix only merges what is
    /// actually connected.
    #[test]
    fn distinct_intermediates_stay_separate() {
        let ctx = leaked_ctx();
        let x = Int::new_const(ctx, "x");
        let y = Int::new_const(ctx, "y");
        let p = Int::new_const(ctx, "p");
        let q = Int::new_const(ctx, "q");
        let asserts: Vec<Bool<'static>> = vec![x._eq(&p), y._eq(&q)];
        let outputs = vec!["x".to_string(), "y".to_string()];
        let (comp_vars, _, _) =
            decompose_simplified(&asserts, &outputs, &HashSet::new());
        assert_eq!(comp_vars.len(), 2);
    }
}
