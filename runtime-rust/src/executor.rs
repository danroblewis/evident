//! Constraint automaton executor — headless v1.
//!
//! Runs an Evident program as a constraint automaton:
//!   1. Inspect `schema main`'s body to find I/O port variables
//!      (`∈ Stdin` / `∈ Stdout`) and state pairs (`foo` / `foo_next`
//!      of the same non-IO type).
//!   2. Initialize state with type defaults (Nat → 0, Bool → false,
//!      String → "", Seq(_) → empty).
//!   3. Step loop:
//!        a. StdinPlugin reads one char (or EOF) → contributes
//!           `src.char` / `src.eof` / friends as `given` values.
//!        b. StdoutPlugin contributes the structural fields of every
//!           Stdout var (`fd`, `open`, `blocking`, …).
//!        c. Current state fields go in as `state.field` givens.
//!        d. `query_cached("main", given)` runs.
//!        e. SAT → StdoutPlugin writes `dst.out`; state advances from
//!           `state_next.*`.
//!        f. UNSAT → silently skip the step (state preserved).
//!        g. Halt after the EOF step.
//!
//! v1 limitations:
//!   - Headless only. No SDL, no TCP, no batch-mode plugins.
//!   - Single Stdin / Stdout pair. Multiple input or output ports
//!     in one program would be best-effort (last-write wins for
//!     stdout, only first stdin var actually receives chars).
//!   - State pair detection: a variable `foo ∈ T` has a state pair
//!     iff `foo_next ∈ T` is also declared. Type T must NOT be one
//!     of the I/O port types (`Stdin`, `Stdout`, `CharInput`, …).

use crate::ast::{BodyItem, Keyword};
use crate::runtime::EvidentRuntime;
use crate::pretty;
use crate::translate::Value;

use std::collections::HashMap;
use std::io::{self, Read, Write};

// ── Built-in I/O type names ──────────────────────────────────────────────────
// The executor recognises these by name when classifying main's variables.
// The type definitions themselves are baked into STDLIB_IO_EV below; the
// executor loads that into the runtime before user source.

/// Stdin-shaped types: input plugin owns these. Plugin contributes
/// `var.char`, `var.eof`, `var.open`, `var.fd`, `var.blocking`,
/// `var.available` per step.
const INPUT_TYPES: &[&str] = &["Stdin", "CharInput"];

/// Stdout-shaped types: output plugin owns these. Plugin contributes
/// `var.fd`, `var.open`, `var.blocking`, `var.send_buffer`,
/// `var.buffer_size`, `var.buffered`, `var.flushed` per step, then
/// reads `var.out` after the solve.
const OUTPUT_TYPES: &[&str] = &["Stdout", "Stderr", "CharOutput"];

/// Embedded I/O stdlib. Flat type definitions (no `..` passthrough)
/// so `declare_var` allocates one Z3 const per leaf field directly.
/// The Python runtime gets these from `stdlib/io.ev` via composition,
/// but that depends on `..` passthrough recursing inside `declare_var`,
/// which the Rust runtime doesn't currently do for sub-schema expansion.
/// Flattening sidesteps that until we add passthrough-aware declaration.
///
/// The fields here mirror exactly what `stdin_given()` / `stdout_given()`
/// produce so every `given` key resolves to a declared variable.
const STDLIB_IO_EV: &str = "
type Stdin
    fd ∈ Nat
    open ∈ Bool
    blocking ∈ Bool
    available ∈ Nat
    eof ∈ Bool
    char ∈ String

type Stdout
    fd ∈ Nat
    open ∈ Bool
    blocking ∈ Bool
    send_buffer ∈ Nat
    buffer_size ∈ Nat
    buffered ∈ Nat
    flushed ∈ Bool
    out ∈ String

type Stderr
    fd ∈ Nat
    open ∈ Bool
    blocking ∈ Bool
    out ∈ String

type CharInput
    fd ∈ Nat
    open ∈ Bool
    eof ∈ Bool
    char ∈ String

type CharOutput
    fd ∈ Nat
    open ∈ Bool
    out ∈ String
";

/// Load the embedded I/O stdlib into a runtime. Idempotent if you pre-load
/// it manually — `EvidentRuntime::load_source` simply overwrites schemas
/// with the same name. Called by `run_headless` automatically.
pub fn load_io_stdlib(rt: &mut EvidentRuntime) -> Result<(), String> {
    rt.load_source(STDLIB_IO_EV).map_err(|e| format!("io stdlib: {e}"))
}

// ── Plugin trait ─────────────────────────────────────────────────────────────

/// I/O plugin protocol. Each plugin claims one or more Evident type names
/// (`handles_types`); the executor activates a plugin if `main` declares
/// any variable of a matching type.
pub trait Plugin {
    /// Type names this plugin handles. The executor matches against
    /// every variable declared in `main` by type name.
    fn handles_types(&self) -> &'static [&'static str];

    /// Called when this plugin's matched-var set might have changed.
    /// In single-program use, called once at startup. In multi-program
    /// use (`run_with_main_coordinator`), called again after every
    /// swap with the new program's matched vars. Plugins must handle
    /// repeated calls gracefully — typically by checking whether
    /// expensive setup (open window, open audio device) is already
    /// done before re-doing it.
    ///
    /// `matched_vars` is `Vec<(name, type_name)>` filtered to the
    /// types this plugin handles. The type name is included so
    /// plugins like SDL (which dispatches on type — SDLInput vs
    /// SDLOutput vs SDLWindow) can route per-var work without a
    /// separate side-channel.
    fn initialize(&mut self, matched_vars: Vec<(String, String)>);

    /// Called before each solve. Returns `given` values to merge into
    /// the per-step solver inputs. Returning `None` signals halt
    /// (e.g., stdin EOF after the final flush step).
    fn before_step(&mut self) -> Option<HashMap<String, Value>>;

    /// Called after each successful solve. May produce side effects
    /// (write to stdout, etc.). Returns `false` to halt the executor.
    fn after_step(&mut self, _bindings: &HashMap<String, Value>) -> bool { true }
}

