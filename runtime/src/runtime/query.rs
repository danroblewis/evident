//! `query`, `query_cached`, and the per-component Z3-AST functionizer
//! fast path.
//!
//! ## Per-component compilation
//!
//! A claim's simplified body is decomposed into independent
//! sub-models (`decompose_simplified` — connected components over the
//! free variables). Each component is compiled to its own callable
//! artifact in isolation; a construct one component can't emit no
//! longer blocks the rest. The components that *do* refuse to compile
//! are gathered into one cached, scoped Z3 solver (only their
//! constraints, not the whole claim) and solved per call via
//! `run_cached`. The whole arrangement is a `ClaimPlan`, cached per
//! `(claim, given-keys)`.

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

/// Does this component carry a *defining* constraint — anything beyond
/// a bare type-bound comparison (`>=`, `>`, `<=`, `<`)? Equalities,
/// guarded implications (`or`), `select`/`len` pins, etc. all define or
/// relate the component's outputs, so the scoped slow solve can recover
/// them. A component whose every assertion is a plain comparison
/// constrains nothing beyond its declared type — its outputs are free
/// (e.g. because a defining constraint was dropped by the translator).
fn component_has_defining_assertion(assertions: &[Bool<'static>]) -> bool {
    !assertions.iter().all(|a| {
        a.safe_decl().ok()
            .map(|d| matches!(d.kind(),
                DeclKind::GE | DeclKind::GT | DeclKind::LE | DeclKind::LT))
            .unwrap_or(false)
    })
}

/// A per-claim execution plan: zero or more compiled components plus
/// an optional combined slow-path solve for the components that
/// refused to compile. Cached in `EvidentRuntime::fn_cache` per
/// `(claim, given-keys)` and run by `EvidentRuntime::execute_plan`.
pub(crate) struct ClaimPlan {
    /// One callable artifact per JIT-able component. Each produces a
    /// disjoint slice of the claim's outputs from `given`.
    pub(super) compiled: Vec<Rc<dyn CompiledFunction>>,
    /// Slow path, one entry per uncompiled component. Each holds a
    /// cached solver carrying that component's assertions (plus the
    /// given-only consistency assertions, replicated into every part)
    /// and the names of the outputs it produces. Decomposition
    /// guarantees the components have disjoint variable sets, so the
    /// parts are independent and their result bindings union cleanly —
    /// which is exactly what lets them solve in parallel threads (see
    /// `execute_plan` / `solve_slow_parts`). Empty when every component
    /// compiled.
    pub(super) slow: Vec<SlowPart>,
    /// When true, every `slow` part owns a private Z3 context and may be
    /// solved on its own thread; `solve_slow_parts` fans them out. When
    /// false, all parts share the runtime's main context and must be
    /// solved sequentially (the safe fallback for claims whose slow
    /// vars don't cleanly translate to a fresh context, and the trivial
    /// path for a single-component claim like every Mario FSM). See
    /// `build_parallel_slow` / `build_sequential_slow`.
    pub(super) slow_parallel: bool,
    /// Statically-resolved integer vars (Z3 `PinnedInt`s), which sit in
    /// no component. Injected into every result so the bindings match
    /// the monolithic path, which emitted them as constant steps.
    pub(super) pinned_ints: Vec<(String, Value)>,
}

/// One uncompiled component's scoped Z3 solve. In the parallel case
/// (`build_parallel_slow`) each part owns its own private Z3 `Context`,
/// built by translating the component's assertions + the env it needs
/// out of the runtime's main context via `Ast::translate` — so distinct
/// parts can `check()` concurrently (a Z3 context is single-threaded,
/// but separate contexts are independent). In the sequential fallback
/// (`build_sequential_slow`) `ctx` is the runtime's main context and the
/// parts are solved one at a time.
pub(crate) struct SlowPart {
    /// Env (for given-pinning + model extraction) paired with a solver
    /// carrying *only* this component's assertions. Both live in `ctx`.
    cached: CachedSchema<'static>,
    /// The context every Z3 object in this part belongs to: a private
    /// leaked `'static` context in the parallel case, or the runtime's
    /// main context in the sequential case. Kept here so a private
    /// context stays alive as long as the plan (the solver + env borrow
    /// from it). One private context per part is what makes parallel
    /// `check()` sound.
    ctx: &'static Context,
    /// Output var names this solve is responsible for — this component's
    /// variables. Other env entries the solver happens to model are
    /// ignored.
    outputs: Vec<String>,
    /// Per-worker enum registry, populated only on the parallel path when
    /// the component carries an enum-typed var. Holds DatatypeSorts built
    /// in *this part's* private context (via `replay_enums_into`) so
    /// `run_cached` can pin enum givens and decode enum-typed model
    /// values without touching the runtime's main-context registry — which
    /// would be both a cross-context error and a `!Sync` `RefCell` shared
    /// across threads. `None` on the sequential path (where
    /// `solve_one_part` is handed the runtime's own `&self.enums`) and on
    /// parallel parts whose vars are all primitive.
    enums: Option<crate::core::EnumRegistry>,
}

// SAFETY: `SlowPart` holds Z3 handles (`Context`, `Solver`, `Var` ASTs)
// that the z3 crate marks neither `Send` nor `Sync` (they wrap raw
// `Z3_context` / `Z3_ast` pointers). Sharing a `&SlowPart` with a worker
// thread (what `std::thread::scope` does) is sound *only* under the
// access discipline this module enforces:
//
//   * A `SlowPart` is sent to a worker thread only when the plan was
//     built parallel (`slow_parallel == true`), in which case the part
//     owns a *private* leaked context (`ctx`) that no other part, no
//     other thread, and not the main runtime ever references. Every Z3
//     object reachable from the part (solver, env `Var` ASTs) lives in
//     that private context.
//   * `solve_slow_parts` gives each part to exactly one worker thread —
//     parts and threads are paired 1:1 — so a single Z3 context is never
//     touched by two threads concurrently, even though `solve_one_part`
//     mutates the solver (push/assert/check/pop) through a shared `&`.
//   * Parts sharing the main context (`slow_parallel == false`) are
//     never sent to a thread; they run on the calling thread only.
//   * A parallel part's `enums: Option<EnumRegistry>` holds a `RefCell`
//     (hence `!Sync`) and `&'static DatatypeSort`s built in the part's
//     OWN private context. Because the part is touched by exactly one
//     thread, that `RefCell` is never borrowed concurrently, and the
//     sorts are only ever applied to that same private context's model.
//
// Under that discipline neither auto-trait would be derived, but the
// concurrency it would forbid never happens, so the impls are sound.
unsafe impl Send for SlowPart {}
unsafe impl Sync for SlowPart {}

/// Max distinct `(given-values → result)` entries kept per claim in the
/// cross-tick value cache. An idle FSM repeats one input set, so even a
/// handful suffices; 100 leaves room for a few alternating idle states
/// (e.g. a blinking cursor) before FIFO eviction kicks in.
const VALUE_CACHE_CAP: usize = 100;

/// One cached `(input, result)` association in the cross-tick value
/// cache. `input` is kept so a hash collision (different given, same
/// `u64`) is caught on hit and falls through to a recompute rather than
/// silently returning the wrong bindings.
pub(crate) struct ValueCacheSlot {
    input: HashMap<String, Value>,
    satisfied: bool,
    bindings: HashMap<String, Value>,
}

/// Per-claim cross-tick value cache: maps `hash(given-values)` to the
/// `try_functionize_z3` result it produced. Capped at `VALUE_CACHE_CAP`
/// entries with FIFO eviction. Keyed by the value hash (O(1) lookup),
/// not the values themselves, so the stored `input` is re-checked on
/// every hit for collision safety.
#[derive(Default)]
pub(crate) struct ClaimValueCache {
    entries: HashMap<u64, ValueCacheSlot>,
    /// Insertion order of live hashes, for FIFO eviction.
    order: VecDeque<u64>,
}

/// Whether the cross-tick value cache is enabled. On by default;
/// `EVIDENT_VALUE_CACHE=0` disables it (for A/B measurement or as a
/// safety valve). Read once and memoized — this sits on the per-tick
/// hot path.
fn value_cache_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("EVIDENT_VALUE_CACHE").map(|s| s != "0").unwrap_or(true)
    })
}

