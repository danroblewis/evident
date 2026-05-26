//! Snapshot + cross-check for the Evident → SMT-LIB path (north-star step 1).
//!
//! For a corpus that the quantifier-free scalar/string subset covers — including
//! claims pulled from the **real example file** `examples/test_39_string_ops.ev`
//! — this test does two things per case:
//!
//!   1. **Snapshot.** Emit the runnable `.smt2` artifact
//!      (`smtlib::schema_to_smtlib_artifact`, what `evident dump-smtlib` writes)
//!      and diff it against a committed fixture under
//!      `tests/fixtures/smtlib/<stem>.smt2`. Regenerate with
//!      `EVIDENT_UPDATE_SNAPSHOTS=1 cargo test --test smtlib_snapshots`.
//!   2. **Cross-check.** Solve the same claim through the SMT-LIB path
//!      (`smtlib::solve_with_given`: emit → `Z3_solver_from_string` → check) and
//!      assert the sat/unsat result EQUALS the production C-API path
//!      (`EvidentRuntime::query`). This is "Z3 solving real Evident programs via
//!      SMT-LIB, verified equal to the production path."
//!
//! The default translate/query path is untouched; the SMT-LIB path is reached
//! only from here and from `evident dump-smtlib`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use evident_runtime::translate::smtlib;
use evident_runtime::{EvidentRuntime, Value};

/// Where a corpus claim comes from.
enum Src {
    /// Self-contained Evident source loaded via `load_source`.
    Inline(&'static str),
    /// A real file under `examples/`, relative to the repo root.
    Example(&'static str),
}

struct Case {
    /// Snapshot fixture stem → `tests/fixtures/smtlib/<stem>.smt2`.
    stem: &'static str,
    src: Src,
    claim: &'static str,
    /// Pre-bound givens (`--given k=v`), inferred like the CLI does.
    given: &'static [(&'static str, &'static str)],
}

/// The covered corpus. Every case is in-subset by construction; out-of-subset
/// boundaries are pinned in `smtlib_roundtrip.rs`.
fn corpus() -> Vec<Case> {
    vec![
        // ── scalar / arithmetic / logic ─────────────────────────────────
        Case { stem: "scalar_nat_gt", claim: "T", given: &[],
            src: Src::Inline("claim T\n    n ∈ Nat\n    n > 5\n") },
        Case { stem: "scalar_unsat_range", claim: "T", given: &[],
            src: Src::Inline("claim T\n    n ∈ Nat\n    n > 10\n    n < 3\n") },
        Case { stem: "scalar_pair_sum", claim: "T", given: &[],
            src: Src::Inline("claim T\n    a ∈ Nat\n    b ∈ Nat\n    a + b = 10\n    a > 0\n    b > 0\n") },
        Case { stem: "scalar_bool_implies", claim: "T", given: &[],
            src: Src::Inline("claim T\n    p ∈ Bool\n    q ∈ Bool\n    p = true\n    p ⇒ q\n") },
        Case { stem: "scalar_int_div", claim: "T", given: &[],
            src: Src::Inline("claim T\n    q ∈ Int\n    r ∈ Int\n    q = 17\n    r = q / 5\n") },
        Case { stem: "scalar_real_linear", claim: "T", given: &[],
            src: Src::Inline("claim T\n    x ∈ Real\n    x + x = 3.0\n") },
        Case { stem: "scalar_set_membership", claim: "T", given: &[],
            src: Src::Inline("claim T\n    m ∈ Int\n    m ∈ {2, 4, 6}\n    m > 3\n") },
        Case { stem: "scalar_range_membership", claim: "T", given: &[],
            src: Src::Inline("claim T\n    k ∈ Int\n    k ∈ {10..20}\n    k ≠ 15\n") },
        Case { stem: "scalar_ternary", claim: "T", given: &[],
            src: Src::Inline("claim T\n    x ∈ Int\n    y ∈ Int\n    x = 7\n    y = (x > 5 ? 1 : 0)\n") },

        // ── given (pinned input, mirrors `EvidentRuntime::query`) ────────
        Case { stem: "given_pins_unsat", claim: "T", given: &[("n", "3")],
            src: Src::Inline("claim T\n    n ∈ Nat\n    n > 5\n") },
        Case { stem: "given_pins_sat", claim: "T", given: &[("n", "8")],
            src: Src::Inline("claim T\n    n ∈ Nat\n    n > 5\n") },

        // ── string builtins (subset grown this session) ──────────────────
        Case { stem: "string_contains_infix", claim: "S", given: &[],
            src: Src::Inline("claim S\n    s ∈ String = \"world.pos\"\n    \"pos\" ∈ s\n") },
        Case { stem: "string_char_at", claim: "S", given: &[],
            src: Src::Inline("claim S\n    s ∈ String = \"abc\"\n    c ∈ String = char_at(s, 1)\n    c = \"b\"\n") },
        Case { stem: "string_from_int", claim: "S", given: &[],
            src: Src::Inline("claim S\n    s ∈ String = str_from_int(0 - 42)\n    s = \"-42\"\n") },
        Case { stem: "string_prefix_suffix", claim: "S", given: &[],
            src: Src::Inline("claim S\n    s ∈ String = \"world.pos\"\n    starts_with(s, \"world.\")\n    ends_with(s, \".pos\")\n") },

        // ── REAL example-corpus claims (examples/test_39_string_ops.ev) ──
        Case { stem: "ex39_sat_split_head", claim: "sat_split_head", given: &[],
            src: Src::Example("examples/test_39_string_ops.ev") },
        Case { stem: "ex39_sat_substitute", claim: "sat_substitute", given: &[],
            src: Src::Example("examples/test_39_string_ops.ev") },
        Case { stem: "ex39_sat_prefix_and_len", claim: "sat_prefix_and_len", given: &[],
            src: Src::Example("examples/test_39_string_ops.ev") },
        Case { stem: "ex39_unsat_wrong_arg", claim: "unsat_wrong_arg", given: &[],
            src: Src::Example("examples/test_39_string_ops.ev") },
    ]
}

/// Infer a `Value` from a `--given` string the same way the CLI does.
fn infer_value(v: &str) -> Value {
    if v == "true" {
        Value::Bool(true)
    } else if v == "false" {
        Value::Bool(false)
    } else if let Ok(n) = v.parse::<i64>() {
        Value::Int(n)
    } else {
        Value::Str(v.to_string())
    }
}

fn given_map(pairs: &[(&str, &str)]) -> HashMap<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), infer_value(v))).collect()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/smtlib")
}

