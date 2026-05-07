//! Trace test runner — executes a `trace name "path"` block by driving
//! the named program one step at a time and checking per-step
//! assertions against the resulting state and accumulated output.
//!
//! Two execution modes share the same loop:
//!
//! * **Stdin** programs (`src ∈ Stdin`): `send "cmd"` feeds the command
//!   chars (plus a trailing `\n`) one-by-one through the program's
//!   Stdin var, breaking when `line_ready = true` fires. Mirrors the
//!   Python `EvidentExecutor.step_line`.
//! * **SDL** programs (`input ∈ SDLInput`): `key_down`/`key_up` toggle
//!   a held-key set; `advance T` ticks the SDL frame loop at a fixed
//!   16ms dt for `T` simulated milliseconds, contributing
//!   `input.<key>_held` from the held set per frame.
//!
//! Assertions (`var = "x"` / `var ∋ "x"`) match against:
//!   - the literal name `output` → per-step accumulated stdout text
//!   - any other identifier → state field across all detected state
//!     pairs (first matching field name wins). State field names are
//!     flat — `state.location` looks up `"location"` in the merged
//!     map — since assertions don't carry the dotted base prefix.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::ast::{AssertOp, TraceAssertion, TraceDecl, TraceStep};
use crate::executor::{
    collect_vars, detect_state_pairs, extract_next_state, initial_state,
    load_io_stdlib, unescape_z3_string, INPUT_TYPES, OUTPUT_TYPES,
};
use crate::plugins::sdl::STDLIB_SDL_EV;
use crate::runtime::EvidentRuntime;
use crate::translate::Value;

/// SDL-input type name. Trace runner uses the same constant the
/// SDLPlugin uses; kept inline to avoid a public re-export dance.
const SDL_INPUT: &str = "SDLInput";
/// SDL-window type name. Like SDLInput, contributed by the runner
/// per-frame so the program's window-size/position givens resolve.
const SDL_WINDOW: &str = "SDLWindow";

/// Fixed simulated-time step for SDL `advance` ticks. Matches a
/// 60Hz frame rate. The real SDLPlugin uses wall-clock dt clamped at
/// 100ms; tests need determinism, so we pin a constant.
const SDL_FRAME_DT_MS: u32 = 16;

/// Result of running one trace block end-to-end.
#[derive(Debug)]
pub struct TraceResult {
    pub name: String,
    pub passed: bool,
    pub failures: Vec<TraceFailure>,
}

#[derive(Debug)]
pub struct TraceFailure {
    pub step_index: usize,
    pub kind: String,
    pub var: String,
    pub op: AssertOp,
    pub expected: String,
    pub actual: String,
}

