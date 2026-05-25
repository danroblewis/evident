//! SatisfierFunctionizer — sample partially-constrained variables.
//!
//! The JIT counterpart to what Z3 does when it picks a satisfying
//! assignment for an *unbound but bounded* variable. Where the
//! Cranelift functionizer requires every output to be defined by an
//! equation (`x = expr`), the satisfier additionally handles outputs
//! whose only constraint is a finite domain:
//!
//!   * `lo ≤ x ≤ hi`  (scalar `Int`/`Nat`/`Pos`)  → `SampleRange`
//!   * `c ∈ EnumType` (no other constraint)        → `SampleEnum`
//!   * `x ∈ {a, b, c}` (concrete finite set)        → `SampleSet`
//!
//! The architectural frame is **probabilistic programming**: an
//! unbound-but-bounded variable is a distribution; a query draws one
//! satisfying sample. The draw is a seeded SplitMix64 PRNG (≈ 5 native
//! instructions) rather than a full Z3 solve cycle (≈ ms).
//!
//! ## Reuse, not reimplementation
//!
//! The satisfier does NOT reimplement integer arithmetic. At compile
//! time it partitions the program's steps:
//!   * `Sample*` steps → recorded as `Sampler`s, stripped out.
//!   * everything else → handed to a real `CraneliftFunctionizer`.
//! At call time it draws the sampled values, injects them into a clone
//! of `given` (where the inner Cranelift function reads them by name as
//! ordinary inputs — see `cranelift::JitProgram::call`), runs the inner
//! function, then merges the sampled values back into the result.
//!
//! A program with NO sampler steps is delegated to Cranelift verbatim,
//! so the satisfier is a strict superset of Cranelift: enabling it
//! never regresses the deterministic path.
//!
//! ## Determinism (non-negotiable)
//!
//! The value cache keys on `(claim, given-keys, given-values)`. If the
//! sampler weren't deterministic, repeated queries would return
//! inconsistent assignments and the cache would be poisoned. The seed
//! is derived from `(EVIDENT_DISPATCH_SEED, program-identity salt,
//! given-values hash)`, so the same inputs always draw the same sample.

use std::collections::HashMap;
use std::rc::Rc;

use crate::core::{DatatypeRegistry, EnumRegistry, Value, Z3Program, Z3Step};
use crate::functionize::cranelift::CraneliftFunctionizer;

/// A single sampler-shaped output, resolved from a `Z3Step::Sample*`.
enum Sampler {
    /// Draw an integer uniformly from the inclusive range `[lo, hi]`.
    Range { var: String, lo: i64, hi: i64 },
    /// Draw one of an enum's nullary variants.
    Enum  { var: String, type_name: String, variants: Vec<String> },
    /// Draw one of a finite list of candidate values.
    Set   { var: String, candidates: Vec<Value> },
}

impl Sampler {
    fn var(&self) -> &str {
        match self {
            Sampler::Range { var, .. }
            | Sampler::Enum { var, .. }
            | Sampler::Set { var, .. } => var,
        }
    }
}

/// A `Functionizer` that compiles `Sample`-augmented `Z3Program`s by
/// drawing the sampled variables and delegating the deterministic
/// remainder to Cranelift. See the module docs.
pub struct SatisfierFunctionizer {
    /// Base seed, from `EVIDENT_DISPATCH_SEED` (default fixed). Folded
    /// with the program salt + given-values hash per call.
    base_seed: u64,
}

impl SatisfierFunctionizer {
    pub fn new() -> Self {
        let base_seed = std::env::var("EVIDENT_DISPATCH_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0x5A71_5F1E_D000_u64);
        SatisfierFunctionizer { base_seed }
    }
}

impl Default for SatisfierFunctionizer {
    fn default() -> Self { Self::new() }
}

impl super::Functionizer for SatisfierFunctionizer {
    fn name(&self) -> &'static str { "satisfier" }

