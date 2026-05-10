//! Effect dispatcher — performs `Effect`s and produces `EffectResult`s.
//!
//! Sits between the executor (which solves for `effects` each step)
//! and the OS / FFI layer. Built-in effects (Print/ReadLine/Time/Exit)
//! hit the OS directly; FFI* effects route through `crate::ffi`.
//!
//! Phase 1.3 lands the dispatcher with built-ins working and FFI*
//! arms returning Error stubs. Phase 1.5 wires the FFI primitives.

use std::io::{BufRead, Write};
use std::time::Instant;

use crate::ast::{Effect, EffectFfiArg, EffectResult};
use crate::ffi::{self, FfiArg, FfiReturn, HandleRegistry};

/// One recorded FFI call for replay mode. When DispatchMode::Replay
/// is active, each FFICall consumes the next RecordedCall from the
/// list; if `symbol` and `args` match, the recorded `result` is
/// returned. Mismatch → Error. Used for trace tests that should run
/// without needing the actual external library.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedCall {
    pub symbol: String,
    pub sig:    String,
    pub args:   Vec<EffectFfiArg>,
    pub result: EffectResult,
}

/// FFI dispatch mode. v1 supports Real (hits libffi) and Replay
/// (consults a pre-supplied log). Recording (write a log alongside
/// the test) is YAGNI for now.
#[derive(Default)]
pub enum DispatchMode {
    #[default]
    Real,
    /// Replay: walk through `calls` left-to-right; each FFI call
    /// must match the next entry. Symbol-name lookup also tracked
    /// via `name_for_handle` so we can compare symbol vs args at
    /// call time (libffi otherwise loses the name).
    Replay {
        calls:           Vec<RecordedCall>,
        cursor:          usize,
        name_for_handle: std::collections::HashMap<u64, String>,
        next_sentinel:   u64,
    },
}

/// Per-runtime mutable state the dispatcher reads/writes between
/// effects. Held in the executor; one DispatchContext per step loop
/// run. stdin/stdout are boxed so unit tests can swap in in-memory
/// streams.
pub struct DispatchContext {
    pub registry: HandleRegistry,
    pub stdin:    Box<dyn BufRead + Send>,
    pub stdout:   Box<dyn Write + Send>,
    pub start:    Instant,
    pub mode:     DispatchMode,
    /// Cache for LibCall: `library_path → lib handle`. Populated on
    /// first reference, reused on subsequent calls to the same lib.
    pub lib_cache: std::collections::HashMap<String, u64>,
    /// Cache for LibCall: `(lib handle, symbol name) → sym handle`.
    pub sym_cache: std::collections::HashMap<(u64, String), u64>,
}

impl DispatchContext {
    pub fn new() -> Self {
        Self::with_streams(
            Box::new(std::io::BufReader::new(std::io::stdin())),
            Box::new(std::io::stdout()),
        )
    }

    pub fn with_streams(
        stdin:  Box<dyn BufRead + Send>,
        stdout: Box<dyn Write + Send>,
    ) -> Self {
        Self {
            registry: HandleRegistry::new(),
            stdin, stdout,
            start: Instant::now(),
            mode: DispatchMode::default(),
            lib_cache: std::collections::HashMap::new(),
            sym_cache: std::collections::HashMap::new(),
        }
    }

    /// Switch to replay mode with the supplied recorded calls. FFI
    /// effects no longer hit real libraries; they consult `calls`
    /// in order.
    pub fn set_replay(&mut self, calls: Vec<RecordedCall>) {
        self.mode = DispatchMode::Replay {
            calls,
            cursor: 0,
            name_for_handle: std::collections::HashMap::new(),
            next_sentinel: 1,
        };
    }
}

impl Default for DispatchContext {
    fn default() -> Self { Self::new() }
}

/// Perform one effect; return the matching result. Errors that
/// don't tear down the runtime are reported as `EffectResult::Error`.
/// `Exit` calls `process::exit` and never returns.
pub fn dispatch_one(ctx: &mut DispatchContext, e: &Effect) -> EffectResult {
    let r = dispatch_one_inner(ctx, e);
    if std::env::var("EVIDENT_FFI_TRACE").is_ok() {
        eprintln!("[ffi] {e:?} → {r:?}");
    }
    r
}

