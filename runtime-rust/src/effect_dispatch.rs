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
                }).collect();
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
        _ => false,
    })
}

/// Walk an effect list, dispatch each, collect results.
pub fn dispatch_all(ctx: &mut DispatchContext, effects: &[Effect]) -> Vec<EffectResult> {
    effects.iter().map(|e| dispatch_one(ctx, e)).collect()
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
