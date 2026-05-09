//! CLI subcommand implementations. Each `cmd_<name>` lives in its
//! own file under `commands/`; shared helpers in `commands/common.rs`.

pub mod common;

pub mod check;
pub mod desugar;
pub mod effect_run;
pub mod infer_types;
pub mod lint;
pub mod query;
pub mod sample;
pub mod test;
