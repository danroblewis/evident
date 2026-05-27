//! Behavior-contract fixture runner.
//!
//! Proves the portable fixtures under `runtime-contract/fixtures/` capture the
//! current runtime's per-tick behavior, via a pluggable [`FsmEngine`] trait.
//!
//! Two engines run over every fixture:
//!   * [`CurrentRuntimeEngine`] — loads each fixture's `source.ev` into a real
//!     `EvidentRuntime` and drives the actual tick primitive
//!     (`query_with_pins_and_given` + the `effects` binding). Diffing its output
//!     against the golden proves the capture is *current behavior*. THIS is the
//!     plan's Phase-4 gate.
//!   * [`SmtLibEngine`] — feeds the portable `problem.smt2 ⧺ prev.smt2 ⧺
//!     inputs.smt2` to Z3 via `from_string` (no Evident pipeline at all) and
//!     checks the golden model is admissible (Method A) and unique (Method B),
//!     or UNSAT for negative fixtures. Diffing its output against the same golden
//!     proves the SMT-LIB capture is implementation-agnostic and faithful.
//!
//! Both engines passing means `problem.smt2` is a faithful, engine-neutral
//! capture of what the current runtime does on one tick.
//!
//! Format spec: `runtime-contract/FORMAT.md`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use evident_runtime::{EvidentRuntime, Value};
use serde_json::Value as Json;

// ===========================================================================
// Fixture model
// ===========================================================================

/// One captured tick: metadata + the directory holding its SMT-LIB + source.
struct Fixture {
    name: String,
    dir: PathBuf,
    meta: Meta,
}

/// Parsed `meta.json` (FORMAT.md §2).
struct Meta {
    fsm_claim: String,
    /// Repo-relative path to the FSM source; resolved against the repo root.
    source_ev: String,
    effects_var: Option<String>,
    /// Input pins: var → typed value (FORMAT.md §3). Includes the enum `state`.
    given: HashMap<String, Value>,
    /// True → a negative fixture: the over-constrained SMT relation is UNSAT,
    /// AND the runtime's forced output differs from `expect_forbidden`.
    expect_unsat: bool,
    /// Negative fixtures: var → the IMPOSSIBLE value. The SMT capture over-pins
    /// these (→ UNSAT); the runtime forces a value that must DIFFER from these.
    /// (Pinning an output as a `given` can't trigger UNSAT under the functionizer
    /// fast-path — given values are taken as ground truth — so the runtime witness
    /// for a negative is "the forced output ≠ the forbidden one".)
    expect_forbidden: HashMap<String, Value>,
    /// Subset of bindings to check (var → golden value).
    expect_model: HashMap<String, Value>,
    /// Golden dispatched effects, in order (each an `Effect` enum value).
    expect_effects: Vec<Value>,
}

/// What an engine produced for one fixture. Compared structurally against the golden.
#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    /// Solve succeeded. `model` covers (at least) the keys in `expect_model`;
    /// `effects` is the ordered dispatched list.
    Sat {
        model: HashMap<String, Value>,
        effects: Vec<Value>,
    },
    Unsat,
    /// The engine legitimately cannot run this fixture (documented boundary).
    Unsupported(String),
}

/// A pluggable FSM-tick engine. A new runtime plugs in by implementing this and
/// passing the suite.
trait FsmEngine {
    fn name(&self) -> &str;
    /// Run one tick of `fx` and report what it produced.
    fn tick(&self, fx: &Fixture) -> Outcome;
}

// ===========================================================================
// Paths + loading
// ===========================================================================

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/runtime
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("runtime/ has a parent")
        .to_path_buf()
}

fn fixtures_dir() -> PathBuf {
    repo_root().join("runtime-contract/fixtures")
}

fn load_fixtures() -> Vec<Fixture> {
    let dir = fixtures_dir();
    let mut out = Vec::new();
    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read fixtures dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();
    for d in entries {
        let meta_path = d.join("meta.json");
        if !meta_path.exists() {
            continue;
        }
        let name = d.file_name().unwrap().to_string_lossy().into_owned();
        let meta = parse_meta(&meta_path, &name);
        out.push(Fixture { name, dir: d, meta });
    }
    out
}

