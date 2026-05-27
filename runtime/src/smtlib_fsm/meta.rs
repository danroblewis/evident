//! Metadata format for SMT-LIB FSMs: the var/sort table, FSM-shape slots, and
//! the effect template. Parsed from JSON via `serde_json::Value` (no serde
//! derive dependency) so error messages can be explicit about what's malformed.
//!
//! A *fixture* is one JSON program describing an optional shared `world` record
//! plus one-or-more FSMs. Each FSM carries its metadata + its SMT-LIB constraint
//! text (inline `"smtlib"`, or `"smtlib_file"` resolved by the loader).

use serde_json::Value as J;

use super::SmtLibFsm;

/// Scalar SMT sort the v1 subset handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtSort {
    Int,
    Bool,
    Real,
    Str,
}

impl SmtSort {
    /// The Evident type name used in the synthetic `SchemaDecl` Membership, so
    /// `declare_var` / `resolve_fsm` classify the var the same way.
    pub fn evident_type(self) -> &'static str {
        match self {
            SmtSort::Int => "Int",
            SmtSort::Bool => "Bool",
            SmtSort::Real => "Real",
            SmtSort::Str => "String",
        }
    }

    fn parse(s: &str) -> Result<SmtSort, String> {
        match s {
            "Int" | "Nat" | "Pos" => Ok(SmtSort::Int),
            "Bool" => Ok(SmtSort::Bool),
            "Real" => Ok(SmtSort::Real),
            "Str" | "String" => Ok(SmtSort::Str),
            other => Err(format!("unknown sort `{other}` (expected Int/Bool/Real/String)")),
        }
    }
}

/// A declared SMT-LIB const: name + scalar sort.
#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub sort: SmtSort,
}

/// Source of an effect argument value.
#[derive(Debug, Clone)]
pub enum ArgSource {
    LitInt(i64),
    LitStr(String),
    LitBool(bool),
    /// Pull the scalar model value of this const.
    Var(String),
}

/// One templated effect: variant name, optional guard Bool var, and args.
#[derive(Debug, Clone)]
pub struct EffectSpec {
    /// When `Some`, the effect fires only if this model Bool is true.
    pub guard: Option<String>,
    /// Effect variant name (`Println`, `Exit`, `Print`, `IntToStr`, …).
    pub variant: String,
    pub args: Vec<ArgSource>,
}

/// Metadata for one SMT-LIB FSM.
#[derive(Debug, Clone)]
pub struct FsmMeta {
    pub fsm: String,
    pub vars: Vec<VarDecl>,
    /// Scalar vars exposed in `bindings` (threaded by the scheduler as state /
    /// world writes). Inputs like `_count` / `is_first_tick` are NOT listed.
    pub outputs: Vec<String>,
    pub effects_var: Option<String>,
    pub last_results_var: Option<String>,
    pub effects: Vec<EffectSpec>,
    // Multi-FSM world coordination (Phase 4). `world.X` reads / `world_next.X`
    // writes flow through the engine's existing world plumbing.
    pub world_var: Option<String>,
    pub world_next_var: Option<String>,
    pub world_type: Option<String>,
}

/// A whole SMT-LIB program: optional shared world record + the FSMs.
#[derive(Debug, Clone)]
pub struct FixtureProgram {
    pub world: Option<WorldDecl>,
    pub fsms: Vec<SmtLibFsm>,
}

/// A shared world record type (the `type World` of a multi-FSM program).
#[derive(Debug, Clone)]
pub struct WorldDecl {
    pub type_name: String,
    pub fields: Vec<VarDecl>,
}

// ---------------------------------------------------------------------------
// JSON parsing helpers
// ---------------------------------------------------------------------------

fn obj_str(o: &J, key: &str) -> Option<String> {
    o.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn require_str(o: &J, key: &str) -> Result<String, String> {
    obj_str(o, key).ok_or_else(|| format!("missing string field `{key}`"))
}

fn parse_var_list(v: &J, ctx: &str) -> Result<Vec<VarDecl>, String> {
    let arr = v.as_array().ok_or_else(|| format!("`{ctx}` must be an array"))?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let name = require_str(item, "name")
            .map_err(|e| format!("{ctx}[{i}]: {e}"))?;
        let sort_s = require_str(item, "sort")
            .map_err(|e| format!("{ctx}[{i}]: {e}"))?;
        let sort = SmtSort::parse(&sort_s).map_err(|e| format!("{ctx}[{i}]: {e}"))?;
        out.push(VarDecl { name, sort });
    }
    Ok(out)
}

