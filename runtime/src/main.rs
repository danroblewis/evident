use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;
use evident_runtime::ast::BodyItem;
use evident_runtime::encode::collect_referenced_names;
use evident_runtime::{EvidentRuntime, Value, trampoline};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
        return ExitCode::from(2);
    }
    match args[0].as_str() {
        "test"        => cmd_test(&args[1..]),
        "effect-run"  => cmd_effect_run(&args[1..]),
        "export"      => cmd_export(&args[1..]),
        "query"       => cmd_query(&args[1..]),
        "fmt"         => cmd_fmt(&args[1..]),
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
    eprintln!("  evident test         [path] [-v] [--no-color]");
    eprintln!("  evident effect-run   <file>           # run an effect-driven program");
    eprintln!("  evident export       <file> [--out PREFIX]  # dump transition SMT-LIB + schema JSON");
    eprintln!("  evident query        <file> [claim] [--given NAME=VALUE]... [--json]  # solve a claim, print a witness");
    eprintln!("  evident fmt          <file>... [--write] [--check]  # gofmt-style formatter");
}

/// `evident fmt <file>... [--write] [--check]` — a gofmt-style formatter.
///
/// Default: print the formatted source to stdout (single file) and leave files
/// untouched. `--write` rewrites each file in place. `--check` exits non-zero if
/// any file is not already formatted (prints the offending paths), writing
/// nothing — for CI. The formatter is comment-preserving and self-verifying: it
/// refuses to emit output that isn't token-equivalent to the input.
fn cmd_fmt(args: &[String]) -> ExitCode {
    let mut paths: Vec<String> = Vec::new();
    let mut write = false;
    let mut check = false;
    for a in args {
        match a.as_str() {
            "--write" | "-w" => write = true,
            "--check"        => check = true,
            "-h" | "--help"  => {
                eprintln!("usage: evident fmt <file>... [--write] [--check]");
                eprintln!("  default      print formatted source to stdout");
                eprintln!("  --write, -w  rewrite each file in place");
                eprintln!("  --check      exit non-zero if any file is unformatted (writes nothing)");
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("fmt: unknown flag {other:?}");
                return ExitCode::from(2);
            }
            other => paths.push(other.to_string()),
        }
    }
    if paths.is_empty() {
        eprintln!("fmt: need at least one file path");
        return ExitCode::from(2);
    }
    if write && check {
        eprintln!("fmt: --write and --check are mutually exclusive");
        return ExitCode::from(2);
    }
    if paths.len() > 1 && !write && !check {
        eprintln!("fmt: refusing to print multiple files to stdout — pass --write or --check");
        return ExitCode::from(2);
    }

    let mut had_error = false;
    let mut needs_format = false;
    for p in &paths {
        let src = match std::fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) => { eprintln!("fmt: read {p}: {e}"); had_error = true; continue; }
        };
        let formatted = match evident_runtime::fmt::format_source(&src) {
            Ok(f) => f,
            Err(e) => { eprintln!("fmt: {p}: {e}"); had_error = true; continue; }
        };
        if check {
            if formatted != src {
                println!("{p}");
                needs_format = true;
            }
        } else if write {
            if formatted != src {
                if let Err(e) = std::fs::write(p, &formatted) {
                    eprintln!("fmt: write {p}: {e}"); had_error = true; continue;
                }
                eprintln!("formatted {p}");
            }
        } else {
            print!("{formatted}");
        }
    }

    if had_error { ExitCode::from(1) }
    else if check && needs_format { ExitCode::from(1) }
    else { ExitCode::SUCCESS }
}

const STDLIB_RUNTIME: &str = "stdlib/runtime.ev";

fn print_help() {
    eprintln!("Usage: evident effect-run <file> [flags]");
    eprintln!();
    eprintln!("Execution:");
    eprintln!("  --max-steps N            cap the scheduler at N ticks (default: 10000)");
    eprintln!();
    eprintln!("Misc:");
    eprintln!("  -h, --help               this message");
}

