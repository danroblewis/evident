//! Effect dispatcher: performs `Effect`s and produces `EffectResult`s.
//! Built-ins hit the OS directly; FFI* effects route through `crate::ffi`.

use std::io::{BufRead, Write};
use std::time::Instant;

use crate::core::ast::{Effect, EffectFfiArg, EffectResult};
use crate::ffi::{self, FfiArg, FfiReturn, HandleRegistry};

/// One recorded FFI call for replay mode. Symbol + args must match; on mismatch → Error.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedCall {
    pub symbol: String,
    pub sig:    String,
    pub args:   Vec<EffectFfiArg>,
    pub result: EffectResult,
}

/// Real (hits libffi) or Replay (walks pre-supplied log left-to-right).
#[derive(Default)]
pub enum DispatchMode {
    #[default]
    Real,
    /// Each FFI call must match the next entry; `name_for_handle` tracks symbol names
    /// (libffi otherwise loses them between Open/Lookup and Call).
    Replay {
        calls:           Vec<RecordedCall>,
        cursor:          usize,
        name_for_handle: std::collections::HashMap<u64, String>,
        next_sentinel:   u64,
    },
}

/// Mutable dispatcher state for one step-loop run. stdin/stdout boxed for test swapping.
pub struct DispatchContext {
    pub registry: HandleRegistry,
    pub stdin:    Box<dyn BufRead + Send>,
    pub stdout:   Box<dyn Write + Send>,
    pub start:    Instant,
    pub mode:     DispatchMode,
    /// LibCall cache: `library_path → lib handle`.
    pub lib_cache: std::collections::HashMap<String, u64>,
    /// LibCall cache: `(lib handle, symbol name) → sym handle`.
    pub sym_cache: std::collections::HashMap<(u64, String), u64>,
    /// Set by `Effect::Exit`; effect loop halts cleanly at end-of-tick. First Exit wins.
    pub exit_requested: Option<i32>,
    /// True when StdinSource owns fd 0 (World declares `stdin_line`).
    /// `Effect::ReadLine` is an error in this mode — it would race the background thread.
    pub stdin_owned_by_plugin: bool,
    /// FSM-spawn requests drained at end-of-tick. Each entry: `(claim_name, arg)`.
    pub pending_spawns: Vec<(String, i64)>,
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
            exit_requested: None,
            stdin_owned_by_plugin: false,
            pending_spawns: Vec::new(),
        }
    }

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

/// Dispatch one effect; return its result. Non-fatal errors → `EffectResult::Error`.
pub fn dispatch_one(ctx: &mut DispatchContext, e: &Effect) -> EffectResult {
    let r = dispatch_one_inner(ctx, e);
    if std::env::var("EVIDENT_FFI_TRACE").is_ok() {
        eprintln!("[ffi] {e:?} → {r:?}");
    }
    r
}

/// Detect Z3 auto-named sentinels like "!0!" — unconstrained String vars Z3 fills with
/// `!name!` symbols. Filter at the dispatch boundary to avoid polluting stdout.
fn is_z3_sentinel_string(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 3 { return false; }
    if b[0] != b'!' || b[b.len() - 1] != b'!' { return false; }
    let middle = &s[1..s.len() - 1];
    // Z3 sentinels are short, contain only alphanumeric +
    // optional `!` separator (e.g. "0", "a", "val!12", "k!7").
    middle.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'!')
        && middle.len() <= 16
}