/// Run one trace block. Loads the trace's `program_path` into a fresh
/// runtime (with the embedded I/O + SDL stdlibs pre-loaded), then
/// drives steps one at a time, accumulating output and held-key state,
/// and checks per-step assertions. Trace passes iff every assertion
/// on every step held.
///
/// Errors (load failures, missing main) are reported as a single
/// synthetic `TraceFailure` rather than panicking — the test runner
/// needs to keep going on a broken trace.
pub fn run_trace(trace: &TraceDecl, base_dir: &Path) -> TraceResult {
    let mut failures = Vec::new();

    let prog_path = resolve_program_path(&trace.program_path, base_dir);
    let mut rt = EvidentRuntime::new();
    if let Err(e) = load_io_stdlib(&mut rt) {
        return synthetic_failure(&trace.name, format!("io stdlib load: {e}"));
    }
    // SDL stdlib is opt-in for the program (declares `∈ SDLInput` etc.)
    // but pre-loading it here means SDL trace tests work even when the
    // user program doesn't import the file directly.
    if let Err(e) = rt.load_source(STDLIB_SDL_EV) {
        return synthetic_failure(&trace.name, format!("sdl stdlib load: {e}"));
    }
    if let Err(e) = rt.load_file(&prog_path) {
        return synthetic_failure(
            &trace.name,
            format!("load {}: {e}", prog_path.display()),
        );
    }
    if rt.get_schema("main").is_none() {
        return synthetic_failure(
            &trace.name,
            format!("no 'main' schema in {}", prog_path.display()),
        );
    }

    let declared = collect_vars(&rt, "main", &mut Vec::new());
    let stdin_vars: Vec<String> = declared.iter()
        .filter(|(_, t)| INPUT_TYPES.contains(&t.as_str()))
        .map(|(v, _)| v.clone()).collect();
    let stdout_vars: Vec<String> = declared.iter()
        .filter(|(_, t)| OUTPUT_TYPES.contains(&t.as_str()))
        .map(|(v, _)| v.clone()).collect();
    let sdl_input_vars: Vec<String> = declared.iter()
        .filter(|(_, t)| t.as_str() == SDL_INPUT)
        .map(|(v, _)| v.clone()).collect();
    let sdl_window_vars: Vec<String> = declared.iter()
        .filter(|(_, t)| t.as_str() == SDL_WINDOW)
        .map(|(v, _)| v.clone()).collect();
    let pairs = detect_state_pairs(&declared);

    // current_state: base → (next_var, field → Value)
    let mut current_state: HashMap<String, (String, HashMap<String, Value>)> =
        HashMap::new();
    for (base, next, t) in &pairs {
        current_state.insert(base.clone(), (next.clone(), initial_state(&rt, t)));
    }

    // SDL session state — held keys + simulated wall clock. Threads
    // through the whole trace (key_down before an advance is visible
    // to every frame within the advance, and continues into the next
    // advance until a key_up).
    let mut held_keys: HashSet<String> = HashSet::new();
    let mut sim_time_ms: i64 = 0;

    for (i, step) in trace.steps.iter().enumerate() {
        let step_index = i + 1;
        match step {
            TraceStep::Send { command, assertions } => {
                let (output, _bindings) = step_send(
                    &rt, command, &stdin_vars, &stdout_vars, &mut current_state,
                );
                check_step_assertions(
                    assertions, &output, &current_state, step_index,
                    &format!("send {:?}", command), &mut failures,
                );
            }
            TraceStep::KeyDown { key } => {
                held_keys.insert(key.clone());
            }
            TraceStep::KeyUp { key } => {
                held_keys.remove(key);
            }
            TraceStep::Advance { duration_ms, assertions } => {
                let frames = (*duration_ms / SDL_FRAME_DT_MS).max(1);
                let mut output = String::new();
                for _ in 0..frames {
                    sim_time_ms += SDL_FRAME_DT_MS as i64;
                    let frame_out = step_sdl_frame(
                        &rt, &sdl_input_vars, &sdl_window_vars,
                        &stdout_vars, &held_keys, sim_time_ms,
                        &mut current_state,
                    );
                    output.push_str(&frame_out);
                }
                check_step_assertions(
                    assertions, &output, &current_state, step_index,
                    &format!("advance {}ms", duration_ms), &mut failures,
                );
            }
        }
    }

    TraceResult {
        name: trace.name.clone(),
        passed: failures.is_empty(),
        failures,
    }
}

