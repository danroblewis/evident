//! `evident-kernel` — the trampoline. Reads a .smt2 file, solves it via Z3,
//! walks the model's `effects` Seq, dispatches each variant, loops.
//!
//! Contract: docs/plans/kernel-input-spec.md.

use std::process::ExitCode;

mod libcall;
mod manifest;
mod tick;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first() else {
        eprintln!("Usage: kernel <file.smt2>");
        return ExitCode::from(2);
    };

    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("kernel: read {path}: {e}");
            return ExitCode::from(3);
        }
    };

    let manifest = match manifest::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("kernel: manifest: {e}");
            return ExitCode::from(3);
        }
    };

    match tick::run(&src, &manifest) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("kernel: {e}");
            ExitCode::from(3)
        }
    }
}
