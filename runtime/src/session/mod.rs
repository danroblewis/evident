use crate::encode::lower;
mod register_enums;
mod query;

pub use crate::core::Value;
#[allow(unused_imports)]
pub use crate::core::{QueryResult, RuntimeError};

use crate::core::ast::{BodyItem, Program, SchemaDecl};
use crate::parser;
use crate::encode::DatatypeRegistry;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use z3::{Config, Context};

pub struct EvidentRuntime {
    pub(super) program: Program,

    pub(super) schemas: HashMap<String, SchemaDecl>,

    pub(super) schema_order: Vec<String>,

    pub(super) z3_ctx: &'static Context,

    pub(super) functionizer: crate::functionize::cranelift::CraneliftFunctionizer,

    pub(super) fn_cache: RefCell<HashMap<(String, Vec<String>),
                              Option<std::rc::Rc<query::ClaimPlan>>>>,

    pub(super) slow_path_cache: RefCell<HashMap<(String, Vec<String>),
                                     std::rc::Rc<crate::core::CompiledModel<'static>>>>,

    pub(super) datatypes: DatatypeRegistry,

    pub(super) enums: crate::core::EnumRegistry,

    pub(super) loaded_files: RefCell<HashSet<PathBuf>>,
}

impl Default for EvidentRuntime { fn default() -> Self { Self::new() } }

impl EvidentRuntime {

    pub fn new() -> Self {
        let cfg = Config::new();
        let ctx: &'static Context = Box::leak(Box::new(Context::new(&cfg)));
        EvidentRuntime {
            program: Program::default(),
            schemas: HashMap::new(),
            schema_order: Vec::new(),
            z3_ctx: ctx,
            functionizer: crate::functionize::cranelift::CraneliftFunctionizer,
            fn_cache: RefCell::new(HashMap::new()),
            slow_path_cache: RefCell::new(HashMap::new()),
            datatypes: RefCell::new(HashMap::new()),
            enums: crate::core::EnumRegistry::new(),
            loaded_files: RefCell::new(HashSet::new()),
        }
    }

    pub fn schema_names(&self) -> impl Iterator<Item = &str> {
        self.schema_order.iter().map(|s| s.as_str())
    }

    pub fn get_schema(&self, name: &str) -> Option<&SchemaDecl> {
        self.schemas.get(name)
    }

    pub fn enums_registry(&self) -> &crate::core::EnumRegistry {
        &self.enums
    }

    pub fn z3_context(&self) -> &'static z3::Context {
        self.z3_ctx
    }

    pub fn datatypes_registry(&self) -> &crate::core::DatatypeRegistry {
        &self.datatypes
    }

    pub fn schemas_map(&self) -> &HashMap<String, SchemaDecl> {
        &self.schemas
    }

    pub fn effect_results_to_value(
        &self,
        items: &[crate::core::ast::EffectResult],
    ) -> crate::core::Value {
        crate::encode::effect_encoder::effect_results_to_value(items)
    }
}

// ───────────────────────── load: parse + import + lower ─────────────────────────

fn register_subclaims(body: &[BodyItem], schemas: &mut HashMap<String, SchemaDecl>) {
    for item in body {
        if let BodyItem::SubclaimDecl(s) = item {
            schemas.insert(s.name.clone(), s.clone());
            register_subclaims(&s.body, schemas);
        }
    }
}

impl EvidentRuntime {

    pub fn load_source(&mut self, src: &str) -> Result<(), RuntimeError> {
        self.load_source_with_base(src, None)
    }