fn parse_meta(path: &Path, name: &str) -> Meta {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{name}: read meta.json: {e}"));
    let j: Json = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: meta.json parse: {e}"));

    let fsm_claim = j["fsm_claim"].as_str().unwrap_or_else(|| panic!("{name}: fsm_claim")).to_string();
    let source_ev = j["source_ev"].as_str().unwrap_or_else(|| panic!("{name}: source_ev")).to_string();
    let effects_var = j["effects_var"].as_str().map(|s| s.to_string());

    let given = match &j["given"] {
        Json::Object(m) => m.iter().map(|(k, v)| (k.clone(), json_to_value(v, name))).collect(),
        _ => HashMap::new(),
    };

    let expect = &j["expect"];
    let expect_unsat = expect.get("unsat").and_then(|b| b.as_bool()).unwrap_or(false);
    let expect_forbidden = match expect.get("forbidden") {
        Some(Json::Object(m)) => m.iter().map(|(k, v)| (k.clone(), json_to_value(v, name))).collect(),
        _ => HashMap::new(),
    };
    let expect_model = match expect.get("model") {
        Some(Json::Object(m)) => m.iter().map(|(k, v)| (k.clone(), json_to_value(v, name))).collect(),
        _ => HashMap::new(),
    };
    let expect_effects = match expect.get("effects") {
        Some(Json::Array(a)) => a.iter().map(|v| json_to_value(v, name)).collect(),
        _ => Vec::new(),
    };

    Meta { fsm_claim, source_ev, effects_var, given, expect_unsat, expect_forbidden, expect_model, expect_effects }
}

/// Tagged-JSON → `Value` (FORMAT.md §3). Panics on an unknown/malformed tag so a
/// bad fixture fails loudly rather than silently mis-decoding.
fn json_to_value(j: &Json, ctx: &str) -> Value {
    let obj = j.as_object().unwrap_or_else(|| panic!("{ctx}: value must be a tagged object, got {j}"));
    let int = |v: &Json| v.as_i64().unwrap_or_else(|| panic!("{ctx}: expected int, got {v}"));
    if let Some(v) = obj.get("int") {
        return Value::Int(int(v));
    }
    if let Some(v) = obj.get("bool") {
        return Value::Bool(v.as_bool().unwrap_or_else(|| panic!("{ctx}: bool")));
    }
    if let Some(v) = obj.get("real") {
        return Value::Real(v.as_f64().unwrap_or_else(|| panic!("{ctx}: real")));
    }
    if let Some(v) = obj.get("str") {
        return Value::Str(v.as_str().unwrap_or_else(|| panic!("{ctx}: str")).to_string());
    }
    if let Some(Json::Array(a)) = obj.get("seq_int") {
        return Value::SeqInt(a.iter().map(int).collect());
    }
    if let Some(Json::Array(a)) = obj.get("seq_bool") {
        return Value::SeqBool(a.iter().map(|v| v.as_bool().unwrap()).collect());
    }
    if let Some(Json::Array(a)) = obj.get("seq_str") {
        return Value::SeqStr(a.iter().map(|v| v.as_str().unwrap().to_string()).collect());
    }
    if let Some(Json::Array(a)) = obj.get("set_int") {
        return Value::SetInt(a.iter().map(int).collect());
    }
    if let Some(Json::Array(a)) = obj.get("set_bool") {
        return Value::SetBool(a.iter().map(|v| v.as_bool().unwrap()).collect());
    }
    if let Some(Json::Array(a)) = obj.get("set_str") {
        return Value::SetStr(a.iter().map(|v| v.as_str().unwrap().to_string()).collect());
    }
    if obj.contains_key("seq_enum") {
        let elems = match obj.get("elems") {
            Some(Json::Array(a)) => a.iter().map(|v| json_to_value(v, ctx)).collect(),
            _ => Vec::new(),
        };
        return Value::SeqEnum(elems);
    }
    if let Some(name) = obj.get("enum").and_then(|v| v.as_str()) {
        let variant = obj.get("variant").and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("{ctx}: enum missing variant")).to_string();
        let fields = match obj.get("fields") {
            Some(Json::Array(a)) => a.iter().map(|v| json_to_value(v, ctx)).collect(),
            _ => Vec::new(),
        };
        return Value::Enum { enum_name: name.to_string(), variant, fields };
    }
    if let Some(Json::Object(m)) = obj.get("composite") {
        return Value::Composite(m.iter().map(|(k, v)| (k.clone(), json_to_value(v, ctx))).collect());
    }
    if let Some(Json::Array(a)) = obj.get("seq_composite") {
        let v = a.iter().map(|el| {
            el.as_object().unwrap().iter().map(|(k, v)| (k.clone(), json_to_value(v, ctx))).collect()
        }).collect();
        return Value::SeqComposite(v);
    }
    panic!("{ctx}: unrecognized tagged value: {j}");
}