// ── Stdin / Stdout plugins ───────────────────────────────────────────────────

/// Reads one char per step from a `Read`. Emits `var.char` (a 1-char string),
/// `var.eof` (Bool), plus the descriptor structural fields. After EOF,
/// emits one final step with `eof=true` and `char=""`, then halts.
pub struct StdinPlugin<R: Read> {
    reader: R,
    matched_vars: Vec<String>,
    /// True once we've delivered the EOF step. The next `before_step`
    /// returns None to halt.
    eof_emitted: bool,
}

impl<R: Read> StdinPlugin<R> {
    pub fn new(reader: R) -> Self {
        StdinPlugin { reader, matched_vars: Vec::new(), eof_emitted: false }
    }
}

impl<R: Read> Plugin for StdinPlugin<R> {
    fn handles_types(&self) -> &'static [&'static str] { INPUT_TYPES }

    fn initialize(&mut self, matched_vars: Vec<(String, String)>) {
        self.matched_vars = matched_vars.into_iter().map(|(n, _)| n).collect();
    }

    fn before_step(&mut self) -> Option<HashMap<String, Value>> {
        if self.eof_emitted {
            return None;  // halt — the EOF step already ran
        }
        let mut buf = [0u8; 1];
        let n = self.reader.read(&mut buf).unwrap_or(0);
        let (eof, ch) = if n == 0 {
            self.eof_emitted = true;
            (true, String::new())
        } else {
            (false, String::from_utf8_lossy(&buf[..1]).into_owned())
        };
        let mut out = HashMap::new();
        for v in &self.matched_vars {
            out.insert(format!("{}.fd", v),        Value::Int(0));
            out.insert(format!("{}.open", v),      Value::Bool(!eof));
            out.insert(format!("{}.blocking", v),  Value::Bool(true));
            out.insert(format!("{}.available", v), Value::Int(if eof { 0 } else { 1 }));
            out.insert(format!("{}.eof", v),       Value::Bool(eof));
            out.insert(format!("{}.char", v),      Value::Str(ch.clone()));
        }
        Some(out)
    }
}

/// Constrains the structural fields of every Stdout var (so the solver
/// doesn't have to choose them) and writes `var.out` to a `Write` after
/// each successful solve.
pub struct StdoutPlugin<W: Write> {
    writer: W,
    matched_vars: Vec<String>,
}

impl<W: Write> StdoutPlugin<W> {
    pub fn new(writer: W) -> Self {
        StdoutPlugin { writer, matched_vars: Vec::new() }
    }

    /// Consume the plugin and return its writer. Useful in tests where
    /// the writer is a `Vec<u8>` that needs inspection after the run.
    pub fn into_writer(self) -> W { self.writer }
}

impl<W: Write> Plugin for StdoutPlugin<W> {
    fn handles_types(&self) -> &'static [&'static str] { OUTPUT_TYPES }

    fn initialize(&mut self, matched_vars: Vec<(String, String)>) {
        self.matched_vars = matched_vars.into_iter().map(|(n, _)| n).collect();
    }

    fn before_step(&mut self) -> Option<HashMap<String, Value>> {
        let mut out = HashMap::new();
        for v in &self.matched_vars {
            out.insert(format!("{}.fd", v),          Value::Int(1));
            out.insert(format!("{}.open", v),        Value::Bool(true));
            out.insert(format!("{}.blocking", v),    Value::Bool(true));
            out.insert(format!("{}.send_buffer", v), Value::Int(0));
            out.insert(format!("{}.buffer_size", v), Value::Int(8192));
            out.insert(format!("{}.buffered", v),    Value::Int(0));
            out.insert(format!("{}.flushed", v),     Value::Bool(true));
        }
        Some(out)
    }

    fn after_step(&mut self, bindings: &HashMap<String, Value>) -> bool {
        for v in &self.matched_vars {
            if let Some(Value::Str(s)) = bindings.get(&format!("{}.out", v)) {
                if !s.is_empty() {
                    let decoded = unescape_z3_string(s);
                    let _ = self.writer.write_all(decoded.as_bytes());
                    let _ = self.writer.flush();
                }
            }
        }
        true
    }
}