fn dispatch_one_inner(ctx: &mut DispatchContext, e: &Effect) -> EffectResult {
    match e {
        Effect::NoEffect => EffectResult::NoResult,

        Effect::Print(s) => {
            let _ = write!(ctx.stdout, "{s}");
            let _ = ctx.stdout.flush();
            EffectResult::NoResult
        }
        Effect::Println(s) => {
            let _ = writeln!(ctx.stdout, "{s}");
            let _ = ctx.stdout.flush();
            EffectResult::NoResult
        }
        Effect::ReadLine => {
            let mut line = String::new();
            match ctx.stdin.read_line(&mut line) {
                Ok(0)  => EffectResult::Error("readline: EOF".into()),
                Ok(_)  => {
                    if line.ends_with('\n') { line.pop(); }
                    if line.ends_with('\r') { line.pop(); }
                    EffectResult::Str(line)
                }
                Err(e) => EffectResult::Error(format!("readline: {e}")),
            }
        }
        Effect::Time => {
            let ms = ctx.start.elapsed().as_millis() as i64;
            EffectResult::Int(ms)
        }
        Effect::Exit(n) => std::process::exit(*n as i32),

        Effect::FFIOpen(path) => match &mut ctx.mode {
            DispatchMode::Real => {
                match ffi::ffi_open(&ctx.registry, path) {
                    Ok(h)  => EffectResult::Handle(h),
                    Err(e) => EffectResult::Error(e.0),
                }
            }
            DispatchMode::Replay { name_for_handle, next_sentinel, .. } => {
                let h = *next_sentinel; *next_sentinel += 1;
                name_for_handle.insert(h, format!("LIB:{path}"));
                EffectResult::Handle(h)
            }
        },
        Effect::FFILookup(lib, sym) => match &mut ctx.mode {
            DispatchMode::Real => {
                match ffi::ffi_lookup(&ctx.registry, *lib, sym) {
                    Ok(h)  => EffectResult::Handle(h),
                    Err(e) => EffectResult::Error(e.0),
                }
            }
            DispatchMode::Replay { name_for_handle, next_sentinel, .. } => {
                let h = *next_sentinel; *next_sentinel += 1;
                name_for_handle.insert(h, sym.clone());
                let _ = lib;
                EffectResult::Handle(h)
            }
        },
        Effect::FFICall(fn_id, sig, args) => match &mut ctx.mode {
            DispatchMode::Real => {
                let ffi_args: Vec<FfiArg> = args.iter().map(|a| match a {
                    EffectFfiArg::Int(n)    => FfiArg::Int(*n),
                    EffectFfiArg::Bool(b)   => FfiArg::Bool(*b),
                    EffectFfiArg::Str(s)    => FfiArg::Str(s.clone()),
                    EffectFfiArg::Real(r)   => FfiArg::Real(*r),
                    EffectFfiArg::Handle(h) => FfiArg::Handle(*h),
                    EffectFfiArg::StrArr(v) => FfiArg::StrArr(v.clone()),
                    EffectFfiArg::IntOut    => FfiArg::IntOut,
                    EffectFfiArg::I32Buf(v) => FfiArg::I32Buf(v.clone()),
                    EffectFfiArg::SdlVertexBuf(v) => FfiArg::SdlVertexBuf(v.clone()),
                    // PriorResult is resolved by dispatch_seq before
                    // it reaches us. If one slips through, bail.
                    EffectFfiArg::PriorResult(_) => FfiArg::Int(0),
                }).collect();
                if args.iter().any(|a| matches!(a, EffectFfiArg::PriorResult(_))) {
                    return EffectResult::Error(
                        "ArgPriorResult must be inside Effect::Seq".into(),
                    );
                }
                match ffi::ffi_call(&ctx.registry, *fn_id, sig, &ffi_args) {
                    Ok(FfiReturn::Void)      => EffectResult::NoResult,
                    Ok(FfiReturn::Int(n))    => EffectResult::Int(n),
                    Ok(FfiReturn::Bool(b))   => EffectResult::Bool(b),
                    Ok(FfiReturn::Str(s))    => EffectResult::Str(s),
                    Ok(FfiReturn::Real(d))   => EffectResult::Real(d),
                    Ok(FfiReturn::Handle(h)) => EffectResult::Handle(h),
                    Err(e) => EffectResult::Error(e.0),
                }
            }
            DispatchMode::Replay { calls, cursor, name_for_handle, .. } => {
                if *cursor >= calls.len() {
                    return EffectResult::Error(format!(
                        "replay: ran out of recorded calls at index {cursor}"));
                }
                let expected = &calls[*cursor];
                let actual_name = name_for_handle.get(fn_id)
                    .cloned()
                    .unwrap_or_else(|| format!("<handle:{fn_id}>"));
                if actual_name != expected.symbol {
                    return EffectResult::Error(format!(
                        "replay mismatch at index {cursor}: expected symbol {:?}, got {:?}",
                        expected.symbol, actual_name));
                }
                if *sig != expected.sig {
                    return EffectResult::Error(format!(
                        "replay mismatch at index {cursor}: expected sig {:?}, got {:?}",
                        expected.sig, sig));
                }
                if !args_equal(args, &expected.args) {
                    return EffectResult::Error(format!(
                        "replay mismatch at index {cursor}: args differ"));
                }
                let r = expected.result.clone();
                *cursor += 1;
                r
            }
        },
        Effect::CloseHandle(h) => {
            match &ctx.mode {
                DispatchMode::Real => {
                    if ctx.registry.close(*h) {
                        EffectResult::NoResult
                    } else {
                        EffectResult::Error(format!("close: unknown handle {h}"))
                    }
                }
                DispatchMode::Replay { .. } => EffectResult::NoResult,
            }
        }
        // Seq is handled at the dispatch_all level (transparent
        // expansion); a Seq landing here means the caller went through
        // dispatch_one directly. Return NoResult so the call doesn't
        // crash, but the inner effects WON'T fire — use dispatch_all
        // / dispatch_seq for proper Seq semantics.
        Effect::Seq(_) => EffectResult::NoResult,
        Effect::LibCall(lib_path, sym_name, sig, args) => match &mut ctx.mode {
            DispatchMode::Real => {
                // Cached lib handle: reuse if the library was opened
                // in any prior step; else dlopen and remember.
                let lib_handle = match ctx.lib_cache.get(lib_path) {
                    Some(h) => *h,
                    None => match ffi::ffi_open(&ctx.registry, lib_path) {
                        Ok(h)  => { ctx.lib_cache.insert(lib_path.clone(), h); h }
                        Err(e) => return EffectResult::Error(e.0),
                    },
                };
                // Cached symbol handle: keyed on (lib_handle, sym_name)
                // so the same symbol resolved against different libs
                // doesn't collide.
                let key = (lib_handle, sym_name.clone());
                let sym_handle = match ctx.sym_cache.get(&key) {
                    Some(h) => *h,
                    None => match ffi::ffi_lookup(&ctx.registry, lib_handle, sym_name) {
                        Ok(h)  => { ctx.sym_cache.insert(key, h); h }
                        Err(e) => return EffectResult::Error(e.0),
                    },
                };
                // The actual call. Same arg-marshalling as FFICall.
                let ffi_args: Vec<FfiArg> = args.iter().map(|a| match a {
                    EffectFfiArg::Int(n)    => FfiArg::Int(*n),
                    EffectFfiArg::Bool(b)   => FfiArg::Bool(*b),
                    EffectFfiArg::Str(s)    => FfiArg::Str(s.clone()),
                    EffectFfiArg::Real(r)   => FfiArg::Real(*r),
                    EffectFfiArg::Handle(h) => FfiArg::Handle(*h),
                    EffectFfiArg::StrArr(v) => FfiArg::StrArr(v.clone()),
                    EffectFfiArg::IntOut    => FfiArg::IntOut,
                    EffectFfiArg::I32Buf(v) => FfiArg::I32Buf(v.clone()),
                    EffectFfiArg::SdlVertexBuf(v) => FfiArg::SdlVertexBuf(v.clone()),
                    // PriorResult is resolved by dispatch_seq before
                    // it reaches us. If one slips through, bail.
                    EffectFfiArg::PriorResult(_) => FfiArg::Int(0),
                }).collect();
                if args.iter().any(|a| matches!(a, EffectFfiArg::PriorResult(_))) {
                    return EffectResult::Error(
                        "ArgPriorResult must be inside Effect::Seq".into(),
                    );
                }
                match ffi::ffi_call(&ctx.registry, sym_handle, sig, &ffi_args) {
                    Ok(FfiReturn::Void)      => EffectResult::NoResult,
                    Ok(FfiReturn::Int(n))    => EffectResult::Int(n),
                    Ok(FfiReturn::Bool(b))   => EffectResult::Bool(b),
                    Ok(FfiReturn::Str(s))    => EffectResult::Str(s),
                    Ok(FfiReturn::Real(d))   => EffectResult::Real(d),
                    Ok(FfiReturn::Handle(h)) => EffectResult::Handle(h),
                    Err(e) => EffectResult::Error(e.0),
                }
            }
            DispatchMode::Replay { calls, cursor, .. } => {
                if *cursor >= calls.len() {
                    return EffectResult::Error(format!(
                        "replay: ran out of recorded calls at index {cursor}"));
                }
                let expected = &calls[*cursor];
                if *sym_name != expected.symbol || *sig != expected.sig
                   || !args_equal(args, &expected.args)
                {
                    return EffectResult::Error(format!(
                        "replay mismatch at index {cursor}: LibCall {sym_name:?} vs expected {:?}",
                        expected.symbol));
                }
                let r = expected.result.clone();
                let _ = lib_path;
                *cursor += 1;
                r
            }
        },
    }
}

