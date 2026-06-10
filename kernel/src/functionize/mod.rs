//! Functionizer (Z3-tactic version) for the kernel tick loop.
//!
//! Design + reference: `docs/plans/functionizer-integration.md` and the
//! high-level-`z3` reference port in `legacy-rust/functionizer/` (`z3_eval.rs`,
//! `z3_program.rs`, `cranelift.rs`). This is a raw-`z3-sys` re-port targeting
//! the kernel's SMT-LIB pipeline (the kernel uses `z3-sys`, not the `z3`
//! crate), per the integration doc §2 option (a) "keep the kernel minimal".
//!
//! Pipeline:
//!   1. `simplify_assertions` — Z3 tactic chain `simplify` + `propagate-values`
//!      over the cached body assertions (matches the reference; `solve-eqs` is
//!      deliberately excluded so `(= var expr)` shapes survive).
//!   2. `extract_program` — partition the simplified assertions, keyed by the
//!      manifest's state fields + `effects`, into per-output `Step`s
//!      (`Scalar` / `Guarded` / `Seq`). Any output without a defining
//!      assignment ⇒ `None` ⇒ the whole tick falls through to a real Z3 solve.
//!   3. JIT — each scalar Int/Bool step is handed to `jit::compile_step`;
//!      steps that compile call native code per tick, the rest interpret.
//!   4. Verify — before returning, run the extracted program against a real Z3
//!      solve on tick 0 AND tick 1 and compare state + effects. A mismatch
//!      disables the fast path entirely (returns `None`). This makes the fast
//!      path sound even though the extractor is conservative: a shape we
//!      mis-read can never produce wrong output — it just reverts to Z3.
//!
//! Env flags (read in `tick.rs`):
//!   - `EVIDENT_FUNCTIONIZE=0`   — bypass extraction + fast path entirely.
//!   - `EVIDENT_FUNCTIONIZE_JIT=0` — extract + interpret, but don't JIT.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::time::{Duration, Instant};
use z3_sys::*;

use crate::manifest::Manifest;
use crate::tick::{self, Sv};

pub mod eval;
pub mod jit;
pub mod low;

// ── Program IR ──────────────────────────────────────────────────

#[derive(Clone)]
pub enum GBody {
    Scalar(Z3_ast),
    Seq(Vec<Z3_ast>),
}

/// Lowered (FFI-free) mirror of `StepBody` — see `low.rs` for why. Built once
/// at load; the per-tick hot path evaluates these instead of the Z3 ASTs.
pub enum LowBody {
    Scalar(low::LExpr),
    Seq(Vec<low::LExpr>),
    Guarded(Vec<LowBranch>),
}

pub struct LowBranch {
    pub guard: low::LExpr,
    pub neg: bool,
    pub body: LowGBody,
}

pub enum LowGBody {
    Scalar(low::LExpr),
    Seq(Vec<low::LExpr>),
}

#[derive(Clone)]
pub struct Branch {
    /// The guard AST. When `neg` is set, the branch fires on its *negation*
    /// — this captures `(or X Q)` ⇒ `¬X ⇒ Q` where Z3 emitted the negated
    /// guard `X` as a plain predicate rather than `(not …)`.
    pub guard: Z3_ast,
    pub neg: bool,
    pub body: GBody,
}

#[derive(Clone)]
pub enum StepBody {
    Scalar(Z3_ast),
    Seq(Vec<Z3_ast>),
    Guarded(Vec<Branch>),
}

pub struct Step {
    pub var: String,
    pub body: StepBody,
    /// FFI-free lowered body (the per-tick hot path).
    pub low: LowBody,
    /// Env slot this step writes (`var` interned in `Program::names`).
    pub var_slot: u32,
    /// Slot ids of `jit.inputs`, in pack order (empty when `jit` is None).
    pub jit_slots: Vec<u32>,
    /// Member of a mention-level dependency cycle: excluded from the topo
    /// order, resolved by run_program fixpoint rounds (eval laziness makes
    /// guard-acyclic graphs converge; real cycles stall → Z3 tick).
    pub deferred: bool,
    /// Bool-sorted scalar output (vs Int). Selects how a JIT i64 result and an
    /// eval result are interpreted. Irrelevant for Seq steps.
    pub result_is_bool: bool,
    /// True for the single `effects` step (its Seq feeds the effect dispatch).
    /// A non-effects Seq step is a record-Seq *intermediate* — its `Sv::Seq`
    /// value is bound into the eval env so later scalar steps can index it.
    pub is_effects: bool,
    /// Present only for scalar Int/Bool steps the JIT accepted.
    pub jit: Option<jit::JitStep>,
}

/// Pre-resolved env slots for the per-tick input fill and output read
/// (mirrors `build_inputs` / the manifest state-field order).
pub struct SlotPlan {
    pub is_first_tick: u32,
    pub last_results: u32,
    pub last_results_len: u32,
    /// Slot of `_<name>` per manifest state field.
    pub carries: Vec<u32>,
    /// Slot of `<name>` per manifest state field.
    pub state_out: Vec<u32>,
    /// Tick-0 fallback value per state field (type sentinel; see
    /// `build_inputs`).
    pub sentinels: Vec<Sv>,
}

pub struct Program {
    pub steps: Vec<Step>,
    pub predicates: Vec<Z3_ast>,
    /// Lowered mirrors of `predicates`.
    pub low_predicates: Vec<low::LExpr>,
    /// Variable-name interner; env slot i holds the value of `names.list[i]`.
    pub names: low::Names,
    pub plan: SlotPlan,
    /// `EVIDENT_FUNCTIONIZE_LOWER=0` flips the per-tick path back to the
    /// legacy FFI interpreter (A/B + escape hatch).
    pub lowered: bool,
    /// Number of scalar steps the JIT compiled vs interpreted (reporting).
    pub jit_count: usize,
    pub interp_count: usize,
    /// Tick-0 values of the `_<name>` state carries, read from the verify
    /// solve's Z3 model. On tick 0 the carries are unconstrained by pins but
    /// mentioned in the body, so Z3 assigns them deterministic default-ish
    /// values (e.g. TLNil for an unconstrained TokenList) — and unguarded
    /// body equations (`items_nil = is-TLNil(_items)`) OBSERVE those values.
    /// Seeding the eval env with the same values keeps the fast path
    /// bit-identical to the Z3 path on tick 0. Empty when verify is skipped
    /// (falls back to type sentinels).
    pub tick0_carries: HashMap<String, Sv>,
    /// inc_ref'd simplified formulas; keeps every sub-AST in `steps` alive for
    /// the program's (process) lifetime. Never dec_ref'd — the kernel is a
    /// short-lived process.
    _keepalive: Vec<Z3_ast>,
}

pub struct RunOut {
    /// New state values aligned to `manifest.state_fields`; `None` = the
    /// step left the field unbound (caller falls through to Z3).
    pub state: Vec<Option<Sv>>,
    pub effects: Vec<Sv>,
}

// ── Diagnostics (env-gated; default = Summary) ──────────────────
//
// Three levels, controlled by `EVIDENT_FUNCTIONIZE_STATS` +
// `EVIDENT_FUNCTIONIZE_TRACE`:
//   unset / =1     → one-line summary at exit (counts + timings).
//   STATS=verbose  → the summary PLUS a per-step load report at startup.
//   STATS=0        → silence everything.
//   TRACE=1        → per-tick timing lines (in addition to any STATS level).
// Default is Summary so that performance-sensitive sessions get
// immediate visibility into what functionized (see CLAUDE.md
// §"Functionizer diagnostics" and architecture-invariants.md
// §"Functionizability over Z3-fast"). Set STATS=0 to silence
// when the noise is unwanted.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatsLevel {
    Off,
    Summary,
    Verbose,
}

impl StatsLevel {
    pub fn from_env() -> Self {
        match std::env::var("EVIDENT_FUNCTIONIZE_STATS").ok().as_deref() {
            Some("verbose") | Some("VERBOSE") | Some("v") => StatsLevel::Verbose,
            Some("0") | Some("off") | Some("OFF") => StatsLevel::Off,
            _ => StatsLevel::Summary, // default
        }
    }
}

/// One row of the verbose load report: an extracted output step and how it runs.
pub struct StepReport {
    pub var: String,
    /// Truncated pretty form of the defining expression.
    pub expr: String,
    /// true = JIT-compiled, false = interpreted.
    pub jitted: bool,
    /// Shape category (`binop`, `ite`, `select`, `accessor`, `select+accessor`,
    /// `seq-literal`, `guarded-seq`, `guarded-scalar`, `datatype`, `var`).
    pub category: &'static str,
}

/// Counts + accumulated timings for one program run. Built once by
/// `functionize`; the per-tick timings are accumulated by `tick.rs` during the
/// loop, and the summary line is emitted on `Drop`.
pub struct FunctionizeStats {
    pub level: StatsLevel,
    pub trace: bool,
    /// `EVIDENT_FUNCTIONIZE=0` — the functionizer was not even attempted.
    pub disabled: bool,
    /// Did the fast path engage (extraction + verification both succeeded).
    pub functionized: bool,
    /// Why the fast path is off (None case / disabled). Shown in verbose.
    pub refuse_reason: Option<String>,
    /// N — total assertions in the simplified, flattened body.
    pub total_asserts: usize,
    /// J — extracted steps that JIT-compiled.
    pub jit: usize,
    /// I — extracted steps that fell back to the interpreter.
    pub interp: usize,
    /// R — residual assertions not turned into a functionized step. When
    /// functionized these are the eval-time `predicates`; when not, all N go to
    /// Z3 every tick.
    pub residual: usize,
    /// Per-step rows, populated only at `Verbose`.
    pub steps: Vec<StepReport>,
    // ── runtime, accumulated by tick.rs ──
    pub ticks: usize,
    pub t_func: Duration,
    pub t_z3: Duration,
    pub t_dispatch: Duration,
    /// Set when timing is on; `Drop` derives T_total from it.
    pub loop_start: Option<Instant>,
}

