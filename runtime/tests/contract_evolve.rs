//! Strategy-2 behavior-contract adapter: the EXISTING runtime, driven in
//! SMT-LIB mode, plugged into the portable `runtime_contract::FsmEngine` matrix.
//!
//! This is the dual of `runtime-smt/tests/contract.rs` (strategy 1, greenfield).
//! Here we drive `evident_runtime::smtlib_fsm::solve_tick` — the seam that lets
//! the existing multi-FSM engine run an FSM whose per-tick constraint is raw
//! SMT-LIB text + metadata, bypassing the Evident parser. We feed each fixture's
//! fully-pinned SMT-LIB (problem ⧺ prev ⧺ inputs — all pins inline) and declare
//! ONLY the scalar outputs (Int/Bool/Real/Str) the v1 subset can read.
//!
//! The documented v1 boundary is enum-typed `state` driven by SMT-LIB
//! `(declare-datatypes …)`: the scalar engine can't read that key, so it simply
//! doesn't produce it. The matrix classifies a missing-but-expected key as a
//! `Gap` (green) — strategy 2's honest entanglement boundary, NOT a failure.
//!
//! Per the contract: the only red verdict is a WRONG answer (a produced scalar
//! that mismatches, or a negative coming back SAT with a forbidden value).

use std::collections::{BTreeMap, HashMap};

use evident_runtime::smtlib_fsm::{
    solve_smtlib_decode_all, solve_tick, DecodeOutcome, FsmMeta, SmtLibFsm, SmtSort, VarDecl,
};
use evident_runtime::{EvidentRuntime, Value};

use runtime_contract::{
    fixtures_dir_from_manifest, load_fixtures, run_matrix, CVal, Fixture, FsmEngine, Outcome,
};

/// Strategy-2 engine: existing runtime, SMT-LIB mode, scalar-output subset.
struct EvolveEngine;

impl FsmEngine for EvolveEngine {
    fn name(&self) -> &str {
        "Existing+SMTLIB (strategy 2)"
    }

    fn tick(&self, fx: &Fixture) -> Outcome {
        // Derive scalar outputs from the golden model (Int/Bool/Real/Str only).
        // Enum/Seq/Set/Composite golden values are unreadable by the v1 scalar
        // path → we skip them, so that key is simply absent → classified as a Gap.
        // Dotted names (`pos.x`) are pipe-quoted in SMT-LIB but the symbol name is
        // the bare `pos.x` — use it verbatim in VarDecl.name (no pipes).
        let mut vars = Vec::new();
        let mut outputs = Vec::new();
        for (k, v) in &fx.meta.expect_model {
            let sort = match v {
                CVal::Int(_) => SmtSort::Int,
                CVal::Bool(_) => SmtSort::Bool,
                CVal::Real(_) => SmtSort::Real,
                CVal::Str(_) => SmtSort::Str,
                _ => continue, // enum/seq/etc -> engine can't read -> skip (Gap)
            };
            vars.push(VarDecl { name: k.clone(), sort });
            outputs.push(k.clone());
        }

        let meta = FsmMeta {
            fsm: fx.meta.fsm_claim.clone(),
            vars,
            outputs,
            effects_var: None,
            last_results_var: None,
            inputs: vec![],
            effects: vec![],
            world_var: None,
            world_next_var: None,
            world_type: None,
        };
        let fsm = SmtLibFsm { meta, smtlib: fx.pinned_smtlib() };

        let rt = EvidentRuntime::new();
        let qr = solve_tick(&rt, &fsm, &[], &HashMap::new());
        if !qr.satisfied {
            // Pinned text was unsatisfiable (the genuine negative witness, or a
            // parse error already logged by solve_tick).
            return Outcome::Unsat;
        }

        let mut model = BTreeMap::new();
        for k in fx
            .meta
            .expect_model
            .keys()
            .chain(fx.meta.expect_forbidden.keys())
        {
            if let Some(v) = qr.bindings.get(k) {
                model.insert(k.clone(), conv(v));
            }
        }
        // v1 doesn't surface dispatched effects here -> None == "not checked".
        Outcome::Sat { model, effects: None }
    }
}

