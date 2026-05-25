//! Extracted Z3 program shape used by the functionizer pipeline.
//!
//! `Z3Program` is the intermediate form produced by `z3_eval`'s
//! extraction pass: a claim's body simplified by Z3 tactics and then
//! partitioned into per-output assignments + consistency checks +
//! residual predicates. The Cranelift functionizer consumes this to
//! emit native code; the slow-path evaluator interprets it directly.

use z3::ast::{Bool, Dynamic};

use crate::core::Value;

/// A claim's body, simplified by Z3 tactics and indexed by output
/// variable name. Each entry maps `output_var → Z3 expression AST`.
/// The AST may reference other output variables (which appear
/// earlier in the topo-sort order) or input variables (in `given`).
///
/// `consistency_checks` records assertions that don't define an
/// output variable — typically equalities between two `given` vars
/// that the body further constrains. The evaluator verifies these
/// against the given values; failure = UNSAT.
#[derive(Debug, Clone)]
pub struct Z3Program<'ctx> {
    /// Topologically-ordered: each step's expression only references
    /// inputs (`given`) or earlier steps' outputs.
    pub steps: Vec<Z3Step<'ctx>>,
    /// `(lhs, rhs)` pairs — assertions of form `(= a b)` where
    /// neither side defines a fresh output.
    pub checks: Vec<(Dynamic<'ctx>, Dynamic<'ctx>)>,
    /// Non-equality Bool assertions that must evaluate to true
    /// under the given values + computed outputs. e.g. `x < 5`
    /// from `schema S; x ∈ Nat; x < 5` when `x` is in given.
    pub predicates: Vec<Bool<'ctx>>,
}

#[derive(Debug, Clone)]
pub enum Z3Step<'ctx> {
    /// Scalar output: `var = expr`. Eval expr.
    Scalar { var: String, expr: Dynamic<'ctx> },
    /// Sequence output: built from `len = N` and per-index
    /// `(select var i) = elem` assertions Z3 emits when a Seq
    /// gets pinned by `seq = ⟨a, b, c⟩`. Eval each elem, build
    /// a `Value::SeqEnum` (or appropriate Seq* variant).
    Seq    { var: String, elem_exprs: Vec<Dynamic<'ctx>> },
    /// Guarded output (from `match`/`ITE`/`Implies` patterns).
    /// Z3 emits these as `(or (not P) Q)` assertions where Q
    /// constrains `var` under guard P. At eval time we find
    /// the first branch whose guard evaluates to true and use
    /// that branch's expression.
    Guarded { var: String, branches: Vec<GuardedBranch<'ctx>> },
    /// Pre-baked constant Value, computed once at compile time
    /// via model extraction. Used for outputs whose simplified
    /// body decomposed into per-field accessor pins (record-Seq
    /// constants like `platforms` / `e_init` in Mario) that
    /// `extract_program` can't recompose. At eval time, just
    /// insert the value into the env.
    PreBaked { var: String, value: Value },

    // ── Sampler steps (probabilistic-programming style) ──────────
    //
    // These describe an output variable that is NOT defined by an
    // equation but IS bounded by a range / enum / finite set. A
    // satisfying assignment is *drawn* (deterministically, per the
    // SatisfierFunctionizer's seeded PRNG) rather than computed. The
    // Cranelift functionizer refuses them (returns `None`); only the
    // `SatisfierFunctionizer` consumes them. The extractor emits them
    // only when sampling is opted-in (see `z3_eval::recover_samplers`).

    /// Scalar `Int`/`Nat`/`Pos` output bounded by `lo ≤ var ≤ hi`
    /// (inclusive on both ends). Sampled to a value in `[lo, hi]`.
    SampleRange { var: String, lo: i64, hi: i64 },
    /// Enum output with no other constraint. `type_name` keys the
    /// `EnumRegistry`; the variant set (and thus the count) is
    /// resolved at compile time. Sampled to one of the nullary
    /// variants.
    SampleEnum  { var: String, type_name: String },
    /// Output drawn from a concrete finite set of candidate values
    /// (from `var ∈ {a, b, c}`). Sampled to one of `candidates`.
    SampleSet   { var: String, candidates: Vec<Value> },
}

#[derive(Debug, Clone)]
pub struct GuardedBranch<'ctx> {
    pub guard: Dynamic<'ctx>,
    pub body:  GuardedBody<'ctx>,
}

#[derive(Debug, Clone)]
pub enum GuardedBody<'ctx> {
    Scalar(Dynamic<'ctx>),
    Seq(Vec<Dynamic<'ctx>>),
}

impl<'ctx> Z3Step<'ctx> {
    pub fn var(&self) -> &str {
        match self {
            Z3Step::Scalar      { var, .. }
            | Z3Step::Seq         { var, .. }
            | Z3Step::Guarded     { var, .. }
            | Z3Step::PreBaked    { var, .. }
            | Z3Step::SampleRange { var, .. }
            | Z3Step::SampleEnum  { var, .. }
            | Z3Step::SampleSet   { var, .. } => var,
        }
    }
}