impl FunctionizeStats {
    pub fn new(level: StatsLevel, trace: bool) -> Self {
        FunctionizeStats {
            level, trace,
            disabled: false,
            functionized: false,
            refuse_reason: None,
            total_asserts: 0,
            jit: 0,
            interp: 0,
            residual: 0,
            steps: Vec::new(),
            ticks: 0,
            t_func: Duration::ZERO,
            t_z3: Duration::ZERO,
            t_dispatch: Duration::ZERO,
            loop_start: None,
        }
    }

    /// True when any per-tick timing must be collected. When false, tick.rs
    /// skips every `Instant::now()` so the off path pays only a branch.
    pub fn timing_on(&self) -> bool {
        self.level != StatsLevel::Off || self.trace
    }

    /// Verbose-only: print the per-step load report once, before the tick loop.
    pub fn print_load_report(&self) {
        if self.level != StatsLevel::Verbose {
            return;
        }
        eprintln!("[functionizer] load:");
        eprintln!("  body asserts: {}", self.total_asserts);
        if !self.functionized {
            let why = self.refuse_reason.as_deref().unwrap_or("unfunctionizable");
            eprintln!("  not functionized — fast path disabled; all {} asserts run on Z3 each tick", self.total_asserts);
            eprintln!("  reason: {why}");
            return;
        }
        eprintln!("  extracted:    {} ({} JIT, {} interp)", self.steps.len(), self.jit, self.interp);
        eprintln!("  residual:     {}", self.residual);
        eprintln!("  steps:");
        let wvar = self.steps.iter().map(|s| s.var.len()).max().unwrap_or(0).min(16);
        let wexpr = self.steps.iter().map(|s| s.expr.len()).max().unwrap_or(0).min(48);
        for (i, s) in self.steps.iter().enumerate() {
            let mode = if s.jitted { "JIT    " } else { "interp " };
            eprintln!("    [{i}] {:<wvar$} ← {:<wexpr$}  {mode} [{}]",
                s.var, s.expr, s.category, wvar = wvar, wexpr = wexpr);
        }
    }
}

impl Drop for FunctionizeStats {
    fn drop(&mut self) {
        if self.level == StatsLevel::Off {
            return;
        }
        let total_ms = self.loop_start.map(|s| s.elapsed()).unwrap_or(Duration::ZERO).as_secs_f64() * 1000.0;
        let func_ms = self.t_func.as_secs_f64() * 1000.0;
        let z3_ms = self.t_z3.as_secs_f64() * 1000.0;
        let prefix = if self.functionized {
            String::new()
        } else {
            let why = self.refuse_reason.as_deref().unwrap_or("unfunctionizable");
            format!("not functionized ({why}); ")
        };
        eprintln!(
            "[functionizer] {prefix}{} total / {} JIT / {} interp / {} residual; {total_ms:.1} ms total ({func_ms:.1} ms func / {z3_ms:.1} ms z3)",
            self.total_asserts, self.jit, self.interp, self.residual);
    }
}

unsafe fn ast_str(ctx: Z3_context, a: Z3_ast) -> String {
    let p = Z3_ast_to_string(ctx, a);
    if p.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(p).to_string_lossy().replace('\n', " ")
}

fn truncate(mut s: String, max: usize) -> String {
    if s.chars().count() > max {
        s = s.chars().take(max.saturating_sub(1)).collect::<String>();
        s.push('…');
    }
    s
}

/// Shape category of an extracted step, for the verbose report.
unsafe fn categorize(ctx: Z3_context, body: &StepBody) -> &'static str {
    match body {
        StepBody::Seq(_) => "seq-literal",
        StepBody::Guarded(bs) => {
            if bs.iter().any(|b| matches!(b.body, GBody::Seq(_))) { "guarded-seq" } else { "guarded-scalar" }
        }
        StepBody::Scalar(e) => categorize_scalar(ctx, *e),
    }
}

unsafe fn categorize_scalar(ctx: Z3_context, a: Z3_ast) -> &'static str {
    match decl_kind(ctx, a) {
        Some(DeclKind::ITE) => "ite",
        Some(DeclKind::SELECT) => "select",
        Some(DeclKind::DT_ACCESSOR) => if subtree_has_select(ctx, a) { "select+accessor" } else { "accessor" },
        Some(DeclKind::DT_CONSTRUCTOR) => "datatype",
        Some(DeclKind::ADD) | Some(DeclKind::SUB) | Some(DeclKind::MUL) | Some(DeclKind::UMINUS)
        | Some(DeclKind::LE) | Some(DeclKind::LT) | Some(DeclKind::GE) | Some(DeclKind::GT)
        | Some(DeclKind::EQ) | Some(DeclKind::IFF) | Some(DeclKind::NOT) | Some(DeclKind::AND)
        | Some(DeclKind::OR) | Some(DeclKind::IMPLIES) => "binop",
        Some(DeclKind::UNINTERPRETED) => "var",
        _ => "scalar",
    }
}

unsafe fn subtree_has_select(ctx: Z3_context, a: Z3_ast) -> bool {
    if decl_kind(ctx, a) == Some(DeclKind::SELECT) {
        return true;
    }
    children(ctx, a).iter().any(|&c| subtree_has_select(ctx, c))
}

/// Build the verbose per-step rows for a successfully extracted program.
unsafe fn build_step_reports(ctx: Z3_context, prog: &Program) -> Vec<StepReport> {
    prog.steps.iter().map(|s| {
        let category = categorize(ctx, &s.body);
        let expr = match &s.body {
            StepBody::Scalar(e) => truncate(ast_str(ctx, *e), 48),
            StepBody::Seq(es) => format!("⟨{} elem seq⟩", es.len()),
            StepBody::Guarded(bs) => format!("guarded({} branches)", bs.len()),
        };
        StepReport { var: s.var.clone(), expr, jitted: s.jit.is_some(), category }
    }).collect()
}

// ── AST helpers (shared with eval.rs / jit.rs) ──────────────────

pub(crate) unsafe fn decl_kind(ctx: Z3_context, a: Z3_ast) -> Option<DeclKind> {
    if Z3_get_ast_kind(ctx, a) != AstKind::App {
        return None;
    }
    let app = Z3_to_app(ctx, a);
    if app.is_null() {
        return None;
    }
    Some(Z3_get_decl_kind(ctx, Z3_get_app_decl(ctx, app)))
}

pub(crate) unsafe fn app_decl_name(ctx: Z3_context, a: Z3_ast) -> Option<String> {
    if Z3_get_ast_kind(ctx, a) != AstKind::App {
        return None;
    }
    let app = Z3_to_app(ctx, a);
    if app.is_null() {
        return None;
    }
    let decl = Z3_get_app_decl(ctx, app);
    Some(tick::decode_sym_pub(ctx, Z3_get_decl_name(ctx, decl)))
}

/// The constructor name a datatype recognizer tests for. Z3 renders the
/// parametric recognizer `(_ is C)` with decl NAME "is" and the constructor
/// decl as the first decl PARAMETER (z3 ≥ ~4.12; older builds and the
/// standalone recognizer form spell the name "is-C" / "is_C"). Prefer the
/// parameter; fall back to stripping the name prefix.
pub(crate) unsafe fn recognizer_target(ctx: Z3_context, a: Z3_ast) -> Option<String> {
    if Z3_get_ast_kind(ctx, a) != AstKind::App {
        return None;
    }
    let app = Z3_to_app(ctx, a);
    if app.is_null() {
        return None;
    }
    let decl = Z3_get_app_decl(ctx, app);
    if Z3_get_decl_num_parameters(ctx, decl) >= 1
        && Z3_get_decl_parameter_kind(ctx, decl, 0) == ParameterKind::FuncDecl
    {
        let ctor = Z3_get_decl_func_decl_parameter(ctx, decl, 0);
        if !ctor.is_null() {
            return Some(tick::decode_sym_pub(ctx, Z3_get_decl_name(ctx, ctor)));
        }
    }
    let name = tick::decode_sym_pub(ctx, Z3_get_decl_name(ctx, decl));
    Some(
        name.strip_prefix("is-")
            .or_else(|| name.strip_prefix("is_"))
            .unwrap_or(&name)
            .to_string(),
    )
}

pub(crate) unsafe fn children(ctx: Z3_context, a: Z3_ast) -> Vec<Z3_ast> {
    let app = Z3_to_app(ctx, a);
    if app.is_null() {
        return Vec::new();
    }
    let n = Z3_get_app_num_args(ctx, app);
    (0..n).map(|i| Z3_get_app_arg(ctx, app, i)).collect()
}

/// Name of a 0-arity application (a Z3 "constant"/variable).
pub(crate) unsafe fn ast_app_name(ctx: Z3_context, a: Z3_ast) -> Option<String> {
    if Z3_get_ast_kind(ctx, a) != AstKind::App {
        return None;
    }
    let app = Z3_to_app(ctx, a);
    if app.is_null() || Z3_get_app_num_args(ctx, app) != 0 {
        return None;
    }
    let decl = Z3_get_app_decl(ctx, app);
    Some(tick::decode_sym_pub(ctx, Z3_get_decl_name(ctx, decl)))
}

pub(crate) unsafe fn numeral_i64(ctx: Z3_context, a: Z3_ast) -> Option<i64> {
    if Z3_get_ast_kind(ctx, a) != AstKind::Numeral {
        return None;
    }
    let mut n: i64 = 0;
    if Z3_get_numeral_int64(ctx, a, &mut n) {
        Some(n)
    } else {
        None
    }
}

/// Does `a`'s tree mention a 0-arity application named `name`?
pub(crate) unsafe fn mentions_name(ctx: Z3_context, a: Z3_ast, name: &str) -> bool {
    if let Some(n) = ast_app_name(ctx, a) {
        if n == name {
            return true;
        }
    }
    for c in children(ctx, a) {
        if mentions_name(ctx, c, name) {
            return true;
        }
    }
    false
}

