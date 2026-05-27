//! CLI subcommand implementations. Each `cmd_<name>` lives in its
//! own file under `commands/`; shared helpers in `commands/common.rs`.

pub mod common;

pub mod effect_run;
pub mod effect_run_smtlib;
pub mod sample;
pub mod test;