fn cmd_effect_run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        print_help();
        return ExitCode::from(2);
    }
    if args.iter().any(|a| matches!(a.as_str(), "-h" | "--help")) {
        print_help();
        return ExitCode::SUCCESS;
    }
    let mut path: Option<String> = None;
    let mut max_steps = 10_000usize;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--max-steps" => {
                i += 1;
                let v = args.get(i).and_then(|s| s.parse().ok())
                    .unwrap_or(10_000);
                max_steps = v;
            }
            "-h" | "--help" => {
                print_help();
                return ExitCode::SUCCESS;
            }
            other if other.starts_with("--") || other.starts_with('-') => {
                eprintln!("effect-run: unknown flag {other:?}");
                eprintln!("Run `evident effect-run --help` for the flag list.");
                return ExitCode::from(2);
            }
            other => {

                if path.is_some() {
                    eprintln!("effect-run: multiple program paths given: {:?}, {:?}",
                              path.unwrap(), other);
                    return ExitCode::from(2);
                }
                path = Some(other.to_string());
            }
        }
        i += 1;
    }

    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_file(Path::new(STDLIB_RUNTIME)) {
        eprintln!("effect-run: load {STDLIB_RUNTIME}: {e}");
        return ExitCode::from(1);
    }
    let Some(path) = path else {
        eprintln!("effect-run: need a program path");
        eprintln!("Run `evident effect-run --help` for the flag list.");
        return ExitCode::from(2);
    };
    if let Err(e) = rt.load_file(Path::new(&path)) {
        eprintln!("effect-run: load {path}: {e}");
        return ExitCode::from(1);
    }

    match trampoline::run(&rt, &trampoline::LoopOpts { max_steps }) {
        Ok(r) => {

            if let Some(code) = r.exit_code {
                let clamped = code.clamp(0, 255) as u8;
                return ExitCode::from(clamped);
            }
            if !r.halted_clean {
                eprintln!("effect-run: did not halt cleanly after {} steps", r.steps);
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("effect-run: {e}");
            ExitCode::from(1)
        }
    }
}

/// `evident export <file> [--out PREFIX]` — dump the FSM's transition relation as
/// SMT-LIB (`PREFIX.smt2`) + a JSON state schema (`PREFIX.schema.json`) for the
/// Python visualization tools. Replaces the old baked-in `phase-portrait` command.
fn cmd_export(args: &[String]) -> ExitCode {
    let mut path: Option<String> = None;
    let mut out: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => { i += 1; out = args.get(i).cloned(); }
            "-h" | "--help" => {
                eprintln!("usage: evident export <file> [--out PREFIX]");
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("export: unknown flag {other:?}");
                return ExitCode::from(2);
            }
            other => { path = Some(other.to_string()); }
        }
        i += 1;
    }
    let Some(path) = path else {
        eprintln!("export: need a program path");
        return ExitCode::from(2);
    };
    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_file(Path::new(STDLIB_RUNTIME)) {
        eprintln!("export: load {STDLIB_RUNTIME}: {e}");
        return ExitCode::from(1);
    }
    if let Err(e) = rt.load_file(Path::new(&path)) {
        eprintln!("export: load {path}: {e}");
        return ExitCode::from(1);
    }
    let fsm = match trampoline::single_fsm(&rt) {
        Ok(s) => s.claim_name,
        Err(e) => { eprintln!("export: no single fsm in {path}: {e}"); return ExitCode::from(2); }
    };
    let (smt2, json) = match rt.export_transition(&fsm) {
        Ok(x) => x,
        Err(e) => { eprintln!("export: {e}"); return ExitCode::from(1); }
    };
    let prefix = out.unwrap_or_else(|| {
        Path::new(&path).file_stem().and_then(|s| s.to_str()).unwrap_or(&fsm).to_string()
    });
    let smt_path = format!("{prefix}.smt2");
    let json_path = format!("{prefix}.schema.json");
    if let Err(e) = std::fs::write(&smt_path, smt2) {
        eprintln!("export: write {smt_path}: {e}"); return ExitCode::from(1);
    }
    if let Err(e) = std::fs::write(&json_path, json) {
        eprintln!("export: write {json_path}: {e}"); return ExitCode::from(1);
    }
    println!("wrote {smt_path} + {json_path}  (fsm: {fsm})");
    ExitCode::SUCCESS
}

#[derive(Debug, Clone)]
struct Opts {
    path:      PathBuf,
    verbose:   bool,
    use_color: bool,
}