/// Collect ALL 0-arity application names mentioned in `a`'s tree, into `out`.
/// Used by reachability — far cheaper than calling `mentions_name(ctx, a, name)`
/// once per candidate `name` (which would re-walk the tree N times).
pub(crate) unsafe fn collect_mentioned_names(
    ctx: Z3_context, a: Z3_ast, out: &mut HashSet<String>,
) {
    if let Some(n) = ast_app_name(ctx, a) {
        if children(ctx, a).is_empty() {
            out.insert(n);
        }
    }
    for c in children(ctx, a) {
        collect_mentioned_names(ctx, c, out);
    }
}

unsafe fn is_bool_sorted(ctx: Z3_context, a: Z3_ast) -> bool {
    Z3_get_sort_kind(ctx, Z3_get_sort(ctx, a)) == SortKind::Bool
}

/// A 0-arity uninterpreted constant (a program variable), not a builtin like
/// `true`/`false` or a numeral.
pub(crate) unsafe fn is_uninterp_const(ctx: Z3_context, a: Z3_ast) -> bool {
    decl_kind(ctx, a) == Some(DeclKind::UNINTERPRETED) && children(ctx, a).is_empty()
}

/// Int/Bool-sorted (the sorts the JIT and scalar pins handle).
pub(crate) unsafe fn is_int_or_bool(ctx: Z3_context, a: Z3_ast) -> bool {
    let k = Z3_get_sort_kind(ctx, Z3_get_sort(ctx, a));
    k == SortKind::Int || k == SortKind::Bool
}

/// For a `DT_ACCESSOR` application `a` (e.g. `(w (select rs 0))`), the 0-based
/// field index of the accessed field within its constructor. Matches the
/// accessor decl by name against the argument's datatype sort — record field
/// names (`x`,`w`) and enum accessors (`Cons__f0`) are unique within a sort, so
/// the first match is correct. `None` for non-datatype args / unknown
/// accessors (caller falls through to Z3).
pub(crate) unsafe fn accessor_field_index(ctx: Z3_context, a: Z3_ast) -> Option<usize> {
    let acc_name = app_decl_name(ctx, a)?;
    let ch = children(ctx, a);
    if ch.len() != 1 {
        return None;
    }
    let sort = Z3_get_sort(ctx, ch[0]);
    if Z3_get_sort_kind(ctx, sort) != SortKind::Datatype {
        return None;
    }
    let nc = Z3_get_datatype_sort_num_constructors(ctx, sort);
    for ci in 0..nc {
        let ctor = Z3_get_datatype_sort_constructor(ctx, sort, ci);
        let arity = Z3_get_domain_size(ctx, ctor);
        for fi in 0..arity {
            let acc = Z3_get_datatype_sort_constructor_accessor(ctx, sort, ci, fi);
            let nm = tick::decode_sym_pub(ctx, Z3_get_decl_name(ctx, acc));
            if nm == acc_name {
                return Some(fi as usize);
            }
        }
    }
    None
}

// ── Step 1: tactic chain ────────────────────────────────────────

unsafe fn simplify_assertions(ctx: Z3_context, body: &[Z3_ast]) -> Vec<Z3_ast> {
    let goal = Z3_mk_goal(ctx, false, false, false);
    Z3_goal_inc_ref(ctx, goal);
    for &a in body {
        Z3_goal_assert(ctx, goal, a);
    }
    let c_simplify = CString::new("simplify").unwrap();
    let c_propagate = CString::new("propagate-values").unwrap();
    let t1 = Z3_mk_tactic(ctx, c_simplify.as_ptr());
    Z3_tactic_inc_ref(ctx, t1);
    let t2 = Z3_mk_tactic(ctx, c_propagate.as_ptr());
    Z3_tactic_inc_ref(ctx, t2);
    let chain = Z3_tactic_and_then(ctx, t1, t2);
    Z3_tactic_inc_ref(ctx, chain);

    let res = Z3_tactic_apply(ctx, chain, goal);
    Z3_apply_result_inc_ref(ctx, res);

    let mut out = Vec::new();
    let ng = Z3_apply_result_get_num_subgoals(ctx, res);
    for i in 0..ng {
        let sg = Z3_apply_result_get_subgoal(ctx, res, i);
        Z3_goal_inc_ref(ctx, sg);
        let n = Z3_goal_size(ctx, sg);
        for j in 0..n {
            let f = Z3_goal_formula(ctx, sg, j);
            Z3_inc_ref(ctx, f);
            out.push(f);
        }
        Z3_goal_dec_ref(ctx, sg);
    }

    Z3_apply_result_dec_ref(ctx, res);
    Z3_tactic_dec_ref(ctx, chain);
    Z3_tactic_dec_ref(ctx, t2);
    Z3_tactic_dec_ref(ctx, t1);
    Z3_goal_dec_ref(ctx, goal);
    out
}

/// Split top-level conjunctions into separate assertions so `extract_program`
/// sees each `(=> P Q)` / `(= var expr)` clause individually. The tactic chain
/// does not always flatten a top-level `(and …)` into separate goal formulas.
unsafe fn flatten_conjunctions(ctx: Z3_context, asts: &[Z3_ast]) -> Vec<Z3_ast> {
    fn push(ctx: Z3_context, a: Z3_ast, out: &mut Vec<Z3_ast>) {
        unsafe {
            if decl_kind(ctx, a) == Some(DeclKind::AND) {
                for c in children(ctx, a) {
                    push(ctx, c, out);
                }
            } else {
                out.push(a);
            }
        }
    }
    let mut out = Vec::new();
    for &a in asts {
        push(ctx, a, &mut out);
    }
    out
}

fn trace_enabled() -> bool {
    std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok()
}

// ── Step 2: extraction ──────────────────────────────────────────

unsafe fn split_equality(ctx: Z3_context, a: Z3_ast) -> Option<(Z3_ast, Z3_ast)> {
    if decl_kind(ctx, a)? != DeclKind::EQ {
        return None;
    }
    let ch = children(ctx, a);
    if ch.len() != 2 {
        return None;
    }
    Some((ch[0], ch[1]))
}

/// Z3 simplify rewrites `(= boolvar (not <expr>))` into
/// `(not (= <expr'> boolvar))` (an XOR shape with `not` lifted outside).
/// That breaks scalar extraction for bool definitions in compiler.ev.
///
/// Recognize the shape and return BOTH directions so the caller can probe
/// each: `((var_l, not_r), (var_r, not_l))`. Used by the assertion loop to
/// try inserting either direction, gated by the standard mentions_name /
/// not-already-defined checks.
unsafe fn split_not_eq_bool_both(
    ctx: Z3_context, a: Z3_ast,
) -> Option<(Z3_ast, Z3_ast, Z3_ast, Z3_ast)> {
    if decl_kind(ctx, a)? != DeclKind::NOT {
        return None;
    }
    let nch = children(ctx, a);
    if nch.len() != 1 {
        return None;
    }
    let (l, r) = split_equality(ctx, nch[0])?;
    // Only valid for Bool sort.
    let lsort = Z3_get_sort(ctx, l);
    if Z3_get_sort_kind(ctx, lsort) != SortKind::Bool {
        return None;
    }
    let not_r = Z3_mk_not(ctx, r);
    Z3_inc_ref(ctx, not_r);
    let not_l = Z3_mk_not(ctx, l);
    Z3_inc_ref(ctx, not_l);
    Some((l, not_r, r, not_l))
}

/// Try to read `a` as a guarded implication that constrains an output, and
/// record the branch in `raw`. Handles `(=> P Q)` (guard `P`) and `(or X Q)`
/// (guard `¬X`, with `Q` whichever side constrains an output).
unsafe fn try_record_guarded(
    ctx: Z3_context,
    a: Z3_ast,
    outputs: &HashSet<String>,
    raw: &mut Raw,
) -> bool {
    let Some(dk) = decl_kind(ctx, a) else { return false };
    let ch = children(ctx, a);
    if dk == DeclKind::IMPLIES && ch.len() == 2 {
        return classify_guarded(ctx, ch[1], ch[0], false, outputs, raw);
    }
    if dk == DeclKind::OR && ch.len() == 2 {
        // `(or X Q)` ≡ `¬X ⇒ Q`. Try each side as the consequent.
        return classify_guarded(ctx, ch[1], ch[0], true, outputs, raw)
            || classify_guarded(ctx, ch[0], ch[1], true, outputs, raw);
    }
    false
}

/// `(= var__len N)` ⇒ `(var, N)`.
unsafe fn match_len_pin(ctx: Z3_context, l: Z3_ast, r: Z3_ast) -> Option<(String, i64)> {
    let name = ast_app_name(ctx, l)?;
    let base = name.strip_suffix("__len")?;
    let n = numeral_i64(ctx, r)?;
    Some((base.to_string(), n))
}

/// `(= (select arr idx_lit) elem)` ⇒ `(arr, idx, elem)`.
unsafe fn match_select_pin(ctx: Z3_context, l: Z3_ast, r: Z3_ast) -> Option<(String, i64, Z3_ast)> {
    if decl_kind(ctx, l)? != DeclKind::SELECT {
        return None;
    }
    let ch = children(ctx, l);
    if ch.len() != 2 {
        return None;
    }
    let arr = ast_app_name(ctx, ch[0])?;
    let idx = numeral_i64(ctx, ch[1])?;
    Some((arr, idx, r))
}

/// `(and (= arr__len N) (= (select arr 0) e0) …)` over a single output `arr`.
unsafe fn collect_seq_in_and(
    ctx: Z3_context,
    and_expr: Z3_ast,
    outputs: &HashSet<String>,
) -> Option<(String, Vec<Z3_ast>)> {
    if decl_kind(ctx, and_expr)? != DeclKind::AND {
        return None;
    }
    let mut arr_name: Option<String> = None;
    let mut declared_len: Option<i64> = None;
    let mut indexed: HashMap<i64, Z3_ast> = HashMap::new();
    for c in children(ctx, and_expr) {
        let (l, r) = split_equality(ctx, c)?;
        if let Some((name, n)) = match_len_pin(ctx, l, r).or_else(|| match_len_pin(ctx, r, l)) {
            if !outputs.contains(&name) {
                return None;
            }
            match &arr_name {
                Some(p) if *p != name => return None,
                _ => arr_name = Some(name),
            }
            declared_len = Some(n);
            continue;
        }
        if let Some((name, idx, elem)) =
            match_select_pin(ctx, l, r).or_else(|| match_select_pin(ctx, r, l))
        {
            if !outputs.contains(&name) {
                return None;
            }
            match &arr_name {
                Some(p) if *p != name => return None,
                _ => arr_name = Some(name),
            }
            indexed.insert(idx, elem);
            continue;
        }
        return None;
    }
    let arr = arr_name?;
    let n = declared_len.unwrap_or(indexed.len() as i64);
    let mut elems = Vec::with_capacity(n as usize);
    for i in 0..n {
        elems.push(*indexed.get(&i)?);
    }
    Some((arr, elems))
}

