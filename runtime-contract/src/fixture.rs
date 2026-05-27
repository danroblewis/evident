//! Fixture loading: parse `meta.json` (FORMAT.md §2) into a typed [`Meta`] and
//! expose the SMT-LIB / golden files of one fixture directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value as Json;

use crate::value::CVal;

/// Parsed `meta.json` plus the raw effect-text golden. Field names mirror
/// FORMAT.md §2; `*_var` roles are `None` when the FSM has no such slot.
#[derive(Debug, Clone)]
pub struct Meta {
    pub name: String,
    pub fsm_claim: String,
    pub source_ev: String,
    pub how_built: String,
    pub state_var: Option<String>,
    pub state_next_var: Option<String>,
    pub effects_var: Option<String>,
    pub last_results_var: Option<String>,
    pub world_fields: Vec<String>,
    pub effects_in_smt: bool,
    /// Input pins: var → typed value (includes enum `state`, `_count`, …).
    pub given: BTreeMap<String, CVal>,
    pub expect_unsat: bool,
    /// Subset of next-state bindings to check (var → golden value).
    pub expect_model: BTreeMap<String, CVal>,
    /// Negative fixtures: var → the IMPOSSIBLE value (forced output must differ).
    pub expect_forbidden: BTreeMap<String, CVal>,
    /// Ordered golden effects, typed (mirrors `expected_effects.txt`).
    pub expect_effects: Vec<CVal>,
}

/// One captured tick: its metadata + the directory holding the SMT-LIB + source.
#[derive(Debug, Clone)]
pub struct Fixture {
    pub name: String,
    pub dir: PathBuf,
    pub meta: Meta,
}

impl Fixture {
    /// Read one file in the fixture dir; missing → empty string (matches the
    /// `behavior_contract.rs` convention for absent `inputs.smt2`, etc.).
    pub fn read(&self, file: &str) -> String {
        fs::read_to_string(self.dir.join(file)).unwrap_or_default()
    }
    pub fn problem(&self) -> String {
        self.read("problem.smt2")
    }
    pub fn prev(&self) -> String {
        self.read("prev.smt2")
    }
    pub fn inputs(&self) -> String {
        self.read("inputs.smt2")
    }
    pub fn expected_model(&self) -> String {
        self.read("expected_model.smt2")
    }
    /// `problem ⧺ prev ⧺ inputs` — the fully-pinned single-tick SMT-LIB problem
    /// (the form an SMT-LIB-ingesting engine solves; pins are inline asserts).
    pub fn pinned_smtlib(&self) -> String {
        format!("{}\n{}\n{}\n", self.problem(), self.prev(), self.inputs())
    }
    /// Golden dispatched effects as canonical lines (FORMAT.md §6), one per
    /// non-empty line. Empty file → empty vec.
    pub fn expected_effect_lines(&self) -> Vec<String> {
        self.read("expected_effects.txt")
            .lines()
            .map(|l| l.trim_end().to_string())
            .filter(|l| !l.is_empty())
            .collect()
    }
}

/// Compute `<repo>/runtime-contract/fixtures` from a consuming crate's
/// `CARGO_MANIFEST_DIR` (which is `<repo>/runtime` or `<repo>/runtime-smt`).
pub fn fixtures_dir_from_manifest(manifest_dir: &str) -> PathBuf {
    Path::new(manifest_dir)
        .parent()
        .expect("crate manifest dir has a parent (the repo root)")
        .join("runtime-contract/fixtures")
}

/// Load every fixture directory (one with a `meta.json`) under `dir`, sorted by
/// name for deterministic matrix ordering.
pub fn load_fixtures(dir: &Path) -> Vec<Fixture> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read fixtures dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    entries.sort();

    let mut out = Vec::new();
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

fn obj_map(j: &Json, ctx: &str) -> BTreeMap<String, CVal> {
    match j {
        Json::Object(m) => m
            .iter()
            .map(|(k, v)| (k.clone(), CVal::from_tagged_json(v, &format!("{ctx}.{k}"))))
            .collect(),
        _ => BTreeMap::new(),
    }
}

fn parse_meta(path: &Path, name: &str) -> Meta {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("{name}: read meta.json: {e}"));
    let j: Json =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: meta.json parse: {e}"));

    let s = |key: &str| -> Option<String> { j[key].as_str().map(|s| s.to_string()) };
    let req = |key: &str| -> String {
        j[key].as_str().unwrap_or_else(|| panic!("{name}: missing `{key}`")).to_string()
    };

    let world_fields = match &j["world_fields"] {
        Json::Array(a) => a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect(),
        _ => Vec::new(),
    };

    let expect = &j["expect"];
    let expect_unsat = expect.get("unsat").and_then(|b| b.as_bool()).unwrap_or(false);

    Meta {
        name: name.to_string(),
        fsm_claim: req("fsm_claim"),
        source_ev: s("source_ev").unwrap_or_default(),
        how_built: s("how_built").unwrap_or_else(|| "?".to_string()),
        state_var: s("state_var"),
        state_next_var: s("state_next_var"),
        effects_var: s("effects_var"),
        last_results_var: s("last_results_var"),
        world_fields,
        effects_in_smt: j.get("effects_in_smt").and_then(|b| b.as_bool()).unwrap_or(false),
        given: obj_map(&j["given"], &format!("{name}.given")),
        expect_unsat,
        expect_model: expect
            .get("model")
            .map(|m| obj_map(m, &format!("{name}.expect.model")))
            .unwrap_or_default(),
        expect_forbidden: expect
            .get("forbidden")
            .map(|m| obj_map(m, &format!("{name}.expect.forbidden")))
            .unwrap_or_default(),
        expect_effects: match expect.get("effects") {
            Some(Json::Array(a)) => a
                .iter()
                .map(|v| CVal::from_tagged_json(v, &format!("{name}.expect.effects")))
                .collect(),
            _ => Vec::new(),
        },
    }
}