fn args_equal(a: &[EffectFfiArg], b: &[EffectFfiArg]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).all(|(x, y)| match (x, y) {
        (EffectFfiArg::Int(p), EffectFfiArg::Int(q)) => p == q,
        (EffectFfiArg::Bool(p), EffectFfiArg::Bool(q)) => p == q,
        (EffectFfiArg::Str(p), EffectFfiArg::Str(q)) => p == q,
        (EffectFfiArg::Real(p), EffectFfiArg::Real(q)) => (p - q).abs() < 1e-12,
        // Handle args don't have to match exactly under replay (sentinels
        // differ between record and replay runs); sufficient that both
        // sides are Handle.
        (EffectFfiArg::Handle(_), EffectFfiArg::Handle(_)) => true,
        (EffectFfiArg::StrArr(p), EffectFfiArg::StrArr(q)) => p == q,
        (EffectFfiArg::IntOut,    EffectFfiArg::IntOut)    => true,
        (EffectFfiArg::I32Buf(p), EffectFfiArg::I32Buf(q)) => p == q,
        (EffectFfiArg::SdlVertexBuf(p), EffectFfiArg::SdlVertexBuf(q)) => p == q,
        (EffectFfiArg::PriorResult(p), EffectFfiArg::PriorResult(q)) => p == q,
        _ => false,
    })
}

