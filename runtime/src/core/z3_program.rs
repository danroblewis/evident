//! `Z3Program` IR: a claim's body simplified by Z3 tactics and partitioned into
//! per-output assignments + consistency checks + residual predicates.

use z3::ast::{Bool, Dynamic};

use crate::core::Value;

/// Topo-ordered per-output assignments + consistency checks + residual predicates.
#[derive(Debug, Clone)]
pub struct Z3Program<'ctx> {
    /// Each step's expression references only inputs (`given`) or earlier outputs.
    pub steps: Vec<Z3Step<'ctx>>,
    /// `(lhs, rhs)` equalities where neither side defines a fresh output.
    pub checks: Vec<(Dynamic<'ctx>, Dynamic<'ctx>)>,
    /// Non-equality Bool assertions (e.g. `x < 5`) that must hold under given + computed values.
    pub predicates: Vec<Bool<'ctx>>,
    /// Claim name for diagnostics (EVIDENT_FZ_DUMP_PROGRAM header). None for anonymous programs.
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Z3Step<'ctx> {
    /// `var = expr` scalar assignment.
    Scalar { var: String, expr: Dynamic<'ctx> },
    /// Seq output built from per-index elem assertions.
    Seq    { var: String, elem_exprs: Vec<Dynamic<'ctx>> },
    /// Guard-branched output from `match`/ITE/Implies. First true guard wins.
    Guarded { var: String, branches: Vec<GuardedBranch<'ctx>> },
    /// Pre-baked constant extracted at compile time; inserted verbatim at eval.
    PreBaked { var: String, value: Value },

    // Sampler steps: output bounded but not equation-defined.
    // Cranelift refuses these; only SatisfierFunctionizer consumes them.

    /// Int/Nat/Pos bounded by `[lo, hi]`; drawn by the seeded PRNG.
    SampleRange { var: String, lo: i64, hi: i64 },
    /// Enum with no constraint; drawn from nullary variants of `type_name`.
    SampleEnum  { var: String, type_name: String },
    /// Output drawn from `var ∈ {a, b, c}` candidates.
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

/// Render a `GuardedBody` as either a scalar expr or a `⟨…⟩` seq.
fn fmt_guarded_body(body: &GuardedBody<'_>) -> String {
    match body {
        GuardedBody::Scalar(e) => e.to_string(),
        GuardedBody::Seq(es) => {
            let elems: Vec<String> = es.iter().map(|e| e.to_string()).collect();
            format!("⟨{}⟩", elems.join(", "))
        }
    }
}

impl<'ctx> std::fmt::Display for Z3Step<'ctx> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Z3Step::Scalar { var, expr } => write!(f, "{var} := {expr}"),
            Z3Step::Seq { var, elem_exprs } => {
                let elems: Vec<String> =
                    elem_exprs.iter().map(|e| e.to_string()).collect();
                write!(f, "{var} := ⟨{}⟩", elems.join(", "))
            }
            Z3Step::Guarded { var, branches } => {
                write!(f, "guarded:")?;
                for br in branches {
                    write!(f, "\n  | {} ⇒ {var} := {}",
                        br.guard, fmt_guarded_body(&br.body))?;
                }
                Ok(())
            }
            Z3Step::PreBaked { var, value } => {
                write!(f, "prebaked: {var} := {value:?}")
            }
            Z3Step::SampleRange { var, lo, hi } => {
                write!(f, "sample: {var} ∈ [{lo}, {hi}]")
            }
            Z3Step::SampleEnum { var, type_name } => {
                write!(f, "sample: {var} ∈ enum {type_name}")
            }
            Z3Step::SampleSet { var, candidates } => {
                let cands: Vec<String> =
                    candidates.iter().map(|c| format!("{c:?}")).collect();
                write!(f, "sample: {var} ∈ {{{}}}", cands.join(", "))
            }
        }
    }
}

impl<'ctx> std::fmt::Display for Z3Program<'ctx> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for step in &self.steps {
            writeln!(f, "{step}")?;
        }
        if !self.checks.is_empty() {
            writeln!(f, "checks:")?;
            for (lhs, rhs) in &self.checks {
                writeln!(f, "  {lhs} = {rhs}")?;
            }
        }
        if !self.predicates.is_empty() {
            writeln!(f, "predicates:")?;
            for p in &self.predicates {
                writeln!(f, "  {p}")?;
            }
        }
        Ok(())
    }
}