fn parse_opts(args: &[String]) -> Result<Opts, String> {
    let mut path: Option<PathBuf> = None;
    let mut verbose = false;
    let mut use_color = stdout_supports_color();
    for a in args {
        match a.as_str() {
            "-v" | "--verbose" => verbose = true,
            "--no-color"       => use_color = false,
            "--color"          => use_color = true,
            other if other.starts_with("--") => {
                return Err(format!("unknown flag {other:?}"));
            }
            _ => {
                if path.is_some() {
                    return Err(format!("only one path argument supported (extra: {a:?})"));
                }
                path = Some(PathBuf::from(a));
            }
        }
    }
    Ok(Opts { path: path.unwrap_or_else(|| PathBuf::from(".")), verbose, use_color })
}

fn stdout_supports_color() -> bool {
    if std::env::var("NO_COLOR").map(|v| !v.is_empty()).unwrap_or(false) {
        return false;
    }
    use std::os::fd::AsRawFd;

    unsafe { libc_isatty(std::io::stdout().as_raw_fd()) != 0 }
}

extern "C" {
    #[link_name = "isatty"]
    fn libc_isatty(fd: i32) -> i32;
}

const RESET:   &str = "\x1b[0m";
const BOLD:    &str = "\x1b[1m";
const DIM:     &str = "\x1b[2m";
const RED:     &str = "\x1b[91m";
const GREEN:   &str = "\x1b[92m";
const YELLOW:  &str = "\x1b[93m";
const BLUE:    &str = "\x1b[94m";
const CYAN:    &str = "\x1b[96m";

fn paint(on: bool, code: &str, text: &str) -> String {
    if on { format!("{code}{text}{RESET}") } else { text.to_string() }
}
fn red(on: bool, t: &str)    -> String { paint(on, RED, t) }
fn green(on: bool, t: &str)  -> String { paint(on, GREEN, t) }
fn yellow(on: bool, t: &str) -> String { paint(on, YELLOW, t) }
fn cyan(on: bool, t: &str)   -> String { paint(on, CYAN, t) }
fn blue(on: bool, t: &str)   -> String { paint(on, BLUE, t) }
fn dim(on: bool, t: &str)    -> String { paint(on, DIM, t) }
fn bold(on: bool, t: &str)   -> String { paint(on, BOLD, t) }

#[derive(Debug)]
enum FailDetail {

    UnexpectedUnsat,

    SatCounterexample(HashMap<String, Value>),
}

#[derive(Debug)]
enum Outcome { Pass, Fail(FailDetail), Error(String) }

#[derive(Debug)]
struct TestRun {
    file:       PathBuf,
    name:       String,
    outcome:    Outcome,
    elapsed_ms: u32,
}

fn cmd_test(args: &[String]) -> ExitCode {
    let opts = match parse_opts(args) {
        Ok(o)  => o,
        Err(e) => { eprintln!("test: {e}"); return ExitCode::from(2); }
    };

    let mut files = Vec::new();
    if opts.path.is_file() {
        files.push(opts.path.clone());
    } else if opts.path.is_dir() {
        collect_test_files(&opts.path, &mut files);
    } else {
        eprintln!("test: not a file or directory: {}", opts.path.display());
        return ExitCode::from(2);
    }
    files.sort();
    if files.is_empty() {
        {
            eprintln!("test: no test_*.ev files found under {}", opts.path.display());
        }
        return ExitCode::SUCCESS;
    }

    let started = Instant::now();
    let mut runs: Vec<TestRun> = Vec::new();
    let mut prev_file: Option<PathBuf> = None;

    for f in &files {
        let mut rt = EvidentRuntime::new();
        if let Err(e) = rt.load_file(f) {

            runs.push(TestRun {
                file: f.clone(), name: f.display().to_string(),
                outcome: Outcome::Error(format!("load: {e}")),
                elapsed_ms: 0,
            });
            {
                live_file_header(&opts, &mut prev_file, f);
                live_emit(&opts, &runs[runs.len() - 1]);
            }
            continue;
        }

        let mut names: Vec<String> = rt.schema_names()
            .filter(|n| n.starts_with("sat_") || n.starts_with("unsat_"))
            .map(|s| s.to_string()).collect();
        names.sort();
        let empty = HashMap::new();

        for name in &names {
            let expected_sat = name.starts_with("sat_");
            let t0 = Instant::now();

            let outcome = if expected_sat {
                match rt.query(name, &empty) {
                    Ok(r) if r.satisfied => Outcome::Pass,
                    Ok(_) => Outcome::Fail(FailDetail::UnexpectedUnsat),
                    Err(e) => Outcome::Error(format!("{e}")),
                }
            } else {
                match rt.query(name, &empty) {
                    Ok(r) if !r.satisfied => Outcome::Pass,
                    Ok(r) => Outcome::Fail(FailDetail::SatCounterexample(r.bindings)),
                    Err(e) => Outcome::Error(format!("{e}")),
                }
            };
            runs.push(TestRun {
                file: f.clone(), name: name.clone(),
                outcome, elapsed_ms: t0.elapsed().as_millis() as u32,
            });
            {
                live_file_header(&opts, &mut prev_file, f);
                live_emit(&opts, runs.last().unwrap());
            }
        }

    }

    let elapsed_ms = started.elapsed().as_millis() as u32;
    report_human(&runs, &opts, elapsed_ms);

    let any_fail = runs.iter().any(|r| !matches!(r.outcome, Outcome::Pass));
    if any_fail { ExitCode::from(1) } else { ExitCode::SUCCESS }
}