/// Walk an effect list, dispatch each, collect results. `Effect::Seq`
/// is expanded inline: its inner calls' results are appended to the
/// output as if they had been issued as separate top-level effects,
/// so the next state's `last_results` sees them in the same flat
/// sequence — but the whole Seq executes WITHOUT returning to the
/// solver between calls. Within a Seq, `ArgPriorResult(N)` resolves
/// to the Nth prior-in-this-Seq result.
pub fn dispatch_all(ctx: &mut DispatchContext, effects: &[Effect]) -> Vec<EffectResult> {
    let mut out: Vec<EffectResult> = Vec::new();
    for e in effects {
        if let Effect::Seq(inner) = e {
            dispatch_seq(ctx, inner, &mut out);
        } else {
            out.push(dispatch_one(ctx, e));
        }
    }
    out
}

/// Run a sequenced effect group: each inner call's result joins a
/// per-Seq `prior` list AND the global result vector. Later calls in
/// the same Seq can reference earlier results via `ArgPriorResult(N)`,
/// which is resolved to a typed FfiArg at marshal time.
fn dispatch_seq(
    ctx: &mut DispatchContext,
    inner: &[Effect],
    out: &mut Vec<EffectResult>,
) {
    let mut prior: Vec<EffectResult> = Vec::new();
    for sub in inner {
        if let Effect::Seq(deeper) = sub {
            // Nested Seq: inner Seq has its own prior scope. Its
            // calls' results join the global out and the OUTER Seq's
            // prior list sees the LAST inner result as a single
            // entry.
            let before = out.len();
            dispatch_seq(ctx, deeper, out);
            let summary = if out.len() > before {
                out[out.len() - 1].clone()
            } else {
                EffectResult::NoResult
            };
            prior.push(summary);
        } else {
            let resolved = resolve_prior_in_effect(sub, &prior);
            let r = dispatch_one(ctx, &resolved);
            out.push(r.clone());
            prior.push(r);
        }
    }
}

