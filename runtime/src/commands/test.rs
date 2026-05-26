//! `evident test [path]` — discover and run `sat_*`/`unsat_*` claims in `test_*.ev` files.
//! Color: auto when TTY; disable with `--no-color` or `NO_COLOR` (per no-color.org).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use evident_runtime::ast::BodyItem;
// Trace tests removed (trace_runner crate deleted in Phase 2 plugin removal).
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::translate::collect_referenced_names;
use evident_runtime::pretty;

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

/// `NO_COLOR` (any non-empty value) disables color; also requires stdout TTY.
fn stdout_supports_color() -> bool {
    if std::env::var("NO_COLOR").map(|v| !v.is_empty()).unwrap_or(false) {
        return false;
    }
    use std::os::fd::AsRawFd;
    // SAFETY: fd 1 is stdout; isatty() is read-only and thread-safe.
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
enum TestKind {
    Schema { expected_sat: bool },
}

#[derive(Debug)]
enum FailDetail {
    UnsatCore { core_indices: Vec<usize> },
    SatCounterexample(HashMap<String, Value>),
}

#[derive(Debug)]
enum Outcome { Pass, Fail(FailDetail), Error(String) }

#[derive(Debug)]
struct TestRun {
    file:       PathBuf,
    name:       String,
    kind:       TestKind,
    outcome:    Outcome,
    elapsed_ms: u32,
}

pub fn cmd_test(args: &[String]) -> ExitCode {
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
                kind: TestKind::Schema { expected_sat: true },
                outcome: Outcome::Error(format!("load: {e}")),
                elapsed_ms: 0,
            });
            {
                live_file_header(&opts, &mut prev_file, f);
                live_emit(&opts, &runs[runs.len() - 1]);
            }
            continue;
        }

        super::common::auto_apply_desugar(&mut rt, &[f.to_string_lossy().to_string()]);

        let mut names: Vec<String> = rt.schema_names()
            .filter(|n| n.starts_with("sat_") || n.starts_with("unsat_"))
            .map(|s| s.to_string()).collect();
        names.sort();
        let empty = HashMap::new();

        for name in &names {
            let expected_sat = name.starts_with("sat_");
            let t0 = Instant::now();
            // sat_* uses query_with_core (shows conflict); unsat_* uses standard query (shows bindings).
            let outcome = if expected_sat {
                match rt.query_with_core(name, &empty) {
                    Ok((r, _)) if r.satisfied => Outcome::Pass,
                    Ok((_, core)) => Outcome::Fail(FailDetail::UnsatCore {
                        core_indices: core.unwrap_or_default(),
                    }),
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
                kind: TestKind::Schema { expected_sat },
                outcome, elapsed_ms: t0.elapsed().as_millis() as u32,
            });
            {
                live_file_header(&opts, &mut prev_file, f);
                live_emit(&opts, runs.last().unwrap());
            }
        }

        // Trace tests removed (no plugin runner).
    }

    let elapsed_ms = started.elapsed().as_millis() as u32;
    report_human(&runs, &opts, elapsed_ms);

    let any_fail = runs.iter().any(|r| !matches!(r.outcome, Outcome::Pass));
    if any_fail { ExitCode::from(1) } else { ExitCode::SUCCESS }
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

/// Print file header before first test in verbose mode (compact mode shows dots only).
fn live_file_header(opts: &Opts, prev: &mut Option<PathBuf>, f: &Path) {
    if !opts.verbose { return; }
    if prev.as_deref() == Some(f) { return; }
    *prev = Some(f.to_path_buf());
    println!("{}:", dim(opts.use_color, &f.display().to_string()));
}

/// Emit per-test: dot in compact mode, PASS/FAIL line in verbose mode.
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
        Outcome::Fail(FailDetail::UnsatCore { core_indices }) => {
            println!("    expected {}, got {}",
                green(oc, "SAT"), red(oc, "UNSAT"));
            print_unsat_core(run, core_indices, opts);
        }
        Outcome::Fail(FailDetail::SatCounterexample(bindings)) => {
            print_counterexample(run, bindings, opts);
        }
    }
}

/// Print conflicting body items from UNSAT core. Empty core = Z3 couldn't
/// pinpoint the conflict (built-in axiom or non-splittable item).
fn print_unsat_core(run: &TestRun, core_indices: &[usize], opts: &Opts) {
    let oc = opts.use_color;
    if core_indices.is_empty() {
        println!("    {}",
            dim(oc, "(Z3 returned no specific conflict — try EVIDENT_LENIENT=0 to surface dropped constraints)"));
        return;
    }
    let mut rt = EvidentRuntime::new();
    if rt.load_file(&run.file).is_err() { return; }
    let Some(schema) = rt.get_schema(&run.name) else { return };
    println!("    {}", dim(oc, "conflicting constraints:"));
    for &i in core_indices {
        if let Some(item) = schema.body.get(i) {
            let text = pretty::body_item(item);
            println!("      {}", highlight_constraint(&text, oc));
        }
    }
}

/// Print SAT counterexample: each body constraint + its referenced binding values.
/// Flattens SeqComposite/Composite so `coins[0].collected` shows as a scalar.
fn print_counterexample(
    run: &TestRun, bindings: &HashMap<String, Value>, opts: &Opts,
) {
    let oc = opts.use_color;
    println!("    expected {}, got {} — {}",
        red(oc, "UNSAT"), green(oc, "SAT"),
        dim(oc, "counterexample:"));

    // Re-load the file (print_failure doesn't hold a runtime handle; re-loading is cheap).
    let mut rt = EvidentRuntime::new();
    if rt.load_file(&run.file).is_err() {
        return dump_raw_bindings(bindings, opts); // fallback if file was edited mid-run
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
                pretty::body_item(item)
            }
            BodyItem::ClaimCall { .. } => pretty::body_item(item),
                _ => continue, // declarations, passthroughs, subclaim decls — not assertions
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
        // No constraint-shaped items (schema is all declarations); fall back to raw dump.
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

/// True for leaf values; Composite/SeqComposite are containers whose leaves carry the info.
fn is_leaf_value(v: &Value) -> bool {
    !matches!(v, Value::Composite(_) | Value::SeqComposite(_))
}

/// `#cur = 1` style length pins: load-bearing for the solver but uninteresting in reports.
fn is_cardinality_pin(e: &evident_runtime::ast::Expr) -> bool {
    use evident_runtime::ast::{BinOp, Expr};
    matches!(e,
        Expr::Binary(BinOp::Eq, lhs, _) if matches!(lhs.as_ref(), Expr::Cardinality(_))
    )
}

/// Whether binding key matches ref `r`: exact, dotted child (`r.…`), or indexed child (`r[…]`).
fn matches_ref(key: &str, r: &str) -> bool {
    if key == r { return true; }
    if let Some(rest) = key.strip_prefix(r) {
        return rest.starts_with('.') || rest.starts_with('[');
    }
    false
}

/// Expand Composite/SeqComposite into per-leaf keys alongside originals for scalar matching.
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

/// Collect env-key names referenced by a body item. ClaimCall captures mapping values;
/// slot names are internal to the called claim.
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

/// ANSI highlighter: Unicode operators → bold-white; strings → yellow;
/// lowercase identifiers → blue; uppercase-initial → cyan. Not a full lexer.
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

/// Compact value for counterexample output; Seq/Composite types show `Type[N]` only.
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
