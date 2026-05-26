//! `evident dump-smtlib <files…> <claim> [--given k=v …] [-o out.smt2] [--solve]`
//! — emit the SMT-LIB text for a claim as a runnable `.smt2` artifact.
//!
//! This is the first real artifact of the north star in
//! `docs/design/smtlib-as-compile-target.md`: instead of building the Z3 AST
//! through the C API, the claim is transpiled to SMT-LIB *text* (the
//! quantifier-free scalar/string subset in `translate/smtlib.rs`) and written to
//! disk. With `--solve` it also runs that text back through Z3 and prints the
//! result — a from-the-CLI demonstration that Z3 solves the emitted SMT-LIB.
//!
//! Out-of-subset claims are reported as errors (exit 1), never mis-emitted.

use std::process::ExitCode;

use evident_runtime::translate::smtlib;

use super::common::{
    auto_apply_desugar, format_value, load_runtime, parse_flags, split_files_and_flags,
};

pub fn cmd_dump_smtlib(args: &[String]) -> ExitCode {
    // Peel off `-o <path>` and `--solve`; the rest goes through the standard
    // files/claim + `--given` parsing shared with `sample`/`query`.
    let mut out_path: Option<String> = None;
    let mut do_solve = false;
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--out" => {
                i += 1;
                match args.get(i) {
                    Some(p) => out_path = Some(p.clone()),
                    None => {
                        eprintln!("dump-smtlib: -o needs a path");
                        return ExitCode::from(2);
                    }
                }
                i += 1;
            }
            "--solve" => {
                do_solve = true;
                i += 1;
            }
            other => {
                rest.push(other.to_string());
                i += 1;
            }
        }
    }

    let (files_and_schema, flag_args) = split_files_and_flags(&rest);
    if files_and_schema.len() < 2 {
        eprintln!("dump-smtlib: need <files…> <claim>");
        return ExitCode::from(2);
    }
    let claim = files_and_schema.last().unwrap().clone();
    let files: Vec<String> = files_and_schema[..files_and_schema.len() - 1].to_vec();

    let flags = match parse_flags(&flag_args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };

    let mut rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(1);
        }
    };
    auto_apply_desugar(&mut rt, &files);

    let Some(schema) = rt.get_schema(&claim) else {
        eprintln!("dump-smtlib: no claim `{claim}` loaded from {files:?}");
        return ExitCode::from(1);
    };

    let artifact = match smtlib::schema_to_smtlib_artifact(schema, &flags.given) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("dump-smtlib: claim `{claim}` is out of the SMT-LIB subset: {e}");
            return ExitCode::from(1);
        }
    };

    match &out_path {
        Some(p) => {
            if let Err(e) = std::fs::write(p, &artifact) {
                eprintln!("dump-smtlib: write {p}: {e}");
                return ExitCode::from(1);
            }
            eprintln!("wrote SMT-LIB for `{claim}` → {p} ({} bytes)", artifact.len());
        }
        None => print!("{artifact}"),
    }

    if do_solve {
        match smtlib::solve_with_given(schema, &flags.given) {
            Ok(r) => {
                eprintln!("smtlib-solve: {}", if r.satisfied { "sat" } else { "unsat" });
                if r.satisfied {
                    let mut keys: Vec<&String> = r.bindings.keys().collect();
                    keys.sort();
                    for k in keys {
                        eprintln!("  {k} = {}", format_value(&r.bindings[k]));
                    }
                }
            }
            Err(e) => {
                eprintln!("dump-smtlib: solve failed: {e}");
                return ExitCode::from(1);
            }
        }
    }

    ExitCode::SUCCESS
}
