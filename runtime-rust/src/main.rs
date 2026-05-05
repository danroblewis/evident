//! `evident` — CLI for the Rust port of the Evident runtime.
//! Mirrors `evident.py`'s subcommand shape so the two tools are
//! interchangeable for everything the Rust runtime currently supports.
//!
//! Subcommands:
//!   query   <files…> <schema> [--given k=v …] [--json]
//!   check   <files…>
//!   sample  <files…> <schema> [-n N] [--given k=v …] [--json]
//!   test    [path]
//!   execute <file>          — run schema main as a constraint automaton
//!                             (headless: stdin → solver → stdout)
//!   parse   <file>          — Rust-only, debug helper
//!
//! Parked behind plugin work (covered by the Python `evident.py` but
//! not yet by this binary):
//!   batch     — stdin ↔ Seq round-trip
//!   repl      — interactive session
//!
//! Notes on `execute`:
//!   - v1 is HEADLESS: stdin/stdout only. No SDL, no TCP, no batch.
//!   - The runtime auto-loads a small embedded io stdlib defining
//!     Stdin / Stdout / CharInput / CharOutput / Stderr (flat, with
//!     just the fields the executor populates). Programs that import
//!     these via `..` passthrough won't work because the Rust parser
//!     doesn't yet handle `import` statements.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use evident_runtime::{executor, EvidentRuntime, QueryResult, Value};
use evident_runtime::executor::Plugin;
use evident_runtime::plugins::sdl as sdl_plugin;
use evident_runtime::ast::BodyItem;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
        return ExitCode::from(2);
    }
    match args[0].as_str() {
        "query"   => cmd_query(&args[1..]),
        "check"   => cmd_check(&args[1..]),
        "sample"  => cmd_sample(&args[1..]),
        "test"    => cmd_test(&args[1..]),
        "parse"   => cmd_parse(&args[1..]),
        "execute" => cmd_execute(&args[1..]),
        "batch" | "repl" => {
            eprintln!("error: '{}' is not yet implemented in the Rust runtime.", args[0]);
            eprintln!("       Use evident.py for these subcommands. See PROGRESS.md for status.");
            ExitCode::from(2)
        }
        "help" | "--help" | "-h" => { usage(); ExitCode::SUCCESS }
        other => {
            eprintln!("unknown subcommand: {}", other);
            usage();
            ExitCode::from(2)
        }
    }
}

fn usage() {
    eprintln!("usage:");
    eprintln!("  evident query   <files…> <schema> [--given k=v …] [--json]");
    eprintln!("  evident check   <files…>");
    eprintln!("  evident sample  <files…> <schema> [-n N] [--given k=v …] [--json]");
    eprintln!("  evident test    [path]");
    eprintln!("  evident execute <file> [--width N] [--height N] [--title S]");
    eprintln!("                         [--host H] [--port P]");
    eprintln!("  evident parse   <file>");
    eprintln!();
    eprintln!("execute flags (mirror evident.py):");
    eprintln!("  --width  N   SDL window width  (default 800; used by SDL plugin)");
    eprintln!("  --height N   SDL window height (default 600; used by SDL plugin)");
    eprintln!("  --title  S   SDL window title  (default \"Evident\")");
    eprintln!("  --host   H   TCP listen host   (default 127.0.0.1; reserved for TCP plugin)");
    eprintln!("  --port   P   TCP listen port   (default 8080;       reserved for TCP plugin)");
    eprintln!();
    eprintln!("not yet implemented (use evident.py):");
    eprintln!("  evident batch|repl …");
}

// ---------------------------------------------------------------------------
// Argument-parsing helpers
// ---------------------------------------------------------------------------

/// Split positional file paths from flag arguments. Files are everything
/// before the first `--…` flag. Returns `(files, flags)`.
fn split_files_and_flags(args: &[String]) -> (Vec<String>, Vec<String>) {
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() && !args[i].starts_with('-') {
        files.push(args[i].clone());
        i += 1;
    }
    (files, args[i..].to_vec())
}

/// Parse `--given k=v k2=v2 …` (consecutive k=v args after `--given`)
/// and `--json`. Unknown flags trigger an error.
struct Flags {
    given: HashMap<String, Value>,
    json: bool,
    n_samples: usize,
}