/// Drive one `send` line through the Stdin automaton. See module docs.
fn step_send(
    rt: &EvidentRuntime,
    command: &str,
    input_vars: &[String],
    output_vars: &[String],
    current_state: &mut HashMap<String, (String, HashMap<String, Value>)>,
) -> (String, HashMap<String, Value>) {
    let mut output = String::new();
    let mut final_bindings: HashMap<String, Value> = HashMap::new();

    let chars: Vec<char> = command.chars().chain(std::iter::once('\n')).collect();
    for ch in chars {
        let mut given: HashMap<String, Value> = HashMap::new();
        let ch_str = ch.to_string();
        for v in input_vars {
            given.insert(format!("{v}.fd"),        Value::Int(0));
            given.insert(format!("{v}.open"),      Value::Bool(true));
            given.insert(format!("{v}.blocking"),  Value::Bool(true));
            given.insert(format!("{v}.available"), Value::Int(1));
            given.insert(format!("{v}.eof"),       Value::Bool(false));
            given.insert(format!("{v}.char"),      Value::Str(ch_str.clone()));
        }
        for v in output_vars {
            given.insert(format!("{v}.fd"),          Value::Int(1));
            given.insert(format!("{v}.open"),        Value::Bool(true));
            given.insert(format!("{v}.blocking"),    Value::Bool(true));
            given.insert(format!("{v}.send_buffer"), Value::Int(0));
            given.insert(format!("{v}.buffer_size"), Value::Int(8192));
            given.insert(format!("{v}.buffered"),    Value::Int(0));
            given.insert(format!("{v}.flushed"),     Value::Bool(true));
        }
        for (base, (_next, fields)) in current_state.iter() {
            for (k, val) in fields {
                given.insert(format!("{base}.{k}"), val.clone());
            }
        }

        let result = match rt.query_cached("main", &given) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !result.satisfied {
            continue;
        }

        for v in output_vars {
            if let Some(Value::Str(s)) = result.bindings.get(&format!("{v}.out")) {
                if !s.is_empty() {
                    output.push_str(&unescape_z3_string(s));
                }
            }
        }
        for (_base, (next, fields)) in current_state.iter_mut() {
            let extracted = extract_next_state(&result.bindings, next);
            if !extracted.is_empty() {
                *fields = extracted;
            }
        }
        final_bindings = result.bindings;
        if let Some(Value::Bool(true)) = final_bindings.get("line_ready") {
            break;
        }
    }

    (output, final_bindings)
}

/// Drive one SDL frame: build per-frame given (held keys + dt + time
/// + window structural fields), query `main`, and advance state from
/// `state_next.*`. Returns the frame's accumulated stdout-style output
/// (almost always empty for SDL programs, but harmless to support).
///
/// Mouse is stuck at (0,0) and click/quit are always false — held-key
/// + advance-clock is the supported input model. An explicit
/// `mouse_move` / `click` / `quit` step would extend that later.
fn step_sdl_frame(
    rt: &EvidentRuntime,
    input_vars: &[String],
    window_vars: &[String],
    stdout_vars: &[String],
    held_keys: &HashSet<String>,
    sim_time_ms: i64,
    current_state: &mut HashMap<String, (String, HashMap<String, Value>)>,
) -> String {
    let mut given: HashMap<String, Value> = HashMap::new();

    let right = held_keys.contains("Right");
    let left  = held_keys.contains("Left");
    let up    = held_keys.contains("Up");
    let down  = held_keys.contains("Down");

    for v in input_vars {
        given.insert(format!("{v}.right_held"), Value::Bool(right));
        given.insert(format!("{v}.left_held"),  Value::Bool(left));
        given.insert(format!("{v}.up_held"),    Value::Bool(up));
        given.insert(format!("{v}.down_held"),  Value::Bool(down));
        given.insert(format!("{v}.mouse.x"),    Value::Int(0));
        given.insert(format!("{v}.mouse.y"),    Value::Int(0));
        given.insert(format!("{v}.click"),      Value::Bool(false));
        given.insert(format!("{v}.quit"),       Value::Bool(false));
        given.insert(format!("{v}.time"),       Value::Int(sim_time_ms));
        given.insert(format!("{v}.dt"),         Value::Int(SDL_FRAME_DT_MS as i64));
    }
    // SDLWindow defaults: 800×600 fixed, no drag. Real plugin reports
    // the actual SDL window position; trace runs are headless so we
    // pin sensible numbers.
    for v in window_vars {
        given.insert(format!("{v}.screen.x"), Value::Int(0));
        given.insert(format!("{v}.screen.y"), Value::Int(0));
        given.insert(format!("{v}.size.x"),   Value::Int(800));
        given.insert(format!("{v}.size.y"),   Value::Int(600));
        given.insert(format!("{v}.drag.x"),   Value::Int(0));
        given.insert(format!("{v}.drag.y"),   Value::Int(0));
    }
    // Stdout structural fields — SDL programs may still mix in an
    // `dst ∈ Stdout` for diagnostic prints. Cheap to populate.
    for v in stdout_vars {
        given.insert(format!("{v}.fd"),          Value::Int(1));
        given.insert(format!("{v}.open"),        Value::Bool(true));
        given.insert(format!("{v}.blocking"),    Value::Bool(true));
        given.insert(format!("{v}.send_buffer"), Value::Int(0));
        given.insert(format!("{v}.buffer_size"), Value::Int(8192));
        given.insert(format!("{v}.buffered"),    Value::Int(0));
        given.insert(format!("{v}.flushed"),     Value::Bool(true));
    }

    for (base, (_next, fields)) in current_state.iter() {
        for (k, val) in fields {
            given.insert(format!("{base}.{k}"), val.clone());
        }
    }

    let result = match rt.query_cached("main", &given) {
        Ok(r) => r,
        Err(_) => return String::new(),
    };
    if !result.satisfied {
        return String::new();
    }

    let mut output = String::new();
    for v in stdout_vars {
        if let Some(Value::Str(s)) = result.bindings.get(&format!("{v}.out")) {
            if !s.is_empty() {
                output.push_str(&unescape_z3_string(s));
            }
        }
    }
    for (_base, (next, fields)) in current_state.iter_mut() {
        let extracted = extract_next_state(&result.bindings, next);
        if !extracted.is_empty() {
            *fields = extracted;
        }
    }
    output
}

