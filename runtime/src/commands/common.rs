//! Shared helpers used by the `cmd_*` subcommands: usage banner,
//! file/flag splitting, and runtime loading.

use std::path::Path;

use evident_runtime::EvidentRuntime;

/// Path to the AST schema file every self-hosted pass loads first.
/// Single source of truth for the lint pipeline and the self-hosted
/// desugar passes that run automatically during load.
pub const STDLIB_AST: &str = "stdlib/ast.ev";

pub fn usage() {
    eprintln!("usage:");
    eprintln!("  evident check        <files…>");
    eprintln!("  evident test         [path] [-v] [--no-color]");
    eprintln!("  evident effect-run   <file>           # run an effect-driven program");
    eprintln!("  evident lint         <file>");
}

/// Split positional file paths from flag arguments. Files are everything
/// before the first `--…` flag. Returns `(files, flags)`.
pub fn split_files_and_flags(args: &[String]) -> (Vec<String>, Vec<String>) {
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() && !args[i].starts_with('-') {
        files.push(args[i].clone());
        i += 1;
    }
    (files, args[i..].to_vec())
}

pub fn load_runtime(files: &[String]) -> Result<EvidentRuntime, String> {
    let mut rt = EvidentRuntime::new();
    for f in files {
        // Use load_file so any `import "..."` statements inside the
        // file resolve relative to the file itself.
        rt.load_file(Path::new(f)).map_err(|e| format!("{f}: {e}"))?;
    }
    Ok(rt)
}

/// Load a fresh runtime pre-seeded with `STDLIB_AST` + the given pass
/// files (marked as system loads), then load the user's files. Used
/// by every self-hosted pass driver (lint, desugar, infer-types).
pub fn load_runtime_with_passes(
    pass_files: &[&str],
    user_files: &[String],
) -> Result<EvidentRuntime, String> {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(STDLIB_AST))
        .map_err(|e| format!("load {STDLIB_AST}: {e}"))?;
    for f in pass_files {
        rt.load_file(Path::new(f))
            .map_err(|e| format!("load {f}: {e}"))?;
    }
    rt.mark_system_loads_complete();
    for path in user_files {
        rt.load_file(Path::new(path))
            .map_err(|e| format!("load {path}: {e}"))?;
    }
    Ok(rt)
}