/// Z3's `String::as_string()` returns escaped sequences like `\u{a}`
/// for newline, `\u{9}` for tab, etc. — i.e., a backslash followed by
/// `u{HEX}`. This decoder replaces those with the actual code points
/// so `dst.out` written to a file matches what the program "meant".
///
/// Doesn't try to be a full unescaper — only `\u{HEX}` sequences are
/// handled. Anything else (lone `\\`, plain text) passes through.
/// Mirrors what we'd get from a `Value::Str` consumer in Python (the
/// Python z3 binding's `as_string` returns the same escaped form, and
/// the executor's `_extract_output` similarly trusts the value as-is —
/// the difference is Python's stdout text mode handles the literal
/// backslash-escapes on its own. We render bytes directly.)
fn unescape_z3_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Look for `\u{HEX}` (literal backslash + u + { + hex + })
        if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1] == b'u' && bytes[i + 2] == b'{' {
            // Find the closing brace.
            if let Some(end) = bytes[i + 3..].iter().position(|&b| b == b'}') {
                let hex = std::str::from_utf8(&bytes[i + 3..i + 3 + end]).unwrap_or("");
                if let Ok(code) = u32::from_str_radix(hex, 16) {
                    if let Some(c) = char::from_u32(code) {
                        out.push(c);
                        i += 3 + end + 1;
                        continue;
                    }
                }
            }
        }
        // Plain byte: pass through. Treat as one UTF-8 code unit (safe
        // because Rust &str is valid UTF-8 to begin with).
        let c_end = (i + 1..=bytes.len())
            .find(|&j| std::str::from_utf8(&bytes[i..j]).is_ok())
            .unwrap_or(i + 1);
        out.push_str(std::str::from_utf8(&bytes[i..c_end]).unwrap_or(""));
        i = c_end;
    }
    out
}

// ── Schema inspection helpers ────────────────────────────────────────────────

/// Walk a schema body collecting `var → type_name`. Follows `..ClaimName`
/// passthroughs recursively (matching the Python `_collect_vars` behavior),
/// so vars declared in an included claim are visible at the parent level.
fn collect_vars(rt: &EvidentRuntime, schema_name: &str, visited: &mut Vec<String>)
    -> HashMap<String, String>
{
    if visited.iter().any(|n| n == schema_name) {
        return HashMap::new();
    }
    visited.push(schema_name.to_string());
    let Some(schema) = rt.get_schema(schema_name) else { return HashMap::new() };
    let mut out = HashMap::new();
    for item in &schema.body {
        match item {
            BodyItem::Membership { name, type_name } => {
                out.entry(name.clone()).or_insert_with(|| type_name.clone());
            }
            BodyItem::Passthrough(claim) => {
                for (v, t) in collect_vars(rt, claim, visited) {
                    out.entry(v).or_insert(t);
                }
            }
            _ => {}
        }
    }
    out
}

/// Given the declared vars of `main`, identify state pairs: `(base, next, t)`
/// for each `base ∈ T` whose `base_next ∈ T` is also declared, where `T`
/// is NOT one of the I/O port types. Excluding I/O types stops e.g.
/// `dst ∈ Stdout` from being treated as half of a state pair just because
/// some other variable happens to be named `dst_next`.
fn detect_state_pairs(declared: &HashMap<String, String>) -> Vec<(String, String, String)> {
    let io_types: std::collections::HashSet<&str> =
        INPUT_TYPES.iter().chain(OUTPUT_TYPES.iter()).copied().collect();
    let mut pairs = Vec::new();
    for (var, t) in declared {
        if io_types.contains(t.as_str()) { continue; }
        let next = format!("{}_next", var);
        if declared.get(&next).map(|s| s.as_str()) == Some(t.as_str()) {
            pairs.push((var.clone(), next, t.clone()));
        }
    }
    // Stable order so test output is deterministic.
    pairs.sort();
    pairs
}

/// Build the initial state for one variable of type `type_name`. Looks up
/// `type_name` in the runtime's schemas and produces a `field → default-Value`
/// map for every `Membership` field whose type has a sensible default
/// (Nat/Int → 0, Bool → false, String → ""). Sub-schema fields and Seq
/// fields are skipped (no useful default).
fn initial_state(rt: &EvidentRuntime, type_name: &str) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    let Some(schema) = rt.get_schema(type_name) else { return out };
    if !matches!(schema.keyword, Keyword::Type | Keyword::Schema | Keyword::Claim) {
        return out;
    }
    for item in &schema.body {
        if let BodyItem::Membership { name, type_name: ftype } = item {
            if let Some(v) = default_for_type(ftype) {
                out.insert(name.clone(), v);
            }
        }
    }
    out
}

/// Verbose UNSAT dump for a single executor step. Called when the user
/// passed `--explain` to `evident execute`. Mirrors `explain_unsat` in
/// `main.rs` (which is for `evident query`) but pulls the per-step
/// `given` from the executor's loop instead of the CLI's `--given`.
fn explain_step_unsat(rt: &EvidentRuntime, given: &HashMap<String, Value>) {
    let Some(schema) = rt.get_schema("main") else { return };
    eprintln!("--- explain UNSAT step (schema main) ---");
    if !given.is_empty() {
        let mut keys: Vec<&String> = given.keys().collect();
        keys.sort();
        eprintln!("given values:");
        for k in keys {
            eprintln!("  {k} = {}", value_for_diag(&given[k]));
        }
    }
    eprintln!("schema body has {} items:", schema.body.len());
    for (i, item) in schema.body.iter().enumerate() {
        eprintln!("  [{i}] {}", pretty::body_item(item));
    }
    eprintln!("--- end explain ---");
}

/// Compact one-line rendering of a `Value` for UNSAT diagnostics. Not
/// the full pretty-printer in `pretty::expr` — this is for runtime
/// values, not AST exprs. Long Seq/Set values are truncated.
fn value_for_diag(v: &Value) -> String {
    match v {
        Value::Int(n)  => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s)  => format!("\"{}\"", s),
        Value::SeqInt(v)  => format!("Seq(Int)[{}]",  v.len()),
        Value::SeqBool(v) => format!("Seq(Bool)[{}]", v.len()),
        Value::SeqStr(v)  => format!("Seq(String)[{}]", v.len()),
        Value::SeqComposite(v) => format!("Seq(_)[{}]", v.len()),
        other => format!("{:?}", other),
    }
}

