//! `evident` CLI.
//!
//!   `evident check <file>`                         — load + parse. Exit 0 if accepted.
//!   `evident query <file> <claim> [--json] [--given k=v …]`
//!                                                  — run `query` on a single schema.
//!   `evident sample <file> <claim> [-n 1] [--json] [--given k=v …]`
//!                                                  — alias for query; -n is ignored (we return one model).
//!   `evident sample <file> --all --json`           — sat-check every loaded schema; emit `{name: bool}`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use evident_runtime::{EvidentRuntime, Value};

fn usage() {
    eprintln!("Usage:");
    eprintln!("  evident sample <file> <claim> [-n N] [--json] [--given k=v ...]");
    eprintln!("  evident sample <file> --all [--json]");
    eprintln!("  evident emit   <file> <claim> [-o <out.smt2>]");
    eprintln!("  evident run    <file> <claim>     # emit + kernel in one step");
    eprintln!("  evident dump-tokens <file>        # lex + dump tokens as JSON (oracle diagnostic)");
}

fn load(file: &str) -> Option<EvidentRuntime> {
    let mut rt = EvidentRuntime::new();
    let path = PathBuf::from(file);
    if let Err(e) = rt.load_file(&path) {
        eprintln!("load error: {e:?}");
        return None;
    }
    Some(rt)
}

