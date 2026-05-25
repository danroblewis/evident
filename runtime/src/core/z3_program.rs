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
    /// Name of the claim this program was extracted from, for
    /// diagnostics (the `EVIDENT_FZ_DUMP_PROGRAM` dump header).
    /// `None` for hand-built / anonymous programs.
    pub label: Option<String>,
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

// ── Pretty-printing ──────────────────────────────────────────────
//
// `Z3Program` sits between the raw simplified Z3 assertions
// (`EVIDENT_FZ_DUMP_BODY`) and the Cranelift CLIF (`EVIDENT_JIT_DUMP`).
// It's already linearised, topologically ordered, and one step per
// output variable — so the Display impl prints it as pseudo-code:
// each step on its own line with `:=`, the embedded Z3 ASTs rendered
// in their native SMT-LIB form via their own `Display`.

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

#[cfg(test)]
mod tests {
    use super::*;
    use z3::ast::{Ast, Bool, Dynamic, Int};
    use z3::{Config, Context};

    fn ctx() -> &'static Context {
        // Leak a Context for a 'static lifetime — same trick the
        // runtime + decompose tests use.
        let cfg = Config::new();
        Box::leak(Box::new(Context::new(&cfg)))
    }

    #[test]
    fn pretty_print_covers_every_step_shape() {
        let ctx = ctx();
        let count = Int::new_const(ctx, "count");
        let one = Int::from_i64(ctx, 1);

        let program = Z3Program {
            steps: vec![
                // Scalar: count_next := count + 1
                Z3Step::Scalar {
                    var: "count_next".to_string(),
                    expr: Dynamic::from_ast(&(count.clone() + one.clone())),
                },
                // Seq: items := ⟨0, 1⟩
                Z3Step::Seq {
                    var: "items".to_string(),
                    elem_exprs: vec![
                        Dynamic::from_ast(&Int::from_i64(ctx, 0)),
                        Dynamic::from_ast(&Int::from_i64(ctx, 1)),
                    ],
                },
                // Guarded: two branches
                Z3Step::Guarded {
                    var: "vy_next".to_string(),
                    branches: vec![
                        GuardedBranch {
                            guard: Dynamic::from_ast(&Bool::new_const(ctx, "grounded")),
                            body: GuardedBody::Scalar(Dynamic::from_ast(&Int::from_i64(ctx, 0))),
                        },
                        GuardedBranch {
                            guard: Dynamic::from_ast(&Bool::new_const(ctx, "airborne")),
                            body: GuardedBody::Scalar(Dynamic::from_ast(&count.clone())),
                        },
                    ],
                },
                // PreBaked: a constant value
                Z3Step::PreBaked {
                    var: "platforms".to_string(),
                    value: Value::Int(42),
                },
            ],
            checks: vec![(
                Dynamic::from_ast(&Int::new_const(ctx, "given_a")),
                Dynamic::from_ast(&Int::new_const(ctx, "given_b")),
            )],
            predicates: vec![count.lt(&Int::from_i64(ctx, 5))],
            label: Some("demo".to_string()),
        };

        let s = program.to_string();

        // Scalar step uses `:=`.
        assert!(s.contains("count_next := "), "missing scalar step:\n{s}");
        // Seq step uses the ⟨…⟩ bracket form.
        assert!(s.contains("items := ⟨"), "missing seq step:\n{s}");
        // Guarded step has a header and indented branches with guards.
        assert!(s.contains("guarded:"), "missing guarded header:\n{s}");
        assert!(s.contains("  | "), "missing guarded branch:\n{s}");
        assert!(s.contains("⇒ vy_next := "), "missing guarded body:\n{s}");
        // PreBaked step prints via Debug.
        assert!(s.contains("prebaked: platforms := "), "missing prebaked:\n{s}");
        // Checks + predicates sections.
        assert!(s.contains("checks:"), "missing checks section:\n{s}");
        assert!(s.contains("predicates:"), "missing predicates section:\n{s}");
    }
}
