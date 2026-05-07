//! CLI subcommand implementations. Each `cmd_<name>` lives in its
//! own file under `commands/`; shared helpers (flag parsing, value
//! formatting, runtime loading) live in `commands/common.rs`.
//!
//! Adding a new subcommand: create `commands/<name>.rs` with a
//! `pub fn cmd_<name>(args: &[String]) -> ExitCode`, add `pub mod <name>;`
//! below, and wire it into `main.rs`'s dispatch.

pub mod common;
pub mod initial_state;

pub mod check;
pub mod execute;
pub mod parse;
pub mod query;
pub mod sample;
pub mod test;
pub mod transpile_shader;
