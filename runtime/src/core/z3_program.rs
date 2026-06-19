use z3::ast::{Bool, Dynamic};

use crate::core::Value;

#[derive(Debug, Clone)]
pub struct Z3Program<'ctx> {

    pub steps: Vec<Z3Step<'ctx>>,

    pub checks: Vec<(Dynamic<'ctx>, Dynamic<'ctx>)>,

    pub predicates: Vec<Bool<'ctx>>,
}

#[derive(Debug, Clone)]
pub enum Z3Step<'ctx> {

    Scalar { var: String, expr: Dynamic<'ctx> },

    Seq    { var: String, elem_exprs: Vec<Dynamic<'ctx>> },

    Guarded { var: String, branches: Vec<GuardedBranch<'ctx>> },

    PreBaked { var: String, value: Value },
}

#[derive(Debug, Clone)]
pub struct GuardedBranch<'ctx> {
    pub guard: Dynamic<'ctx>,
    pub body:  GuardedBody<'ctx>,
}

#[derive(Debug, Clone)]
pub enum GuardedBody<'ctx> {
    Scalar(Dynamic<'ctx>),
    Seq(Vec<Dynamic<'ctx>>),
}

impl<'ctx> Z3Step<'ctx> {
    pub fn var(&self) -> &str {
        match self {
            Z3Step::Scalar   { var, .. }
            | Z3Step::Seq      { var, .. }
            | Z3Step::Guarded  { var, .. }
            | Z3Step::PreBaked { var, .. } => var,
        }
    }
}
