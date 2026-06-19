use crate::core::{QueryResult, RuntimeError};
use super::EvidentRuntime;
use std::collections::HashMap;

impl EvidentRuntime {

    pub fn query_free(&self, name: &str) -> Result<QueryResult, RuntimeError> {
        self.query(name, &HashMap::new())
    }
}