#[derive(Default)]
struct Raw {
    scalar: HashMap<String, Z3_ast>,
    seq_lengths: HashMap<String, i64>,
    seq_elements: HashMap<String, HashMap<i64, Z3_ast>>,
    guarded: HashMap<String, Vec<Branch>>,
    predicates: Vec<Z3_ast>,
}

/// Classify a guarded consequent `Q` (under guard `P`); returns true if it
/// constrained an output.
unsafe fn classify_guarded(
    ctx: Z3_context,
    conseq: Z3_ast,
    guard: Z3_ast,
    neg: bool,
    outputs: &HashSet<String>,
    raw: &mut Raw,
) -> bool {
    // `Q = (= var expr)` — scalar guarded.
    if let Some((l, r)) = split_equality(ctx, conseq) {
        if let Some(name) = ast_app_name(ctx, l) {
            if outputs.contains(&name) && !mentions_name(ctx, r, &name) {
                raw.guarded.entry(name).or_default().push(Branch { guard, neg, body: GBody::Scalar(r) });
                return true;
            }
        }
        if let Some(name) = ast_app_name(ctx, r) {
            if outputs.contains(&name) && !mentions_name(ctx, l, &name) {
                raw.guarded.entry(name).or_default().push(Branch { guard, neg, body: GBody::Scalar(l) });
                return true;
            }
        }
    }
    // `Q = (and (= var__len N) (= (select var i) e) …)` — seq guarded.
    if let Some((arr, elems)) = collect_seq_in_and(ctx, conseq, outputs) {
        raw.guarded.entry(arr).or_default().push(Branch { guard, neg, body: GBody::Seq(elems) });
        return true;
    }
    // `Q = (and Q1 … Qn)` where the Qi are themselves guarded shapes —
    // Z3's simplifier renders else-if chains this way, e.g. compiler.smt2's
    // effects writer: `(or is_first_tick (and (or _got_path B1)
    // (or (not _got_path) (and (or (not emit_now) B2) (or emit_now B3)))))`.
    // Recurse with guards compounded conjunctively down the tree.
    // Transactional: ALL conjuncts must classify, else roll back — a partial
    // capture would mark the assertion handled while silently dropping the
    // unrecognized conjuncts' constraints.
    if decl_kind(ctx, conseq) == Some(DeclKind::AND) {
        let snapshot = raw.guarded.clone();
        let mut all = true;
        for c in children(ctx, conseq) {
            let ok = 'child: {
                let Some(dk) = decl_kind(ctx, c) else { break 'child false };
                let cch = children(ctx, c);
                if dk == DeclKind::IMPLIES && cch.len() == 2 {
                    let g = conj_guard(ctx, guard, neg, cch[0], false);
                    break 'child classify_guarded(ctx, cch[1], g, false, outputs, raw);
                }
                if dk == DeclKind::OR && cch.len() == 2 {
                    let g1 = conj_guard(ctx, guard, neg, cch[0], true);
                    if classify_guarded(ctx, cch[1], g1, false, outputs, raw) {
                        break 'child true;
                    }
                    let g2 = conj_guard(ctx, guard, neg, cch[1], true);
                    break 'child classify_guarded(ctx, cch[0], g2, false, outputs, raw);
                }
                // Plain conjunct (equality / seq-and / deeper and): same guard.
                classify_guarded(ctx, c, guard, neg, outputs, raw)
            };
            if !ok {
                all = false;
                break;
            }
        }
        if all {
            return true;
        }
        raw.guarded = snapshot;
    }
    false
}

/// Conjunction of an outer guard (with its neg flag) and an inner condition
/// (negated when it came from an `(or X B)` shape). The built ASTs are
/// inc_ref'd and intentionally leaked — Program ASTs live for the process
/// lifetime anyway (see `Program::_keepalive`).
unsafe fn conj_guard(
    ctx: Z3_context,
    outer: Z3_ast,
    outer_neg: bool,
    inner: Z3_ast,
    inner_neg: bool,
) -> Z3_ast {
    let o = if outer_neg {
        let n = Z3_mk_not(ctx, outer);
        Z3_inc_ref(ctx, n);
        n
    } else {
        outer
    };
    let i = if inner_neg {
        let n = Z3_mk_not(ctx, inner);
        Z3_inc_ref(ctx, n);
        n
    } else {
        inner
    };
    let args = [o, i];
    let r = Z3_mk_and(ctx, 2, args.as_ptr());
    Z3_inc_ref(ctx, r);
    r
}

/// Assemble a single var's body from the raw partition, or `None` if `var` has
/// no covering definition. Reads by reference (callable for many vars).
unsafe fn build_body(raw: &Raw, var: &str) -> Option<StepBody> {
    if let Some(branches) = raw.guarded.get(var) {
        if !branches.is_empty() {
            return Some(StepBody::Guarded(branches.clone()));
        }
    }
    if let Some(e) = raw.scalar.get(var) {
        return Some(StepBody::Scalar(*e));
    }
    // Seq from explicit/inferred length + contiguous element pins.
    let elems = raw.seq_elements.get(var);
    let explicit = raw.seq_lengths.get(var).copied();
    let inferred = elems.and_then(|m| {
        let mut i = 0i64;
        while m.contains_key(&i) { i += 1; }
        if i == 0 { None } else { Some(i) }
    });
    let n = explicit.or(inferred)?;
    let map = elems?;
    let mut seq = Vec::with_capacity(n as usize);
    for i in 0..n {
        seq.push(*map.get(&i)?);
    }
    Some(StepBody::Seq(seq))
}