fn default_for_type(t: &str) -> Option<Value> {
    match t {
        "Nat" | "Int" | "Pos" => Some(Value::Int(0)),
        "Bool"                => Some(Value::Bool(false)),
        "String"              => Some(Value::Str(String::new())),
        s if s.starts_with("Seq(") => match &s[4..s.len() - 1] {
            "Int"    => Some(Value::SeqInt(Vec::new())),
            "Bool"   => Some(Value::SeqBool(Vec::new())),
            "String" => Some(Value::SeqStr(Vec::new())),
            _ => None,
        },
        _ => None,
    }
}

/// Extract `state_next.*` from `bindings` and return a `field → value` map
/// suitable for replacing the current `base`'s state on the next step.
fn extract_next_state(bindings: &HashMap<String, Value>, next_var: &str)
    -> HashMap<String, Value>
{
    let prefix = format!("{}.", next_var);
    let mut out = HashMap::new();
    for (k, v) in bindings {
        if let Some(field) = k.strip_prefix(&prefix) {
            out.insert(field.to_string(), v.clone());
        }
    }
    out
}

// ── The step loop ────────────────────────────────────────────────────────────

/// Run `schema main` as a constraint automaton with stdin → stdout I/O.
///
/// Loads the embedded I/O stdlib if `main` references types that aren't
/// already declared (you can pre-load your own version via
/// `load_io_stdlib`). Inspects `main`'s body to find Stdin/Stdout vars
/// and state pairs, then loops: read char → solve → write `out` →
/// advance state. Halts on EOF (after one final flush step) or when
/// any plugin's `after_step` returns false.
///
/// Errors only on hard failures (missing `main`, query errors). UNSAT
/// per-step results are silently skipped (state unchanged), matching
/// the Python executor.
pub fn run_headless<R, W>(rt: &EvidentRuntime, input: R, output: W)
    -> io::Result<()>
where
    R: Read + 'static,
    W: Write + 'static,
{
    let stdin  = StdinPlugin::new(input);
    let stdout = StdoutPlugin::new(output);
    let mut plugins: Vec<Box<dyn Plugin>> = vec![Box::new(stdin), Box::new(stdout)];
    run_with_plugins(rt, &mut plugins)
}

/// Knobs for the per-step loop. UNSAT-handling lives here so the CLI
/// can decide between silent (test fixtures), loud (default `execute`),
/// and verbose `--explain` (dump body + givens) without each call site
/// re-implementing the policy.
#[derive(Debug, Clone, Default)]
pub struct ExecOptions {
    /// Suppress the per-step UNSAT warning entirely. Tests that
    /// intentionally produce transient UNSAT (e.g. an automaton that
    /// halts on a no-input frame) set this to keep stderr clean.
    pub quiet: bool,
    /// On UNSAT, dump the schema body items + the per-step `given` to
    /// stderr — so the user can see the constraints that conflicted
    /// without needing to re-run `evident query --explain` separately.
    pub explain: bool,
}

/// Lower-level entry point: run with an explicit plugin list. Useful
/// for tests that want to inspect a plugin's state after halt
/// (e.g. read out the bytes a `StdoutPlugin<Vec<u8>>` collected) —
/// in that case, build the `Vec<Box<dyn Plugin>>` and pass it in,
/// then downcast or move ownership back out after. Most callers
/// should use `run_headless`.
pub fn run_with_plugins(
    rt: &EvidentRuntime,
    plugins: &mut [Box<dyn Plugin>],
) -> io::Result<()> {
    run_with_plugins_opts(rt, plugins, &ExecOptions::default())
}

pub fn run_with_plugins_opts(
    rt: &EvidentRuntime,
    plugins: &mut [Box<dyn Plugin>],
    opts: &ExecOptions,
) -> io::Result<()> {
    let main = rt.get_schema("main").ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "no 'schema main' found in program")
    })?;

    // Collect every var declared (directly or via passthrough) in main,
    // then route each to the plugin (if any) that handles its type.
    let declared = collect_vars(rt, &main.name, &mut Vec::new());

    // For each plugin, compute its matched (name, type) pairs and
    // `initialize`. Only keep plugins that matched something.
    let mut active: Vec<&mut Box<dyn Plugin>> = Vec::new();
    for p in plugins.iter_mut() {
        let types: std::collections::HashSet<&str> =
            p.handles_types().iter().copied().collect();
        let matched: Vec<(String, String)> = declared.iter()
            .filter(|(_, t)| types.contains(t.as_str()))
            .map(|(v, t)| (v.clone(), t.clone()))
            .collect();
        if !matched.is_empty() {
            p.initialize(matched);
            active.push(p);
        }
    }

    // State pairs (base, next, type). Stored separately from plugins —
    // the executor itself owns state advancement, not plugins.
    let pairs = detect_state_pairs(&declared);
    let mut current_state: HashMap<String, HashMap<String, Value>> = HashMap::new();
    for (base, _next, t) in &pairs {
        current_state.insert(base.clone(), initial_state(rt, t));
    }

    // Step counter for UNSAT warnings. Counts every loop iteration
    // (SAT and UNSAT alike), incremented just before reporting.
    let mut step_idx: u64 = 0;

    loop {
        // Build per-step `given`. Plugins contribute first; their
        // `before_step` returning None signals halt.
        let mut given: HashMap<String, Value> = HashMap::new();
        let mut halt = false;
        for p in active.iter_mut() {
            match p.before_step() {
                Some(g) => given.extend(g),
                None    => { halt = true; break; }
            }
        }
        if halt { break; }

        // Add current state as `base.field` givens.
        for (base, fields) in &current_state {
            for (field, value) in fields {
                given.insert(format!("{}.{}", base, field), value.clone());
            }
        }

        // Solve. Errors here are hard failures (e.g. missing schema);
        // map to io::Error so the caller sees something useful.
        let result = rt.query_cached("main", &given).map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("query error: {e}"))
        })?;

        if !result.satisfied {
            // UNSAT step: state stays as-is, plugins still get their
            // next-step turn. By default we print a one-line warning so
            // the user can't miss a silently-broken automaton (Python's
            // executor was silent here, which made the AxisPhysics-shared-
            // var bug invisible until anchor_collect.ev was visibly
            // black). `--quiet` turns this off; `--explain` adds the
            // body + givens dump.
            step_idx += 1;
            if !opts.quiet {
                eprintln!("warning: step {step_idx} UNSAT — state preserved, frame skipped");
                if opts.explain {
                    explain_step_unsat(rt, &given);
                }
            }
            continue;
        }
        step_idx += 1;

        // SAT — let plugins run side effects.
        let mut after_halt = false;
        for p in active.iter_mut() {
            if !p.after_step(&result.bindings) {
                after_halt = true;
            }
        }

        // Advance state from state_next.* bindings.
        for (base, next, _t) in &pairs {
            let next_state = extract_next_state(&result.bindings, next);
            if !next_state.is_empty() {
                current_state.insert(base.clone(), next_state);
            }
        }

        if after_halt { break; }
    }
    Ok(())
}