    pub fn load_file(&mut self, path: &Path) -> Result<(), RuntimeError> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !self.loaded_files.borrow_mut().insert(canonical.clone()) {

            return Ok(());
        }
        let src = std::fs::read_to_string(path)
            .map_err(|e| RuntimeError::Io(format!("read {}: {e}", path.display())))?;
        self.load_source_with_base(&src, Some(&canonical))
    }

    pub(super) fn load_source_with_base(&mut self, src: &str, base: Option<&Path>) -> Result<(), RuntimeError> {
        let prog = parser::parse(src).map_err(|e| RuntimeError::Parse(e.to_string()))?;

        for import_path in &prog.imports {

            if crate::ffi::is_shimmed_stdlib(import_path) {

                if self.resolve_import(import_path, base).is_err() {
                    continue;
                }
            }
            let resolved = self.resolve_import(import_path, base)?;
            self.load_file(&resolved)?;
        }
        for s in &prog.schemas {
            let mut s = s.clone();

            lower::desugar_seq_concat(&mut s);
            lower::desugar_delta(&mut s);
            lower::inject_fsm_params(&mut s)?;

            lower::inject_lhs_eq_types(&mut s, &self.schemas, &self.enums);
            lower::inject_prev_tick_decls(&mut s)?;

            lower::inject_claim_arg_types(&mut s, &self.schemas)?;
            if !self.schemas.contains_key(&s.name) {
                self.schema_order.push(s.name.clone());
            }
            self.schemas.insert(s.name.clone(), s.clone());
            register_subclaims(&s.body, &mut self.schemas);
        }

        register_enums::register_enums(&prog.enums, self.z3_ctx, &self.enums)?;
        self.program.schemas.extend(prog.schemas);
        self.program.enums.extend(prog.enums);

        self.fn_cache.borrow_mut().clear();
        self.slow_path_cache.borrow_mut().clear();

        self.datatypes.borrow_mut().clear();
        Ok(())
    }

    pub(super) fn resolve_import(&self, import_path: &str, base: Option<&Path>) -> Result<PathBuf, RuntimeError> {
        let p = Path::new(import_path);

        if p.exists() {
            return Ok(p.to_path_buf());
        }

        if let Some(base) = base {
            if let Some(dir) = base.parent() {
                let candidate = dir.join(p);
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
        }

        if let Ok(cwd) = std::env::current_dir() {
            let candidate = cwd.join(p);
            if candidate.exists() {
                return Ok(candidate);
            }
        }

        if let Some(base) = base {
            let mut anc = base.parent();
            for _ in 0..10 {
                let Some(dir) = anc else { break };
                let candidate = dir.join(p);
                if candidate.exists() {
                    return Ok(candidate);
                }
                anc = dir.parent();
            }
        }
        Err(RuntimeError::Io(format!(
            "import not found: {:?} (tried verbatim, relative to source file, cwd, and ancestors of the source file)",
            import_path)))
    }
}

// ───────────────────────── scheduler-facing query API ─────────────────────────

impl EvidentRuntime {

    pub fn query_with_pins_and_given(
        &self,
        claim_name: &str,
        pins: &[(&str, z3::ast::Datatype<'static>)],
        given: &HashMap<String, Value>,
    ) -> Result<QueryResult, RuntimeError> {
        let schema = self.schemas.get(claim_name)
            .ok_or_else(|| RuntimeError::UnknownSchema(claim_name.to_string()))?;

        if let Some(result) = self.try_functionize_z3(claim_name, schema, given) {
            return Ok(result);
        }

        let arith: u32 = 2;

        let mut given_keys: Vec<String> = given.keys().cloned().collect();
        given_keys.sort();
        let cache_key = (claim_name.to_string(), given_keys);
        if let Some(cached) = self.slow_path_cache.borrow().get(&cache_key).cloned() {
            use z3::ast::Ast;
            cached.solver.push();

            for (var_name, value) in pins {
                if let Some(crate::encode::Var::EnumVar { ast, .. }) = cached.env.get(*var_name) {
                    cached.solver.assert(&ast._eq(value));
                }
            }

            for (name, value) in given {
                if let (Some(crate::encode::Var::EnumVar { ast, .. }), Value::Enum { .. }) =
                    (cached.env.get(name), value)
                {
                    if let Some(dt) = crate::encode::value_enum_to_datatype(
                        value, self.z3_ctx, &self.enums)
                    {
                        cached.solver.assert(&ast._eq(&dt));
                    }
                }
            }
            let r = crate::encode::run_cached(&cached, given, self.z3_ctx, Some(&self.enums));
            cached.solver.pop(1);
            return Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings });
        }

        let r = crate::encode::evaluate_with_extra_assertions(
            schema,
            given,
            &self.schemas,
            self.z3_ctx,
            &self.datatypes,
            Some(&self.enums),
            arith,
            pins,
        );
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
}

// ───────────────────────── union-find (enum/output equivalence) ─────────────────────────

pub(crate) struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    pub(crate) fn new(n: usize) -> Self {
        UnionFind { parent: (0..n).collect(), rank: vec![0; n] }
    }

    pub(crate) fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.parent[r] != r { r = self.parent[r]; }
        let mut y = x;
        while self.parent[y] != r {
            let next = self.parent[y];
            self.parent[y] = r;
            y = next;
        }
        r
    }

    pub(crate) fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb { return; }
        if self.rank[ra] < self.rank[rb] {
            self.parent[ra] = rb;
        } else if self.rank[ra] > self.rank[rb] {
            self.parent[rb] = ra;
        } else {
            self.parent[rb] = ra;
            self.rank[ra] += 1;
        }
    }
}