/// Build the raw partition and assemble steps for the outputs plus every
/// *internal* definition (intermediate scalar like `r0.w`, or a record-Seq like
/// `rs`) reachable from an output's expression. `None` if any output lacks a
/// covering assignment, or the dependency graph has a cycle.
///
/// Unlike the reference port — which relied on Z3's `solve-eqs` to substitute
/// intermediates away — the kernel's tactic chain keeps `(= var expr)` shapes,
/// so intermediate defs survive as separate assertions. Pulling the
/// output-reachable ones in as steps is what lets a record-Seq (`(= (select rs
/// i) (mk_Rect …))`) and the scalars feeding it (`(= r0.w (+ 1 count))`) be
/// evaluated when a later step indexes the Seq (`rs[0].w`).
unsafe fn extract_program(
    ctx: Z3_context,
    assertions: &[Z3_ast],
    outputs: &[String],
) -> Option<(Vec<(String, StepBody, bool)>, Vec<Z3_ast>)> {
    let output_set: HashSet<String> = outputs.iter().cloned().collect();
    let mut raw = Raw::default();

    for &a in assertions {
        // Guarded shapes (the `effects` ternary) are only meaningful for the
        // declared outputs.
        if try_record_guarded(ctx, a, &output_set, &mut raw) {
            continue;
        }
        // Bare Bool literal pins: the pre-extraction simplify →
        // propagate-values chain constant-folds Bool defs whose RHS is
        // decidable into `(assert v)` / `(assert (not v))`. Capture them
        // as scalar defs (v := true/false), first-def-wins; when a def
        // already exists the pin stays a predicate (eval-checked per
        // tick). compiler2's driver pins constant-op dispatch flags this
        // way — 9 manifest outputs folded to bare literals and gated ALL
        // extraction (docs/plans/driver-functionizer-diagnosis.md).
        if is_uninterp_const(ctx, a) {
            if let Some(name) = ast_app_name(ctx, a) {
                if !raw.scalar.contains_key(&name) {
                    let t = Z3_mk_true(ctx);
                    Z3_inc_ref(ctx, t);
                    raw.scalar.insert(name, t);
                    continue;
                }
            }
        }
        if decl_kind(ctx, a) == Some(DeclKind::NOT) {
            let nch = children(ctx, a);
            if nch.len() == 1 && is_uninterp_const(ctx, nch[0]) {
                if let Some(name) = ast_app_name(ctx, nch[0]) {
                    if !raw.scalar.contains_key(&name) {
                        let f = Z3_mk_false(ctx);
                        Z3_inc_ref(ctx, f);
                        raw.scalar.insert(name, f);
                        continue;
                    }
                }
            }
        }
        // Handle Z3 simplify's `(not (= a b))` rewrite of `(= boolvar (not expr))`.
        // Try BOTH orientations — both sides may be uninterp consts and we don't
        // know which one is the output. The mentions_name / contains_key gates
        // in the parent branches below filter out wrong-direction captures.
        if let Some((bv1, neg1, bv2, neg2)) = split_not_eq_bool_both(ctx, a) {
            // Symmetric XOR shape. Capturing both directions risks a 2-var
            // cycle (e.g. `is_first_tick = (not got_path)` AND `got_path =
            // (not is_first_tick)`). Restrict to OUTPUT sides only — the
            // kernel needs to compute outputs, and capturing intermediates
            // turned out to break verify on multi-tick test fixtures
            // (test_pipeline_lex_parse regression, commit a597b8c).
            let mut captured = false;
            for (bv, neg) in [(bv1, neg1), (bv2, neg2)] {
                if !is_uninterp_const(ctx, bv) { continue; }
                let Some(name) = ast_app_name(ctx, bv) else { continue };
                if !output_set.contains(&name) { continue; }
                if !raw.scalar.contains_key(&name) && !mentions_name(ctx, neg, &name) {
                    raw.scalar.insert(name, neg);
                    captured = true;
                    break;
                }
            }
            // Intermediates: the flip-flop cycle the OUTPUT-only rule guards
            // against needs bare consts on BOTH sides (`(not (= a b))` with
            // a,b vars — either orientation is a valid def and capturing one
            // here while another assertion captures the reverse creates a
            // 2-var cycle). When exactly ONE side is a const, the shape is
            // `var = (not <compound>)` and the orientation is unambiguous —
            // capture it like any scalar def. compiler.smt2's bool
            // intermediates (`lt_use_cons = (not (= hint ""))`) need this:
            // dropping them left output steps referencing names with no env
            // entry (run-time "missing env entry" refusal).
            if !captured && is_uninterp_const(ctx, bv1) != is_uninterp_const(ctx, bv2) {
                for (bv, neg) in [(bv1, neg1), (bv2, neg2)] {
                    if !is_uninterp_const(ctx, bv) { continue; }
                    let Some(name) = ast_app_name(ctx, bv) else { continue };
                    if !raw.scalar.contains_key(&name) && !mentions_name(ctx, neg, &name) {
                        raw.scalar.insert(name, neg);
                        captured = true;
                        break;
                    }
                }
            }
            if captured { continue; }
        }
        let Some((l, r)) = split_equality(ctx, a) else {
            raw.predicates.push(a);
            continue;
        };
        // Seq length / element pins, for ANY Seq var (outputs and internals).
        if let Some((name, n)) = match_len_pin(ctx, l, r).or_else(|| match_len_pin(ctx, r, l)) {
            raw.seq_lengths.insert(name, n);
            continue;
        }
        if let Some((arr, idx, elem)) =
            match_select_pin(ctx, l, r).or_else(|| match_select_pin(ctx, r, l))
        {
            raw.seq_elements.entry(arr).or_default().insert(idx, elem);
            continue;
        }
        // Scalar `(= var expr)` definitions, for ANY var (first def wins).
        if is_uninterp_const(ctx, l) {
            let name = ast_app_name(ctx, l)?;
            if !raw.scalar.contains_key(&name) && !mentions_name(ctx, r, &name) {
                raw.scalar.insert(name, r);
                continue;
            }
        }
        if is_uninterp_const(ctx, r) {
            let name = ast_app_name(ctx, r)?;
            if !raw.scalar.contains_key(&name) && !mentions_name(ctx, l, &name) {
                raw.scalar.insert(name, l);
                continue;
            }
        }
        raw.predicates.push(a);
    }

    // Assemble candidate bodies: every output (required) + every internally
    // defined var (optional — only kept if reachable from an output below).
    let mut bodies: HashMap<String, StepBody> = HashMap::new();
    for v in outputs {
        match build_body(&raw, v) {
            Some(b) => { bodies.insert(v.clone(), b); }
            None => {
                if std::env::var("EVIDENT_FUNCTIONIZE_WHY").ok().as_deref() == Some("1") {
                    eprintln!("[functionizer-why] uncovered output: {}", v);
                    eprintln!("[functionizer-why]   has guarded?  {}", raw.guarded.contains_key(v));
                    eprintln!("[functionizer-why]   has scalar?   {}", raw.scalar.contains_key(v));
                    eprintln!("[functionizer-why]   seq_lengths?  {:?}", raw.seq_lengths.get(v));
                    eprintln!("[functionizer-why]   seq_elements? {:?}", raw.seq_elements.get(v).map(|m| m.keys().collect::<Vec<_>>()));
                }
                return None;
            }
        }
    }
    let mut internal: HashSet<String> = HashSet::new();
    internal.extend(raw.scalar.keys().cloned());
    internal.extend(raw.seq_elements.keys().cloned());
    internal.extend(raw.guarded.keys().cloned());
    for v in internal {
        if output_set.contains(&v) {
            continue;
        }
        if let Some(b) = build_body(&raw, &v) {
            bodies.insert(v, b);
        }
    }

    // Reachability: keep only outputs and the internal defs an output's
    // expression (transitively) mentions. Unreferenced defs are dropped.
    //
    // The naive shape is O(|bodies|²) — for each body, walk every other body's
    // name through a tree-traversing `mentions_name`. With 200+ outputs and
    // 40K-ish bodies, that's catastrophic. Instead: walk each body's tree ONCE,
    // collecting all 0-arity application names into a HashSet, then intersect.
    let mut reachable: HashSet<String> = output_set.clone();
    let mut queue: Vec<String> = outputs.to_vec();
    while let Some(v) = queue.pop() {
        let Some(b) = bodies.get(&v) else { continue };
        let exprs = body_exprs(b);
        let mut mentioned: HashSet<String> = HashSet::new();
        for &e in &exprs {
            collect_mentioned_names(ctx, e, &mut mentioned);
        }
        for u in &mentioned {
            if !bodies.contains_key(u) || reachable.contains(u) {
                continue;
            }
            reachable.insert(u.clone());
            queue.push(u.clone());
        }
    }
    bodies.retain(|k, _| reachable.contains(k));
    let kept: Vec<String> = bodies.keys().cloned().collect();

    // Topo-order so each step follows the vars it consumes. Steps stuck in
    // mention-level cycles come back in `deferred` (flagged true) and run
    // via run_program's fixpoint rounds instead of refusing extraction.
    let (order, deferred) = topo_order(ctx, &kept, &bodies)?;
    let mut ordered: Vec<(String, StepBody, bool)> = Vec::with_capacity(kept.len());
    for v in order {
        let b = bodies.remove(&v).unwrap();
        ordered.push((v, b, false));
    }
    for v in deferred {
        let b = bodies.remove(&v).unwrap();
        ordered.push((v, b, true));
    }
    Some((ordered, raw.predicates))
}

unsafe fn body_exprs<'a>(body: &'a StepBody) -> Vec<Z3_ast> {
    match body {
        StepBody::Scalar(e) => vec![*e],
        StepBody::Seq(es) => es.clone(),
        StepBody::Guarded(branches) => {
            let mut v = Vec::new();
            for b in branches {
                v.push(b.guard);
                match &b.body {
                    GBody::Scalar(e) => v.push(*e),
                    GBody::Seq(es) => v.extend(es.iter().copied()),
                }
            }
            v
        }
    }
}

unsafe fn topo_order(
    ctx: Z3_context,
    outputs: &[String],
    bodies: &HashMap<String, StepBody>,
) -> Option<(Vec<String>, Vec<String>)> {
    let mut indeg: HashMap<String, usize> = outputs.iter().map(|v| (v.clone(), 0)).collect();
    let mut succ: HashMap<String, Vec<String>> = HashMap::new();
    let outputs_set: HashSet<&str> = outputs.iter().map(|s| s.as_str()).collect();
    for v in outputs {
        let exprs = body_exprs(bodies.get(v)?);
        // Single-pass name collect: O(tree size) vs the naive O(N × tree size).
        let mut mentioned: HashSet<String> = HashSet::new();
        for &e in &exprs {
            collect_mentioned_names(ctx, e, &mut mentioned);
        }
        for other in mentioned.iter() {
            if other == v || !outputs_set.contains(other.as_str()) {
                continue;
            }
            *indeg.get_mut(v).unwrap() += 1;
            succ.entry(other.clone()).or_default().push(v.clone());
        }
    }
    let mut ready: Vec<String> = indeg.iter().filter(|(_, &d)| d == 0).map(|(n, _)| n.clone()).collect();
    ready.sort();
    let mut order = Vec::with_capacity(outputs.len());
    while let Some(n) = ready.pop() {
        order.push(n.clone());
        if let Some(s) = succ.get(&n) {
            for m in s {
                let d = indeg.get_mut(m).unwrap();
                *d -= 1;
                if *d == 0 {
                    ready.push(m.clone());
                }
            }
        }
        ready.sort();
    }
    if order.len() == outputs.len() {
        Some((order, Vec::new()))
    } else {
        // Cycle detected — the remaining bodies have nonzero indegree.
        // No longer a refusal: hand the stuck set back as DEFERRED steps
        // for run_program's fixpoint rounds (the mention graph is guard-
        // blind; runtime-acyclic graphs converge there). The WHY report
        // stays for diagnosis of genuinely stalled cycles.
        if std::env::var("EVIDENT_FUNCTIONIZE_WHY").ok().as_deref() == Some("1") {
            let stuck: HashSet<String> = indeg.iter()
                .filter(|(_, &d)| d > 0)
                .map(|(n, _)| n.clone())
                .collect();
            eprintln!("[functionizer-why] topo_order: cycle ({} of {} stuck)",
                stuck.len(), outputs.len());
            // Build reverse edges (predecessors) inside stuck set: for each v
            // in stuck, find which stuck deps it points TO via succ.
            // succ[u] = [v ...] means u's value is needed by v (v depends on u).
            let mut deps: HashMap<&str, Vec<&str>> = HashMap::new();
            for (u, vs) in &succ {
                if !stuck.contains(u) { continue; }
                for v in vs {
                    if stuck.contains(v) {
                        deps.entry(v.as_str()).or_default().push(u.as_str());
                    }
                }
            }
            // Print all NON-synthesized stuck vars (filter out __call/__wr_).
            // Synthesized intermediates are noise; the real cycle root is in
            // the user-visible vars.
            let mut sorted: Vec<&String> = stuck.iter()
                .filter(|n| !n.contains("__call") && !n.contains("__wr_"))
                .collect();
            sorted.sort();
            eprintln!("[functionizer-why]   non-synthesized stuck vars ({}):", sorted.len());
            for n in &sorted {
                let d = deps.get(n.as_str()).map(|v| v.join(", ")).unwrap_or_default();
                eprintln!("[functionizer-why]   {} ← [{}]", n, &d[..d.len().min(120)]);
            }
        }
        let mut deferred: Vec<String> = indeg
            .iter()
            .filter(|(_, &d)| d > 0)
            .map(|(n, _)| n.clone())
            .collect();
        deferred.sort();
        Some((order, deferred))
    }
}

// ── Step 3+4: assemble, JIT, verify ─────────────────────────────