fn dispatch_one_inner(ctx: &mut DispatchContext, e: &Effect) -> EffectResult {
    match e {
        Effect::NoEffect => EffectResult::NoResult,

        Effect::Print(s) => {
            if !is_z3_sentinel_string(s) {
                let _ = write!(ctx.stdout, "{s}");
                let _ = ctx.stdout.flush();
            }
            EffectResult::NoResult
        }
        Effect::Println(s) => {
            if !is_z3_sentinel_string(s) {
                let _ = writeln!(ctx.stdout, "{s}");
                let _ = ctx.stdout.flush();
            }
            EffectResult::NoResult
        }
        Effect::ReadLine => {
            if ctx.stdin_owned_by_plugin {
                return EffectResult::Error(
                    "readline: stdin is owned by StdinSource plugin (declared via \
                     `stdin_line: String` in World) — programs that auto-install \
                     the plugin cannot also use Effect::ReadLine, since both would \
                     race for the same bytes on fd 0. Use either the plugin pattern \
                     (subscribe to world.stdin_line) OR remove stdin_line from World \
                     and use ReadLine directly.".into()
                );
            }
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
        Effect::ParseInt(s) => match s.parse::<i64>() {
            Ok(n)  => EffectResult::Int(n),
            Err(e) => EffectResult::Error(format!("ParseInt: {e}: {s:?}")),
        },
        Effect::ParseReal(s) => match s.parse::<f64>() {
            Ok(f)  => EffectResult::Real(f),
            Err(e) => EffectResult::Error(format!("ParseReal: {e}: {s:?}")),
        },
        Effect::IntToStr(n)  => EffectResult::Str(n.to_string()),
        Effect::RealToStr(f) => EffectResult::Str(f.to_string()),
        Effect::ShellRun(cmd) => {
            use std::process::Command;
            match Command::new("sh").arg("-c").arg(cmd).output() {
                Ok(out) if out.status.success() => {
                    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
                    if s.ends_with('\n') { s.pop(); }
                    EffectResult::Str(s)
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let snippet: String = stderr.chars().take(200).collect();
                    EffectResult::Error(format!(
                        "ShellRun: exit={} stderr={}",
                        out.status.code().unwrap_or(-1),
                        snippet.trim_end(),
                    ))
                }
                Err(e) => EffectResult::Error(format!("ShellRun: spawn failed: {e}")),
            }
        }
        Effect::SpawnFsm(claim_name, arg) => {
            let idx = ctx.pending_spawns.len() as i64;
            ctx.pending_spawns.push((claim_name.clone(), *arg));
            EffectResult::Int(idx)  // tentative — real ID assigned at instantiation
        }
        Effect::Exit(n) => {
            // Deferred: effect loop checks at end-of-tick so co-scheduled FSMs can finish.
            if ctx.exit_requested.is_none() {
                ctx.exit_requested = Some(*n as i32);
            }
            EffectResult::NoResult
        }

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
                    EffectFfiArg::PackedBuf(v) => FfiArg::PackedBuf(v.clone()),
                    // PriorResult resolved by dispatch_all; if one slips through, bail.
                    EffectFfiArg::PriorResult(_) => FfiArg::Int(0),
                }).collect();
                if args.iter().any(|a| matches!(a, EffectFfiArg::PriorResult(_))) {
                    return EffectResult::Error(
                        "ArgPriorResult unresolved at dispatch_one (should have been \
                         resolved against prior effects in dispatch_all)".into(),
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
        Effect::LibCall(lib_path, sym_name, sig, args) => match &mut ctx.mode {
            DispatchMode::Real => {
                let lib_handle = match ctx.lib_cache.get(lib_path) {
                    Some(h) => *h,
                    None => match ffi::ffi_open(&ctx.registry, lib_path) {
                        Ok(h)  => { ctx.lib_cache.insert(lib_path.clone(), h); h }
                        Err(e) => return EffectResult::Error(e.0),
                    },
                };
                let key = (lib_handle, sym_name.clone());
                let sym_handle = match ctx.sym_cache.get(&key) {
                    Some(h) => *h,
                    None => match ffi::ffi_lookup(&ctx.registry, lib_handle, sym_name) {
                        Ok(h)  => { ctx.sym_cache.insert(key, h); h }
                        Err(e) => return EffectResult::Error(e.0),
                    },
                };
                let ffi_args: Vec<FfiArg> = args.iter().map(|a| match a {
                    EffectFfiArg::Int(n)    => FfiArg::Int(*n),
                    EffectFfiArg::Bool(b)   => FfiArg::Bool(*b),
                    EffectFfiArg::Str(s)    => FfiArg::Str(s.clone()),
                    EffectFfiArg::Real(r)   => FfiArg::Real(*r),
                    EffectFfiArg::Handle(h) => FfiArg::Handle(*h),
                    EffectFfiArg::StrArr(v) => FfiArg::StrArr(v.clone()),
                    EffectFfiArg::IntOut    => FfiArg::IntOut,
                    EffectFfiArg::I32Buf(v) => FfiArg::I32Buf(v.clone()),
                    EffectFfiArg::PackedBuf(v) => FfiArg::PackedBuf(v.clone()),
                    // PriorResult resolved by dispatch_seq; if one slips through, bail.
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
        Effect::ReadByte(handle, offset) =>
            do_read(ctx, *handle, *offset, "ReadByte",
                |ptr| EffectResult::Int(unsafe { *ptr as i64 })),
        Effect::ReadI16(handle, offset) =>
            do_read(ctx, *handle, *offset, "ReadI16",
                |ptr| EffectResult::Int(unsafe {
                    (ptr as *const i16).read_unaligned() as i64
                })),
        Effect::ReadI32(handle, offset) =>
            do_read(ctx, *handle, *offset, "ReadI32",
                |ptr| EffectResult::Int(unsafe {
                    (ptr as *const i32).read_unaligned() as i64
                })),
        Effect::ReadI64(handle, offset) =>
            do_read(ctx, *handle, *offset, "ReadI64",
                |ptr| EffectResult::Int(unsafe {
                    (ptr as *const i64).read_unaligned()
                })),
        Effect::ReadF32(handle, offset) =>
            do_read(ctx, *handle, *offset, "ReadF32",
                |ptr| EffectResult::Real(unsafe {
                    (ptr as *const f32).read_unaligned() as f64
                })),
        Effect::ReadF64(handle, offset) =>
            do_read(ctx, *handle, *offset, "ReadF64",
                |ptr| EffectResult::Real(unsafe {
                    (ptr as *const f64).read_unaligned()
                })),
        Effect::ReadStr(handle, offset) =>
            do_read(ctx, *handle, *offset, "ReadStr",
                |ptr| {
                    let start = ptr as *const u8;
                    let mut len: isize = 0;
                    unsafe {
                        while *start.offset(len) != 0 { len += 1; }
                    }
                    let slice = unsafe {
                        std::slice::from_raw_parts(start, len as usize)
                    };
                    match std::str::from_utf8(slice) {
                        Ok(s) => EffectResult::Str(s.to_string()),
                        Err(_) => EffectResult::Error(
                            "ReadStr: invalid UTF-8".to_string()),
                    }
                }),
        Effect::WriteByte(handle, offset, value) =>
            do_write(ctx, *handle, *offset, "WriteByte",
                |ptr| unsafe { *(ptr as *mut u8) = *value as u8; }),
        Effect::WriteI16(handle, offset, value) =>
            do_write(ctx, *handle, *offset, "WriteI16",
                |ptr| unsafe {
                    (ptr as *mut i16).write_unaligned(*value as i16);
                }),
        Effect::WriteI32(handle, offset, value) =>
            do_write(ctx, *handle, *offset, "WriteI32",
                |ptr| unsafe {
                    (ptr as *mut i32).write_unaligned(*value as i32);
                }),
        Effect::WriteI64(handle, offset, value) =>
            do_write(ctx, *handle, *offset, "WriteI64",
                |ptr| unsafe {
                    (ptr as *mut i64).write_unaligned(*value);
                }),
        Effect::WriteF32(handle, offset, value) =>
            do_write(ctx, *handle, *offset, "WriteF32",
                |ptr| unsafe {
                    (ptr as *mut f32).write_unaligned(*value as f32);
                }),
        Effect::WriteF64(handle, offset, value) =>
            do_write(ctx, *handle, *offset, "WriteF64",
                |ptr| unsafe {
                    (ptr as *mut f64).write_unaligned(*value);
                }),
        Effect::WriteStr(handle, offset, value) =>
            do_write(ctx, *handle, *offset, "WriteStr",
                |ptr| unsafe {
                    let bytes = value.as_bytes();
                    let dst = ptr as *mut u8;
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
                    *dst.offset(bytes.len() as isize) = 0;
                }),
        Effect::RegisterCallback(claim, sig) => match &mut ctx.mode {
            DispatchMode::Real => EffectResult::Error(format!(
                "RegisterCallback({claim}, {sig}) not yet implemented — see \
                 docs/design/ffi-os-evolution.md § Tier 4")),
            DispatchMode::Replay { calls, cursor, .. } => {
                if *cursor >= calls.len() {
                    return EffectResult::Error(format!(
                        "replay: ran out of recorded calls at index {cursor}"));
                }
                let r = calls[*cursor].result.clone();
                *cursor += 1;
                r
            }
        },
        Effect::MonotonicTime => match &mut ctx.mode {
            DispatchMode::Real => {
                // OnceLock epoch anchors the first call; subsequent calls are monotonically
                // increasing without leaking Instant::now's platform-dependent epoch.
                use std::sync::OnceLock;
                static EPOCH: OnceLock<std::time::Instant> = OnceLock::new();
                let epoch = EPOCH.get_or_init(std::time::Instant::now);
                let ns = epoch.elapsed().as_nanos() as i64;
                EffectResult::Int(ns)
            }
            DispatchMode::Replay { calls, cursor, .. } => {
                if *cursor >= calls.len() {
                    return EffectResult::Error(format!(
                        "replay: ran out of recorded calls at index {cursor}"));
                }
                let r = calls[*cursor].result.clone();
                *cursor += 1;
                r
            }
        },
        Effect::Malloc(size) => match &mut ctx.mode {
            DispatchMode::Real => {
                if *size <= 0 {
                    return EffectResult::Error(format!(
                        "Malloc: size must be positive, got {size}"));
                }
                let size_usize = *size as usize;
                let layout = match std::alloc::Layout::from_size_align(size_usize, 8) {
                    Ok(l) => l,
                    Err(e) => return EffectResult::Error(format!(
                        "Malloc: layout for {size} bytes: {e}")),
                };
                let raw = unsafe { std::alloc::alloc_zeroed(layout) }; // zeroed: safe for callers
                if raw.is_null() {
                    return EffectResult::Error(format!(
                        "Malloc: out of memory for {size} bytes"));
                }
                // Box-erased drop fn so HandleRegistry can dealloc with the correct layout.
                let drop_fn: Box<dyn FnOnce(*mut std::ffi::c_void) + Send> =
                    Box::new(move |p| unsafe {
                        std::alloc::dealloc(p as *mut u8, layout);
                    });
                let id = ctx.registry.register_with_drop(
                    raw as *mut std::ffi::c_void, Some(drop_fn));
                EffectResult::Int(id as i64)
            }
            DispatchMode::Replay { calls, cursor, .. } => {
                if *cursor >= calls.len() {
                    return EffectResult::Error(format!(
                        "replay: ran out of recorded calls at index {cursor}"));
                }
                let r = calls[*cursor].result.clone();
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
            // Handle sentinel values differ between record and replay; match on type only.
        (EffectFfiArg::Handle(_), EffectFfiArg::Handle(_)) => true,
        (EffectFfiArg::StrArr(p), EffectFfiArg::StrArr(q)) => p == q,
        (EffectFfiArg::IntOut,    EffectFfiArg::IntOut)    => true,
        (EffectFfiArg::I32Buf(p), EffectFfiArg::I32Buf(q)) => p == q,
        (EffectFfiArg::PackedBuf(p), EffectFfiArg::PackedBuf(q)) => p == q,
        (EffectFfiArg::PriorResult(p), EffectFfiArg::PriorResult(q)) => p == q,
        _ => false,
    })
}

/// Shared read path for `ReadX` effects: look up handle, compute `ptr+offset`, call `extract`.
/// SAFETY: `extract` MUST use `read_unaligned` — user-supplied offset gives no alignment guarantee.
fn do_read(
    ctx: &mut DispatchContext,
    handle: u64,
    offset: i64,
    name: &'static str,
    extract: impl FnOnce(*const u8) -> EffectResult,
) -> EffectResult {
    match &mut ctx.mode {
        DispatchMode::Real => {
            match ctx.registry.lookup(handle) {
                Ok(ptr) => {
                    let p = unsafe { (ptr as *const u8).offset(offset as isize) };
                    extract(p)
                }
                Err(e) => EffectResult::Error(format!("{name}: {}", e.0)),
            }
        }
        DispatchMode::Replay { calls, cursor, .. } => {
            if *cursor >= calls.len() {
                return EffectResult::Error(format!(
                    "replay: ran out of recorded calls at index {cursor}"));
            }
            let r = calls[*cursor].result.clone();
            *cursor += 1;
            r
        }
    }
}

/// Shared write path for `WriteX` effects. Mirrors `do_read`; always returns `NoResult`.
fn do_write(
    ctx: &mut DispatchContext,
    handle: u64,
    offset: i64,
    name: &'static str,
    apply: impl FnOnce(*mut u8),
) -> EffectResult {
    match &mut ctx.mode {
        DispatchMode::Real => {
            match ctx.registry.lookup(handle) {
                Ok(ptr) => {
                    let p = unsafe { (ptr as *mut u8).offset(offset as isize) };
                    apply(p);
                    EffectResult::NoResult
                }
                Err(e) => EffectResult::Error(format!("{name}: {}", e.0)),
            }
        }
        DispatchMode::Replay { calls, cursor, .. } => {
            if *cursor >= calls.len() {
                return EffectResult::Error(format!(
                    "replay: ran out of recorded calls at index {cursor}"));
            }
            let r = calls[*cursor].result.clone();
            *cursor += 1;
            r
        }
    }
}

/// Dispatch effects in order; later effects can reference prior results via `ArgPriorResult(N)`.
pub fn dispatch_all(ctx: &mut DispatchContext, effects: &[Effect]) -> Vec<EffectResult> {
    let mut out: Vec<EffectResult> = Vec::with_capacity(effects.len());
    for sub in effects {
        let resolved = resolve_prior_in_effect(sub, &out);
        let r = dispatch_one(ctx, &resolved);
        out.push(r);
    }
    out
}

/// Replace `PriorResult(N)` args with the typed value from `prior[N]`.
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

/// Map `EffectResult` to the corresponding `EffectFfiArg`. NoResult/Error → None.
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