impl Default for Flags {
    fn default() -> Self {
        Flags { given: HashMap::new(), json: false, n_samples: 5 }
    }
}

fn parse_flags(flags: &[String]) -> Result<Flags, String> {
    let mut out = Flags::default();
    let mut i = 0;
    while i < flags.len() {
        match flags[i].as_str() {
            "--given" => {
                i += 1;
                while i < flags.len() && !flags[i].starts_with('-') {
                    let pair = &flags[i];
                    let (k, v) = pair.split_once('=')
                        .ok_or_else(|| format!("bad --given {pair:?}: need key=value"))?;
                    out.given.insert(k.to_string(), infer_value(v));
                    i += 1;
                }
            }
            "--json" => { out.json = true; i += 1; }
            "-n" => {
                i += 1;
                let n = flags.get(i)
                    .ok_or_else(|| "-n needs a number".to_string())?
                    .parse::<usize>()
                    .map_err(|e| format!("bad -n: {e}"))?;
                out.n_samples = n;
                i += 1;
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(out)
}

fn infer_value(v: &str) -> Value {
    if v == "true" { Value::Bool(true) }
    else if v == "false" { Value::Bool(false) }
    else if let Ok(n) = v.parse::<i64>() { Value::Int(n) }
    else { Value::Str(v.to_string()) }
}

/// Flags accepted by the `execute` subcommand. Mirrors the argparse
/// declarations in `evident.py` (`ex.add_argument('--width', …)` etc.).
///
/// `width` / `height` / `title` are consumed by the SDL plugin; `host`
/// / `port` are reserved for a future TCP-socket plugin. Parsing them
/// today (even though the plugins they target may not be wired in yet)
/// keeps the CLI surface stable so adding the plugin doesn't have to
/// retouch arg-parsing.
#[allow(dead_code)]
struct ExecuteOpts {
    width:  u32,
    height: u32,
    title:  String,
    host:   String,
    port:   u16,
}

impl Default for ExecuteOpts {
    fn default() -> Self {
        ExecuteOpts {
            width:  800,
            height: 600,
            title:  "Evident".to_string(),
            host:   "127.0.0.1".to_string(),
            port:   8080,
        }
    }
}

fn parse_execute_flags(flags: &[String]) -> Result<ExecuteOpts, String> {
    let mut out = ExecuteOpts::default();
    let mut i = 0;
    while i < flags.len() {
        match flags[i].as_str() {
            "--width" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--width needs a value".to_string())?;
                out.width = v.parse::<u32>().map_err(|e| format!("bad --width {v:?}: {e}"))?;
                i += 1;
            }
            "--height" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--height needs a value".to_string())?;
                out.height = v.parse::<u32>().map_err(|e| format!("bad --height {v:?}: {e}"))?;
                i += 1;
            }
            "--title" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--title needs a value".to_string())?;
                out.title = v.clone();
                i += 1;
            }
            "--host" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--host needs a value".to_string())?;
                out.host = v.clone();
                i += 1;
            }
            "--port" => {
                i += 1;
                let v = flags.get(i).ok_or_else(|| "--port needs a value".to_string())?;
                out.port = v.parse::<u16>().map_err(|e| format!("bad --port {v:?}: {e}"))?;
                i += 1;
            }
            "--help" | "-h" => {
                usage();
                std::process::exit(0);
            }
            other => return Err(format!("unknown execute flag: {other}")),
        }
    }
    Ok(out)
}

fn load_runtime(files: &[String]) -> Result<EvidentRuntime, String> {
    let mut rt = EvidentRuntime::new();
    for f in files {
        // Use load_file so any `import "..."` statements inside the
        // file resolve relative to the file itself.
        rt.load_file(Path::new(f)).map_err(|e| format!("{f}: {e}"))?;
    }
    Ok(rt)
}

// ---------------------------------------------------------------------------
// query
// ---------------------------------------------------------------------------

fn cmd_query(args: &[String]) -> ExitCode {
    let (files_and_schema, flag_args) = split_files_and_flags(args);
    if files_and_schema.len() < 2 {
        eprintln!("query: need <files…> <schema>");
        return ExitCode::from(2);
    }
    // Last positional is the schema name; the rest are files.
    let schema = files_and_schema.last().unwrap().clone();
    let files: Vec<String> = files_and_schema[..files_and_schema.len() - 1].to_vec();
    let flags = match parse_flags(&flag_args) {
        Ok(f) => f,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
    };
    let rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
    };
    match rt.query(&schema, &flags.given) {
        Ok(r) => print_query_result(&r, flags.json),
        Err(e) => { eprintln!("query error: {e}"); ExitCode::from(1) }
    }
}