/// Build the `Program` for this body, or `None` to leave the kernel on the
/// existing Z3 path. `decl_preamble` is the body's declaration s-expressions
/// (from `tick::extract_declarations`) used by the verification solves.
pub unsafe fn functionize(
    ctx: Z3_context,
    body: &[Z3_ast],
    manifest: &Manifest,
    decl_preamble: &str,
    jit_enabled: bool,
    level: StatsLevel,
    trace: bool,
) -> (Option<Program>, FunctionizeStats) {
    // Wave 5d minimum: side-car cache for `simplify_assertions`. The
    // cache path is the input .smt2 path + ".evidentc"; we pull the
    // input path + source through env vars set at startup so the
    // signature stays unchanged.
    let cache_inputs = std::env::var("EVIDENT_CACHE_INPUT_PATH")
        .ok()
        .zip(std::env::var("EVIDENT_CACHE_INPUT_SRC").ok());
    let cached = if let Some((ref path_str, ref src)) = cache_inputs {
        let path = std::path::PathBuf::from(path_str);
        crate::evidentc::try_load(&path, src, ctx, decl_preamble)
    } else {
        None
    };
    let simplified = if let Some(cached) = cached {
        if trace {
            eprintln!("[fz] evidentc cache HIT — skipped simplify+propagate-values");
        }
        cached
    } else {
        let s = simplify_assertions(ctx, body);
        if let Some((ref path_str, ref src)) = cache_inputs {
            let path = std::path::PathBuf::from(path_str);
            let _ = crate::evidentc::save(&path, src, ctx, decl_preamble, &s);
        }
        s
    };
    let flat = flatten_conjunctions(ctx, &simplified);
    let mut stats = FunctionizeStats::new(level, trace);
    stats.total_asserts = flat.len();
    if std::env::var("EVIDENT_FUNCTIONIZE_DUMP").is_ok() {
        for (i, &a) in flat.iter().enumerate() {
            let p = Z3_ast_to_string(ctx, a);
            let s = if p.is_null() { String::new() } else { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() };
            eprintln!("[fz/dump] flat[{i}] = {s}");
        }
    }

    // Refuse the fast path, recording why (None case) — leaves the kernel on
    // the Z3 path with `stats` describing the residual.
    macro_rules! refuse {
        ($reason:expr) => {{
            let why: String = $reason.into();
            if trace { eprintln!("[fz] {why}"); }
            stats.functionized = false;
            stats.jit = 0;
            stats.interp = 0;
            stats.residual = stats.total_asserts;
            stats.refuse_reason = Some(why);
            return (None, stats);
        }};
    }

    let mut outputs: Vec<String> = manifest.state_fields.iter().map(|(n, _)| n.clone()).collect();
    outputs.push(manifest.effects_name.clone());

    let Some((raw_steps, predicates)) = extract_program(ctx, &flat, &outputs) else {
        refuse!("extract_program: an output had no covering assignment");
    };

    // Record-Seq intermediates the JIT can inline: `var → element ASTs`.
    // A scalar step indexing one of these (`rs[0].w`) resolves the select +
    // accessor to a leaf field AST at compile time, so it still JITs.
    let seqs: HashMap<String, Vec<Z3_ast>> = raw_steps.iter().filter_map(|(v, b, _)| {
        if v == &manifest.effects_name { return None; }
        if let StepBody::Seq(es) = b { Some((v.clone(), es.clone())) } else { None }
    }).collect();

    // Enforce: each declared state field is a scalar/guarded-scalar; `effects`
    // is a seq/guarded-seq. Internal record-Seq / scalar steps may be either.
    let mut steps: Vec<Step> = Vec::new();
    let mut jit_count = 0usize;
    let mut interp_count = 0usize;
    let mut names = low::Names::default();
    for (var, body, deferred) in raw_steps {
        let is_effects = var == manifest.effects_name;
        let is_state = manifest.state_fields.iter().any(|(n, _)| n == &var);
        let body_is_seq = match &body {
            StepBody::Seq(_) => true,
            StepBody::Guarded(branches) => branches.iter().any(|b| matches!(b.body, GBody::Seq(_))),
            StepBody::Scalar(_) => false,
        };
        // A state field is read back as a primitive/datatype scalar; it cannot
        // be a Seq. `effects` must be a Seq.
        if is_state && body_is_seq {
            refuse!(format!("state field `{var}` is a Seq (carried across ticks → opaque to functionizer)"));
        }
        if is_effects && !body_is_seq {
            refuse!(format!("effects step `{var}` is not a Seq"));
        }

        let (result_is_bool, jit) = match &body {
            StepBody::Scalar(e) => {
                let is_bool = is_bool_sorted(ctx, *e);
                let mut j = None;
                if jit_enabled && is_int_or_bool(ctx, *e) {
                    j = jit::compile_step(ctx, *e, &seqs);
                }
                if j.is_some() { jit_count += 1; } else { interp_count += 1; }
                (is_bool, j)
            }
            _ => (false, None),
        };
        let lowb = match &body {
            StepBody::Scalar(e) => LowBody::Scalar(low::lower(ctx, *e, &mut names)),
            StepBody::Seq(es) => {
                LowBody::Seq(es.iter().map(|&e| low::lower(ctx, e, &mut names)).collect())
            }
            StepBody::Guarded(bs) => LowBody::Guarded(bs.iter().map(|b| LowBranch {
                guard: low::lower(ctx, b.guard, &mut names),
                neg: b.neg,
                body: match &b.body {
                    GBody::Scalar(e) => LowGBody::Scalar(low::lower(ctx, *e, &mut names)),
                    GBody::Seq(es) => LowGBody::Seq(
                        es.iter().map(|&e| low::lower(ctx, e, &mut names)).collect()),
                },
            }).collect()),
        };
        let var_slot = names.intern(&var);
        let jit_slots: Vec<u32> = jit.as_ref()
            .map(|j| j.inputs.iter().map(|n| names.intern(n)).collect())
            .unwrap_or_default();
        steps.push(Step {
            var, body, low: lowb, var_slot, jit_slots,
            deferred, result_is_bool, is_effects, jit,
        });
    }
    let low_predicates: Vec<low::LExpr> =
        predicates.iter().map(|&p| low::lower(ctx, p, &mut names)).collect();
    let plan = SlotPlan {
        is_first_tick: names.intern("is_first_tick"),
        last_results: names.intern("last_results"),
        last_results_len: names.intern("last_results__len"),
        carries: manifest.state_fields.iter()
            .map(|(n, _)| names.intern(&format!("_{n}"))).collect(),
        state_out: manifest.state_fields.iter()
            .map(|(n, _)| names.intern(n)).collect(),
        sentinels: manifest.state_fields.iter().map(|(_, ty)| match ty.as_str() {
            "Int" => Sv::Int(0),
            "Bool" => Sv::Bool(false),
            "String" => Sv::Str(String::new()),
            _ => Sv::Datatype(format!("_sentinel_{ty}"), Vec::new()),
        }).collect(),
    };
    let lowered = std::env::var("EVIDENT_FUNCTIONIZE_LOWER").ok().as_deref() != Some("0");
    if trace {
        eprintln!("[fz] lowered: {} steps, {} env slots, lowered path {}",
            steps.len(), names.len(), if lowered { "ON" } else { "OFF" });
    }

    let mut keepalive = simplified;
    keepalive.shrink_to_fit();
    let mut prog = Program {
        steps, predicates, low_predicates, names, plan, lowered, jit_count, interp_count,
        tick0_carries: HashMap::new(),
        _keepalive: keepalive,
    };

    // ── Verify on tick 0 and tick 1 against a real Z3 solve. ──
    // Set EVIDENT_FUNCTIONIZE_SKIP_VERIFY=1 to bypass entirely (debug only,
    // unsafe — relaxes the soundness gate that prevents silent divergence).
    if std::env::var("EVIDENT_FUNCTIONIZE_SKIP_VERIFY").ok().as_deref() != Some("1") {
        let empty_prev: Vec<Option<Sv>> = vec![None; manifest.state_fields.len()];
        let Ok(Some(z3_0)) = tick::solve_tick_sv(ctx, body, decl_preamble, manifest, true, &empty_prev) else {
            refuse!("verify: tick-0 Z3 solve failed");
        };
        // Seed the eval with the carry values Z3 chose (see Program doc).
        prog.tick0_carries = z3_0.2.clone();
        let Some(mine_0) = run_program(ctx, &prog, manifest, true, &empty_prev, &[]) else {
            refuse!("verify: tick-0 eval refused (unsupported op)");
        };
        if !outputs_match(manifest, &(z3_0.0.clone(), z3_0.1.clone()), &mine_0) {
            refuse!("verify: tick-0 mismatch vs Z3");
        }
        let prev1: Vec<Option<Sv>> = z3_0.0.iter().cloned().map(Some).collect();
        let Ok(Some(z3_1)) = tick::solve_tick_sv(ctx, body, decl_preamble, manifest, false, &prev1) else {
            refuse!("verify: tick-1 Z3 solve failed");
        };
        let Some(mine_1) = run_program(ctx, &prog, manifest, false, &prev1, &[]) else {
            refuse!("verify: tick-1 eval refused (unsupported op)");
        };
        if !outputs_match(manifest, &(z3_1.0.clone(), z3_1.1.clone()), &mine_1) {
            refuse!("verify: tick-1 mismatch vs Z3");
        }
    }

    // Fast path engaged — populate the diagnostic counts. J/I count *steps*
    // (a JIT step vs an interpreted one, incl. guarded/seq steps which never
    // JIT), distinct from `prog.jit_count`/`interp_count` which track scalars
    // only. Residual = the eval-time predicates not turned into output steps.
    stats.functionized = true;
    stats.jit = prog.steps.iter().filter(|s| s.jit.is_some()).count();
    stats.interp = prog.steps.iter().filter(|s| s.jit.is_none()).count();
    stats.residual = prog.predicates.len();
    if level == StatsLevel::Verbose {
        stats.steps = build_step_reports(ctx, &prog);
    }
    (Some(prog), stats)
}