// ===========================================================================
// Engine 1: CurrentRuntimeEngine — runs the real runtime tick
// ===========================================================================

struct CurrentRuntimeEngine;

impl FsmEngine for CurrentRuntimeEngine {
    fn name(&self) -> &str {
        "CurrentRuntimeEngine"
    }

    fn tick(&self, fx: &Fixture) -> Outcome {
        let mut rt = EvidentRuntime::new();
        let src = fx.dir.join("source.ev");
        // load_file resolves `import "stdlib/runtime.ev"` by walking ancestors of
        // the source file up to the repo root (where stdlib/ lives).
        if let Err(e) = rt.load_file(&src) {
            return Outcome::Unsupported(format!("load {}: {e}", src.display()));
        }
        // Empty pins: the enum `state` rides in `given` as a Value::Enum (the
        // functionizer + slow path both re-encode it; pins are redundant — see
        // scheduler_api.rs). This IS the scheduler's per-tick call.
        let r = match rt.query_with_pins_and_given(&fx.meta.fsm_claim, &[], &fx.meta.given) {
            Ok(r) => r,
            Err(e) => return Outcome::Unsupported(format!("query {}: {e}", fx.meta.fsm_claim)),
        };
        if !r.satisfied {
            return Outcome::Unsat;
        }
        let mut model = HashMap::new();
        for k in fx.meta.expect_model.keys().chain(fx.meta.expect_forbidden.keys()) {
            if let Some(v) = r.bindings.get(k) {
                model.insert(k.clone(), v.clone());
            }
        }
        let effects = match &fx.meta.effects_var {
            Some(ev) => match r.bindings.get(ev) {
                Some(Value::SeqEnum(es)) => es.clone(),
                _ => Vec::new(),
            },
            None => Vec::new(),
        };
        Outcome::Sat { model, effects }
    }
}

// ===========================================================================
// Engine 2: SmtLibEngine — solves the portable SMT-LIB capture via Z3
// ===========================================================================

struct SmtLibEngine;

impl FsmEngine for SmtLibEngine {
    fn name(&self) -> &str {
        "SmtLibEngine"
    }

    fn tick(&self, fx: &Fixture) -> Outcome {
        let read = |f: &str| fs::read_to_string(fx.dir.join(f)).unwrap_or_default();
        let problem = read("problem.smt2");
        let prev = read("prev.smt2");
        let inputs = read("inputs.smt2");
        let base = format!("{problem}\n{prev}\n{inputs}\n");

        if fx.meta.expect_unsat {
            return match smt_check(&base) {
                SmtResult::Unsat => Outcome::Unsat,
                SmtResult::Sat => Outcome::Unsupported("expected unsat, z3 said sat".into()),
                SmtResult::Err(e) => Outcome::Unsupported(format!("z3 parse/solve: {e}")),
            };
        }

        let model_smt = read("expected_model.smt2");
        let eqs = top_level_assert_bodies(&model_smt);

        // Method A — golden model is admissible: base ⧺ expected_model is SAT.
        let method_a = format!("{base}\n{model_smt}\n");
        match smt_check(&method_a) {
            SmtResult::Sat => {}
            SmtResult::Unsat => {
                return Outcome::Unsupported("Method A failed: golden model not admissible (unsat)".into())
            }
            SmtResult::Err(e) => return Outcome::Unsupported(format!("Method A z3: {e}")),
        }

        // Method B — golden model is unique: base ⧺ ¬(conjunction of model eqs) is UNSAT.
        if !eqs.is_empty() {
            let negated = if eqs.len() == 1 {
                format!("(assert (not {}))", eqs[0])
            } else {
                format!("(assert (not (and {})))", eqs.join(" "))
            };
            let method_b = format!("{base}\n{negated}\n");
            match smt_check(&method_b) {
                SmtResult::Unsat => {}
                SmtResult::Sat => {
                    return Outcome::Unsupported(
                        "Method B failed: golden model not unique (sat under negation)".into())
                }
                SmtResult::Err(e) => return Outcome::Unsupported(format!("Method B z3: {e}")),
            }
        }

        // Methods A+B proved the golden is THE model the SMT relation forces.
        // Surface it through the trait so the runner diffs it like any engine.
        // (Effects only appear in the model when effects_in_smt; the runner skips
        // effect-diffing for this engine — the typed effect golden is checked
        // against the runtime engine.)
        Outcome::Sat {
            model: fx.meta.expect_model.clone(),
            effects: fx.meta.expect_effects.clone(),
        }
    }
}

