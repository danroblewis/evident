//! Top-level API. Mirrors the Python `EvidentRuntime` for the v0.1 subset.

use crate::ast::Program;
use crate::parser;
use std::collections::HashMap;

pub use crate::translate::Value;

pub struct EvidentRuntime {
    program: Program,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub satisfied: bool,
    pub bindings: HashMap<String, Value>,
}

#[derive(Debug)]
pub enum RuntimeError {
    Parse(String),
    UnknownSchema(String),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RuntimeError::Parse(s) => write!(f, "{}", s),
            RuntimeError::UnknownSchema(s) => write!(f, "unknown schema {:?}", s),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl Default for EvidentRuntime { fn default() -> Self { Self::new() } }

impl EvidentRuntime {
    pub fn new() -> Self {
        EvidentRuntime { program: Program::default() }
    }

    /// Parse and load Evident source. Multiple calls accumulate.
    pub fn load_source(&mut self, src: &str) -> Result<(), RuntimeError> {
        let prog = parser::parse(src).map_err(|e| RuntimeError::Parse(e.to_string()))?;
        self.program.schemas.extend(prog.schemas);
        Ok(())
    }

    /// Evaluate the named schema and return whether it's satisfiable
    /// plus a model.
    pub fn query(&self, name: &str) -> Result<QueryResult, RuntimeError> {
        let schema = self.program.schemas.iter()
            .find(|s| s.name == name)
            .ok_or_else(|| RuntimeError::UnknownSchema(name.to_string()))?;
        let r = crate::translate::evaluate(schema);
        Ok(QueryResult { satisfied: r.satisfied, bindings: r.bindings })
    }
}