/// `build_inputs` + the previous tick's effect results (as decoded
/// `Sv::Datatype` Result values: IntResult/StringResult/…). Overwrites the
/// empty-seed `last_results` entries `build_inputs` installs. The Z3 path
/// pins exactly these values per tick; without them, FSMs that read
/// `last_results` (compiler.smt2 receives its ReadLine/ReadFile inputs that
/// way) silently see empty results on the fast path.
pub fn build_inputs_with_results(
    is_first: bool,
    prev_state: &[Option<Sv>],
    manifest: &Manifest,
    tick0_carries: Option<&HashMap<String, Sv>>,
    results: &[Sv],
) -> HashMap<String, Sv> {
    let mut env = build_inputs(is_first, prev_state, manifest, tick0_carries);
    if !results.is_empty() {
        env.insert("last_results__len".to_string(), Sv::Int(results.len() as i64));
        let no_result = Sv::Datatype("NoResult".to_string(), Vec::new());
        let mut seq: Vec<Sv> = results.to_vec();
        while seq.len() < 16 {
            seq.push(no_result.clone());
        }
        env.insert("last_results".to_string(), Sv::Seq(seq));
    }
    env
}

/// Inputs for a tick: `is_first_tick` + each `_<name>` state-carry.
/// `tick0_carries` (tick 0 only): the carry values Z3's verify model chose —
/// see `Program::tick0_carries`. Fields absent from the map fall back to the
/// type sentinel.
pub fn build_inputs(
    is_first: bool,
    prev_state: &[Option<Sv>],
    manifest: &Manifest,
    tick0_carries: Option<&HashMap<String, Sv>>,
) -> HashMap<String, Sv> {
    let mut env = HashMap::new();
    env.insert("is_first_tick".to_string(), Sv::Bool(is_first));
    // Seed the kernel's effect-result history. On tick 0 these are unconstrained
    // by the body and the kernel injects no pins; an empty Seq is the only
    // value that satisfies `last_results__len >= 0` without contradiction.
    env.insert("last_results__len".to_string(), Sv::Int(0));
    // Match the pin shape in `solve_tick_sv`: index 0..16 = NoResult so OOB
    // reads in extracted bodies match Z3 (both return NoResult, both
    // recognizers say false, both ITEs take the else arm).
    let no_result = Sv::Datatype("NoResult".to_string(), Vec::new());
    env.insert("last_results".to_string(), Sv::Seq(vec![no_result; 16]));
    for (i, (name, ty)) in manifest.state_fields.iter().enumerate() {
        let key = format!("_{name}");
        if is_first {
            // On tick 0 prefer the carry value Z3's verify model chose
            // (bit-identical to the Z3 path; body equations CAN observe
            // carries unguarded — compiler.smt2's `*_nil` recognizers do).
            if let Some(v) = tick0_carries.and_then(|m| m.get(&key)) {
                env.insert(key, v.clone());
                continue;
            }
            // Fallback: a type-correct sentinel so a JIT step that eagerly
            // loads `_<name>` (e.g. the untaken `ite` arm) has a slot.
            let sentinel = match ty.as_str() {
                "Int" => Sv::Int(0),
                "Bool" => Sv::Bool(false),
                "String" => Sv::Str(String::new()),
                // For user datatypes (TokenList, EnumVariantDecl, …) supply
                // an Sv::Datatype with the type's NAME as the constructor —
                // the eval interpreter compares `actual == want` on
                // recognizers; this sentinel matches no real variant so any
                // recognizer returns false (the dead-arm convention). Field
                // count 0 since accessor calls on the sentinel return None
                // (handled by the caller fall-through).
                _ => Sv::Datatype(format!("_sentinel_{ty}"), Vec::new()),
            };
            env.insert(key, sentinel);
        } else if let Some(v) = &prev_state[i] {
            env.insert(key, v.clone());
        }
    }
    env
}

/// Run the extracted program for one tick. `None` ⇒ a shape/predicate the fast
/// path can't honour ⇒ caller falls through to Z3.
///
/// Default path: the lowered (FFI-free) IR over a slot vector — no HashMap
/// build, no env clone (measured 2026-06-10: the legacy path's per-tick
/// `format!`+HashMap input rebuild alone costs ~0.3 ms on the driver's 1,543
/// state fields). `EVIDENT_FUNCTIONIZE_LOWER=0` selects the legacy FFI
/// interpreter.
pub unsafe fn run_program(
    ctx: Z3_context,
    prog: &Program,
    manifest: &Manifest,
    is_first: bool,
    prev_state: &[Option<Sv>],
    results: &[Sv],
) -> Option<RunOut> {
    if !prog.lowered {
        let inputs = build_inputs_with_results(
            is_first, prev_state, manifest, Some(&prog.tick0_carries), results);
        return run_program_legacy(ctx, prog, manifest, &inputs);
    }
    let plan = &prog.plan;
    let mut slots: Vec<Option<Sv>> = vec![None; prog.names.len()];
    slots[plan.is_first_tick as usize] = Some(Sv::Bool(is_first));
    // Mirror `build_inputs_with_results`: pad `last_results` to 16 with
    // NoResult so OOB reads match the Z3 path's pin shape.
    let no_result = Sv::Datatype("NoResult".to_string(), Vec::new());
    if results.is_empty() {
        slots[plan.last_results_len as usize] = Some(Sv::Int(0));
        slots[plan.last_results as usize] = Some(Sv::Seq(vec![no_result; 16]));
    } else {
        slots[plan.last_results_len as usize] = Some(Sv::Int(results.len() as i64));
        let mut seq: Vec<Sv> = results.to_vec();
        while seq.len() < 16 {
            seq.push(no_result.clone());
        }
        slots[plan.last_results as usize] = Some(Sv::Seq(seq));
    }
    for (i, (name, _)) in manifest.state_fields.iter().enumerate() {
        let slot = plan.carries[i] as usize;
        if is_first {
            let key = format!("_{name}");
            slots[slot] = Some(match prog.tick0_carries.get(&key) {
                Some(v) => v.clone(),
                None => plan.sentinels[i].clone(),
            });
        } else if let Some(v) = &prev_state[i] {
            slots[slot] = Some(v.clone());
        }
    }

    let mut effects: Vec<Sv> = Vec::new();
    let mut deferred: Vec<&Step> = Vec::new();
    let mut jit_buf: Vec<i64> = Vec::new();
    let mut meta = vec![low::StrMeta::default(); prog.names.len()];
    // Per-step timing probe (EVIDENT_FZ_STEPTIME=<tick>): on that tick,
    // time each step and print the 25 costliest. The probe tick runs its
    // steps twice (probe + real pass) — timing only, results unchanged.
    let probe: Option<usize> = std::env::var("EVIDENT_FZ_STEPTIME").ok().and_then(|v| v.parse().ok());
    if let Some(want) = probe {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static TICKNO: AtomicUsize = AtomicUsize::new(0);
        let t = TICKNO.fetch_add(1, Ordering::Relaxed);
        if t == want {
            let mut rows: Vec<(String, f64)> = Vec::new();
            for step in &prog.steps {
                if step.deferred { continue; }
                let t0 = std::time::Instant::now();
                let ok = exec_step_low(step, &mut slots, &mut effects, true, &mut jit_buf, &mut meta);
                rows.push((step.var.clone(), t0.elapsed().as_secs_f64() * 1e6));
                if !ok { break; }
            }
            rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            let total: f64 = rows.iter().map(|r| r.1).sum();
            eprintln!("[fz/steptime] tick {t}: {total:.1} us over {} steps", rows.len());
            for (n, us) in rows.iter().take(25) {
                eprintln!("[fz/steptime]   {us:8.2} us  {n}");
            }
        }
    }
    for step in &prog.steps {
        if step.deferred {
            deferred.push(step);
            continue;
        }
        if !exec_step_low(step, &mut slots, &mut effects, true, &mut jit_buf, &mut meta) {
            return None;
        }
    }
    let mut pending = deferred;
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|s| !exec_step_low(s, &mut slots, &mut effects, false, &mut jit_buf, &mut meta));
        if pending.len() == before {
            if trace_enabled() {
                eprintln!(
                    "[fz/run] deferred fixpoint stalled: {} unresolved (first: {:?})",
                    pending.len(),
                    pending.first().map(|s| s.var.as_str())
                );
            }
            return None;
        }
    }
    for p in &prog.low_predicates {
        if let Some(v) = low::eval(p, &slots, &mut meta) {
            if matches!(v.as_ref(), Sv::Bool(false)) {
                return None;
            }
        }
    }
    let state = plan.state_out.iter().map(|&s| slots[s as usize].take()).collect();
    Some(RunOut { state, effects })
}