/// Hash a `given` map deterministically. HashMap iteration order is
/// nondeterministic, so the keys are sorted before hashing; the per-key
/// `Value` is fed through `hash_value`. SipHash (`DefaultHasher`) keeps
/// the collision rate low, and a verified-input check on hit backstops
/// the rest.
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

/// Feed a `Value` into a hasher. `Value` deliberately has no derived
/// `Hash` (it lives in `core/`, which this session must not touch, and a
/// raw `f64` field blocks the derive anyway). Each variant writes a
/// distinct discriminant tag first so structurally different values with
/// the same payload bytes don't collide. Reals hash their bit pattern
/// (not the float value) so `NaN`/`-0.0` are handled deterministically.
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

/// Hash a `HashMap<String, Value>` (a `Composite`/`SeqComposite` element)
/// with keys sorted, same as `hash_given_values`.
fn hash_value_map<H: Hasher>(m: &HashMap<String, Value>, h: &mut H) {
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort_unstable();
    keys.len().hash(h);
    for k in keys {
        k.hash(h);
        hash_value(&m[k], h);
    }
}

/// What `compile_one_component` decided for a component.
enum ComponentOutcome {
    /// Compiled to a callable artifact.
    Compiled(Rc<dyn CompiledFunction>),
    /// Couldn't compile, but is safe to solve in the scoped slow part
    /// (a `Guarded` step, a Set output, a codegen refusal, …).
    Slow,
    /// Gap-fill was refused: a needed output has no safe definition.
    /// This is the case the monolithic path returned `None` for, so we
    /// abandon functionizing the whole claim and let the non-lenient
    /// `evaluate` handle it — which solves a genuinely-free output
    /// correctly, or surfaces a dropped-constraint error rather than
    /// masking it with a baked/solved value from the lenient cache.
    Bail,
}

/// Minimal union-find for `decompose_simplified`. (The one in
/// `crate::decompose` is private and re-normalizes its input; here we
/// partition the already-simplified assertions directly.)
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