/// Check every assertion on one step's outcome; push a `TraceFailure`
/// per mismatch to the shared collector. `kind_label` is the
/// human-readable step description used in failure output (e.g.
/// `"send \"look\""` or `"advance 500ms"`).
fn check_step_assertions(
    assertions: &[TraceAssertion],
    output: &str,
    current_state: &HashMap<String, (String, HashMap<String, Value>)>,
    step_index: usize,
    kind_label: &str,
    failures: &mut Vec<TraceFailure>,
) {
    if assertions.is_empty() {
        return;
    }
    let mut flat_state: HashMap<String, Value> = HashMap::new();
    for (_base, (_next, fields)) in current_state {
        for (k, v) in fields {
            flat_state.insert(k.clone(), v.clone());
        }
    }
    for a in assertions {
        if let Some(failure) = check_assertion(a, output, &flat_state, step_index, kind_label) {
            failures.push(failure);
        }
    }
}

/// Check one assertion against the post-step state and accumulated
/// output. Returns `Some(failure)` on mismatch, `None` on success.
fn check_assertion(
    a: &TraceAssertion,
    output: &str,
    state: &HashMap<String, Value>,
    step_index: usize,
    kind_label: &str,
) -> Option<TraceFailure> {
    let actual: String = if a.var == "output" {
        output.to_string()
    } else {
        match resolve_path(&a.var, state) {
            Some(v) => display_value(&v),
            None => {
                return Some(TraceFailure {
                    step_index,
                    kind: kind_label.to_string(),
                    var: a.var.clone(),
                    op: a.op.clone(),
                    expected: a.value.clone(),
                    actual: format!("<no field '{}' in state>", a.var),
                });
            }
        }
    };

    let ok = match a.op {
        AssertOp::Eq       => actual == a.value,
        AssertOp::Contains => actual.contains(&a.value),
    };
    if ok {
        None
    } else {
        Some(TraceFailure {
            step_index,
            kind: kind_label.to_string(),
            var: a.var.clone(),
            op: a.op.clone(),
            expected: a.value.clone(),
            actual,
        })
    }
}

/// Resolve a trace's `program_path` against the test file's directory,
/// falling back to verbatim. Loose like `import` resolution so trace
/// authors can use either project-relative or file-relative paths.
fn resolve_program_path(spec: &str, base_dir: &Path) -> std::path::PathBuf {
    let verbatim = std::path::PathBuf::from(spec);
    if verbatim.is_file() {
        return verbatim;
    }
    let alongside = base_dir.join(spec);
    if alongside.is_file() {
        return alongside;
    }
    verbatim
}