fn parse_arg(v: &J, ctx: &str) -> Result<ArgSource, String> {
    let o = v.as_object().ok_or_else(|| format!("{ctx}: arg must be an object"))?;
    if let Some(s) = o.get("lit_str").and_then(|x| x.as_str()) {
        return Ok(ArgSource::LitStr(s.to_string()));
    }
    if let Some(n) = o.get("lit_int").and_then(|x| x.as_i64()) {
        return Ok(ArgSource::LitInt(n));
    }
    if let Some(b) = o.get("lit_bool").and_then(|x| x.as_bool()) {
        return Ok(ArgSource::LitBool(b));
    }
    if let Some(name) = o.get("var").and_then(|x| x.as_str()) {
        return Ok(ArgSource::Var(name.to_string()));
    }
    Err(format!("{ctx}: arg must have one of lit_str / lit_int / lit_bool / var"))
}

fn parse_effects(v: &J) -> Result<Vec<EffectSpec>, String> {
    let arr = v.as_array().ok_or("`effects` must be an array")?;
    let mut out = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let variant = require_str(item, "variant")
            .map_err(|e| format!("effects[{i}]: {e}"))?;
        let guard = obj_str(item, "guard");
        let args = match item.get("args") {
            None => Vec::new(),
            Some(a) => {
                let aa = a.as_array()
                    .ok_or_else(|| format!("effects[{i}].args must be an array"))?;
                aa.iter()
                    .enumerate()
                    .map(|(j, arg)| parse_arg(arg, &format!("effects[{i}].args[{j}]")))
                    .collect::<Result<Vec<_>, _>>()?
            }
        };
        out.push(EffectSpec { guard, variant, args });
    }
    Ok(out)
}

/// Parse one FSM's metadata object.
pub fn parse_meta(v: &J) -> Result<FsmMeta, String> {
    let fsm = require_str(v, "fsm")?;
    let vars = match v.get("vars") {
        Some(vs) => parse_var_list(vs, "vars")?,
        None => Vec::new(),
    };
    let outputs = match v.get("outputs") {
        None => Vec::new(),
        Some(o) => o
            .as_array()
            .ok_or("`outputs` must be an array")?
            .iter()
            .map(|x| x.as_str().map(|s| s.to_string()).ok_or("`outputs` entries must be strings".to_string()))
            .collect::<Result<Vec<_>, _>>()?,
    };
    let effects = match v.get("effects") {
        Some(e) => parse_effects(e)?,
        None => Vec::new(),
    };
    Ok(FsmMeta {
        fsm,
        vars,
        outputs,
        effects_var: obj_str(v, "effects_var"),
        last_results_var: obj_str(v, "last_results_var"),
        effects,
        world_var: obj_str(v, "world_var"),
        world_next_var: obj_str(v, "world_next_var"),
        world_type: obj_str(v, "world_type"),
    })
}

fn parse_world(v: &J) -> Result<WorldDecl, String> {
    let type_name = obj_str(v, "type").unwrap_or_else(|| "World".to_string());
    let fields = match v.get("fields") {
        Some(f) => parse_var_list(f, "world.fields")?,
        None => Vec::new(),
    };
    Ok(WorldDecl { type_name, fields })
}

/// Parse a whole fixture program. `resolve` maps a `"smtlib_file"` reference to
/// its text (the loader supplies a directory-relative reader); inline
/// `"smtlib"` strings bypass it.
pub fn parse_fixture(
    json: &str,
    resolve: &dyn Fn(&str) -> Result<String, String>,
) -> Result<FixtureProgram, String> {
    let root: J = serde_json::from_str(json).map_err(|e| format!("JSON parse error: {e}"))?;
    let world = match root.get("world") {
        Some(w) if !w.is_null() => Some(parse_world(w)?),
        _ => None,
    };
    let fsms_json = root
        .get("fsms")
        .and_then(|f| f.as_array())
        .ok_or("fixture must have an `fsms` array")?;
    let mut fsms = Vec::with_capacity(fsms_json.len());
    for (i, fj) in fsms_json.iter().enumerate() {
        let meta_json = fj
            .get("meta")
            .ok_or_else(|| format!("fsms[{i}]: missing `meta`"))?;
        let meta = parse_meta(meta_json).map_err(|e| format!("fsms[{i}].meta: {e}"))?;
        let smtlib = if let Some(s) = obj_str(fj, "smtlib") {
            s
        } else if let Some(file) = obj_str(fj, "smtlib_file") {
            resolve(&file).map_err(|e| format!("fsms[{i}] smtlib_file `{file}`: {e}"))?
        } else {
            return Err(format!("fsms[{i}]: needs `smtlib` or `smtlib_file`"));
        };
        fsms.push(SmtLibFsm { meta, smtlib });
    }
    Ok(FixtureProgram { world, fsms })
}