// ─────────────────────────── query: solve a claim, emit a witness ───────────────────
//
// The relational superpower made runnable: `evident query <file> [claim]` solves the
// named claim (or the unique non-test schema) and prints a satisfying assignment, or
// reports UNSAT. `--given NAME=VALUE` pins a variable so the solver fills the rest
// (solve-for-X). Reuses the same encode+solve path as `test` (rt.query), which already
// returns {satisfied, bindings}; this just exposes the witness, with --json for the IDE.

fn cmd_query(args: &[String]) -> ExitCode {
    let mut path: Option<String> = None;
    let mut claim: Option<String> = None;
    let mut givens: Vec<(String, String)> = Vec::new();
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json = true,
            "--given" => {
                i += 1;
                match args.get(i).and_then(|s| s.split_once('=')) {
                    Some((k, v)) => givens.push((k.to_string(), v.to_string())),
                    None => { eprintln!("query: --given expects NAME=VALUE"); return ExitCode::from(2); }
                }
            }
            "-h" | "--help" => {
                eprintln!("usage: evident query <file> [claim] [--given NAME=VALUE]... [--json]");
                return ExitCode::SUCCESS;
            }
            other if other.starts_with('-') => {
                eprintln!("query: unknown flag {other:?}"); return ExitCode::from(2);
            }
            other => {
                if path.is_none() { path = Some(other.to_string()); }
                else if claim.is_none() { claim = Some(other.to_string()); }
                else { eprintln!("query: unexpected argument {other:?}"); return ExitCode::from(2); }
            }
        }
        i += 1;
    }
    let Some(path) = path else { eprintln!("query: no file given"); return ExitCode::from(2); };

    let emit_err = |msg: String| -> ExitCode {
        if json { println!("{{\"ok\":false,\"error\":{}}}", json_string(&msg)); }
        else { eprintln!("query: {msg}"); }
        ExitCode::from(1)
    };

    let mut rt = EvidentRuntime::new();
    if let Err(e) = rt.load_file(Path::new(&path)) {
        return emit_err(format!("load: {e}"));
    }

    let claim = match claim {
        Some(c) => c,
        None => {
            let cands: Vec<String> = rt.schema_names()
                .filter(|n| !n.starts_with("sat_") && !n.starts_with("unsat_"))
                .map(|s| s.to_string()).collect();
            match cands.len() {
                1 => cands.into_iter().next().unwrap(),
                0 => return emit_err("no claim to query (only sat_/unsat_ schemas present)".into()),
                _ => return emit_err(format!("ambiguous — name a claim. candidates: {}", cands.join(", "))),
            }
        }
    };

    let mut given_map: HashMap<String, Value> = HashMap::new();
    for (k, v) in &givens {
        // rt.query can only pin a whole scalar variable. An indexed/field pin (col[0], p.x)
        // would be SILENTLY IGNORED — producing a witness that violates the displayed pin.
        // Reject it loudly instead of lying. (Indexed solve-for-X is a tracked feature.)
        if k.contains('[') || k.contains('.') {
            return emit_err(format!(
                "can't pin an indexed/field element yet ({k:?}); pin a whole scalar variable"));
        }
        given_map.insert(k.clone(), parse_value_literal(v));
    }

    match rt.query(&claim, &given_map) {
        Ok(r) => {
            if json {
                let binds: Vec<String> = {
                    let mut keys: Vec<&String> = r.bindings.keys().collect();
                    keys.sort();
                    keys.iter()
                        .map(|k| format!("{}:{}", json_string(k), value_to_json(&r.bindings[*k])))
                        .collect()
                };
                println!("{{\"ok\":true,\"claim\":{},\"satisfied\":{},\"bindings\":{{{}}}}}",
                    json_string(&claim), r.satisfied, binds.join(","));
            } else if r.satisfied {
                println!("SAT  ({claim})");
                let mut keys: Vec<&String> = r.bindings.keys().collect();
                keys.sort();
                for k in keys { println!("  {k} = {}", value_to_json(&r.bindings[k])); }
            } else {
                println!("UNSAT  ({claim})");
            }
            ExitCode::SUCCESS
        }
        Err(e) => emit_err(format!("{e}")),
    }
}

