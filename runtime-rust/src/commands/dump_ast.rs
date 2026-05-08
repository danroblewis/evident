//! `evident dump-ast <file>` — Stage 2 bridge demo. Loads the file
//! (auto-loading `stdlib/ast.ev` first so the AST enum bundle is
//! registered), encodes the parsed Program as a Z3 Datatype value
//! matching `Program` in `stdlib/ast.ev`, and prints the encoded
//! value.
//!
//! Useful for verifying the encoder output without writing a pass.
//! In Stage 3 the same encoder feeds the value as a `given` to a
//! self-hosted inference / desugar / lint pass.

use std::path::Path;
use std::process::ExitCode;

use evident_runtime::EvidentRuntime;

pub fn cmd_dump_ast(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("dump-ast: need <file.ev>");
        eprintln!("       evident dump-ast <file.ev>");
        eprintln!();
        eprintln!("Loads the file (and stdlib/ast.ev), encodes the parsed");
        eprintln!("program as a Z3 Datatype value matching Program in");
        eprintln!("stdlib/ast.ev, and prints the encoded value.");
        return ExitCode::from(2);
    }
    let user_path = &args[0];
    let mut rt = EvidentRuntime::new();

    // Load stdlib/ast.ev first so the encoder's lookups succeed.
    // We try the in-tree path first; a future packaging story might
    // ship it embedded.
    let stdlib = Path::new("stdlib/ast.ev");
    if let Err(e) = rt.load_file(stdlib) {
        eprintln!("error: failed to load stdlib/ast.ev: {e}");
        eprintln!("       (run `evident dump-ast` from the repo root, or");
        eprintln!("        ensure stdlib/ast.ev is reachable from cwd)");
        return ExitCode::from(1);
    }

    if let Err(e) = rt.load_file(Path::new(user_path)) {
        eprintln!("error: {e}");
        return ExitCode::from(1);
    }

    match rt.encode_program_value() {
        Ok(value) => {
            // Datatype implements Display via Z3's pretty-printer.
            println!("{}", value);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: encoding failed: {e}");
            ExitCode::from(1)
        }
    }
}
