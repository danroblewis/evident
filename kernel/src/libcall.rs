//! libffi dispatch for `LibCall(lib, fn, args)` Effect variants.
//!
//! v1 scope:
//! - Arg types: `ArgInt(Int)`, `ArgStr(String)`, `ArgReal(Real)`.
//! - Return type: assumed `i64`. Functions returning `void` / pointers / etc.
//!   can still be called — the returned `i64` is whatever the platform ABI
//!   left in the integer return register; the user is responsible for
//!   interpreting it.
//! - `dlopen` handles are cached per-library-string for the kernel's lifetime
//!   (closed implicitly on process exit; we don't track an explicit lifetime).
//!
//! Limitations:
//! - No `void *` arg type. Use `ArgInt` carrying a u64 handle, or `ArgStr`.
//! - No `float`/`double` return yet — needs a separate Cif return type, and
//!   `last_results` round-trip work to surface as `RealResult`.
//! - No structured error reporting from libffi itself (segfaults kill the
//!   kernel with exit 3 per the spec, but soft failures like dlopen-null
//!   surface as the textual `Err(_)` returned here).

use std::collections::HashMap;
use std::ffi::{CString, c_void};
use std::sync::Mutex;

use libffi::middle::{Arg, Cif, CodePtr, Type};
use libloading::os::unix::Library;

/// Argument value for one libffi call.
#[derive(Debug, Clone)]
pub enum LibArg {
    Int(i64),
    Str(String),
    Real(f64),
}

/// Return value from one libcall. The historical assumption is "everything
/// returns i64"; that's still true for the libffi-dispatched generic path
/// (`Int(i64)`). The `__cstr.copy` pseudo-library breaks the assumption so
/// that `Z3_ast_to_string` / `Z3_get_string` / `Z3_get_error_msg` (all
/// `const char *` returns) can be marshaled back as Evident `String`
/// (`Res::Str` → `StringResult`) instead of as an opaque pointer. The two
/// existing call-sites in `tick.rs` translate `Int` → `Res::Int` and
/// `Str` → `Res::Str`. See translate_arith.ev's "WIP: Z3-AST builders"
/// note for why this matters: the per-binop translation reads back the
/// SMT-LIB pretty-print of the final root AST via `Z3_ast_to_string` and
/// compares it as a String — without a char*→String marshal that
/// readback can't happen.
#[derive(Debug, Clone)]
pub enum LibRet {
    Int(i64),
    Str(String),
}

/// Internal: keep alive any heap-allocated arg storage (CStrings) for the
/// duration of one call. The libffi `Arg` references borrow from these.
struct ArgStorage {
    cstrings: Vec<CString>,
    ints:     Vec<i64>,
    reals:    Vec<f64>,
}

/// Process-wide cache of `dlopen`'d libraries, keyed by the library name
/// the user passed to LibCall. `Library::open` is idempotent on the same
/// path but we cache to avoid the dlopen syscall per call.
static LIB_CACHE: Mutex<Option<HashMap<String, Library>>> = Mutex::new(None);