    fn compile(
        &self,
        program:   &Z3Program,
        enums:     &EnumRegistry,
        datatypes: &DatatypeRegistry,
    ) -> Option<Rc<dyn super::CompiledFunction>> {
        let trace = std::env::var("EVIDENT_FUNCTIONIZE_TRACE").is_ok();

        // ── Partition steps into samplers + the computed remainder ──
        let mut samplers: Vec<Sampler> = Vec::new();
        let mut stripped_steps: Vec<Z3Step> = Vec::new();
        for step in &program.steps {
            match step {
                Z3Step::SampleRange { var, lo, hi } => {
                    if hi < lo { return None; }   // empty range — unsatisfiable
                    samplers.push(Sampler::Range { var: var.clone(), lo: *lo, hi: *hi });
                }
                Z3Step::SampleEnum { var, type_name } => {
                    let by_name = enums.by_name.borrow();
                    let (_, variants) = by_name.get(type_name)?;
                    if variants.is_empty() { return None; }
                    // v1: nullary variants only — sampling a payload-bearing
                    // variant would need to also sample its fields (deferred).
                    if variants.iter().any(|v| !v.fields.is_empty()) {
                        if trace {
                            eprintln!("[satisfier] bail: enum {type_name} has \
                                      payload-bearing variants (deferred)");
                        }
                        return None;
                    }
                    let names: Vec<String> = variants.iter().map(|v| v.name.clone()).collect();
                    samplers.push(Sampler::Enum {
                        var: var.clone(), type_name: type_name.clone(), variants: names,
                    });
                }
                Z3Step::SampleSet { var, candidates } => {
                    if candidates.is_empty() { return None; }
                    samplers.push(Sampler::Set {
                        var: var.clone(), candidates: candidates.clone(),
                    });
                }
                other => stripped_steps.push(other.clone()),
            }
        }

        // No sampler steps → behave exactly like Cranelift. This makes
        // the satisfier a strict superset: every program Cranelift
        // compiles, we compile identically by delegation.
        if samplers.is_empty() {
            return CraneliftFunctionizer.compile(program, enums, datatypes);
        }

        // Residual checks/predicates would be silently ignored by the
        // Cranelift delegate, and we don't validate them here. The
        // extractor (`recover_samplers`) already removed the bounds it
        // turned into Sample steps, so a non-empty residue is a *real*
        // extra constraint (a relation on a derived var, a free
        // inequality, …) — refuse to the slow Z3 solve, which validates
        // everything. (Mirrors `symbolic.rs`'s conservative refusal.)
        if !program.checks.is_empty() || !program.predicates.is_empty() {
            if trace {
                eprintln!("[satisfier] bail: {} checks + {} predicates remain after sampling",
                    program.checks.len(), program.predicates.len());
            }
            return None;
        }

        // Deterministic draw order, independent of step/HashMap order.
        samplers.sort_by(|a, b| a.var().cmp(b.var()));

        // Delegate the computed remainder to Cranelift. The sampled
        // vars appear there as referenced-but-undefined names → inputs,
        // read by name from the augmented `given` at call time.
        let inner: Option<Rc<dyn super::CompiledFunction>> = if stripped_steps.is_empty() {
            None
        } else {
            let stripped = Z3Program {
                steps: stripped_steps,
                checks: Vec::new(),
                predicates: Vec::new(),
            };
            match CraneliftFunctionizer.compile(&stripped, enums, datatypes) {
                Some(c) => Some(c),
                None => {
                    if trace {
                        eprintln!("[satisfier] bail: Cranelift refused the computed remainder");
                    }
                    return None;
                }
            }
        };

        let salt = program_salt(&samplers);
        if trace {
            eprintln!("[satisfier] compiled: {} sampler(s), {} computed step(s), salt={salt:#x}",
                samplers.len(), inner.is_some() as usize);
        }
        Some(Rc::new(SatisfierFn { inner, samplers, salt, base_seed: self.base_seed }))
    }
}

/// The compiled artifact: the inner Cranelift function (for the
/// computed remainder) plus the resolved samplers.
struct SatisfierFn {
    inner: Option<Rc<dyn super::CompiledFunction>>,
    samplers: Vec<Sampler>,
    salt: u64,
    base_seed: u64,
}

impl super::CompiledFunction for SatisfierFn {
    fn call(&self, given: &HashMap<String, Value>) -> Option<HashMap<String, Value>> {
        // Seed deterministically from (base seed, program salt, given).
        let mut state = seed_state(given, self.salt, self.base_seed);

        // Draw each sampler, in the fixed (sorted) order.
        let mut augmented = given.clone();
        let mut sampled: Vec<(String, Value)> = Vec::with_capacity(self.samplers.len());
        for sm in &self.samplers {
            let v = match sm {
                Sampler::Range { lo, hi, .. } => {
                    // Inclusive span, computed in i128 to avoid overflow
                    // on wide ranges; `lo ≤ hi` is enforced at compile.
                    let span = (*hi as i128) - (*lo as i128) + 1;
                    let off = (splitmix64(&mut state) as u128 % span as u128) as i128;
                    Value::Int((*lo as i128 + off) as i64)
                }
                Sampler::Enum { type_name, variants, .. } => {
                    let i = (splitmix64(&mut state) % variants.len() as u64) as usize;
                    Value::Enum {
                        enum_name: type_name.clone(),
                        variant: variants[i].clone(),
                        fields: Vec::new(),
                    }
                }
                Sampler::Set { candidates, .. } => {
                    let i = (splitmix64(&mut state) % candidates.len() as u64) as usize;
                    candidates[i].clone()
                }
            };
            augmented.insert(sm.var().to_string(), v.clone());
            sampled.push((sm.var().to_string(), v));
        }

        // Compute the remainder from the augmented inputs.
        let mut out = match &self.inner {
            Some(inner) => inner.call(&augmented)?,
            None => HashMap::new(),
        };
        // The inner function treats sampled vars as inputs and doesn't
        // return them — surface them as part of the assignment.
        for (k, v) in sampled {
            out.insert(k, v);
        }
        Some(out)
    }
}

// ── Deterministic PRNG (SplitMix64) ──────────────────────────────

/// One SplitMix64 step: advances `state` and returns a well-mixed
/// 64-bit value. Deterministic, no dependencies, bit-stable across
/// platforms.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// FNV-1a over bytes — a stable, dependency-free string hash.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Per-program salt: a stable hash of the sampler shapes (names +
/// bounds / type / arity). Ensures two different sampler programs draw
/// different sequences even for an identical `given`.
fn program_salt(samplers: &[Sampler]) -> u64 {
    let mut s = String::new();
    for sm in samplers {
        match sm {
            Sampler::Range { var, lo, hi } =>
                s.push_str(&format!("R|{var}|{lo}|{hi};")),
            Sampler::Enum { var, type_name, variants } =>
                s.push_str(&format!("E|{var}|{type_name}|{};", variants.len())),
            Sampler::Set { var, candidates } =>
                s.push_str(&format!("S|{var}|{};", candidates.len())),
        }
    }
    fnv1a64(s.as_bytes())
}

/// Deterministic initial PRNG state from `(base seed, program salt,
/// given values)`. The given map is folded in a key-sorted, stable
/// order so iteration order can't perturb the result.
fn seed_state(given: &HashMap<String, Value>, salt: u64, base: u64) -> u64 {
    let mut pairs: Vec<(&String, String)> = given.iter()
        .map(|(k, v)| (k, format!("{v:?}")))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let mut buf = String::new();
    for (k, v) in pairs {
        buf.push_str(k);
        buf.push('=');
        buf.push_str(&v);
        buf.push(';');
    }
    let gh = fnv1a64(buf.as_bytes());
    base ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ gh.rotate_left(32)
}
