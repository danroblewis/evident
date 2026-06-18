//! CLI subcommand implementations. Each `cmd_<name>` lives in its
//! own file under `commands/`; shared helpers in `commands/common.rs`.

pub mod common;

pub mod check;
pub mod effect_run;
pub mod lint;
pub mod test;

// Not CLI subcommands: these hold the load-time desugar / type-inference
// passes that run automatically (auto_apply_*), called by the commands
// above. TODO: relocate into a dedicated runtime module (naming TBD).
pub mod desugar;
pub mod infer_types;