fn parse_value_literal(s: &str) -> Value {
    if let Ok(i) = s.parse::<i64>() { return Value::Int(i); }
    if s == "true" { return Value::Bool(true); }
    if s == "false" { return Value::Bool(false); }
    if let Ok(f) = s.parse::<f64>() { return Value::Real(f); }
    Value::Str(s.to_string())
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"'  => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_arr<I: Iterator<Item = String>>(it: I) -> String {
    let parts: Vec<String> = it.collect();
    format!("[{}]", parts.join(","))
}

fn json_obj(m: &HashMap<String, Value>) -> String {
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort();
    let parts: Vec<String> = keys.iter()
        .map(|k| format!("{}:{}", json_string(k), value_to_json(&m[*k])))
        .collect();
    format!("{{{}}}", parts.join(","))
}

fn value_to_json(v: &Value) -> String {
    match v {
        Value::Int(i)  => i.to_string(),
        Value::Real(f) => if f.is_finite() { f.to_string() } else { json_string(&f.to_string()) },
        Value::Bool(b) => b.to_string(),
        Value::Str(s)  => json_string(s),
        Value::SeqInt(xs)  => json_arr(xs.iter().map(|x| x.to_string())),
        Value::SeqBool(xs) => json_arr(xs.iter().map(|x| x.to_string())),
        Value::SeqStr(xs)  => json_arr(xs.iter().map(|x| json_string(x))),
        Value::SetInt(xs)  => json_arr(xs.iter().map(|x| x.to_string())),
        Value::SetBool(xs) => json_arr(xs.iter().map(|x| x.to_string())),
        Value::SetStr(xs)  => json_arr(xs.iter().map(|x| json_string(x))),
        Value::Composite(m)     => json_obj(m),
        Value::SeqComposite(xs) => json_arr(xs.iter().map(json_obj)),
        Value::SeqEnum(xs)      => json_arr(xs.iter().map(value_to_json)),
        Value::Enum { variant, fields, .. } => {
            if fields.is_empty() {
                json_string(variant)
            } else {
                let inner: Vec<String> = fields.iter().map(value_to_json).collect();
                json_string(&format!("{}({})", variant, inner.join(", ")))
            }
        }
    }
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

fn live_file_header(opts: &Opts, prev: &mut Option<PathBuf>, f: &Path) {
    if !opts.verbose { return; }
    if prev.as_deref() == Some(f) { return; }
    *prev = Some(f.to_path_buf());
    println!("{}:", dim(opts.use_color, &f.display().to_string()));
}

fn live_emit(opts: &Opts, run: &TestRun) {
    use std::io::Write;
    if opts.verbose {
        let (tag, color) = match &run.outcome {
            Outcome::Pass       => ("PASS",  GREEN),
            Outcome::Fail(_)    => ("FAIL",  RED),
            Outcome::Error(_)   => ("ERROR", RED),
        };
        println!("  {} {} {}",
            paint(opts.use_color, color, tag),
            cyan(opts.use_color, &run.name),
            dim(opts.use_color, &format!("({}ms)", run.elapsed_ms)),
        );
    } else {
        let (ch, color) = match &run.outcome {
            Outcome::Pass       => (".", GREEN),
            Outcome::Fail(_)    => ("F", RED),
            Outcome::Error(_)   => ("E", RED),
        };
        print!("{}", paint(opts.use_color, color, ch));
        let _ = std::io::stdout().flush();
    }
}

fn report_human(runs: &[TestRun], opts: &Opts, elapsed_ms: u32) {
    if !opts.verbose {
        println!();
    }

    let failures: Vec<&TestRun> = runs.iter()
        .filter(|r| !matches!(r.outcome, Outcome::Pass))
        .collect();

    if !failures.is_empty() {
        println!();
        println!("{}", bold(opts.use_color, "FAILURES"));
        println!("{}", dim(opts.use_color, &"─".repeat(60)));
        for run in &failures {
            println!();
            print_failure(run, opts);
        }
        println!();
        println!("{}", dim(opts.use_color, &"─".repeat(60)));
    }

    let (passed, failed, errors) = tally(runs);
    let mut parts = Vec::new();
    parts.push(green(opts.use_color, &format!("{passed} passed")));
    if failed > 0 {
        parts.push(red(opts.use_color, &format!("{failed} failed")));
    }
    if errors > 0 {
        parts.push(red(opts.use_color, &format!("{errors} errors")));
    }
    parts.push(dim(opts.use_color, &format!("in {:.1}s", elapsed_ms as f64 / 1000.0)));
    println!("{}", parts.join("  "));
}

fn print_failure(run: &TestRun, opts: &Opts) {
    let oc = opts.use_color;
    println!("  {} :: {}",
        dim(oc, &run.file.display().to_string()),
        cyan(oc, &run.name),
    );
    match &run.outcome {
        Outcome::Pass => unreachable!(),
        Outcome::Error(msg) => {
            println!("    {} {}", red(oc, "ERROR"), msg);
        }
        Outcome::Fail(FailDetail::UnexpectedUnsat) => {
            println!("    expected {}, got {}",
                green(oc, "SAT"), red(oc, "UNSAT"));
        }
        Outcome::Fail(FailDetail::SatCounterexample(bindings)) => {
            print_counterexample(run, bindings, opts);
        }
    }
}

fn print_counterexample(
    run: &TestRun, bindings: &HashMap<String, Value>, opts: &Opts,
) {
    let oc = opts.use_color;
    println!("    expected {}, got {} — {}",
        red(oc, "UNSAT"), green(oc, "SAT"),
        dim(oc, "counterexample:"));

    let mut rt = EvidentRuntime::new();
    if rt.load_file(&run.file).is_err() {

        return dump_raw_bindings(bindings, opts);
    }
    let Some(schema) = rt.get_schema(&run.name) else {
        return dump_raw_bindings(bindings, opts);
    };

    let flat = flatten_bindings(bindings);

    let mut shown = false;
    for item in &schema.body {
        let constraint_text = match item {
            BodyItem::Constraint(e) => {
                if is_cardinality_pin(e) { continue; }
                item.to_string()
            }
            BodyItem::ClaimCall { .. } => item.to_string(),

            _ => continue,
        };
        shown = true;
        println!("      {}", highlight_constraint(&constraint_text, oc));
        let refs = referenced_names_in(item);
        let mut shown_keys: std::collections::BTreeSet<&String> =
            std::collections::BTreeSet::new();
        for r in &refs {
            for k in flat.keys() {
                if matches_ref(k, r) && is_leaf_value(&flat[k]) {
                    shown_keys.insert(k);
                }
            }
        }
        for k in shown_keys {
            println!("        {} = {}",
                blue(oc, k), yellow(oc, &display_value_compact(&flat[k])));
        }
    }
    if !shown {

        dump_raw_bindings(bindings, opts);
    }
}

fn dump_raw_bindings(bindings: &HashMap<String, Value>, opts: &Opts) {
    let oc = opts.use_color;
    let mut keys: Vec<&String> = bindings.keys()
        .filter(|k| !k.starts_with('_'))
        .collect();
    keys.sort();
    for k in keys {
        println!("      {} = {}",
            blue(oc, k), yellow(oc, &display_value_compact(&bindings[k])));
    }
}

fn is_leaf_value(v: &Value) -> bool {
    !matches!(v, Value::Composite(_) | Value::SeqComposite(_))
}

fn is_cardinality_pin(e: &evident_runtime::ast::Expr) -> bool {
    use evident_runtime::ast::{BinOp, Expr};
    matches!(e,
        Expr::Binary(BinOp::Eq, lhs, _) if matches!(lhs.as_ref(), Expr::Cardinality(_))
    )
}

fn matches_ref(key: &str, r: &str) -> bool {
    if key == r { return true; }
    if let Some(rest) = key.strip_prefix(r) {
        return rest.starts_with('.') || rest.starts_with('[');
    }
    false
}

fn flatten_bindings(b: &HashMap<String, Value>) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for (k, v) in b {
        flatten_value(k, v, &mut out);
    }
    out
}