// ── Multi-program executor ───────────────────────────────────────────────────

/// Multi-program executor. Programs participate by including the
/// `MainCoordinator` stdlib trait (provides the `next_main` field);
/// the executor reads that field after each step and decides what
/// to do:
///
///   - `next_main = ""` (or no `MainCoordinator` in main) → stay
///   - `next_main = "<path>"` → swap to that program; world.* state
///                              survives the swap (see below)
///   - `next_main = "halt"` → shut down
///
/// World state forwarding: any state pair named `world` / `world_next`
/// is preserved across swaps. The latest `world.*` values from the
/// previous program seed the new program's first frame as `given`.
/// Programs that don't declare a `world` pair just don't get any
/// state carried — the new program starts fresh.
///
/// Single-program use (no `MainCoordinator`, no `next_main` in
/// bindings) is the N=1 case: the loop runs the same program forever
/// and never swaps.
///
/// `loader` is called whenever a new program path is requested (and
/// the path isn't already in the runtime cache). It should return a
/// fully-loaded `EvidentRuntime` (stdlibs + the user file).
///
/// Plugin activation happens once against the FIRST program. This
/// means programs after the first must use the same SDL/audio var
/// names as the first one. Future work: re-activate plugins per
/// program so each can have its own var names.
pub fn run_with_main_coordinator<F>(
    initial_path: std::path::PathBuf,
    mut loader: F,
    plugins: &mut [Box<dyn Plugin>],
    opts: &ExecOptions,
    initial_given: HashMap<String, Value>,
) -> io::Result<()>
where
    F: FnMut(&std::path::Path) -> Result<EvidentRuntime, String>,
{
    use std::path::PathBuf;

    /// Cache cap: number of program runtimes kept warm in memory.
    /// Tuned for the menu↔level back-and-forth pattern (8 lets you
    /// keep menu, settings, pause, game-over plus 4 recent levels
    /// without paying re-load cost). Beyond this, LRU eviction
    /// drops the least-recently-used. Increase if you have lots of
    /// frequently-toured screens; decrease if memory matters more
    /// than swap latency.
    const CACHE_CAP: usize = 8;

    let mut runtimes: HashMap<PathBuf, EvidentRuntime> = HashMap::new();
    // LRU order: most-recently-used at the back. Touched on every
    // program activation (initial load + each swap).
    let mut lru: Vec<PathBuf> = Vec::new();
    let mut current = initial_path.clone();

    // Load initial program.
    let initial_rt = loader(&current)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
    runtimes.insert(current.clone(), initial_rt);
    lru.push(current.clone());

    // Plugin activation: matched against the FIRST program only.
    // Re-activation per swap would be cleaner but is a larger change;
    // for v1 we require consistent SDL/audio var names across programs.
    let initial_declared = {
        let rt = runtimes.get(&current).unwrap();
        collect_vars(rt, "main", &mut Vec::new())
    };
    let mut active: Vec<usize> = activate_plugins(plugins, &initial_declared);

    // Per-program state: state-pairs and current-state map.
    let mut pairs = detect_state_pairs(&initial_declared);
    let mut current_state: HashMap<String, HashMap<String, Value>> = HashMap::new();
    {
        let rt = runtimes.get(&current).unwrap();
        for (base, _next, t) in &pairs {
            current_state.insert(base.clone(), initial_state(rt, t));
        }
    }

    let mut step_idx: u64 = 0;
    // `initial_given` is consumed only on the first iteration —
    // typically used to seed `world.*` from a JSON file via the CLI's
    // `--initial-state` flag. After step 1, state-pair forwarding
    // takes over from the program's own `state_next.*` outputs.
    let mut initial_given_remaining = Some(initial_given);

    // Optional per-second timing breakdown (EVIDENT_BENCH=1):
    // before_step total, query_cached total, after_step total, frames.
    let bench = std::env::var("EVIDENT_BENCH").as_deref() == Ok("1");
    let mut bench_t_before = std::time::Duration::ZERO;
    let mut bench_t_solve  = std::time::Duration::ZERO;
    let mut bench_t_after  = std::time::Duration::ZERO;
    let mut bench_frames: u64 = 0;
    let mut bench_rebuilds_baseline: u64 = 0;
    let mut bench_last_report = std::time::Instant::now();

    loop {
        // Build per-step `given`. Plugins first; halt if any returns None.
        let t_b0 = if bench { Some(std::time::Instant::now()) } else { None };
        let mut given: HashMap<String, Value> = HashMap::new();
        let mut halt = false;
        for &i in &active {
            match plugins[i].before_step() {
                Some(g) => given.extend(g),
                None    => { halt = true; break; }
            }
        }
        if halt { break; }

        // Add current state as `base.field` givens.
        for (base, fields) in &current_state {
            for (field, value) in fields {
                given.insert(format!("{}.{}", base, field), value.clone());
            }
        }

        // Inject `--initial-state` JSON values on the FIRST frame only.
        // Overrides plugin + state contributions for any matching key
        // (a JSON `world.score=42` wins over the state-pair default of 0).
        if let Some(seed) = initial_given_remaining.take() {
            for (k, v) in seed {
                given.insert(k, v);
            }
        }
        if let Some(t) = t_b0 { bench_t_before += t.elapsed(); }

        // Solve.
        let rt = runtimes.get(&current).unwrap();
        let t_s0 = if bench { Some(std::time::Instant::now()) } else { None };
        let result = rt.query_cached("main", &given).map_err(|e| {
            io::Error::new(io::ErrorKind::Other, format!("query error: {e}"))
        })?;
        if let Some(t) = t_s0 { bench_t_solve += t.elapsed(); }

        if !result.satisfied {
            step_idx += 1;
            if !opts.quiet {
                eprintln!("warning: step {step_idx} UNSAT — state preserved, frame skipped");
                if opts.explain {
                    explain_step_unsat(rt, &given);
                }
            }
            continue;
        }
        step_idx += 1;

        // After-step (plugins run side effects).
        let t_a0 = if bench { Some(std::time::Instant::now()) } else { None };
        let mut after_halt = false;
        for &i in &active {
            if !plugins[i].after_step(&result.bindings) {
                after_halt = true;
            }
        }

        // Advance state-pairs from state_next.* bindings.
        for (base, next, _t) in &pairs {
            let next_state = extract_next_state(&result.bindings, next);
            if !next_state.is_empty() {
                current_state.insert(base.clone(), next_state);
            }
        }
        if let Some(t) = t_a0 { bench_t_after += t.elapsed(); }

        if bench {
            bench_frames += 1;
            let dt = bench_last_report.elapsed();
            if dt.as_secs() >= 1 {
                let rt = runtimes.get(&current).unwrap();
                let cur_rebuilds = rt.cache_rebuilds();
                let rb = cur_rebuilds.saturating_sub(bench_rebuilds_baseline);
                bench_rebuilds_baseline = cur_rebuilds;
                let n = bench_frames as f64;
                eprintln!(
                    "[bench] {:.0} fps   solve={:.2}ms  before={:.2}ms  after={:.2}ms  cache_rebuilds={}",
                    n / dt.as_secs_f64(),
                    bench_t_solve.as_secs_f64()  * 1000.0 / n,
                    bench_t_before.as_secs_f64() * 1000.0 / n,
                    bench_t_after.as_secs_f64()  * 1000.0 / n,
                    rb,
                );
                bench_t_before = std::time::Duration::ZERO;
                bench_t_solve  = std::time::Duration::ZERO;
                bench_t_after  = std::time::Duration::ZERO;
                bench_frames = 0;
                bench_last_report = std::time::Instant::now();
            }
        }

        if after_halt { break; }

        // Check next_main for swap signal. Missing field, empty string,
        // or unchanged value all mean "stay in current program".
        let Some(Value::Str(next_main)) = result.bindings.get("next_main") else {
            continue;
        };
        if next_main == "halt" { break; }
        if next_main.is_empty() { continue; }

        let next_path = resolve_swap_path(&current, next_main);
        if next_path == current { continue; }

        // Preserve `world` state across the swap; everything else
        // resets to the new program's initial-state defaults.
        let preserved_world = current_state.remove("world");

        // Load the new program if we haven't seen it before, or if it
        // was evicted from the LRU. Otherwise reuse the cached runtime.
        if !runtimes.contains_key(&next_path) {
            let new_rt = loader(&next_path)
                .map_err(|e| io::Error::new(io::ErrorKind::Other,
                    format!("load {}: {e}", next_path.display())))?;
            runtimes.insert(next_path.clone(), new_rt);
            lru.push(next_path.clone());
            // Evict least-recently-used while over cap. The currently-
            // active program is always at the back (we just pushed it),
            // so it's safe from eviction.
            while runtimes.len() > CACHE_CAP {
                let evicted = lru.remove(0);
                runtimes.remove(&evicted);
            }
        } else {
            // Touch: move to back of LRU as most-recently-used.
            if let Some(pos) = lru.iter().position(|p| p == &next_path) {
                let touched = lru.remove(pos);
                lru.push(touched);
            }
        }

        // Switch active program.
        current = next_path;
        let rt = runtimes.get(&current).unwrap();
        let new_declared = collect_vars(rt, "main", &mut Vec::new());
        pairs = detect_state_pairs(&new_declared);
        current_state.clear();
        for (base, _next, t) in &pairs {
            let s = if base == "world" {
                // If the new program declares a `world` pair too, use the
                // preserved values. Otherwise fall back to defaults.
                preserved_world.clone().unwrap_or_else(|| initial_state(rt, t))
            } else {
                initial_state(rt, t)
            };
            current_state.insert(base.clone(), s);
        }

        // Re-activate plugins against the new program's vars. Plugins
        // that no longer match (program A had SDL but program B
        // doesn't) are removed from the active set; new matches join.
        // Each plugin's initialize() is called with its new matched-
        // var set — plugins must handle repeated calls (window/audio
        // device stays open, just var dispatch tables update).
        active = activate_plugins(plugins, &new_declared);
    }

    Ok(())
}