enum SmtResult {
    Sat,
    Unsat,
    Err(String),
}

// z3::Context is a single-pointer newtype; reach the raw ptr to read the parser
// error state (the z3 crate's from_string swallows parse errors). Same guarded
// pattern as runtime/src/translate/smtlib.rs.
const _: () = {
    assert!(
        std::mem::size_of::<z3::Context>() == std::mem::size_of::<z3_sys::Z3_context>(),
        "z3::Context is no longer a single-pointer newtype"
    );
};

fn smt_check(text: &str) -> SmtResult {
    use z3::{Config, Context, SatResult, Solver};
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let solver = Solver::new(&ctx);
    solver.from_string(text.to_string());
    // Detect a parser rejection the crate swallowed.
    let raw = unsafe { *(&ctx as *const Context as *const z3_sys::Z3_context) };
    let code = unsafe { z3_sys::Z3_get_error_code(raw) };
    if code != z3_sys::ErrorCode::OK {
        let msg = unsafe {
            let p = z3_sys::Z3_get_error_msg(raw, code);
            std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
        };
        return SmtResult::Err(format!("{code:?}: {msg}"));
    }
    match solver.check() {
        SatResult::Sat => SmtResult::Sat,
        SatResult::Unsat => SmtResult::Unsat,
        SatResult::Unknown => SmtResult::Err("z3 returned unknown".into()),
    }
}

