use std::collections::HashMap;

#[derive(Default, Clone, Debug)]
pub struct FunctionizeStats {
    pub claims: HashMap<String, PerClaimStats>,
}

#[derive(Default, Clone, Debug)]
pub struct PerClaimStats {

    pub analyses: u32,

    pub cache_hits: u32,

    pub decided_unsat: u32,

    pub simplified_total: u32,

    pub steps_total: u32,

    pub checks_total: u32,

    pub predicates_total: u32,

    pub last_extract_ok: Option<bool>,

    pub compiled: u32,

    pub components: u32,

    pub components_compiled: u32,
}
