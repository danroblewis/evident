use crate::core::RuntimeError;
use super::EvidentRuntime;
use crate::parser;
use std::path::{Path, PathBuf};

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

            if crate::fti::is_shimmed_stdlib(import_path) {

                if self.resolve_import(import_path, base).is_err() {
                    continue;
                }
            }
            let resolved = self.resolve_import(import_path, base)?;
            self.load_file(&resolved)?;
        }
        for s in &prog.schemas {
            let mut s = s.clone();

            super::desugar::unify_world_syntax(&mut s)?;

            super::desugar::desugar_seq_concat(&mut s);
            super::inject::inject_fsm_params(&mut s)?;

            super::inject::inject_lhs_eq_types(&mut s, &self.schemas, &self.enums);
            super::inject::inject_prev_tick_decls(&mut s)?;

            super::inject::inject_claim_arg_types(&mut s, &self.schemas)?;
            if !self.schemas.contains_key(&s.name) {
                self.schema_order.push(s.name.clone());
            }
            self.schemas.insert(s.name.clone(), s.clone());
            super::validate::register_subclaims(&s.body, &mut self.schemas);
        }

        super::register_enums::register_enums(&prog.enums, self.z3_ctx, &self.enums)?;
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