/// State for EVIDENT_EFFECT_TRACE (see `call`). The flag is read once and
/// cached so we don't hit the env on every call.
static EFFECT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static EFFECT_TRACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Call once at startup to latch the EVIDENT_EFFECT_TRACE env into the flag.
pub fn init_effect_trace() {
    if std::env::var("EVIDENT_EFFECT_TRACE").ok().as_deref() == Some("1") {
        EFFECT_TRACE.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Resolve a function in a (possibly cached) library and call it with the
/// given args. Returns either an i64 (the libffi register-return value, the
/// historical default) or a String (used by pseudo-libraries that need to
/// surface a textual result — today only `__cstr.copy`).
pub fn call(lib_name: &str, fn_name: &str, args: &[LibArg]) -> Result<LibRet, String> {
    // EVIDENT_EFFECT_TRACE=1 prints every C call the program dispatches, in
    // order, with a global sequence number. Joins the kernel's diagnostic
    // suite (EVIDENT_PHASE_TRACE / EVIDENT_FUNCTIONIZE_STATS / EVIDENT_UNSAT_CORE)
    // and is off by default (one relaxed atomic load per call when off). The
    // effects ARE the driver's Z3-model build trace (each LibCall is one libz3
    // C call); the last line before a crash localizes the failing op far better
    // than guessing. Includes __mem loads/stores so a bad FTI address shows.
    if EFFECT_TRACE.load(std::sync::atomic::Ordering::Relaxed) {
        let n = EFFECT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        eprintln!("[eff {n}] {lib_name}::{fn_name}({args:?})");
    }
    // `__mem`: the minimal pointer-deref escape hatch the FTI honesty audit
    // (task #23) requires. An honest FTI keeps its data in libc-`malloc`'d
    // memory and reads it back; libffi's int/str/real arg grammar cannot
    // express a faithful 8-byte load (no libc one-shot reader) nor a faithful
    // 8-byte store (`memset` only writes a repeated byte, lossy for any
    // value > 255). These two functions are that pair and nothing more — no
    // allocation tracking, no typed-array abstraction, no handles. This is
    // deliberately NOT the legacy `__mem__` library (alloc/free/typed loads/
    // GC); it is the single minimal deref primitive, justified in
    // docs/plans/architecture-invariants.md §"The `__mem` deref primitive".
    if lib_name == "__mem" {
        return mem_call(fn_name, args).map(LibRet::Int);
    }
    if lib_name == "__dlsym" {
        return dlsym_addr_call(fn_name, args).map(LibRet::Int);
    }
    // `__cstr`: read a NUL-terminated C string from a raw address and return
    // it as an Evident String. The sole consumer today is the Z3-AST-as-text
    // path in compiler/translate_arith.ev — after the FSM builds an AST via
    // libz3 calls, it asks `Z3_ast_to_string` for the canonical SMT-LIB
    // pretty-print, then `__cstr.copy(ptr)` reads the bytes back. Without
    // this, the libffi return path can carry a pointer but not the bytes
    // behind it, so the test fixture has no way to assert on the rendered
    // text. This is wave-5a "blocker B2 — char* → Evident String" from
    // docs/plans/wave-5a-z3-in-evident.md §5.
    if lib_name == "__cstr" {
        return cstr_call(fn_name, args).map(LibRet::Str);
    }

    let lib_path = resolve_lib_path(lib_name);

    // Load (or reuse) the library.
    let mut guard = LIB_CACHE.lock().map_err(|e| format!("lib cache poisoned: {e}"))?;
    let cache = guard.get_or_insert_with(HashMap::new);

    if !cache.contains_key(&lib_path) {
        // Try the resolved name first; if dyld can't find it via its
        // default search rules, fall back to the prefixed paths that
        // are conventional on this host. This mirrors what
        // .cargo/config.toml's DYLD_LIBRARY_PATH does at build time,
        // but applies at runtime so end users don't need to export the
        // env var to use libraries from /opt/homebrew or Anaconda.
        let candidates = candidate_paths(&lib_path);
        let mut lib_opt: Option<Library> = None;
        let mut last_err = String::new();
        for c in &candidates {
            match unsafe { Library::new(c) } {
                Ok(l) => { lib_opt = Some(l); break; }
                Err(e) => { last_err = format!("dlopen({c}): {e}"); }
            }
        }
        let lib = lib_opt.ok_or(last_err)?;
        cache.insert(lib_path.clone(), lib);
    }
    let lib = cache.get(&lib_path).expect("just inserted");

    // dlsym.
    let fn_name_c = CString::new(fn_name)
        .map_err(|e| format!("function name has nul byte: {e}"))?;
    let sym_ptr: *mut c_void = unsafe {
        let sym: libloading::os::unix::Symbol<unsafe extern "C" fn()> = lib
            .get(fn_name_c.as_bytes_with_nul())
            .map_err(|e| format!("dlsym({fn_name}): {e}"))?;
        *(&sym as *const _ as *const *mut c_void)
    };
    if sym_ptr.is_null() {
        return Err(format!("dlsym({fn_name}) returned null"));
    }

    // Build the libffi Cif from arg shapes. Return type: i64 (sint64).
    let arg_types: Vec<Type> = args.iter().map(|a| match a {
        LibArg::Int(_)  => Type::i64(),
        LibArg::Str(_)  => Type::pointer(),
        LibArg::Real(_) => Type::f64(),
    }).collect();
    let cif = Cif::new(arg_types.into_iter(), Type::i64());

    // Build owned storage so pointers passed into libffi remain valid for
    // the duration of the call.
    let mut storage = ArgStorage {
        cstrings: Vec::new(),
        ints:     Vec::new(),
        reals:    Vec::new(),
    };
    for a in args {
        match a {
            LibArg::Int(n)  => storage.ints.push(*n),
            LibArg::Str(s)  => {
                let cs = CString::new(s.as_str())
                    .map_err(|e| format!("string arg has nul byte: {e}"))?;
                storage.cstrings.push(cs);
            }
            LibArg::Real(r) => storage.reals.push(*r),
        }
    }

    // Walk args again to build the `Arg` references. Each pointer borrows
    // from `storage`.
    let mut int_idx = 0usize;
    let mut str_idx = 0usize;
    let mut real_idx = 0usize;
    let mut ffi_args: Vec<Arg> = Vec::with_capacity(args.len());
    // Also collect pointer-typed arg backing storage (CString → *const c_char).
    let mut string_ptrs: Vec<*const std::os::raw::c_char> = Vec::with_capacity(storage.cstrings.len());
    for cs in &storage.cstrings {
        string_ptrs.push(cs.as_ptr());
    }
    let mut sp_idx = 0usize;
    for a in args {
        match a {
            LibArg::Int(_)  => {
                let r = &storage.ints[int_idx];
                int_idx += 1;
                ffi_args.push(Arg::new(r));
            }
            LibArg::Str(_)  => {
                let r = &string_ptrs[sp_idx];
                sp_idx += 1;
                str_idx += 1;
                ffi_args.push(Arg::new(r));
            }
            LibArg::Real(_) => {
                let r = &storage.reals[real_idx];
                real_idx += 1;
                ffi_args.push(Arg::new(r));
            }
        }
    }

    // Call. Treat the return as i64.
    let code_ptr = CodePtr::from_ptr(sym_ptr);
    let result: i64 = unsafe { cif.call(code_ptr, &ffi_args) };

    // AST-lifetime policy. A Z3_ast built via the C API starts at refcount 0
    // and Z3 GCs it under memory pressure unless inc_ref'd. The compiler
    // (compiler2/driver.ev) builds tens of thousands of ASTs across ticks and
    // carries them as Int handles; a long-lived one — a cached sort/constant,
    // or an operand built early and passed to Z3_mk_ite much later — gets
    // reclaimed mid-build and Z3 segfaults (observed compiling sample.ev at
    // ~142k ASTs; latent below that because small programs never trigger a
    // GC). Refcount discipline is imperative memory management, not a
    // constraint, so it can't live in the model cleanly; and the kernel owns
    // the Z3 context's lifetime. So: keep every AST the program builds alive
    // for the (short-lived) process — inc_ref the result of AST-returning
    // libz3 builders. Identified by name (Z3_mk_* + Z3_simplify) minus the
    // Z3_mk_* that return non-AST objects (config/context/solver/symbol/…),
    // an allowlist by construction: an unrecognised call is never inc_ref'd,
    // so a non-AST i64 is never mistaken for an ast pointer.
    if lib_name == "libz3" && result != 0 && returns_ast(fn_name) {
        if let Some(LibArg::Int(ctx)) = args.first() {
            unsafe {
                z3_sys::Z3_inc_ref(*ctx as z3_sys::Z3_context, result as z3_sys::Z3_ast);
            }
        }
    }
    Ok(LibRet::Int(result))
}

/// True iff a `libz3` function returns an inc_ref-able `Z3_ast` (incl. sorts
/// and func_decls, which are ast subtypes). The builders Z3_mk_* qualify
/// except the handful that return other object kinds; Z3_simplify also
/// returns an ast. Everything else (introspection, solver/tactic ops, the
/// char*/lbool/void returns) is excluded — inc_ref'ing those would deref a
/// non-pointer i64.
fn returns_ast(fn_name: &str) -> bool {
    if fn_name == "Z3_simplify" {
        return true;
    }
    fn_name.starts_with("Z3_mk_")
        && !matches!(
            fn_name,
            "Z3_mk_config"
                | "Z3_mk_context"
                | "Z3_mk_context_rc"
                | "Z3_mk_solver"
                | "Z3_mk_simple_solver"
                | "Z3_mk_solver_from_tactic"
                | "Z3_mk_tactic"
                | "Z3_mk_goal"
                | "Z3_mk_params"
                | "Z3_mk_string_symbol"
                | "Z3_mk_int_symbol"
                | "Z3_mk_constructor"
                | "Z3_mk_constructor_list"
                | "Z3_mk_datatypes"
        )
}

/// The `__cstr` pseudo-library: a faithful char* → Evident-String marshal.
/// Sole function today is `copy(addr) → String`. The address is whatever
/// a prior libcall returned as IntResult (typically `Z3_ast_to_string`'s
/// `Z3_string` return — Z3 owns the underlying buffer for the lifetime of
/// the context). We strlen by byte-scan, copy, and return the lossy-utf8
/// decoded string. The result lands on the next tick as `StringResult`.
fn cstr_call(fn_name: &str, args: &[LibArg]) -> Result<String, String> {
    if fn_name != "copy" {
        return Err(format!("__cstr: unknown function `{fn_name}` (only `copy`)"));
    }
    let addr = match args.first() {
        Some(LibArg::Int(n)) => *n as usize,
        Some(other) => return Err(format!("__cstr::copy arg 0 must be ArgInt, got {other:?}")),
        None => return Err("__cstr::copy missing arg 0".to_string()),
    };
    if addr == 0 {
        return Err("__cstr::copy: null pointer".to_string());
    }
    let max_len = match args.get(1) {
        Some(LibArg::Int(n)) => (*n).max(0) as usize,
        Some(other) => return Err(format!("__cstr::copy arg 1 must be ArgInt, got {other:?}")),
        // Default cap = 1 MiB. A non-string pointer accidentally fed in
        // would otherwise walk into unmapped memory and SIGSEGV.
        None => 1 << 20,
    };
    let mut bytes = Vec::with_capacity(64);
    unsafe {
        let p = addr as *const u8;
        let mut i: usize = 0;
        while i < max_len {
            let b = p.add(i).read_unaligned();
            if b == 0 { break; }
            bytes.push(b);
            i += 1;
        }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// The `__mem` primitive: a faithful machine-long load/store on a raw
/// address. Two functions only:
///   - `read_long(addr)`        → `*(long*)addr`
///   - `write_long(addr, value)` → `*(long*)addr = value`, returns 0
/// Addresses come from a prior `LibCall("libc","malloc",…)` whose pointer the
/// FTI carries as `base ∈ Int`. The reads/writes are `unaligned` so an FTI is
/// free to choose any byte offset; in practice the FTIs use 8-byte slots.
fn mem_call(fn_name: &str, args: &[LibArg]) -> Result<i64, String> {
    let int_arg = |i: usize| -> Result<i64, String> {
        match args.get(i) {
            Some(LibArg::Int(n)) => Ok(*n),
            Some(other) => Err(format!("__mem::{fn_name} arg {i} must be ArgInt, got {other:?}")),
            None => Err(format!("__mem::{fn_name} missing arg {i}")),
        }
    };
    match fn_name {
        "read_long" => {
            let addr = int_arg(0)? as usize;
            let p = addr as *const i64;
            Ok(unsafe { p.read_unaligned() })
        }
        "write_long" => {
            let addr = int_arg(0)? as usize;
            let val = int_arg(1)?;
            let p = addr as *mut i64;
            unsafe { p.write_unaligned(val) };
            Ok(0)
        }
        other => Err(format!("__mem: unknown function `{other}` (only read_long/write_long)")),
    }
}

/// The `__dlsym` pseudo-library: dlsym a symbol from a named library and
/// return its ADDRESS as i64, without calling it.
///
/// Wave 5b Path A enabler. libffi's own dispatch (`ffi_prep_cif`, `ffi_call`)
/// needs the addresses of data symbols like `ffi_type_sint64` and
/// `ffi_type_pointer` so it can describe a CIF — and those are exactly the
/// shapes the `LibCall(lib, fn, …)` path cannot return because it always
/// *calls* the symbol it resolves. This adds a single shape: "give me the
/// address of <lib>.<sym> without invoking it."
///
/// API:
///   LibCall("__dlsym", "addr", ⟨ArgStr(lib), ArgStr(sym)⟩) → IntResult(ptr)
fn dlsym_addr_call(fn_name: &str, args: &[LibArg]) -> Result<i64, String> {
    if fn_name != "addr" {
        return Err(format!(
            "__dlsym: unknown function `{fn_name}` (only `addr`)"
        ));
    }
    let str_arg = |i: usize| -> Result<&str, String> {
        match args.get(i) {
            Some(LibArg::Str(s)) => Ok(s.as_str()),
            Some(other) => Err(format!(
                "__dlsym::addr arg {i} must be ArgStr, got {other:?}"
            )),
            None => Err(format!("__dlsym::addr missing arg {i}")),
        }
    };
    let lib_name = str_arg(0)?;
    let sym_name = str_arg(1)?;
    let lib_path = resolve_lib_path(lib_name);

    let mut guard = LIB_CACHE.lock().map_err(|e| format!("lib cache poisoned: {e}"))?;
    let cache = guard.get_or_insert_with(HashMap::new);
    if !cache.contains_key(&lib_path) {
        let candidates = candidate_paths(&lib_path);
        let mut lib_opt: Option<Library> = None;
        let mut last_err = String::new();
        for c in &candidates {
            match unsafe { Library::new(c) } {
                Ok(l) => { lib_opt = Some(l); break; }
                Err(e) => { last_err = format!("dlopen({c}): {e}"); }
            }
        }
        let lib = lib_opt.ok_or(last_err)?;
        cache.insert(lib_path.clone(), lib);
    }
    let lib = cache.get(&lib_path).expect("just inserted");
    let sym_c = CString::new(sym_name).map_err(|e| format!("symbol has nul byte: {e}"))?;
    let addr: i64 = unsafe {
        let sym: libloading::os::unix::Symbol<*const c_void> = lib
            .get(sym_c.as_bytes_with_nul())
            .map_err(|e| format!("dlsym({sym_name}): {e}"))?;
        // `*sym` is the raw symbol pointer (a `*const c_void`); cast its
        // address to i64. `into_raw` returns a non-Copy slot wrapper, so
        // hold it via `&` and read the pointer through it.
        let raw = sym.into_raw();
        *(&raw as *const _ as *const usize) as i64
    };
    Ok(addr)
}

/// Map the user-given library name to a path libloading can dlopen.
/// Conventions:
/// - Exact path (`/usr/lib/libfoo.dylib`) → use as-is.
/// - Bare `"libc"` → platform default (`libc.so.6` on Linux,
///   `libSystem.dylib` on macOS).
/// - Anything else → pass through; libloading handles search via the
///   dynamic linker's standard rules.
fn resolve_lib_path(name: &str) -> String {
    if name.contains('/') {
        return name.to_string();
    }
    match name {
        "libc" => {
            if cfg!(target_os = "macos") {
                "libSystem.dylib".to_string()
            } else {
                "libc.so.6".to_string()
            }
        }
        other => {
            // If the name already has a platform-recognized extension,
            // pass through. Otherwise append the host platform's library
            // extension so the user can write `"libz3"` and have it
            // resolve to `libz3.dylib` on macOS / `libz3.so` on Linux.
            let has_ext = other.ends_with(".dylib")
                || other.ends_with(".so")
                || other.ends_with(".dll")
                || other.contains(".so.");
            if has_ext {
                other.to_string()
            } else if cfg!(target_os = "macos") {
                format!("{other}.dylib")
            } else if cfg!(target_os = "windows") {
                format!("{other}.dll")
            } else {
                format!("{other}.so")
            }
        }
    }
}

/// Generate a list of paths to try when dlopen'ing `lib_path`. The bare
/// name is tried first (lets dyld's normal search work), then a series
/// of host-conventional prefixes covers the macOS dev env's "Homebrew
/// is the default" reality without forcing every user to set
/// DYLD_LIBRARY_PATH at runtime.
fn candidate_paths(lib_path: &str) -> Vec<String> {
    if lib_path.contains('/') {
        return vec![lib_path.to_string()];
    }
    let prefixes: &[&str] = if cfg!(target_os = "macos") {
        &[
            "",
            "/opt/homebrew/lib/",
            "/usr/local/lib/",
            "/opt/anaconda3/lib/python3.13/site-packages/z3/lib/",
            "/opt/anaconda3/lib/",
            "/opt/local/lib/",
        ]
    } else {
        &[
            "",
            "/usr/local/lib/",
            "/usr/lib/x86_64-linux-gnu/",
            "/lib/x86_64-linux-gnu/",
        ]
    };
    prefixes.iter().map(|p| format!("{p}{lib_path}")).collect()
}