/// Compute matched (name, type) pairs per plugin against the given
/// declared-vars map; call `initialize` on each plugin that has any
/// matches; return the indices of plugins that ended up active.
///
/// Used both for initial activation and for re-activation on every
/// program swap. Plugins must be idempotent under repeated
/// `initialize` calls.
fn activate_plugins(
    plugins: &mut [Box<dyn Plugin>],
    declared: &HashMap<String, String>,
) -> Vec<usize> {
    use std::collections::HashSet;
    let mut active: Vec<usize> = Vec::new();
    for (i, p) in plugins.iter_mut().enumerate() {
        let types: HashSet<&str> = p.handles_types().iter().copied().collect();
        let matched: Vec<(String, String)> = declared.iter()
            .filter(|(_, t)| types.contains(t.as_str()))
            .map(|(v, t)| (v.clone(), t.clone()))
            .collect();
        if !matched.is_empty() {
            p.initialize(matched);
            active.push(i);
        }
    }
    active
}

/// Resolve a `next_main` path string against the current program's
/// directory. Absolute paths pass through; relative paths are joined
/// with the current program's parent so `next_main = "level_02.ev"`
/// from `levels/level_01.ev` resolves to `levels/level_02.ev`.
fn resolve_swap_path(current: &std::path::Path, target: &str) -> std::path::PathBuf {
    let target_path = std::path::Path::new(target);
    if target_path.is_absolute() {
        target_path.to_path_buf()
    } else if let Some(parent) = current.parent() {
        if parent.as_os_str().is_empty() {
            target_path.to_path_buf()
        } else {
            parent.join(target_path)
        }
    } else {
        target_path.to_path_buf()
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A `Write` adapter backed by a shared `Rc<RefCell<Vec<u8>>>` so
    /// the test can read the captured bytes after the executor drops
    /// the plugin. Avoids dyn-downcasting tricks.
    #[derive(Clone)]
    struct SharedSink(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);
    impl SharedSink {
        fn new() -> Self {
            SharedSink(std::rc::Rc::new(std::cell::RefCell::new(Vec::new())))
        }
        fn into_bytes(self) -> Vec<u8> {
            std::rc::Rc::try_unwrap(self.0)
                .map(|c| c.into_inner())
                .unwrap_or_else(|rc| rc.borrow().clone())
        }
    }
    impl Write for SharedSink {
        fn write(&mut self, b: &[u8]) -> io::Result<usize> {
            self.0.borrow_mut().write(b)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.borrow_mut().flush()
        }
    }

    /// Recording plugin: captures every step's bindings so tests can
    /// assert on the *full* per-step state, not just the side effects
    /// the StdoutPlugin happens to write. Doesn't claim any I/O type
    /// itself — returned via a shared Rc so the test can read after
    /// the executor finishes.
    #[derive(Clone, Default)]
    struct Recorder(std::rc::Rc<std::cell::RefCell<Vec<HashMap<String, Value>>>>);
    impl Recorder {
        fn into_steps(self) -> Vec<HashMap<String, Value>> {
            std::rc::Rc::try_unwrap(self.0)
                .map(|c| c.into_inner())
                .unwrap_or_else(|rc| rc.borrow().clone())
        }
    }
    impl Plugin for Recorder {
        // Claim a fictional type so initialize() always succeeds: route
        // through `__recorder__` which never appears in any program. We
        // override initialize() directly to bypass the matcher.
        fn handles_types(&self) -> &'static [&'static str] { &[] }
        fn initialize(&mut self, _matched: Vec<(String, String)>) {}
        fn before_step(&mut self) -> Option<HashMap<String, Value>> { Some(HashMap::new()) }
        fn after_step(&mut self, b: &HashMap<String, Value>) -> bool {
            self.0.borrow_mut().push(b.clone());
            true
        }
    }

    fn run_capture(program: &str, input: &[u8]) -> Vec<u8> {
        let mut rt = EvidentRuntime::new();
        load_io_stdlib(&mut rt).expect("stdlib loads");
        rt.load_source(program).expect("user program parses");
        let sink = SharedSink::new();
        let stdin  = StdinPlugin::new(Cursor::new(input.to_vec()));
        let stdout = StdoutPlugin::new(sink.clone());
        let mut plugins: Vec<Box<dyn Plugin>> = vec![Box::new(stdin), Box::new(stdout)];
        run_with_plugins(&rt, &mut plugins).expect("run ok");
        drop(plugins);
        sink.into_bytes()
    }

    /// Run with input + a recorder; return captured stdout AND the
    /// per-step bindings the recorder saw. Used by tests that want to
    /// assert against state evolution rather than byte-level output.
    fn run_capture_with_record(program: &str, input: &[u8])
        -> (Vec<u8>, Vec<HashMap<String, Value>>)
    {
        let mut rt = EvidentRuntime::new();
        load_io_stdlib(&mut rt).expect("stdlib loads");
        rt.load_source(program).expect("user program parses");
        let sink = SharedSink::new();
        let rec = Recorder::default();
        // To get the recorder activated, we need it in the active list.
        // The matcher requires matched_vars to be non-empty — but the
        // recorder declares no types. Workaround: directly construct
        // the active list and bypass the matcher. Simplest: call
        // run_with_plugins, but force-activate by wrapping the recorder
        // so it always returns at least one matched var.
        // Alternative simpler: build the executor logic inline here.
        let stdin  = StdinPlugin::new(Cursor::new(input.to_vec()));
        let stdout = StdoutPlugin::new(sink.clone());
        let mut plugins: Vec<Box<dyn Plugin>> = vec![
            Box::new(stdin),
            Box::new(stdout),
            Box::new(ForceActiveRecorder(rec.clone())),
        ];
        run_with_plugins(&rt, &mut plugins).expect("run ok");
        drop(plugins);
        (sink.into_bytes(), rec.into_steps())
    }

    /// Wrapper that forces the recorder to be activated by reporting at
    /// least one matched var for some type the user program declares
    /// (we ask for `Stdin`, which the test programs always have).
    struct ForceActiveRecorder(Recorder);
    impl Plugin for ForceActiveRecorder {
        // Match against Stdin so the matcher gives us the same matched
        // vars as StdinPlugin and activates us. The recorder doesn't
        // contribute givens — its before_step returns an empty map.
        fn handles_types(&self) -> &'static [&'static str] { INPUT_TYPES }
        fn initialize(&mut self, m: Vec<(String, String)>) { self.0.initialize(m); }
        fn before_step(&mut self) -> Option<HashMap<String, Value>> { self.0.before_step() }
        fn after_step(&mut self, b: &HashMap<String, Value>) -> bool { self.0.after_step(b) }
    }

    #[test]
    fn executor_echoes_input() {
        // Simplest possible automaton: copy src.char to dst.out every step.
        // No state. Stops on EOF (the EOF step has src.char="" so
        // dst.out="" — nothing extra written).
        let program = r#"
schema main
    src ∈ Stdin
    dst ∈ Stdout
    dst.out = src.char
"#;
        let out = run_capture(program, b"hi\n");
        assert_eq!(String::from_utf8(out).unwrap(), "hi\n");
    }

    #[test]
    fn executor_state_increments() {
        // Counter: state.n increments each step. Recorder captures the
        // bindings of every step so we can verify state.n advances 0→1→2.
        // (3 input chars produces 3 visible state.n values: 0, 1, 2.)
        let program = r#"
schema CounterState
    n ∈ Nat

schema main
    src ∈ Stdin
    dst ∈ Stdout
    state ∈ CounterState
    state_next ∈ CounterState

    state_next.n = state.n + 1
"#;
        let (_out, steps) = run_capture_with_record(program, b"abc");
        // 3 input chars + 1 EOF step = 4 total. We assert the first 3.
        assert!(steps.len() >= 3, "expected ≥3 steps, got {}", steps.len());
        for (i, expected_n) in [0i64, 1, 2].iter().enumerate() {
            let actual = match steps[i].get("state.n") {
                Some(Value::Int(n)) => *n,
                other => panic!("step {}: expected Int, got {:?}", i, other),
            };
            assert_eq!(actual, *expected_n, "step {} state.n", i);
        }
    }

    #[test]
    fn executor_state_gated_output() {
        // Output gated by state.n: emit "X" only when n=2. With "abc"
        // input we step through n=0,1,2,3 (one EOF step) so "X"
        // fires exactly once.
        let program = r#"
schema CounterState
    n ∈ Nat

schema main
    src ∈ Stdin
    dst ∈ Stdout
    state ∈ CounterState
    state_next ∈ CounterState

    state_next.n = state.n + 1
    state.n = 2 ⇒ dst.out = "X"
    state.n ≠ 2 ⇒ dst.out = ""
"#;
        let out = run_capture(program, b"abc");
        let s = String::from_utf8(out).unwrap();
        assert_eq!(s, "X", "output was {:?}", s);
    }

    #[test]
    fn executor_unsat_step_is_skipped() {
        // Force UNSAT on every step: dst.out = src.char AND dst.out = "X"
        // — impossible whenever src.char != "X". We feed "ab" plus EOF so
        // every step is UNSAT, output should be empty, and the executor
        // must complete (not hang or panic) even when nothing satisfies.
        let program = r#"
schema main
    src ∈ Stdin
    dst ∈ Stdout
    dst.out = src.char
    dst.out = "X"
"#;
        let out = run_capture(program, b"ab");
        assert_eq!(out, b"", "expected empty output, got {:?}", out);
    }

    #[test]
    fn detect_state_pairs_basic() {
        let mut declared = HashMap::new();
        declared.insert("state".to_string(),       "S".to_string());
        declared.insert("state_next".to_string(),  "S".to_string());
        declared.insert("dst".to_string(),         "Stdout".to_string());
        declared.insert("src".to_string(),         "Stdin".to_string());
        let pairs = detect_state_pairs(&declared);
        assert_eq!(pairs, vec![("state".to_string(), "state_next".to_string(), "S".to_string())]);
    }

    #[test]
    fn detect_state_pairs_excludes_io_types() {
        // A pathological case: dst / dst_next both ∈ Stdout. They should
        // NOT be a state pair (Stdout is an I/O type, not a state type).
        let mut declared = HashMap::new();
        declared.insert("dst".to_string(),      "Stdout".to_string());
        declared.insert("dst_next".to_string(), "Stdout".to_string());
        let pairs = detect_state_pairs(&declared);
        assert!(pairs.is_empty());
    }
}