fn flatten_value(prefix: &str, v: &Value, out: &mut HashMap<String, Value>) {
    out.insert(prefix.to_string(), v.clone());
    match v {
        Value::Composite(map) => {
            for (field, sub) in map {
                let key = format!("{prefix}.{field}");
                flatten_value(&key, sub, out);
            }
        }
        Value::SeqComposite(items) => {
            for (i, map) in items.iter().enumerate() {
                for (field, sub) in map {
                    let key = format!("{prefix}[{i}].{field}");
                    flatten_value(&key, sub, out);
                }
            }
        }
        _ => {}
    }
}

fn referenced_names_in(item: &BodyItem) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    match item {
        BodyItem::Constraint(e) => collect_referenced_names(e, &mut out),
        BodyItem::ClaimCall { mappings, .. } => {
            for m in mappings { collect_referenced_names(&m.value, &mut out); }
        }
        _ => {}
    }
    out
}

fn highlight_constraint(text: &str, on: bool) -> String {
    if !on { return text.to_string(); }
    let ops = [
        "∈", "∉", "⊆", "⊇", "∋", "∧", "∨", "¬", "⇒", "∀", "∃",
        "≠", "≤", "≥", "++", "∪", "∩", "↦",
    ];
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' { i += 1; }
            if i < bytes.len() { i += 1; }
            out.push_str(YELLOW);
            out.push_str(&text[start..i]);
            out.push_str(RESET);
            continue;
        }
        let mut matched_op = false;
        for op in &ops {
            if text[i..].starts_with(op) {
                out.push_str(BOLD);
                out.push_str(op);
                out.push_str(RESET);
                i += op.len();
                matched_op = true;
                break;
            }
        }
        if matched_op { continue; }
        let c = bytes[i];
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len() {
                let cc = bytes[i];
                if cc.is_ascii_alphanumeric() || cc == b'_' || cc == b'.' { i += 1; }
                else { break; }
            }
            let ident = &text[start..i];
            let color = if ident.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
                CYAN
            } else {
                BLUE
            };
            out.push_str(color);
            out.push_str(ident);
            out.push_str(RESET);
        } else {
            out.push(c as char);
            i += 1;
        }
    }
    out
}