/// Extract the body `E` of each top-level `(assert E)` form in an SMT-LIB text,
/// skipping `;`-comments (quote-aware). Returns the inner expressions so the
/// caller can build a negated conjunction for the uniqueness check.
fn top_level_assert_bodies(text: &str) -> Vec<String> {
    // Strip comments: a `;` outside a double-quoted string begins a comment.
    let mut stripped = String::with_capacity(text.len());
    for line in text.lines() {
        let mut in_str = false;
        let mut cut = line.len();
        for (i, c) in line.char_indices() {
            match c {
                '"' => in_str = !in_str,
                ';' if !in_str => {
                    cut = i;
                    break;
                }
                _ => {}
            }
        }
        stripped.push_str(&line[..cut]);
        stripped.push('\n');
    }

    // Scan balanced top-level forms; for each `(assert E)` collect `E`.
    let mut forms = Vec::new();
    let bytes: Vec<char> = stripped.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '(' {
            let start = i;
            let mut depth = 0;
            let mut in_str = false;
            while i < bytes.len() {
                match bytes[i] {
                    '"' => in_str = !in_str,
                    '(' if !in_str => depth += 1,
                    ')' if !in_str => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            let form: String = bytes[start..i].iter().collect();
            let trimmed = form.trim();
            if let Some(rest) = trimmed.strip_prefix("(assert") {
                // rest = " E)" — drop the leading ws and the final ')'.
                let inner = rest.trim_start();
                let inner = inner.strip_suffix(')').unwrap_or(inner);
                forms.push(inner.trim().to_string());
            }
        } else {
            i += 1;
        }
    }
    forms
}

// ===========================================================================
// Golden comparison + reporting
// ===========================================================================

/// Diff an engine's outcome against a fixture's golden. `check_effects` is false
/// for engines (SmtLibEngine) that don't surface the dispatched effect list.
fn diff(fx: &Fixture, outcome: &Outcome, check_effects: bool) -> Result<(), String> {
    if fx.meta.expect_unsat {
        // A negative fixture. Two faithful witnesses of "transition impossible":
        //   * SMT engine over-pins the forbidden output → the relation is UNSAT.
        //   * runtime engine forces an output that DIFFERS from the forbidden one
        //     (it can't reproduce the UNSAT directly — see expect_forbidden docs).
        return match outcome {
            Outcome::Unsat => Ok(()),
            Outcome::Sat { model, .. } => {
                for (k, forbidden) in &fx.meta.expect_forbidden {
                    match model.get(k) {
                        Some(got) if got != forbidden => {} // forced ≠ forbidden ✓
                        Some(got) => return Err(format!(
                            "forbidden transition occurred: {k} = {got:?} (== forbidden {forbidden:?})")),
                        None => return Err(format!("forbidden key `{k}` absent from model")),
                    }
                }
                if fx.meta.expect_forbidden.is_empty() {
                    return Err("negative fixture has no `forbidden` block and engine returned Sat".into());
                }
                Ok(())
            }
            other => Err(format!("expected UNSAT or forced-output-differs, got {other:?}")),
        };
    }
    let (model, effects) = match outcome {
        Outcome::Sat { model, effects } => (model, effects),
        other => return Err(format!("expected SAT, got {other:?}")),
    };
    for (k, want) in &fx.meta.expect_model {
        match model.get(k) {
            Some(got) if got == want => {}
            got => return Err(format!("model[{k}]: got {got:?}, want {want:?}")),
        }
    }
    if check_effects && effects != &fx.meta.expect_effects {
        return Err(format!(
            "effects: got {}, want {}",
            fmt_effects(effects),
            fmt_effects(&fx.meta.expect_effects)
        ));
    }
    Ok(())
}

/// Render an effect list to the `expected_effects.txt` grammar (FORMAT.md §6).
fn fmt_effects(effects: &[Value]) -> String {
    let lines: Vec<String> = effects.iter().map(fmt_effect).collect();
    format!("[{}]", lines.join(", "))
}

fn fmt_effect(v: &Value) -> String {
    match v {
        Value::Enum { variant, fields, .. } => {
            if fields.is_empty() {
                variant.clone()
            } else {
                let args: Vec<String> = fields.iter().map(fmt_arg).collect();
                format!("{variant}({})", args.join(", "))
            }
        }
        other => format!("{other:?}"),
    }
}

fn fmt_arg(v: &Value) -> String {
    match v {
        Value::Str(s) => format!("{s:?}"), // Rust debug = backslash-escaped quotes
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Real(r) => r.to_string(),
        other => format!("{other:?}"),
    }
}

/// Run one engine over all fixtures; return (passes, Vec<(fixture, error)>).
fn run_engine(engine: &dyn FsmEngine, fixtures: &[Fixture], check_effects: bool)
    -> (usize, Vec<(String, String)>)
{
    let mut pass = 0;
    let mut failures = Vec::new();
    for fx in fixtures {
        let outcome = engine.tick(fx);
        match diff(fx, &outcome, check_effects) {
            Ok(()) => {
                pass += 1;
                eprintln!("  [{}] {} ✓", engine.name(), fx.name);
            }
            Err(e) => {
                eprintln!("  [{}] {} ✗ — {e}", engine.name(), fx.name);
                failures.push((fx.name.clone(), e));
            }
        }
    }
    (pass, failures)
}

// ===========================================================================
// Tests
// ===========================================================================

#[test]
fn fixtures_discovered() {
    let fixtures = load_fixtures();
    assert!(
        fixtures.len() >= 6,
        "expected ≥6 fixtures (the plan gate), found {}",
        fixtures.len()
    );
    eprintln!("discovered {} fixtures", fixtures.len());
}

/// THE PHASE-4 GATE: the current runtime reproduces every fixture's golden.
#[test]
fn current_runtime_engine_matches_all_goldens() {
    let fixtures = load_fixtures();
    assert!(!fixtures.is_empty(), "no fixtures found");
    let engine = CurrentRuntimeEngine;
    let (pass, failures) = run_engine(&engine, &fixtures, /*check_effects=*/ true);
    eprintln!("CurrentRuntimeEngine: {pass}/{} passed", fixtures.len());
    assert!(
        failures.is_empty(),
        "CurrentRuntimeEngine failed {} fixture(s): {:#?}",
        failures.len(),
        failures
    );
}

/// Bonus faithfulness layer: the portable SMT-LIB capture (solved by Z3 with no
/// Evident pipeline) reproduces the same golden — Method A (admissible) + Method
/// B (unique), or UNSAT for negative fixtures.
#[test]
fn smtlib_capture_is_faithful() {
    let fixtures = load_fixtures();
    assert!(!fixtures.is_empty(), "no fixtures found");
    let engine = SmtLibEngine;
    // Effects aren't extracted from the SMT model here (effects_in_smt governs
    // whether they're even encoded); the typed effect golden is checked by the
    // runtime engine. The model + sat/unsat checks ARE the SMT faithfulness proof.
    let (pass, failures) = run_engine(&engine, &fixtures, /*check_effects=*/ false);
    eprintln!("SmtLibEngine: {pass}/{} passed", fixtures.len());
    assert!(
        failures.is_empty(),
        "SmtLibEngine failed {} fixture(s): {:#?}",
        failures.len(),
        failures
    );
}