/// Decompose the (already-simplified) assertions into independent
/// components over the claim's *non-broadcast* variables — outputs AND
/// intermediates (the internal temps a body introduces, e.g. an FFI
/// argument buffer `draw_rect__rect_buf__callN` or a `*_eff` LibCall
/// result). Two variables join the same component when some assertion
/// mentions both; each assertion is then owned by the component holding
/// its variables. Returns, per component, the OUTPUT names it owns and
/// the indices of `simplified` assertions in it; plus the indices of
/// assertions that touch no component variable at all (given-only
/// consistency constraints) as `global`.
///
/// **Why intermediates must be connectivity nodes (the Mario fix).** An
/// earlier version unioned only over `outputs`, so an intermediate that
/// bridges two components went unnoticed. In Mario's `display`,
/// `draw_rect__rect_buf__callN` (the `⟨pos.x, pos.y, size.x, size.y⟩`
/// buffer) has its position elements *defined* in the component holding
/// `mario.rects`, but its value is *consumed* by the `SDL_RenderFillRect`
/// `LibCall` that the `mario_effs` component carries. With only outputs
/// as nodes, those landed in two components: `mario.rects`' component
/// compiled, `mario_effs`' went to the scoped slow solve — and that slow
/// solve never saw the buffer's position constraint, so Z3 left Mario's
/// rect x/y free and the sprite drew at garbage coordinates (invisible).
/// Treating every non-broadcast variable as a node keeps a temp and all
/// of its mentions in one component, so whichever solver owns the
/// component sees the complete definition. (Inline-built rects —
/// platforms, enemies, coins — survived the bug because their buffer
/// coordinates anchored in *given* world data, pinned into every part.)
///
/// **`broadcast` = givens ∪ statically-known constants** (`PinnedInt` /
/// enum literals). These carry values pinned into every part, so they
/// are *not* connectivity nodes: two components both reading the same
/// given — or the same `LEVEL_W` constant — are still independent.
/// Making them nodes would collapse everything that references a shared
/// constant into one component. (Substituted-away temps like test_29's
/// `tick`/`seed_*` don't appear in `simplified` at all — `solve-eqs`
/// inlined them — so the independent chains stay independent.)
///
/// Operating on `simplified` directly (rather than
/// `analyze_decomposition`, which rebuilds the solver and re-runs
/// `simplify`) keeps the component partition and the assertion buckets
/// derived from the *same* formula set, so every assertion lands in
/// exactly one component.
fn decompose_simplified(
    simplified: &[Bool<'static>],
    outputs: &[String],
    broadcast: &HashSet<String>,
) -> (Vec<Vec<String>>, Vec<Vec<usize>>, Vec<usize>) {
    // Connectivity nodes: outputs interned first (indices 0..n, so
    // component ordering follows output declaration order), then
    // intermediates as they're discovered. Givens/constants are skipped.
    let mut node_of: HashMap<String, usize> = HashMap::with_capacity(outputs.len());
    for o in outputs {
        let n = node_of.len();
        node_of.entry(o.clone()).or_insert(n);
    }
    // For each assertion, the sorted/deduped node indices it touches.
    // A Seq var `s` splits into Z3-internal `s__arr` / `s__len` consts;
    // fold those back to the base name so a length pin (`#s = 4` →
    // `s__len = 4`) joins the SAME component as the element pins (`s[0]
    // = …` → `select s …`).
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
    // Union every node touched together within each assertion.
    let mut uf = UnionFind::new(node_of.len());
    for idxs in &per_assert {
        for w in idxs.windows(2) { uf.union(w[0], w[1]); }
    }
    // Bucket output nodes (indices 0..outputs.len()) by root, in
    // first-appearance order for deterministic component ordering.
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
    // Assign each assertion to the component of its variables (they all
    // share a root by construction). An assertion with no node (only
    // broadcast vars) is a given-only consistency constraint → global.
    // An assertion whose component has no output (a pure intermediate
    // island nothing observes) also goes to global, harmlessly.
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

/// Build a solver tuned the same way `make_tuned_solver` does (tactic
/// chain from `EVIDENT_TACTICS`, default `solve-eqs`; `smt.arith.solver`
/// param). Re-implemented here because that helper is `pub(super)` to
/// `translate::eval` — the per-component slow solver needs the same
/// tuning as the cached slow path it replaces.
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

/// Solve one slow part with `given` pinned and return its output
/// bindings (or `None` if UNSAT). All Z3 work happens in the part's own
/// context (`part.ctx`); the part's solver + env Vars + enum datatypes
/// all live there. Enum-typed givens are pinned in an outer push frame
/// (run_cached only handles scalar/seq/set givens); the frame is popped
/// before return so the cached solver is reusable next tick.
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
                // Pin an enum-typed given. `enums` is the right registry
                // for `ctx`: the runtime's `&self.enums` on the sequential
                // (main-context) path, or the part's replayed worker
                // registry on the parallel path — both built against the
                // same context as `ast`, so `value_enum_to_datatype` builds
                // a sort-compatible Datatype value.
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

/// Process-global lock serializing Z3 setup that touches global state.
/// Held across both context creation (`Z3_mk_context`, which has
/// historically raced on the memory manager / symbol tables) and the
/// per-context datatype replay (`create_datatypes` /
/// `get_or_build_datatype`), so no two threads run Z3 type/context
/// construction concurrently. Plan-building is off the per-tick hot path
/// (cached per claim), so the lock costs nothing measurable.
fn z3_setup_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::Mutex;
    static SETUP_LOCK: Mutex<()> = Mutex::new(());
    SETUP_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Mint a fresh leaked `'static` Z3 context for a parallel slow part.
/// Serialized via [`z3_setup_lock`]. The context is leaked to `'static`
/// — same trade as the runtime's main context — so the part's solver +
/// translated env vars can borrow it without lifetime gymnastics. One
/// context per parallel slow part per cached plan; bounded, not per-tick.
fn new_leaked_context() -> &'static Context {
    let _guard = z3_setup_lock();
    let cfg = z3::Config::new();
    Box::leak(Box::new(Context::new(&cfg)))
}

/// Can the env entries this part needs (its `outputs`, plus any `given`
/// keys it pins) be reproduced in a private worker context?
///
/// Primitive/seq/set scalar vars translate directly (`Ast::translate`).
/// Enum (`EnumVar`) and enum-element `Seq` (`DatatypeSeqVar` with empty
/// `fields`) vars carry a `&'static DatatypeSort` bound to the source
/// context, but the sort itself is *rebuildable*: `replay_enums_into`
/// re-runs enum registration against the worker context (Z3 interns
/// datatypes by name + variant structure, so the replayed sort coincides
/// with the one `Ast::translate` recreates for the translated assertions),
/// and `translate_var` rebinds the var's AST + the worker-context sort.
///
/// Still excluded — fall back to the sequential main-context path:
///   * **Record-element `DatatypeSeqVar`** (`Seq(UserRecord)`, non-empty
///     `fields`). Its per-field `FieldKind::Nested` entries each hold
///     their OWN `&'static DatatypeSort` bound to the main context;
///     extraction (`extract_seq_composite`) applies those accessors to
///     worker-context values, which is a cross-context func-decl
///     application Z3 rejects (panic). Rebinding the whole nested-field
///     sort tree per worker is doable but out of scope here — Mario's
///     `world.enemies : Seq(Enemy)` is the case this guards. (Bare record
///     vars like `p : Point` are already fine: they expand to primitive
///     `IntVar`/`BoolVar` leaves, no datatype handle.)
///   * `DatatypeSetVar` — model extraction is unsupported in v1
///     (`decode.rs` `Var::DatatypeSetVar => {/* unsupported */}`).
///   * `EnumValue` / `EnumCtor` — enum literals / un-applied constructors;
///     never `outputs` (the output filter drops them) and never `given`
///     values, so they shouldn't appear — conservative choice is sequential.
fn env_subset_translatable(
    env: &HashMap<String, Var<'static>>,
    outputs: &[String],
    given: &HashMap<String, Value>,
) -> bool {
    outputs.iter().chain(given.keys()).all(|name| {
        match env.get(name) {
            None => true,   // not in env → nothing to translate
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

/// Translate a `Var` into `dst`. Primitive variants translate their AST
/// handle(s) directly. The supported datatype-bearing variants (`EnumVar`
/// and enum-element `DatatypeSeqVar`) translate their AST handle(s) AND
/// rebind the `&'static DatatypeSort` to the worker context's replayed
/// enum registry (`worker_enums`, built by `replay_enums_into`); the
/// worker sort coincides with the one `Ast::translate` recreates for the
/// translated assertions because Z3 interns datatypes by name + variant
/// structure. Returns `None` for the unsupported variants (record-element
/// `DatatypeSeqVar`, `DatatypeSetVar`, `EnumValue`, `EnumCtor`);
/// `env_subset_translatable` gates callers so those arms aren't hit on the
/// parallel path, but the match stays exhaustive for safety.
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
            // Worker-context DatatypeSort for this enum (rebuilt by the
            // registry replay). The translated `ast` resolves against the
            // same sort, so model.eval + the registry's tester/accessor
            // FuncDecls all live in `dst`.
            let dt = worker_enums?.by_name.borrow().get(enum_name).map(|(d, _)| *d)?;
            Var::EnumVar {
                ast: ast.translate(dst),
                enum_name: enum_name.clone(),
                dt,
            }
        }
        // Enum-element Seq only (fields empty → no nested record sorts to
        // rebind). `dt` is the element enum's sort; look it up in the
        // worker enum registry. Record-element Seq (non-empty fields)
        // returns None — its `FieldKind::Nested` sorts are main-context
        // bound (see `env_subset_translatable`), so it stays sequential.
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
    /// Functionizer fast path with a cross-tick value cache wrapped
    /// around it. The result of [`functionize_z3_uncached`] is a pure
    /// function of `(name, schema, given)` — pins aren't even a
    /// parameter — so the same `given` values always reproduce the same
    /// bindings while the program is loaded. An idle FSM (Mario with no
    /// input) feeds `display` byte-identical inputs frame after frame,
    /// so we memoize the last results keyed by `hash(given-values)` and
    /// skip the compiled-function call entirely on a hit.
    ///
    /// The cache is invalidated wholesale on reload (`load.rs` clears it
    /// alongside `fn_cache`), so a schema or functionizer change can
    /// never serve a stale value.
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
        // Only memoize when the fast path actually produced a result.
        // `None` means "fall through to slow-path Z3" — caching that
        // would short-circuit a path we never took.
        if let (Some(h), Some(r)) = (vhash, &result) {
            self.value_cache_put(name, h, given, r);
        }
        result
    }

    /// Read a memoized result from the cross-tick value cache. On a hash
    /// hit the stored input is compared against `given`: an exact match
    /// returns the cached bindings; a mismatch (hash collision) returns
    /// `None` so the caller recomputes.
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

    /// Store a `(given, result)` association in the cross-tick value
    /// cache, evicting the oldest entry for this claim once the per-claim
    /// cap is exceeded (FIFO).
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

    /// Per-component Z3-AST functionizer. Decomposes the claim's
    /// simplified body into independent sub-models, compiles each one
    /// it can to native code, and gathers the rest into a single
    /// cached scoped Z3 solve. Returns `Some(QueryResult)` when the
    /// plan executed (compiled components ran + any slow part was
    /// SAT), `None` to fall through to a full Z3 solve.
    ///
    /// Cached per `(claim, given-keys)` as a `ClaimPlan`; subsequent
    /// calls just re-run the plan (JIT calls at ~µs + one scoped solve).
    fn functionize_z3_uncached(&self, name: &str, schema: &crate::core::ast::SchemaDecl,
                          given: &HashMap<String, Value>) -> Option<QueryResult>
    {
        // Cache key: name + sorted given_keys. The plan is generic
        // over given VALUES — compiled components read inputs per call
        // and the slow part re-pins `given` each call — so a stable
        // set of given_keys per FSM keeps the cached plan correct
        // across ticks.
        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (name.to_string(), given_keys.clone());

        // Cache hit: re-run the cached plan. `None` cached means the
        // claim can't be functionized — fall through to slow-path Z3.
        if let Some(entry) = self.fn_cache.borrow().get(&cache_key).cloned() {
            let Some(plan) = entry else { return None };
            self.functionize_stats.borrow_mut()
                .claims.entry(name.to_string()).or_default().cache_hits += 1;
            return self.execute_plan(&plan, given);
        }

        // Cache miss: build a CachedSchema, capture the body
        // assertions (without given values pinned so the
        // extracted program is generic over input values), apply
        // Z3's tactic chain, and extract per-output assignments.
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        // The Z3 translator fatal-exits on dropped constraints
        // (constraints it can't express). For schemas with such
        // gaps (e.g. enum ctors carrying Seq payloads), the slow
        // path is the only correct option — fall through there.
        if crate::z3_eval::has_known_translator_gap(&schema.body) {
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }
        // Pass the ACTUAL given to build_cache so apply_pinned_ints
        // can resolve symbolic bounds (∀ i ∈ {0..n - 1}) into
        // statically-known ranges before the translator runs.
        // Without these pins, body shapes like ∀-over-symbolic-Range
        // would trip the translator's dropped-constraint fatal-exit.
        //
        // R27: temporarily enable EVIDENT_LENIENT for the
        // build_cache call so untranslatable body items (like
        // SDL_Window's `install ∈ Seq(InstallStep) = ⟨...⟩` with
        // payloaded LibCalls) become warnings rather than
        // fatal-exit. extract_program will produce a partial
        // program; if it's incomplete for the outputs we need,
        // we fall through to the slow path which handles these
        // cases via the silently-skipping inheritance path
        // (inline.rs line 906).
        let _lenient_guard = LenientGuard::enable();
        // Pass an empty given to build_cache so the extracted program
        // is generic over input values. If we passed `given` here,
        // apply_pinned_ints would bake `_count`/state/etc. into the
        // body as constants, and the cached program would be wrong
        // for any other tick's values. Structural pins (Seq lengths)
        // still propagate because they come from the schema body
        // itself (`#platforms = 4`), not from given.
        let empty_given: HashMap<String, Value> = HashMap::new();
        let cached = crate::translate::build_cache(
            schema, &self.schemas, self.z3_ctx, &self.datatypes,
            Some(&self.enums), &empty_given, arith);
        drop(_lenient_guard);
        // get_assertions ties the Bool lifetime to the solver, but
        // the underlying Z3 ASTs are reference-counted by the
        // 'static Context — they outlive the solver wrapper. Same
        // pattern as `effect_loop.rs` uses for BodyItem slices.
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

        // Outputs: vars actually constrained by the simplified
        // body. Many env entries (world.player.vel.x when this
        // FSM doesn't read it, FTI bridge leaves, type-level
        // siblings of an unused field) appear in env but have no
        // body assertion — Z3 would pick any value. For the
        // function-izer, those vars are NOT outputs; the
        // scheduler's downstream paths either carry through from
        // world_snapshot or just don't need them.
        //
        // We compute the constraint-touched set by walking the
        // simplified assertions and collecting every 0-arity
        // App name that appears anywhere. An output is then:
        //   - in env (declared at build_cache time)
        //   - NOT in given (input)
        //   - NOT a PinnedInt/EnumValue/EnumCtor constant
        //   - actually appears in the simplified body
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
        // Pinned ints — vars whose value was statically resolvable
        // at build_cache time. Synthesize Scalar steps for them so
        // the cached program produces these bindings without any
        // re-derivation needed at hit time.
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
        // The same pinned ints as plain bindings — injected into every
        // result regardless of which component (if any) consumed them,
        // so the output set matches the monolithic path.
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
            // No constrained outputs — the body is just type
            // bounds / predicates with nothing to compute. We
            // can't claim to produce bindings the caller needs;
            // fall through to the slow path which extracts the
            // model directly.
            self.fn_cache.borrow_mut().insert(cache_key, None);
            return None;
        }
        // ── Per-component compilation ──────────────────────────────
        // Decompose the simplified body into independent sub-models,
        // compile each one we can, and gather the rest into one cached
        // scoped slow solve. A construct one component can't emit no
        // longer blocks the others.
        // Broadcast names: givens + statically-known constants
        // (`PinnedInt` / enum literals). These are pinned into every
        // part, so they are NOT connectivity nodes in the decomposition
        // — see `decompose_simplified`.
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
        // Per uncompiled component: its outputs + the assertion indices
        // it owns. Kept separate (not merged) so each becomes its own
        // independently-solvable `SlowPart`.
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

        // A gap-fill refusal abandons functionizing this claim: cache the
        // built body for the scheduler's slow path to reuse, mark the
        // plan absent, and fall through to the non-lenient `evaluate`
        // (matches the pre-decomposition behavior — see `ComponentOutcome::Bail`).
        if bail {
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

        // Build one slow part per uncompiled component. Each carries
        // that component's assertions plus the given-only consistency
        // assertions (`global_idx`, replicated into every part so each
        // independently rejects an inconsistent `given`). The components
        // have disjoint variable sets — solving them separately is
        // equivalent to one combined solve.
        //
        // Parallel vs sequential: a claim whose slow work is one
        // connected component (every Mario FSM) has a single part, so
        // there is nothing to fan out — keep it on the main context.
        // With ≥2 parts we *can* fan out, but only if each part's vars
        // translate cleanly into a private context (primitive/seq/set
        // scalars — no enum/user-datatype handles, whose sorts would
        // have to be re-registered per context). When all that holds,
        // each part gets its own context and `solve_slow_parts` runs
        // them on separate threads; otherwise we fall back to sequential
        // solving on the main context (correct, just not parallel).
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

    /// Compile one decomposed component to a callable artifact, scoped
    /// to its own outputs + assertions. Mirrors the monolithic
    /// extract → recompose → gap-fill → compile pipeline, but scoped to
    /// this component. Returns a `ComponentOutcome`:
    ///   * `Compiled` — native artifact ready;
    ///   * `Slow`     — solve in the scoped slow part (Set output,
    ///                  `Guarded`/codegen refusal, extract cycle);
    ///   * `Bail`     — gap-fill refused; the whole claim must fall to
    ///                  the non-lenient `evaluate`.
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
        // The JIT can't represent a Set output — a Z3 Set is a
        // characteristic function (`Array elem → Bool`), and the codegen
        // would misread its `store`-chain as a value-Seq. Send any
        // Set-bearing component to the slow path, where run_cached's
        // `extract_set` produces the right value from the candidate list.
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
            // Extraction cycle — the scoped slow solve handles it.
            return ComponentOutcome::Slow;
        };
        // Recompose record-element Seq outputs (Z3's simplify breaks a
        // whole-element ctor pin into per-field accessor pins).
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
                // Can't safely bake. If the component carries a *defining*
                // constraint (an equality / guarded implication / select
                // pin — anything beyond a bare type-bound comparison),
                // the missing outputs are determined; the scoped slow
                // solve recovers them from the real `given`. If every
                // assertion is just a type bound, the output is genuinely
                // unconstrained (e.g. its defining constraint was dropped
                // by the translator) — bail so the non-lenient `evaluate`
                // surfaces that as an error instead of masking it with an
                // arbitrary value.
                if component_has_defining_assertion(comp_assertions) {
                    return ComponentOutcome::Slow;
                }
                return ComponentOutcome::Bail;
            }
            // Safe to gap-fill from a model of the full cached body. The
            // missing outputs are constants (record-Seq literals like
            // Mario's `platforms`), so the full body's model values for
            // them are correct regardless of `given`.
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
        // Count the absorbed work (per-claim totals, summed over
        // components).
        {
            let mut stats = self.functionize_stats.borrow_mut();
            let per = stats.claims.entry(name.to_string()).or_default();
            per.steps_total      += program.steps.len() as u32;
            per.checks_total     += program.checks.len() as u32;
            per.predicates_total += program.predicates.len() as u32;
        }
        // Prepend pinned-int steps so component exprs that reference a
        // statically-known constant (e.g. `x_max = LEVEL_W - p_size.x`)
        // resolve it from env instead of loading an absent input.
        let mut all = pinned_steps.to_vec();
        all.append(&mut program.steps);
        program.steps = all;
        // Tag the program with its claim name so the
        // EVIDENT_FZ_DUMP_PROGRAM diagnostic header can identify it.
        program.label = Some(name.to_string());
        match self.functionizer.compile(&program, &self.enums, &self.datatypes) {
            // Codegen refused (e.g. a `Guarded` step) — the scoped slow
            // solve produces the right value, so this is not a Bail.
            Some(c) => ComponentOutcome::Compiled(c),
            None => ComponentOutcome::Slow,
        }
    }

    /// Run a cached `ClaimPlan`: call each compiled component, solve
    /// every slow part (fanned across threads when the plan was built
    /// parallel and has ≥2 parts), and merge. Returns `None` (→ caller
    /// falls through to a full Z3 solve) if a compiled component bails or
    /// any slow part is UNSAT.
    ///
    /// No finer work-threshold gates the fan-out: a slow part is, by
    /// construction, a component the JIT *couldn't* absorb, i.e. the
    /// heavy work — so the per-part thread spawn (tens of µs) is
    /// dominated by even a sub-millisecond Z3 solve, and the solve time
    /// can't be known before solving anyway. The compiled components
    /// (µs of native code, and holding `!Send` `Rc`s) stay on the
    /// calling thread.
    fn execute_plan(&self, plan: &ClaimPlan, given: &HashMap<String, Value>)
        -> Option<QueryResult>
    {
        let mut out: HashMap<String, Value> = HashMap::new();
        // Statically-pinned ints sit in no component; emit them first so
        // every result carries them (matches the monolithic path).
        for (k, v) in &plan.pinned_ints {
            if !given.contains_key(k) { out.insert(k.clone(), v.clone()); }
        }
        for c in &plan.compiled {
            let bindings = c.call(given)?;
            for (k, v) in bindings {
                if !given.contains_key(&k) { out.insert(k, v); }
            }
        }
        // Slow parts: independent components. Fanned across threads when
        // the plan was built parallel (each part on its own context).
        let slow_bindings = self.solve_slow_parts(&plan.slow, plan.slow_parallel, given)?;
        for (k, v) in slow_bindings {
            if !given.contains_key(&k) { out.insert(k, v); }
        }
        for (k, v) in given { out.insert(k.clone(), v.clone()); }
        Some(QueryResult { satisfied: true, bindings: out })
    }

    /// Solve every slow part and merge their output bindings. Returns
    /// `None` if any part is UNSAT (→ fall through to a full Z3 solve).
    ///
    /// When `parallel` and there are ≥2 parts, each part is solved on its
    /// own scoped thread; each part owns a private Z3 context, so the
    /// `check()`s run truly concurrently (a Z3 context is single-threaded,
    /// but distinct contexts are independent). Otherwise the parts solve
    /// sequentially on the calling thread. Output var sets are disjoint
    /// across parts (decomposition guarantees it), so the merge is a
    /// clean union regardless of completion order.
    fn solve_slow_parts(&self, parts: &[SlowPart], parallel: bool,
                        given: &HashMap<String, Value>)
        -> Option<HashMap<String, Value>>
    {
        if parts.is_empty() { return Some(HashMap::new()); }
        let timing = std::env::var("EVIDENT_PLAN_TIMING").is_ok();

        if parallel && parts.len() >= 2 {
            // Each part has a private context. A part that carries enum /
            // record vars also owns its OWN replayed `EnumRegistry`
            // (`part.enums`), built in that private context — so we pass
            // `part.enums.as_ref()` rather than the runtime's `&self.enums`
            // (a `RefCell`, hence `!Sync`, and bound to the wrong context).
            // Primitive-only parts have `enums: None` and never need it.
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
                    // A worker panic would poison the result; propagate it
                    // as "UNSAT" (None) so the caller falls back to the
                    // full Z3 solve rather than tearing down the process.
                    handles.into_iter()
                        .map(|h| h.join().unwrap_or(None))
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

    /// Build slow parts that all share the runtime's main context.
    /// Sound only because `solve_slow_parts` runs same-context parts
    /// sequentially. Used when there's a single component (nothing to
    /// fan out) or when a component's vars can't translate to a private
    /// context. Each part carries the full env clone (cheap — Z3 AST
    /// handles are refcounted) so `run_cached` extraction is uniform.
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
                // Sequential parts run on the main context, where
                // `solve_slow_parts` hands `solve_one_part` the runtime's
                // own `&self.enums` — no per-part registry needed.
                enums: None,
            }
        }).collect()
    }

    /// Build slow parts that each own a private leaked Z3 context, so
    /// they can `check()` concurrently. Each part's assertions and the
    /// (restricted) env it needs for given-pinning + extraction are
    /// translated out of the runtime's main context via `Ast::translate`
    /// — Z3 interns const decls by name+sort within a context, so a
    /// translated assertion and a separately-translated env var resolve
    /// to the same decl, and `model.eval` reads the right value.
    ///
    /// **Enum components.** When a component carries an enum-typed var
    /// (`EnumVar` or enum-element `DatatypeSeqVar`), the worker context
    /// first gets an enum replay (`replay_enums_into`) *before* assertion
    /// translation, so the datatype sort the translated assertions
    /// reference unifies with the replayed one (Z3 interns datatypes by
    /// name + variant structure). The part keeps the replayed
    /// `EnumRegistry` so `run_cached` can pin enum givens and decode
    /// enum-typed outputs in the worker context.
    ///
    /// Precondition: every component's `env_subset_translatable` is true
    /// (checked by the caller), so `translate_var` never bails here.
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
            // Does this component reference any enum-typed var? If so, the
            // worker context needs the enum datatypes replayed before its
            // assertions are translated in. (`env_subset_translatable` has
            // already excluded record-element Seq / Set / etc., so the only
            // datatype handles reachable here are enum sorts.)
            let needs_enums = outputs.iter().chain(given.keys()).any(|name|
                matches!(env.get(name),
                    Some(Var::EnumVar { .. }) | Some(Var::DatatypeSeqVar { .. })));
            // Context creation + (optional) enum replay share one lock
            // (`z3_setup_lock`) so concurrent plan-builds across runtimes
            // never run Z3 context/type construction at the same time.
            let (ctx, wenums) = {
                let _guard = z3_setup_lock();
                let cfg = z3::Config::new();
                let ctx: &'static Context = Box::leak(Box::new(Context::new(&cfg)));
                let wenums = if needs_enums { Some(self.replay_enums_into(ctx)) } else { None };
                (ctx, wenums)
            };
            let solver = build_tuned_solver(ctx, arith);
            for a in &component_assertions[ci] { solver.assert(&a.translate(ctx)); }
            for a in global_assertions { solver.assert(&a.translate(ctx)); }
            // Restricted env: the outputs (for extraction) + any given
            // keys (for pinning). Other env entries aren't reachable by
            // this part's solver, so translating them would be wasted —
            // and run_cached only extracts what's in the env it's given.
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
                // The worker enum registry travels with the part (one part
                // ↔ one thread) so extraction/pinning runs in this ctx.
                // `None` for primitive-only parts (no enum vars).
                enums: wenums,
            }
        }).collect()
    }

    /// Replay every `enum` definition into `ctx`, returning a fresh
    /// `EnumRegistry` whose `DatatypeSort`s live in `ctx`. Z3 interns
    /// datatypes by name + variant structure, so the sorts built here
    /// coincide with the ones `Ast::translate` recreates for translated
    /// assertions — which is what lets a worker context solve AND extract
    /// enum values exactly as the main context would. Called from
    /// `build_parallel_slow` while holding `z3_setup_lock`.
    ///
    /// User-record datatypes (`Seq(UserRecord)`) are NOT replayed: those
    /// vars are excluded from the parallel path by `env_subset_translatable`
    /// (their `FieldKind::Nested` sorts are main-context bound), so a worker
    /// context never needs them.
    fn replay_enums_into(&self, ctx: &'static Context) -> crate::core::EnumRegistry {
        let wenums = crate::core::EnumRegistry::new();
        // `self.program.enums` is the ORIGINAL decl list (internal
        // Cons-helpers are regenerated inside register_enums, not stored
        // back) — replaying it against a fresh registry reproduces exactly
        // what the main context got at load. Any error would already have
        // fired at load against the main context; if one somehow surfaces
        // here, leave the registry partial — extraction then yields no
        // enum bindings and the caller's `None`-fallback re-solves on the
        // full main-context path.
        let _ = super::register_enums::register_enums(&self.program.enums, ctx, &wenums);
        wenums
    }

    /// Evaluate the named schema and return whether it's satisfiable
    /// plus a model. `given` pre-binds variables to concrete values
    /// (mirrors the Python `query(schema, given=...)` parameter).
    pub fn query(&self, name: &str, given: &HashMap<String, Value>) -> Result<QueryResult, RuntimeError> {
        let base = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        // Tier-3 nested-FSM resolution: drive any `run(F, init)` in the
        // body to its final-state value and pin it as a literal BEFORE
        // the solve (see runtime/nested.rs). No `run` → no clone.
        let resolved = self.resolve_runs(base, given)?;
        let schema = resolved.as_ref().unwrap_or(base);

        // Functionizer fast path: extract a Z3Program from the body
        // and JIT-compile to native code. On miss (extract refused
        // or JIT codegen refused) we fall through to a full Z3 solve.
        //
        // Skip the JIT + value cache for `run`-containing bodies: both
        // key on given-KEYS, but a `run`'s resolved literal depends on
        // given-VALUES, so a cached plan could replay a stale value.
        // v1 keeps run-containing bodies on the always-fresh Z3 path
        // (they're rare; tiers 1/2 accelerate nested runs later).
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

        // One-shot query: don't auto-tune (no chance to learn over many
        // calls). Use the env override if set, default 2 (the value
        // that wins on Z3 4.8.12 for our typical workload).
        let arith: u32 = std::env::var("EVIDENT_Z3_ARITH_SOLVER").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(2);
        let r = crate::translate::evaluate(schema, given, &self.schemas, self.z3_ctx, &self.datatypes, Some(&self.enums), arith);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }

    /// Faster query — translates the schema once on first call and
    /// reuses the resulting Z3 solver across subsequent calls
    /// (push/pop per query). Mirrors Python's `query(name, given,
    /// cached=True)` and the `evaluate_cached` optimization.
    ///
    /// **Structural-signature invalidation.** The cache stores the
    /// subset of the previous `given` keyed on names that appear in
    /// quantifier bounds — the structural signature. If this query's
    /// signature differs (e.g. a config value that drives an unroll
    /// count just changed), the cache is dropped and rebuilt against
    /// the new given. Non-structural changes (player position, etc.)
    /// reuse the cache and just re-assert the new value per-query.
    ///
    /// Bindings, satisfaction result, and overall semantics are
    /// identical to `query()`. Faster when called many times against
    /// the same schema with mostly-stable structural givens (e.g. an
    /// executor stepping a state machine 60×/sec where lengths and
    /// bound names don't change).
    pub fn query_cached(&self, name: &str, given: &HashMap<String, Value>)
        -> Result<QueryResult, RuntimeError>
    {
        let base = self.schemas.get(name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        // Tier-3 nested-FSM resolution before caching: a body with
        // `run(F, init)` is rewritten to its literal final state, so the
        // cache keys on the resolved body (see runtime/nested.rs).
        let schema = match self.resolve_runs(base, given)? {
            Some(rewritten) => rewritten,
            None => base.clone(), // cheap: SchemaDecl is small + Arc-friendly clones
        };
        let cur_sig = structural_signature(&schema.body, given);

        // Auto-tuner: which arith.solver should the cache use right now?
        let arith_solver = {
            let mut hist = self.solve_history.borrow_mut();
            hist.entry(name.to_string()).or_insert_with(SolveHistory::new)
                .current_config()
        };

        let mut cache = self.cache.borrow_mut();
        // Rebuild if (a) no entry, (b) structural signature changed, or
        // (c) cached config doesn't match the auto-tuner's current pick.
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

        // Time the actual solve so the auto-tuner can decide whether to
        // advance to the next pricing window.
        let t0 = Instant::now();
        let r = run_cached(&entry.0, given, self.z3_ctx, Some(&self.enums));
        let dt = t0.elapsed();
        drop(cache);  // release before we may invalidate below

        // Record the timing. If the tuner says to switch configs,
        // evict so the next call rebuilds under the new value.
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
        // HashMap iteration order is nondeterministic; the hash must not
        // depend on it. Build the same logical map two different ways.
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