/// Resolve a dotted assertion var against the flat state map.
/// Sub-schema field expansion already flattens record paths into
/// single-string keys (`state_next.hero.pos.x` → flat key
/// `"hero.pos.x"`), so scalar lookups hit on the first try.
///
/// Falls back to drilling through `Value::Composite` (records) and
/// `Value::SeqComposite` (sequences of records) for two cases the
/// flat keys don't cover:
///
///   - Top-level fields that the Z3 model returns as a record value
///     rather than per-leaf bindings (rare but supported).
///   - Sequence elements: `coins[0].collected` indexes into a
///     `Seq(Coin)` state field. `[N]` segments parse out of the path
///     as you'd expect; out-of-bounds is `None` (assertion fails
///     loudly with `<no field …>`).
fn resolve_path(path: &str, state: &HashMap<String, Value>) -> Option<Value> {
    if let Some(v) = state.get(path) {
        return Some(v.clone());
    }
    // Tokenize: identifiers, `.`, `[N]`. Split on `.` first, then peel
    // any trailing `[N]` off each segment so `coins[0]` is two steps:
    // pick `coins` from the map, then index 0 into the resulting
    // SeqComposite.
    let parts = split_path(path)?;
    let mut iter = parts.into_iter();
    let head = iter.next()?;
    let PathSeg::Field(head_name) = head else { return None };
    let mut cur = state.get(&head_name)?.clone();
    for seg in iter {
        cur = match (cur, seg) {
            (Value::Composite(map), PathSeg::Field(name)) => map.get(&name)?.clone(),
            (Value::SeqComposite(items), PathSeg::Index(i)) => {
                Value::Composite(items.get(i)?.clone())
            }
            _ => return None,
        };
    }
    Some(cur)
}

/// One segment of a parsed assertion path.
enum PathSeg {
    Field(String),
    Index(usize),
}

/// Lex an assertion path like `coins[0].collected` into segments:
/// `[Field("coins"), Index(0), Field("collected")]`. Returns `None`
/// on any syntactic glitch (unmatched bracket, non-numeric index,
/// empty segment) — the assertion then surfaces as a missing-field
/// failure rather than crashing.
fn split_path(path: &str) -> Option<Vec<PathSeg>> {
    let mut segs = Vec::new();
    let mut buf = String::new();
    let mut chars = path.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            '.' => {
                if !buf.is_empty() {
                    segs.push(PathSeg::Field(std::mem::take(&mut buf)));
                }
                chars.next();
            }
            '[' => {
                if !buf.is_empty() {
                    segs.push(PathSeg::Field(std::mem::take(&mut buf)));
                }
                chars.next();
                let mut idx_str = String::new();
                while let Some(&d) = chars.peek() {
                    if d == ']' { break; }
                    idx_str.push(d);
                    chars.next();
                }
                if chars.next() != Some(']') { return None; }
                let n: usize = idx_str.parse().ok()?;
                segs.push(PathSeg::Index(n));
            }
            _ => { buf.push(c); chars.next(); }
        }
    }
    if !buf.is_empty() {
        segs.push(PathSeg::Field(buf));
    }
    Some(segs)
}

/// Render a leaf value as the string we compare against the assertion
/// literal. Mirrors the formatting we use for top-level state values.
fn display_value(v: &Value) -> String {
    match v {
        Value::Str(s)  => s.clone(),
        Value::Int(n)  => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Real(r) => r.to_string(),
        other          => format!("{:?}", other),
    }
}

fn synthetic_failure(name: &str, msg: String) -> TraceResult {
    TraceResult {
        name: name.to_string(),
        passed: false,
        failures: vec![TraceFailure {
            step_index: 0,
            kind: String::new(),
            var: String::new(),
            op: AssertOp::Eq,
            expected: String::new(),
            actual: msg,
        }],
    }
}