fn display_value_compact(v: &Value) -> String {
    match v {
        Value::Int(n)  => n.to_string(),
        Value::Real(r) => r.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s)  => format!("{:?}", s),
        Value::SeqInt(v)       => format!("Seq(Int)[{}]", v.len()),
        Value::SeqBool(v)      => format!("Seq(Bool)[{}]", v.len()),
        Value::SeqStr(v)       => format!("Seq(String)[{}]", v.len()),
        Value::SeqComposite(v) => format!("Seq(struct)[{}]", v.len()),
        Value::SeqEnum(v)      => format!("Seq(enum)[{}]", v.len()),
        Value::SetInt(v)       => format!("Set(Int)[{}]", v.len()),
        Value::SetBool(v)      => format!("Set(Bool)[{}]", v.len()),
        Value::SetStr(v)       => format!("Set(String)[{}]", v.len()),
        Value::Composite(map)  => format!("{{{} fields}}", map.len()),
        Value::Enum { variant, fields, .. } => {
            if fields.is_empty() { variant.clone() }
            else { format!("{}({} fields)", variant, fields.len()) }
        }
    }
}

fn tally(runs: &[TestRun]) -> (usize, usize, usize) {
    let mut pass = 0; let mut fail = 0; let mut err = 0;
    for r in runs {
        match r.outcome {
            Outcome::Pass        => pass += 1,
            Outcome::Fail(_)     => fail += 1,
            Outcome::Error(_)    => err  += 1,
        }
    }
    (pass, fail, err)
}