/// Execute one lowered step against the slot env — semantics mirror
/// `exec_step` (the legacy AST-eval path) exactly.
fn exec_step_low(
    step: &Step,
    slots: &mut Vec<Option<Sv>>,
    effects: &mut Vec<Sv>,
    use_jit: bool,
    jit_buf: &mut Vec<i64>,
    meta: &mut [low::StrMeta],
) -> bool {
    match &step.low {
        LowBody::Scalar(le) => {
            // Deferred (fixpoint) callers pass use_jit=false — see `exec_step`.
            let v = if let (true, Some(j)) = (use_jit, &step.jit) {
                match j.call_slots(&step.jit_slots, slots, jit_buf) {
                    Some(r) => if step.result_is_bool { Sv::Bool(r != 0) } else { Sv::Int(r) },
                    None => {
                        if trace_enabled() {
                            eprintln!("[fz/run] scalar step {:?} JIT call refused", step.var);
                        }
                        return false;
                    }
                }
            } else {
                match low::eval(le, slots, meta) {
                    Some(v) => v.into_owned(),
                    None => {
                        if trace_enabled() { eprintln!("[fz/run] scalar step {:?} eval refused", step.var); }
                        return false;
                    }
                }
            };
            slots[step.var_slot as usize] = Some(v);
            low::reset_meta(meta, step.var_slot as usize);
        }
        LowBody::Seq(les) => {
            let mut seq = Vec::with_capacity(les.len());
            for le in les {
                match low::eval(le, slots, meta) {
                    Some(v) => seq.push(v.into_owned()),
                    None => {
                        if trace_enabled() { eprintln!("[fz/run] seq step {:?} elem eval refused", step.var); }
                        return false;
                    }
                }
            }
            if step.is_effects {
                *effects = seq;
            } else {
                slots[step.var_slot as usize] = Some(Sv::Seq(seq));
                low::reset_meta(meta, step.var_slot as usize);
            }
        }
        LowBody::Guarded(branches) => {
            let mut chosen: Option<&LowGBody> = None;
            for b in branches {
                match low::eval(&b.guard, slots, meta) {
                    Some(v) => {
                        if let Sv::Bool(g) = v.as_ref() {
                            let fires = if b.neg { !g } else { *g };
                            if fires { chosen = Some(&b.body); break; }
                        }
                    }
                    None => {
                        if trace_enabled() { eprintln!("[fz/run] guarded step {:?} guard eval refused", step.var); }
                        return false;
                    }
                }
            }
            let Some(body) = chosen else {
                if trace_enabled() {
                    eprintln!("[fz/run] guarded step {:?}: no branch guard matched", step.var);
                }
                return false;
            };
            match body {
                LowGBody::Scalar(le) => {
                    let Some(v) = low::eval(le, slots, meta) else {
                        if trace_enabled() { eprintln!("[fz/run] guarded step {:?} scalar body refused", step.var); }
                        return false;
                    };
                    slots[step.var_slot as usize] = Some(v.into_owned());
                    low::reset_meta(meta, step.var_slot as usize);
                }
                LowGBody::Seq(les) => {
                    let mut seq = Vec::with_capacity(les.len());
                    for le in les {
                        match low::eval(le, slots, meta) {
                            Some(v) => seq.push(v.into_owned()),
                            None => {
                                if trace_enabled() { eprintln!("[fz/run] guarded step {:?} seq elem refused", step.var); }
                                return false;
                            }
                        }
                    }
                    if step.is_effects {
                        *effects = seq;
                    } else {
                        slots[step.var_slot as usize] = Some(Sv::Seq(seq));
                        low::reset_meta(meta, step.var_slot as usize);
                    }
                }
            }
        }
    }
    true
}

/// Legacy per-tick path: the FFI AST interpreter over a name-keyed env.
unsafe fn run_program_legacy(
    ctx: Z3_context,
    prog: &Program,
    manifest: &Manifest,
    inputs: &HashMap<String, Sv>,
) -> Option<RunOut> {
    let mut env = inputs.clone();
    let mut effects: Vec<Sv> = Vec::new();

    // Pass 1: the topo-ordered DAG steps. Pass 2: fixpoint rounds over the
    // DEFERRED steps (members of mention-level dependency cycles). The
    // mention graph over-approximates: `ite` hides which branch actually
    // reads what, so guard-acyclic FSMs (compiler2's P3e enum machine) look
    // cyclic to topo_order. eval_scalar is lazy — it only recurses into the
    // TAKEN branch — so retry rounds resolve every runtime-acyclic step;
    // a stalled round means a REAL cycle (or unsupported shape) → refuse
    // the tick (Z3 fallback), with verify still gating overall soundness.
    let mut deferred: Vec<&Step> = Vec::new();
    for step in &prog.steps {
        if step.deferred {
            deferred.push(step);
            continue;
        }
        if !exec_step(ctx, step, &mut env, &mut effects, true) {
            return None;
        }
    }
    let mut pending = deferred;
    while !pending.is_empty() {
        let before = pending.len();
        pending.retain(|s| !exec_step(ctx, s, &mut env, &mut effects, false));
        if pending.len() == before {
            if trace_enabled() {
                eprintln!(
                    "[fz/run] deferred fixpoint stalled: {} unresolved (first: {:?})",
                    pending.len(),
                    pending.first().map(|s| s.var.as_str())
                );
            }
            return None;
        }
    }

    // Enforce predicates that reference only bound vars. A predicate that
    // evaluates false ⇒ this tick is UNSAT for the fast path ⇒ fall through.
    for &p in &prog.predicates {
        if let Some(Sv::Bool(b)) = eval::eval_scalar(ctx, p, &env) {
            if !b {
                return None;
            }
        }
    }

    let state = manifest.state_fields.iter().map(|(n, _)| env.remove(n)).collect();
    Some(RunOut { state, effects })
}

/// Execute one step against the env: bind its value (or set `effects`) and
/// return true, or return false when something it needs isn't available /
/// supported — non-deferred callers treat false as tick refusal; the
/// deferred fixpoint treats it as retry-next-round.
unsafe fn exec_step(
    ctx: Z3_context,
    step: &Step,
    env: &mut HashMap<String, Sv>,
    effects: &mut Vec<Sv>,
    use_jit: bool,
) -> bool {
    {
        match &step.body {
            StepBody::Scalar(ast) => {
                // Deferred (fixpoint) callers pass use_jit=false: JIT loads
                // EVERY named input eagerly, defeating the eval interpreter's
                // branch-laziness that fixpoint resolution depends on — a
                // JIT'd cycle member would never succeed while any
                // mentioned-but-unneeded input is still unbound.
                let v = if let (true, Some(j)) = (use_jit, &step.jit) {
                    match j.call(&env) {
                        Some(r) => if step.result_is_bool { Sv::Bool(r != 0) } else { Sv::Int(r) },
                        None => {
                            if trace_enabled() {
                                eprintln!("[fz/run] scalar step {:?} JIT call refused; inputs={:?} env keys={:?}",
                                    step.var, j.inputs, env.keys().collect::<Vec<_>>());
                            }
                            return false;
                        }
                    }
                } else {
                    match eval::eval_scalar(ctx, *ast, &env) {
                        Some(v) => v,
                        None => {
                            if trace_enabled() { eprintln!("[fz/run] scalar step {:?} eval refused", step.var); }
                            return false;
                        }
                    }
                };
                env.insert(step.var.clone(), v);
            }
            StepBody::Seq(asts) => {
                let mut seq = Vec::with_capacity(asts.len());
                for &e in asts {
                    match eval::eval_scalar(ctx, e, &env) {
                        Some(v) => seq.push(v),
                        None => {
                            if trace_enabled() { eprintln!("[fz/run] seq step {:?} elem eval refused", step.var); }
                            return false;
                        }
                    }
                }
                if step.is_effects {
                    *effects = seq;
                } else {
                    // Record-Seq intermediate: bind so later steps can index it.
                    env.insert(step.var.clone(), Sv::Seq(seq));
                }
            }
            StepBody::Guarded(branches) => {
                let mut chosen: Option<&GBody> = None;
                for b in branches {
                    match eval::eval_scalar(ctx, b.guard, &env) {
                        Some(Sv::Bool(g)) => {
                            let fires = if b.neg { !g } else { g };
                            if fires { chosen = Some(&b.body); break; }
                        }
                        Some(_) => {}
                        None => {
                            if trace_enabled() { eprintln!("[fz/run] guarded step {:?} guard eval refused", step.var); }
                            return false;
                        }
                    }
                }
                let Some(body) = chosen else {
                    if trace_enabled() {
                        eprintln!("[fz/run] guarded step {:?}: no branch guard matched", step.var);
                        for (i, b) in branches.iter().enumerate() {
                            let p = Z3_ast_to_string(ctx, b.guard);
                            let s = if p.is_null() { String::new() }
                                    else { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() };
                            let head: String = s.chars().take(120).collect();
                            eprintln!("[fz/run]   guard[{i}] neg={} val={:?}: {head}",
                                b.neg, eval::eval_scalar(ctx, b.guard, &env));
                        }
                    }
                    return false;
                };
                match body {
                    GBody::Scalar(e) => {
                        let Some(v) = eval::eval_scalar(ctx, *e, &env) else {
                            if trace_enabled() { eprintln!("[fz/run] guarded step {:?} scalar body refused", step.var); }
                            return false;
                        };
                        env.insert(step.var.clone(), v);
                    }
                    GBody::Seq(es) => {
                        let mut seq = Vec::with_capacity(es.len());
                        for &e in es {
                            match eval::eval_scalar(ctx, e, &env) {
                                Some(v) => seq.push(v),
                                None => {
                                    if trace_enabled() { eprintln!("[fz/run] guarded step {:?} seq elem refused", step.var); }
                                    return false;
                                }
                            }
                        }
                        if step.is_effects {
                            *effects = seq;
                        } else {
                            env.insert(step.var.clone(), Sv::Seq(seq));
                        }
                    }
                }
            }
        }
    }
    true
}

type Z3Tick = (Vec<Sv>, Vec<Sv>);

fn outputs_match(manifest: &Manifest, z3: &Z3Tick, mine: &RunOut) -> bool {
    let trace = std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok()
        || std::env::var("EVIDENT_FUNCTIONIZE_WHY").ok().as_deref() == Some("1");
    let mut diffs = 0usize;
    for (i, (name, _)) in manifest.state_fields.iter().enumerate() {
        match mine.state.get(i).and_then(|v| v.as_ref()) {
            Some(v) if tick::compare_sv_pub(v, &z3.0[i]) => {}
            Some(v) => {
                if trace && diffs < 10 {
                    eprintln!("[fz/verify] mismatch on {name}: mine={v:?} z3={:?}", &z3.0[i]);
                }
                diffs += 1;
            }
            None => {
                if trace && diffs < 10 {
                    eprintln!("[fz/verify] missing {name} in mine; z3={:?}", &z3.0[i]);
                }
                diffs += 1;
            }
        }
    }
    if diffs > 0 {
        if trace { eprintln!("[fz/verify] total state-field mismatches: {diffs}"); }
        return false;
    }
    if mine.effects.len() != z3.1.len() {
        if trace { eprintln!("[fz/verify] effects len mismatch: mine={} z3={}", mine.effects.len(), z3.1.len()); }
        return false;
    }
    mine.effects.iter().zip(z3.1.iter()).all(|(a, b)| tick::compare_sv_pub(a, b))
}