/// Strategy-2 **enum increment** engine: the existing runtime's SMT-LIB path
/// extended to read enum-typed state output + effects directly from the model
/// via the generic raw-z3-sys decoder (`solve_smtlib_decode_all`). This crosses
/// the v1 entanglement boundary documented above WITHOUT the registered
/// `DatatypeSort` — it walks the solved model generically (the same shape the
/// greenfield engine uses). Additive: a new function on the SMT-LIB path; the
/// scalar `solve_tick` / live `effect-run-smtlib` command are untouched.
struct EvolveEnumEngine;

impl FsmEngine for EvolveEnumEngine {
    fn name(&self) -> &str {
        "Existing+SMTLIB enum-increment (strategy 2)"
    }

    fn tick(&self, fx: &Fixture) -> Outcome {
        let rt = EvidentRuntime::new();
        match solve_smtlib_decode_all(rt.z3_context(), &fx.pinned_smtlib()) {
            DecodeOutcome::Unsat => Outcome::Unsat,
            DecodeOutcome::Err(e) => Outcome::Unsupported(format!("z3: {e}")),
            DecodeOutcome::Sat(all) => {
                // State outputs: every checked key, decoded as enum OR scalar.
                let mut model = BTreeMap::new();
                for k in fx
                    .meta
                    .expect_model
                    .keys()
                    .chain(fx.meta.expect_forbidden.keys())
                {
                    if let Some(v) = all.get(k) {
                        model.insert(k.clone(), conv(v));
                    }
                }
                // Effects: surface them only when genuinely encoded in the
                // portable SMT (effects_in_smt) — decode the `effects` Seq const.
                let effects = if fx.meta.effects_in_smt {
                    fx.meta.effects_var.as_ref().map(|ev| match all.get(ev) {
                        Some(Value::SeqEnum(xs)) => xs.iter().map(conv).collect::<Vec<_>>(),
                        _ => Vec::new(),
                    })
                } else {
                    None
                };
                Outcome::Sat { model, effects }
            }
        }
    }
}

/// `evident_runtime::Value` -> engine-neutral `CVal`.
fn conv(v: &Value) -> CVal {
    match v {
        Value::Int(n) => CVal::Int(*n),
        Value::Bool(b) => CVal::Bool(*b),
        Value::Real(r) => CVal::Real(*r),
        Value::Str(s) => CVal::Str(s.clone()),
        Value::SeqInt(xs) => CVal::SeqInt(xs.clone()),
        Value::SeqBool(xs) => CVal::SeqBool(xs.clone()),
        Value::SeqStr(xs) => CVal::SeqStr(xs.clone()),
        Value::SetInt(xs) => CVal::SetInt(xs.clone()),
        Value::SetBool(xs) => CVal::SetBool(xs.clone()),
        Value::SetStr(xs) => CVal::SetStr(xs.clone()),
        Value::Enum { enum_name, variant, fields } => CVal::Enum {
            enum_name: enum_name.clone(),
            variant: variant.clone(),
            fields: fields.iter().map(conv).collect(),
        },
        Value::SeqEnum(xs) => CVal::SeqEnum(xs.iter().map(conv).collect()),
        Value::Composite(m) => {
            CVal::Composite(m.iter().map(|(k, v)| (k.clone(), conv(v))).collect())
        }
        Value::SeqComposite(rows) => CVal::SeqComposite(
            rows.iter()
                .map(|r| r.iter().map(|(k, v)| (k.clone(), conv(v))).collect())
                .collect(),
        ),
    }
}

#[test]
fn evolve_contract_matrix() {
    let fixtures = load_fixtures(&fixtures_dir_from_manifest(env!("CARGO_MANIFEST_DIR")));
    assert!(
        fixtures.len() >= 15,
        "expected ≥15 fixtures, found {}",
        fixtures.len()
    );
    // Two columns: the v1 scalar baseline (enum state = documented Gap) and the
    // enum-increment (reads enum state + effects from the model). The matrix
    // shows the progression across strategy 2's documented entanglement boundary.
    let v1 = EvolveEngine;
    let enum_inc = EvolveEnumEngine;
    let report = run_matrix(&[&v1, &enum_inc], &fixtures);
    eprintln!("\n{}\n", report.to_text());
    assert!(
        !report.any_fail(),
        "strategy-2 engine FAILED (wrong answers, not gaps):\n{:#?}",
        report.failures()
    );
}