/// Walk an Effect's args and replace each `EffectFfiArg::PriorResult(N)`
/// with the typed arg derived from `prior[N]`. Non-LibCall/FFICall
/// effects don't carry args and are returned as-is.
fn resolve_prior_in_effect(e: &Effect, prior: &[EffectResult]) -> Effect {
    let resolve_args = |args: &[EffectFfiArg]| -> Vec<EffectFfiArg> {
        args.iter().map(|a| match a {
            EffectFfiArg::PriorResult(n) => match prior.get(*n) {
                Some(r) => result_to_ffi_arg(r).unwrap_or(EffectFfiArg::Int(0)),
                None => EffectFfiArg::Int(0),
            },
            other => other.clone(),
        }).collect()
    };
    match e {
        Effect::FFICall(fn_id, sig, args) =>
            Effect::FFICall(*fn_id, sig.clone(), resolve_args(args)),
        Effect::LibCall(lib, sym, sig, args) =>
            Effect::LibCall(lib.clone(), sym.clone(), sig.clone(), resolve_args(args)),
        other => other.clone(),
    }
}

/// EffectResult → EffectFfiArg variant pick for prior-result resolution.
/// Each result variant has a natural FfiArg counterpart (Handle stays
/// Handle, Int stays Int, etc.); NoResult and Error don't have one and
/// the caller uses a sentinel on miss.
fn result_to_ffi_arg(r: &EffectResult) -> Option<EffectFfiArg> {
    match r {
        EffectResult::Int(n)    => Some(EffectFfiArg::Int(*n)),
        EffectResult::Bool(b)   => Some(EffectFfiArg::Bool(*b)),
        EffectResult::Str(s)    => Some(EffectFfiArg::Str(s.clone())),
        EffectResult::Real(d)   => Some(EffectFfiArg::Real(*d)),
        EffectResult::Handle(h) => Some(EffectFfiArg::Handle(*h)),
        EffectResult::NoResult | EffectResult::Error(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn ctx_with_input(input: &str) -> DispatchContext {
        DispatchContext::with_streams(
            Box::new(std::io::BufReader::new(Cursor::new(input.to_string().into_bytes()))),
            Box::new(Vec::<u8>::new()),
        )
    }

    fn captured_stdout(ctx: DispatchContext) -> String {
        // The Box<dyn Write> can't be downcast; tests that need stdout
        // capture should construct their own Vec<u8> and inspect it
        // via a separate handle pattern. For simplicity these tests
        // mostly verify the result, not the stdout bytes.
        // (Returning empty here since we can't unwrap the Box.)
        let _ = ctx;
        String::new()
    }

    #[test]
    fn no_effect_returns_no_result() {
        let mut ctx = DispatchContext::new();
        assert!(matches!(dispatch_one(&mut ctx, &Effect::NoEffect), EffectResult::NoResult));
    }

    #[test]
    fn print_returns_no_result() {
        let mut ctx = DispatchContext::with_streams(
            Box::new(Cursor::new(Vec::<u8>::new())),
            Box::new(Vec::<u8>::new()),
        );
        let r = dispatch_one(&mut ctx, &Effect::Print("hi".into()));
        assert!(matches!(r, EffectResult::NoResult));
    }

    #[test]
    fn readline_strips_trailing_newline() {
        let mut ctx = ctx_with_input("hello\nworld\n");
        match dispatch_one(&mut ctx, &Effect::ReadLine) {
            EffectResult::Str(s) => assert_eq!(s, "hello"),
            other => panic!("expected Str, got {other:?}"),
        }
        match dispatch_one(&mut ctx, &Effect::ReadLine) {
            EffectResult::Str(s) => assert_eq!(s, "world"),
            other => panic!("expected Str, got {other:?}"),
        }
        // Third read hits EOF.
        assert!(matches!(dispatch_one(&mut ctx, &Effect::ReadLine), EffectResult::Error(_)));
        let _ = captured_stdout(ctx);
    }

    #[test]
    fn time_returns_non_negative_int() {
        let mut ctx = DispatchContext::new();
        match dispatch_one(&mut ctx, &Effect::Time) {
            EffectResult::Int(n) => assert!(n >= 0),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn time_is_non_decreasing() {
        let mut ctx = DispatchContext::new();
        let a = match dispatch_one(&mut ctx, &Effect::Time) {
            EffectResult::Int(n) => n, _ => unreachable!(),
        };
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = match dispatch_one(&mut ctx, &Effect::Time) {
            EffectResult::Int(n) => n, _ => unreachable!(),
        };
        assert!(b >= a, "time went backwards: {a} → {b}");
    }

    #[test]
    fn ffi_open_real_libc_succeeds() {
        let mut ctx = DispatchContext::new();
        let path = if cfg!(target_os = "macos") { "libSystem.dylib" } else { "libc.so.6" };
        match dispatch_one(&mut ctx, &Effect::FFIOpen(path.into())) {
            EffectResult::Handle(h) => assert!(h > 0, "handle should be > 0, got {h}"),
            other => panic!("expected Handle, got {other:?}"),
        }
    }

    #[test]
    fn ffi_open_invalid_path_returns_error() {
        let mut ctx = DispatchContext::new();
        match dispatch_one(&mut ctx, &Effect::FFIOpen("/nonexistent/lib".into())) {
            EffectResult::Error(_) => {}
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn ffi_call_getpid_end_to_end() {
        let mut ctx = DispatchContext::new();
        let path = if cfg!(target_os = "macos") { "libSystem.dylib" } else { "libc.so.6" };
        let lib = match dispatch_one(&mut ctx, &Effect::FFIOpen(path.into())) {
            EffectResult::Handle(h) => h, _ => panic!(),
        };
        let sym = match dispatch_one(&mut ctx, &Effect::FFILookup(lib, "getpid".into())) {
            EffectResult::Handle(h) => h, _ => panic!(),
        };
        match dispatch_one(&mut ctx, &Effect::FFICall(sym, "i()".into(), vec![])) {
            EffectResult::Int(pid) => {
                assert_eq!(pid as u32, std::process::id());
            }
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn close_unknown_handle_errors() {
        let mut ctx = DispatchContext::new();
        match dispatch_one(&mut ctx, &Effect::CloseHandle(9999)) {
            EffectResult::Error(_) => {}
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn replay_mode_returns_recorded_results() {
        let mut ctx = ctx_with_input("");
        ctx.set_replay(vec![
            RecordedCall {
                symbol: "getpid".into(), sig: "i()".into(),
                args: vec![], result: EffectResult::Int(12345),
            },
        ]);
        // FFIOpen + FFILookup return sentinel handles; FFICall consumes
        // the recorded entry.
        let lib = match dispatch_one(&mut ctx, &Effect::FFIOpen("anything".into())) {
            EffectResult::Handle(h) => h, _ => panic!(),
        };
        let sym = match dispatch_one(&mut ctx, &Effect::FFILookup(lib, "getpid".into())) {
            EffectResult::Handle(h) => h, _ => panic!(),
        };
        match dispatch_one(&mut ctx, &Effect::FFICall(sym, "i()".into(), vec![])) {
            EffectResult::Int(n) => assert_eq!(n, 12345),
            other => panic!("expected Int(12345), got {other:?}"),
        }
    }

    #[test]
    fn replay_mode_errors_on_symbol_mismatch() {
        let mut ctx = ctx_with_input("");
        ctx.set_replay(vec![RecordedCall {
            symbol: "expected_sym".into(), sig: "i()".into(),
            args: vec![], result: EffectResult::Int(1),
        }]);
        let lib = match dispatch_one(&mut ctx, &Effect::FFIOpen("x".into())) {
            EffectResult::Handle(h) => h, _ => panic!(),
        };
        let sym = match dispatch_one(&mut ctx, &Effect::FFILookup(lib, "wrong_sym".into())) {
            EffectResult::Handle(h) => h, _ => panic!(),
        };
        match dispatch_one(&mut ctx, &Effect::FFICall(sym, "i()".into(), vec![])) {
            EffectResult::Error(m) => assert!(m.contains("mismatch"), "{}", m),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn replay_mode_errors_when_log_exhausted() {
        let mut ctx = ctx_with_input("");
        ctx.set_replay(vec![]);
        let lib = match dispatch_one(&mut ctx, &Effect::FFIOpen("x".into())) {
            EffectResult::Handle(h) => h, _ => panic!(),
        };
        let sym = match dispatch_one(&mut ctx, &Effect::FFILookup(lib, "any".into())) {
            EffectResult::Handle(h) => h, _ => panic!(),
        };
        match dispatch_one(&mut ctx, &Effect::FFICall(sym, "i()".into(), vec![])) {
            EffectResult::Error(m) => assert!(m.contains("ran out"), "{}", m),
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn libcall_caches_lib_and_sym() {
        let mut ctx = ctx_with_input("");
        let path = if cfg!(target_os = "macos") { "/usr/lib/libSystem.dylib" } else { "libc.so.6" };
        // First call: cache miss for both lib and sym → populates both.
        let r1 = dispatch_one(&mut ctx, &Effect::LibCall(
            path.into(), "getpid".into(), "i()".into(), vec![],
        ));
        match r1 {
            EffectResult::Int(pid) => assert_eq!(pid as u32, std::process::id()),
            other => panic!("expected Int, got {other:?}"),
        }
        assert_eq!(ctx.lib_cache.len(), 1, "lib cache should have one entry");
        assert_eq!(ctx.sym_cache.len(), 1, "sym cache should have one entry");

        // Second call to same lib + sym: cache hit on both. Should
        // still work and not re-dlopen / re-dlsym.
        let next_id_before = ctx.lib_cache.values().copied().max().unwrap();
        let r2 = dispatch_one(&mut ctx, &Effect::LibCall(
            path.into(), "getpid".into(), "i()".into(), vec![],
        ));
        match r2 {
            EffectResult::Int(_) => {}
            other => panic!("expected Int, got {other:?}"),
        }
        let next_id_after = ctx.lib_cache.values().copied().max().unwrap();
        assert_eq!(next_id_before, next_id_after,
            "second call should not have allocated a new lib handle");
    }

    #[test]
    fn libcall_with_string_arg() {
        let mut ctx = ctx_with_input("");
        let path = if cfg!(target_os = "macos") { "/usr/lib/libSystem.dylib" } else { "libc.so.6" };
        let r = dispatch_one(&mut ctx, &Effect::LibCall(
            path.into(), "strlen".into(), "i(s)".into(),
            vec![EffectFfiArg::Str("hello world".into())],
        ));
        match r {
            EffectResult::Int(n) => assert_eq!(n, 11),
            other => panic!("expected Int(11), got {other:?}"),
        }
    }

    #[test]
    fn libcall_invalid_lib_returns_error() {
        let mut ctx = ctx_with_input("");
        let r = dispatch_one(&mut ctx, &Effect::LibCall(
            "/nonexistent/lib".into(), "getpid".into(), "i()".into(), vec![],
        ));
        match r {
            EffectResult::Error(_) => {}
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn dispatch_all_preserves_order_and_count() {
        let mut ctx = ctx_with_input("");
        let effects = vec![
            Effect::NoEffect,
            Effect::Time,
            Effect::NoEffect,
        ];
        let results = dispatch_all(&mut ctx, &effects);
        assert_eq!(results.len(), 3);
        assert!(matches!(results[0], EffectResult::NoResult));
        assert!(matches!(results[1], EffectResult::Int(_)));
        assert!(matches!(results[2], EffectResult::NoResult));
    }
}