fn load_case(case: &Case) -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    match &case.src {
        Src::Inline(src) => rt.load_source(src).expect("inline source should load"),
        Src::Example(rel) => {
            let path = repo_root().join(rel);
            rt.load_file(&path)
                .unwrap_or_else(|e| panic!("load {}: {e}", path.display()));
        }
    }
    rt
}

#[test]
fn snapshots_and_cross_check() {
    let update = std::env::var("EVIDENT_UPDATE_SNAPSHOTS").is_ok();
    let dir = fixtures_dir();
    if update {
        std::fs::create_dir_all(&dir).expect("create fixtures dir");
    }

    let mut failures: Vec<String> = Vec::new();

    for case in corpus() {
        let rt = load_case(&case);
        let given = given_map(case.given);

        let schema = rt
            .get_schema(case.claim)
            .unwrap_or_else(|| panic!("[{}] no claim `{}`", case.stem, case.claim));

        // (1) Emit the runnable artifact.
        let artifact = smtlib::schema_to_smtlib_artifact(schema, &given).unwrap_or_else(|e| {
            panic!("[{}] artifact emit failed (should be in subset): {e}", case.stem)
        });

        // (2) Snapshot.
        let path = dir.join(format!("{}.smt2", case.stem));
        if update {
            std::fs::write(&path, &artifact).expect("write snapshot");
        } else {
            let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
                panic!(
                    "[{}] missing snapshot {} — regenerate with \
                     EVIDENT_UPDATE_SNAPSHOTS=1 cargo test --test smtlib_snapshots",
                    case.stem,
                    path.display()
                )
            });
            if expected != artifact {
                failures.push(format!(
                    "[{}] snapshot mismatch for {}:\n--- expected ---\n{}\n--- got ---\n{}",
                    case.stem,
                    path.display(),
                    expected,
                    artifact
                ));
                continue;
            }
        }

        // (3) Cross-check: SMT-LIB-route sat == C-API-route sat.
        let reference = rt
            .query(case.claim, &given)
            .unwrap_or_else(|e| panic!("[{}] C-API query failed: {e}", case.stem));
        let smt = smtlib::solve_with_given(schema, &given)
            .unwrap_or_else(|e| panic!("[{}] SMT-LIB solve failed: {e}", case.stem));
        if smt.satisfied != reference.satisfied {
            failures.push(format!(
                "[{}] SAT MISMATCH: C-API={} SMT-LIB={}\n{}",
                case.stem, reference.satisfied, smt.satisfied, smt.smtlib
            ));
        }
    }

    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

/// Model parity for the example-file string claim: both routes force
/// `head = "Edge"`, and both must report it. (Detailed model checks across the
/// whole subset live in `smtlib_roundtrip.rs`.)
#[test]
fn string_model_parity() {
    let mut rt = EvidentRuntime::new();
    let path = repo_root().join("examples/test_39_string_ops.ev");
    rt.load_file(&path).unwrap();

    let schema = rt.get_schema("sat_split_head").unwrap();
    let empty = HashMap::new();
    let smt = smtlib::solve(schema).unwrap();
    let capi = rt.query("sat_split_head", &empty).unwrap();
    assert!(smt.satisfied && capi.satisfied);
    assert_eq!(smt.bindings.get("head"), Some(&Value::Str("Edge".into())));
    assert_eq!(capi.bindings.get("head"), Some(&Value::Str("Edge".into())));
}