fn parse_given(args: &[String]) -> HashMap<String, Value> {
    let mut given = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--given" {
            i += 1;
            while i < args.len() && !args[i].starts_with("--") && args[i] != "-n" {
                if let Some(eq) = args[i].find('=') {
                    let k = args[i][..eq].to_string();
                    let v = &args[i][eq + 1..];
                    let val = if let Ok(n) = v.parse::<i64>() {
                        Value::Int(n)
                    } else if v == "true" {
                        Value::Bool(true)
                    } else if v == "false" {
                        Value::Bool(false)
                    } else if let Ok(r) = v.parse::<f64>() {
                        Value::Real(r)
                    } else {
                        Value::Str(v.to_string())
                    };
                    given.insert(k, val);
                }
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    given
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn value_to_json(v: &Value) -> String {
    match v {
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Real(r) => r.to_string(),
        Value::Str(s) => format!("{:?}", s),
        Value::SeqInt(items) => format!("[{}]",
            items.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")),
        Value::SeqBool(items) => format!("[{}]",
            items.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ")),
        Value::SeqStr(items) => format!("[{}]",
            items.iter().map(|s| format!("{:?}", s)).collect::<Vec<_>>().join(", ")),
        Value::SeqEnum(items) => format!("[{}]",
            items.iter().map(value_to_json).collect::<Vec<_>>().join(", ")),
        Value::SeqComposite(_) => "[]".to_string(),
        Value::SetInt(items) => format!("[{}]",
            items.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")),
        Value::SetBool(items) => format!("[{}]",
            items.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ")),
        Value::SetStr(items) => format!("[{}]",
            items.iter().map(|s| format!("{:?}", s)).collect::<Vec<_>>().join(", ")),
        Value::Enum { variant, fields, .. } => {
            if fields.is_empty() {
                format!("{:?}", variant)
            } else {
                let parts: Vec<String> = fields.iter().map(value_to_json).collect();
                format!("{{\"{}\":[{}]}}", variant, parts.join(", "))
            }
        }
        Value::Composite(_) => "null".to_string(),
    }
}

fn cmd_query_or_sample(args: &[String]) -> ExitCode {
    let Some(file) = args.first() else { usage(); return ExitCode::from(2); };
    let rest = &args[1..];
    let json = has_flag(rest, "--json");

    // --all: sat-check every schema, emit {name: bool}.
    if has_flag(rest, "--all") {
        let Some(rt) = load(file) else { return ExitCode::from(1); };
        let given = parse_given(rest);
        let names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
        let mut parts = Vec::new();
        for n in &names {
            // Skip generic templates (won't translate on their own).
            if let Some(s) = rt.get_schema(n) {
                if !s.type_params.is_empty() { continue; }
            }
            let sat = rt.query(n, &given).map(|r| r.satisfied).unwrap_or(false);
            parts.push(format!("\"{}\":{}", n, sat));
        }
        if json {
            println!("{{{}}}", parts.join(","));
        } else {
            for p in &parts { println!("{p}"); }
        }
        return ExitCode::SUCCESS;
    }

    // Single-claim: expect a claim name as second positional.
    let claim = rest.iter().find(|a| !a.starts_with("--") && *a != "-n"
                                       && !a.parse::<i64>().is_ok())
                    .cloned()
                    // fall back to the first non-flag
                    .or_else(|| rest.iter().find(|a| !a.starts_with("--")).cloned());
    let Some(claim) = claim else { usage(); return ExitCode::from(2); };
    let Some(rt) = load(file) else { return ExitCode::from(1); };
    let given = parse_given(rest);
    match rt.query(&claim, &given) {
        Ok(r) => {
            if json {
                if r.satisfied {
                    let parts: Vec<String> = r.bindings.iter()
                        .map(|(k, v)| format!("\"{}\":{}", k, value_to_json(v)))
                        .collect();
                    println!("[{{{}}}]", parts.join(","));
                } else {
                    println!("[]");
                }
            } else {
                println!("satisfied: {}", r.satisfied);
                for (k, v) in &r.bindings {
                    println!("  {k} = {v:?}");
                }
            }
            if r.satisfied { ExitCode::SUCCESS } else { ExitCode::from(1) }
        }
        Err(e) => {
            eprintln!("query error: {e:?}");
            if json { println!("[]"); }
            ExitCode::from(1)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() { usage(); return ExitCode::from(2); }
    match args[0].as_str() {
        "sample" => cmd_query_or_sample(&args[1..]),
        "emit"   => cmd_emit(&args[1..]),
        "run"    => cmd_run(&args[1..]),
        "dump-tokens" => cmd_dump_tokens(&args[1..]),
        "help" | "--help" | "-h" => { usage(); ExitCode::SUCCESS }
        other => { eprintln!("unknown subcommand: {other}"); usage(); ExitCode::from(2) }
    }
}

/// Lex the input file and dump tokens as a JSON array, one per line.
///
/// This is a *diagnostic* subcommand. It exposes the Rust lexer's
/// internal token stream so a Python oracle harness can compare it
/// against the self-hosted Evident lexer's output (Phase A of the
/// completion roadmap). It does NOT define or alter language semantics —
/// it's the same `tokenize` function the parser already calls,
/// printed in JSON form instead of fed forward.
///
/// Output shape: a JSON array of token objects, one per token,
/// terminated by Eof. Each object has a `"k"` (kind) field and
/// optional `"v"` (value) for tokens carrying payload:
///
///   {"k":"Claim"}
///   {"k":"Ident","v":"x"}
///   {"k":"Eq"}
///   {"k":"Int","v":1}
///   {"k":"Eof"}
fn cmd_dump_tokens(args: &[String]) -> ExitCode {
    let Some(file) = args.first() else { usage(); return ExitCode::from(2); };
    let src = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => { eprintln!("dump-tokens: read {file}: {e}"); return ExitCode::from(1); }
    };
    let toks = match evident_runtime::lexer::tokenize(&src) {
        Ok(t) => t,
        Err(e) => { eprintln!("dump-tokens: {e}"); return ExitCode::from(1); }
    };
    println!("[");
    for (i, t) in toks.iter().enumerate() {
        let sep = if i + 1 == toks.len() { "" } else { "," };
        println!("  {}{}", token_to_json(t), sep);
    }
    println!("]");
    ExitCode::SUCCESS
}

fn token_to_json(t: &evident_runtime::lexer::Token) -> String {
    use evident_runtime::lexer::Token::*;
    fn esc(s: &str) -> String {
        // Minimal JSON string escape.
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for c in s.chars() {
            match c {
                '"'  => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
    match t {
        Ident(s)   => format!(r#"{{"k":"Ident","v":{}}}"#, esc(s)),
        Int(n)     => format!(r#"{{"k":"Int","v":{}}}"#, n),
        Real(r)    => format!(r#"{{"k":"Real","v":{}}}"#, r),
        Str(s)     => format!(r#"{{"k":"Str","v":{}}}"#, esc(s)),
        True       => r#"{"k":"True"}"#.to_string(),
        False      => r#"{"k":"False"}"#.to_string(),
        Schema     => r#"{"k":"Schema"}"#.to_string(),
        Claim      => r#"{"k":"Claim"}"#.to_string(),
        Type       => r#"{"k":"Type"}"#.to_string(),
        Subclaim   => r#"{"k":"Subclaim"}"#.to_string(),
        Fsm        => r#"{"k":"Fsm"}"#.to_string(),
        External   => r#"{"k":"External"}"#.to_string(),
        Enum       => r#"{"k":"Enum"}"#.to_string(),
        Match      => r#"{"k":"Match"}"#.to_string(),
        Matches    => r#"{"k":"Matches"}"#.to_string(),
        Import     => r#"{"k":"Import"}"#.to_string(),
        In         => r#"{"k":"In"}"#.to_string(),
        NotIn      => r#"{"k":"NotIn"}"#.to_string(),
        ContainsRev=> r#"{"k":"ContainsRev"}"#.to_string(),
        Eq         => r#"{"k":"Eq"}"#.to_string(),
        Neq        => r#"{"k":"Neq"}"#.to_string(),
        Lt         => r#"{"k":"Lt"}"#.to_string(),
        Le         => r#"{"k":"Le"}"#.to_string(),
        Gt         => r#"{"k":"Gt"}"#.to_string(),
        Ge         => r#"{"k":"Ge"}"#.to_string(),
        Plus       => r#"{"k":"Plus"}"#.to_string(),
        PlusPlus   => r#"{"k":"PlusPlus"}"#.to_string(),
        Minus      => r#"{"k":"Minus"}"#.to_string(),
        Star       => r#"{"k":"Star"}"#.to_string(),
        Slash      => r#"{"k":"Slash"}"#.to_string(),
        And        => r#"{"k":"And"}"#.to_string(),
        Or         => r#"{"k":"Or"}"#.to_string(),
        Not        => r#"{"k":"Not"}"#.to_string(),
        Implies    => r#"{"k":"Implies"}"#.to_string(),
        LParen     => r#"{"k":"LParen"}"#.to_string(),
        RParen     => r#"{"k":"RParen"}"#.to_string(),
        LBrace     => r#"{"k":"LBrace"}"#.to_string(),
        RBrace     => r#"{"k":"RBrace"}"#.to_string(),
        LBracket   => r#"{"k":"LBracket"}"#.to_string(),
        RBracket   => r#"{"k":"RBracket"}"#.to_string(),
        LSeq       => r#"{"k":"LSeq"}"#.to_string(),
        RSeq       => r#"{"k":"RSeq"}"#.to_string(),
        Hash       => r#"{"k":"Hash"}"#.to_string(),
        Comma      => r#"{"k":"Comma"}"#.to_string(),
        Pipe       => r#"{"k":"Pipe"}"#.to_string(),
        Question   => r#"{"k":"Question"}"#.to_string(),
        DotDot     => r#"{"k":"DotDot"}"#.to_string(),
        Dot        => r#"{"k":"Dot"}"#.to_string(),
        Colon      => r#"{"k":"Colon"}"#.to_string(),
        ForAll     => r#"{"k":"ForAll"}"#.to_string(),
        Exists     => r#"{"k":"Exists"}"#.to_string(),
        MapsTo     => r#"{"k":"MapsTo"}"#.to_string(),
        Newline    => r#"{"k":"Newline"}"#.to_string(),
        Indent(n)  => format!(r#"{{"k":"Indent","v":{}}}"#, n),
        Eof        => r#"{"k":"Eof"}"#.to_string(),
    }
}

fn cmd_run(args: &[String]) -> ExitCode {
    let Some(file) = args.first() else { usage(); return ExitCode::from(2); };
    let Some(claim) = args.get(1) else { usage(); return ExitCode::from(2); };

    let Some(rt) = load(file) else { return ExitCode::from(1); };
    let smt = match evident_runtime::emit::emit_kernel_smtlib(&rt, claim) {
        Ok(s) => s,
        Err(e) => { eprintln!("emit: {e}"); return ExitCode::from(1); }
    };

    // Write SMT-LIB to a temp file and exec the kernel binary.
    let pid = std::process::id();
    let tmp = format!("/tmp/evident-run-{pid}.smt2");
    if let Err(e) = std::fs::write(&tmp, &smt) {
        eprintln!("run: write {tmp}: {e}");
        return ExitCode::from(1);
    }

    // Locate the kernel binary: try $EVIDENT_KERNEL then PATH then a few defaults.
    let kernel = std::env::var("EVIDENT_KERNEL").unwrap_or_else(|_| {
        // sibling of this binary at <root>/kernel/target/release/kernel
        match std::env::current_exe().ok().and_then(|p| {
            p.parent().and_then(|d| d.parent()).and_then(|d| d.parent()).and_then(|d| d.parent())
                .map(|root| root.join("kernel").join("target").join("release").join("kernel"))
        }) {
            Some(p) if p.exists() => p.to_string_lossy().into_owned(),
            _ => "kernel".to_string(),  // hope it's on PATH
        }
    });

    let status = std::process::Command::new(&kernel).arg(&tmp).status();
    let _ = std::fs::remove_file(&tmp);
    match status {
        Ok(s) => {
            let code = s.code().unwrap_or(128) as u8;
            ExitCode::from(code)
        }
        Err(e) => {
            eprintln!("run: exec {kernel}: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_emit(args: &[String]) -> ExitCode {
    let Some(file) = args.first() else { usage(); return ExitCode::from(2); };
    let Some(claim) = args.get(1) else { usage(); return ExitCode::from(2); };
    let mut out_path: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "-o" && i + 1 < args.len() {
            out_path = Some(args[i + 1].clone());
            i += 2;
        } else {
            i += 1;
        }
    }
    let Some(rt) = load(file) else { return ExitCode::from(1); };
    match evident_runtime::emit::emit_kernel_smtlib(&rt, claim) {
        Ok(s) => {
            match out_path {
                Some(p) => {
                    if let Err(e) = std::fs::write(&p, &s) {
                        eprintln!("emit: write {p}: {e}");
                        return ExitCode::from(1);
                    }
                }
                None => { print!("{s}"); }
            }
            ExitCode::SUCCESS
        }
        Err(e) => { eprintln!("emit: {e}"); ExitCode::from(1) }
    }
}
