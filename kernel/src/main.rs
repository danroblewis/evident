//! `evident-kernel` — the trampoline. Reads a .smt2 file, solves it via Z3,
//! walks the model's `effects` Seq, dispatches each variant, loops.
//!
//! Contract: docs/plans/kernel-input-spec.md.

use std::process::ExitCode;

mod functionize;
mod libcall;
mod manifest;
mod tick;

fn main() -> ExitCode {
    // Run the actual driver on a worker thread with a generous stack.
    // Real Evident programs (e.g. compiler/sample.ev → ~5400 flattened
    // lines) drive the per-tick Z3 solve through deeply nested ITE
    // chains; on macOS the default main-thread stack (~8 MB) overflows
    // partway through. A 128 MB worker stack covers every program
    // we've measured, with headroom. Override via env if needed.
    let stack_size_mb = std::env::var("EVIDENT_KERNEL_STACK_MB")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(128);
    let stack_size = stack_size_mb * 1024 * 1024;

    let handle = std::thread::Builder::new()
        .stack_size(stack_size)
        .name("kernel-driver".into())
        .spawn(driver)
        .expect("spawn kernel-driver thread");
    handle.join().unwrap_or(ExitCode::from(3))
}

fn driver() -> ExitCode {
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