fn print_query_result(r: &QueryResult, json: bool) -> ExitCode {
    if json {
        // Minimal JSON: {"satisfied": true/false, "bindings": {…}}
        if !r.satisfied {
            println!("{{\"satisfied\": false}}");
            return ExitCode::from(1);
        }
        let mut keys: Vec<&String> = r.bindings.keys().collect();
        keys.sort();
        let mut parts = Vec::new();
        for k in keys {
            parts.push(format!("\"{}\": {}", k, value_as_json(&r.bindings[k])));
        }
        println!("{{\"satisfied\": true, \"bindings\": {{{}}}}}", parts.join(", "));
        return ExitCode::SUCCESS;
    }
    if !r.satisfied {
        println!("UNSAT");
        return ExitCode::from(1);
    }
    let mut keys: Vec<&String> = r.bindings.keys().collect();
    keys.sort();
    for k in keys {
        println!("{}={}", k, format_value(&r.bindings[k]));
    }
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// check — report SAT/UNSAT for every loaded schema
// ---------------------------------------------------------------------------

fn cmd_check(args: &[String]) -> ExitCode {
    let (files, flag_args) = split_files_and_flags(args);
    if files.is_empty() {
        eprintln!("check: need at least one file");
        return ExitCode::from(2);
    }
    if !flag_args.is_empty() {
        eprintln!("check: doesn't take flags (got {:?})", flag_args);
        return ExitCode::from(2);
    }
    let rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
    };
    let mut names: Vec<String> = rt.schema_names().map(|s| s.to_string()).collect();
    names.sort();
    let empty = HashMap::new();
    let mut any_unsat = false;
    for name in &names {
        match rt.query(name, &empty) {
            Ok(r) if r.satisfied  => println!("SAT    {name}"),
            Ok(_)                 => { println!("UNSAT  {name}"); any_unsat = true; }
            Err(e)                => { println!("ERROR  {name}: {e}"); any_unsat = true; }
        }
    }
    if any_unsat { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

// ---------------------------------------------------------------------------
// sample — generate up to N distinct models via blocking-clause loop
// ---------------------------------------------------------------------------

fn cmd_sample(args: &[String]) -> ExitCode {
    let (files_and_schema, flag_args) = split_files_and_flags(args);
    if files_and_schema.len() < 2 {
        eprintln!("sample: need <files…> <schema>");
        return ExitCode::from(2);
    }
    let schema = files_and_schema.last().unwrap().clone();
    let files: Vec<String> = files_and_schema[..files_and_schema.len() - 1].to_vec();
    let flags = match parse_flags(&flag_args) {
        Ok(f) => f,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(2); }
    };
    let rt = match load_runtime(&files) {
        Ok(r) => r,
        Err(e) => { eprintln!("{e}"); return ExitCode::from(1); }
    };

    // Real blocking-clause sample loop: solver.push(), assert givens,
    // loop check + extract + assert ¬(scalar bindings), pop. Returns
    // up to `-n N` distinct models or stops at UNSAT. See
    // `EvidentRuntime::sample` for limitations (Seq/Set bindings don't
    // contribute to the blocking conjunction).
    let samples: Vec<HashMap<String, Value>> = match rt.sample(&schema, &flags.given, flags.n_samples) {
        Ok(s) => s,
        Err(e) => { eprintln!("sample error: {e}"); return ExitCode::from(1); }
    };

    if flags.json {
        print!("[");
        for (i, s) in samples.iter().enumerate() {
            if i > 0 { print!(", "); }
            let mut keys: Vec<&String> = s.keys().collect(); keys.sort();
            let parts: Vec<_> = keys.iter()
                .map(|k| format!("\"{}\": {}", k, value_as_json(&s[*k])))
                .collect();
            print!("{{{}}}", parts.join(", "));
        }
        println!("]");
    } else {
        for (i, s) in samples.iter().enumerate() {
            println!("--- sample {} ---", i + 1);
            let mut keys: Vec<&String> = s.keys().collect(); keys.sort();
            for k in keys {
                println!("{k}={}", format_value(&s[k]));
            }
        }
    }
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// test — run sat_/unsat_ claims in test_*.ev files
// ---------------------------------------------------------------------------

fn cmd_test(args: &[String]) -> ExitCode {
    let path: PathBuf = match args.first().map(String::as_str) {
        Some(p) => PathBuf::from(p),
        None => PathBuf::from("."),
    };
    let mut files = Vec::new();
    if path.is_file() {
        files.push(path.clone());
    } else if path.is_dir() {
        collect_test_files(&path, &mut files);
    } else {
        eprintln!("test: not a file or directory: {}", path.display());
        return ExitCode::from(2);
    }
    if files.is_empty() {
        eprintln!("test: no test_*.ev files found under {}", path.display());
        return ExitCode::from(0);
    }

    let mut total_pass = 0usize;
    let mut total_fail = 0usize;
    let mut total_skip = 0usize;
    let empty = HashMap::new();
    for f in &files {
        let mut rt = EvidentRuntime::new();
        if let Err(e) = rt.load_file(f) {
            eprintln!("{}: load error: {e}", f.display());
            total_fail += 1;
            continue;
        }
        let mut names: Vec<String> = rt.schema_names()
            .filter(|n| n.starts_with("sat_") || n.starts_with("unsat_"))
            .map(|s| s.to_string()).collect();
        names.sort();
        if names.is_empty() {
            total_skip += 1;
            continue;
        }
        println!("{}:", f.display());
        for name in &names {
            let want_sat = name.starts_with("sat_");
            match rt.query(name, &empty) {
                Ok(r) if r.satisfied == want_sat => {
                    println!("  PASS  {}", name);
                    total_pass += 1;
                }
                Ok(r) => {
                    println!("  FAIL  {}  (expected {} got {})",
                        name,
                        if want_sat { "SAT" } else { "UNSAT" },
                        if r.satisfied { "SAT" } else { "UNSAT" });
                    total_fail += 1;
                }
                Err(e) => {
                    println!("  ERROR {}  ({e})", name);
                    total_fail += 1;
                }
            }
        }
    }
    println!();
    println!("{} passed, {} failed, {} files skipped (no sat_/unsat_ claims)",
             total_pass, total_fail, total_skip);
    if total_fail > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

fn collect_test_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_test_files(&p, out);
        } else if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
            if name.starts_with("test_") && name.ends_with(".ev") {
                out.push(p);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// execute — run schema main as a constraint automaton (headless v1)
// ---------------------------------------------------------------------------

fn cmd_execute(args: &[String]) -> ExitCode {
    // `--help` first, before file-positional check, so `execute --help`
    // works without needing a file argument.
    if args.iter().any(|a| a == "--help" || a == "-h") {
        usage();
        return ExitCode::SUCCESS;
    }
    if args.is_empty() {
        eprintln!("execute: need <file.ev>");
        return ExitCode::from(2);
    }
    // First positional is the file; everything after is flags.
    let path = &args[0];
    let opts = match parse_execute_flags(&args[1..]) {
        Ok(o) => o,
        Err(e) => { eprintln!("execute: {e}"); return ExitCode::from(2); }
    };
    let mut rt = EvidentRuntime::new();
    // Load embedded stdlibs first so user programs can declare
    // ∈ Stdin / ∈ Stdout / ∈ SDLInput etc. without `import`. Both are
    // flat shims (no `..` passthrough chains) since the Rust runtime
    // doesn't yet recurse into `..` during sub-schema field expansion.
    if let Err(e) = executor::load_io_stdlib(&mut rt) {
        eprintln!("execute: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = rt.load_source(sdl_plugin::STDLIB_SDL_EV) {
        eprintln!("execute: sdl stdlib: {e}");
        return ExitCode::from(1);
    }
    // Use load_file so `import "..."` statements in the user program
    // resolve relative to the file's own directory.
    if let Err(e) = rt.load_file(Path::new(path)) {
        eprintln!("execute: {path}: {e}");
        return ExitCode::from(1);
    }

    // Inspect main's body to find SDL var declarations. If any are
    // present, instantiate the SDL plugin and add it to the plugin
    // list. Otherwise, fall back to the headless stdin/stdout path.
    let sdl_vars = collect_sdl_vars(&rt);

    if sdl_vars.is_empty() {
        // Pure headless: stdin/stdout only.
        match executor::run_headless(&rt, std::io::stdin(), std::io::stdout()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => { eprintln!("execute: {e}"); ExitCode::from(1) }
        }
    } else {
        // SDL active: defaults from --width/--height/--title (else
        // 800×600 "Evident" — same defaults as evident.py).
        let sdl = sdl_plugin::create_sdl_plugin(
            opts.width, opts.height, opts.title.clone(), sdl_vars);
        let stdin = executor::StdinPlugin::new(std::io::stdin());
        let stdout = executor::StdoutPlugin::new(std::io::stdout());
        let mut plugins: Vec<Box<dyn Plugin>> = vec![Box::new(stdin), Box::new(stdout), sdl];
        match executor::run_with_plugins(&rt, &mut plugins) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => { eprintln!("execute: {e}"); ExitCode::from(1) }
        }
    }
}

/// Walk `main`'s body (including `..` passthroughs) collecting variables
/// whose declared type is one of the SDL types. Returns the same
/// `var → type_name` map shape that `SDLPlugin` needs in `var_types`.
fn collect_sdl_vars(rt: &EvidentRuntime) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(main) = rt.get_schema("main") else { return out };
    let mut visited: Vec<String> = Vec::new();
    walk_body_for_sdl(rt, &main.name, &mut visited, &mut out);
    out
}

fn walk_body_for_sdl(
    rt: &EvidentRuntime,
    schema_name: &str,
    visited: &mut Vec<String>,
    out: &mut HashMap<String, String>,
) {
    if visited.iter().any(|n| n == schema_name) {
        return;
    }
    visited.push(schema_name.to_string());
    let Some(schema) = rt.get_schema(schema_name) else { return };
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name } => {
                if sdl_plugin::SDL_TYPES.iter().any(|t| *t == type_name.as_str()) {
                    out.entry(name.clone()).or_insert_with(|| type_name.clone());
                }
            }
            BodyItem::Passthrough(claim) => {
                walk_body_for_sdl(rt, claim, visited, out);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// parse — debug helper, not in evident.py
// ---------------------------------------------------------------------------

fn cmd_parse(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("parse: need <file.ev>");
        return ExitCode::from(2);
    }
    let path = &args[0];
    let mut rt = EvidentRuntime::new();
    match rt.load_file(Path::new(path)) {
        Ok(()) => {
            for s in rt.schema_names() { println!("{}", s); }
            ExitCode::SUCCESS
        }
        Err(e) => { eprintln!("parse error: {e}"); ExitCode::from(1) }
    }
}

// ---------------------------------------------------------------------------
// Value formatting
// ---------------------------------------------------------------------------

fn format_value(v: &Value) -> String {
    match v {
        Value::Int(n)  => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s)  => format!("{:?}", s),
        Value::SeqInt(v)  => format!("{:?}", v),
        Value::SeqBool(v) => format!("{:?}", v),
        Value::SeqStr(v)  => format!("{:?}", v),
        // Composite / SeqComposite are placeholder Value variants that
        // aren't currently produced by the translator (sub-schema
        // expansion still emits one leaf per field). Render with Debug
        // until first-class formatting lands.
        other => format!("{:?}", other),
    }
}

fn value_as_json(v: &Value) -> String {
    match v {
        Value::Int(n)  => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s)  => json_str(s),
        Value::SeqInt(v) => {
            let parts: Vec<_> = v.iter().map(|n| n.to_string()).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::SeqBool(v) => {
            let parts: Vec<_> = v.iter().map(|b| b.to_string()).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::SeqStr(v) => {
            let parts: Vec<_> = v.iter().map(|s| json_str(s)).collect();
            format!("[{}]", parts.join(", "))
        }
        // See note in format_value: not yet produced by the translator.
        other => json_str(&format!("{:?}", other)),
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c    => out.push(c),
        }
    }
    out.push('"');
    out
}

#[allow(dead_code)]
fn _writer_avoid_warn(w: &mut dyn Write, b: &[u8]) -> std::io::Result<()> { w.write_all(b) }
