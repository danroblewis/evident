//! SatisfierFunctionizer: draws bounded-but-unconstrained variables via seeded SplitMix64 PRNG;
//! delegates the computed remainder to Cranelift. Seed = `EVIDENT_DISPATCH_SEED` + program salt + given.

use std::collections::HashMap;
use std::rc::Rc;

use crate::core::{DatatypeRegistry, EnumRegistry, Value, Z3Program, Z3Step};
use crate::functionize::cranelift::CraneliftFunctionizer;

enum Sampler {
    Range { var: String, lo: i64, hi: i64 },
    Enum  { var: String, type_name: String, variants: Vec<String> },
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

/// Compiles `Sample`-augmented `Z3Program`s by drawing samplers, delegating the rest to Cranelift.
pub struct SatisfierFunctionizer {
    base_seed: u64,  // from EVIDENT_DISPATCH_SEED; folded with program salt + given-values per call
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
                    // Payload-bearing variants deferred: sampling fields needs extra steps.
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

        // No sampler steps → delegate entirely to Cranelift (strict superset).
        if samplers.is_empty() {
            return CraneliftFunctionizer.compile(program, enums, datatypes);
        }

        // Residual checks/predicates indicate a real extra constraint; refuse to the Z3 slow path.
        if !program.checks.is_empty() || !program.predicates.is_empty() {
            if trace {
                eprintln!("[satisfier] bail: {} checks + {} predicates remain after sampling",
                    program.checks.len(), program.predicates.len());
            }
            return None;
        }

        samplers.sort_by(|a, b| a.var().cmp(b.var())); // deterministic draw order

        // Sampled vars appear in Cranelift as free inputs, read from the augmented `given`.
        let inner: Option<Rc<dyn super::CompiledFunction>> = if stripped_steps.is_empty() {
            None
        } else {
            let stripped = Z3Program {
                steps: stripped_steps,
                checks: Vec::new(),
                predicates: Vec::new(),
                label: program.label.clone(),
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

struct SatisfierFn {
    inner: Option<Rc<dyn super::CompiledFunction>>,
    samplers: Vec<Sampler>,
    salt: u64,
    base_seed: u64,
}

impl super::CompiledFunction for SatisfierFn {
    fn call(&self, given: &HashMap<String, Value>) -> Option<HashMap<String, Value>> {
        let mut state = seed_state(given, self.salt, self.base_seed);
        let mut augmented = given.clone();
        let mut sampled: Vec<(String, Value)> = Vec::with_capacity(self.samplers.len());
        for sm in &self.samplers {
            let v = match sm {
                Sampler::Range { lo, hi, .. } => {
                    // i128 to avoid overflow on wide ranges; lo ≤ hi enforced at compile.
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

        let mut out = match &self.inner {
            Some(inner) => inner.call(&augmented)?,
            None => HashMap::new(),
        };
        // Inner function treats sampled vars as inputs and doesn't return them; add them back.
        for (k, v) in sampled {
            out.insert(k, v);
        }
        Some(out)
    }
}

/// SplitMix64: advance `state` and return a well-mixed 64-bit value.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Stable hash of sampler shapes so two programs with the same `given` draw different sequences.
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

/// Initial PRNG state from (base seed, program salt, given values); key-sorted for stability.
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
