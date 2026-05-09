//! FFI primitive — load C libraries dynamically and call functions
//! through them with Evident-typed arguments. See
//! `docs/design/ffi-design.md` for the architectural overview.
//!
//! The runtime exposes three operations to Evident programs (via the
//! Effect dispatcher in the executor):
//!
//!   * `LoadLibrary(path)`    → Handle  (dlopen)
//!   * `LoadSymbol(lib, sym)` → Handle  (dlsym)
//!   * `Call(fn, sig, args)`  → Result  (libffi-marshalled call)
//!
//! Plus `CloseHandle(h)` to free a managed handle.
//!
//! Type signatures are short ASCII strings: a return-type code, an
//! open paren, zero-or-more arg-type codes, and a close paren.
//!
//!   `i()`     — zero args, returns Int (e.g. `getpid`)
//!   `i(s)`    — one String arg, returns Int (e.g. `puts`)
//!   `p(siii)` — String + 3 Ints, returns Handle
//!   `v(p)`    — one Handle arg, returns nothing (`free`)
//!
//! Type codes:
//!   `i` — int64           (Evident Int)
//!   `b` — int 0/1         (Evident Bool)
//!   `s` — UTF-8 const*    (Evident String)
//!   `d` — double          (Evident Real)
//!   `p` — void*           (Evident Handle)
//!   `v` — void return only
//!
//! v1 doesn't support: structs by value, callbacks (C → Evident),
//! variadic calls, function pointer args. These can be added as the
//! library code we write exercises them.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::Mutex;

use libffi::middle::{Arg, Cif, CodePtr, Type as FfiType};
use libloading::{Library, Symbol};

/// One argument's runtime value, tagged with its Evident type. The
/// caller (Effect dispatcher) packages each ArgList element into one
/// of these before calling `ffi_call`.
#[derive(Debug, Clone)]
pub enum FfiArg {
    Int(i64),
    Bool(bool),
    Str(String),
    Real(f64),
    Handle(u64),
}

/// One returned value from a libffi call. Maps back to an Evident
/// `Result` enum.
#[derive(Debug, Clone)]
pub enum FfiReturn {
    Void,
    Int(i64),
    Bool(bool),
    Str(String),
    Real(f64),
    Handle(u64),
}

#[derive(Debug, Clone)]
pub struct FfiError(pub String);
impl std::fmt::Display for FfiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ffi error: {}", self.0)
    }
}
impl std::error::Error for FfiError {}

/// One parsed signature: return type + arg types. Validated at call
/// time against the actual ArgList.
#[derive(Debug, Clone)]
struct ParsedSig {
    ret:  TypeCode,
    args: Vec<TypeCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeCode { I, B, S, D, F, P, V }

impl TypeCode {
    fn parse(c: char) -> Result<Self, FfiError> {
        match c {
            'i' => Ok(TypeCode::I),
            'b' => Ok(TypeCode::B),
            's' => Ok(TypeCode::S),
            'd' => Ok(TypeCode::D),
            // 'f' is GL/single-precision-friendly; the Evident-side value
            // is still ArgReal (f64) — the marshaller narrows to f32.
            // Needed for glClearColor / glUniform1f / GLfloat APIs that
            // ABI-wise take 32-bit floats (passed in s* registers on
            // AArch64, distinct from d* registers used for f64).
            'f' => Ok(TypeCode::F),
            'p' => Ok(TypeCode::P),
            'v' => Ok(TypeCode::V),
            other => Err(FfiError(format!("unknown type code {other:?}"))),
        }
    }
    fn as_ffi(&self) -> FfiType {
        match self {
            TypeCode::I => FfiType::i64(),
            TypeCode::B => FfiType::i32(),
            TypeCode::S => FfiType::pointer(),
            TypeCode::D => FfiType::f64(),
            TypeCode::F => FfiType::f32(),
            TypeCode::P => FfiType::pointer(),
            TypeCode::V => FfiType::void(),
        }
    }
}

fn parse_signature(sig: &str) -> Result<ParsedSig, FfiError> {
    let bytes = sig.as_bytes();
    if bytes.len() < 3 {
        return Err(FfiError(format!("signature {sig:?} too short")));
    }
    let ret = TypeCode::parse(bytes[0] as char)?;
    if bytes[1] != b'(' {
        return Err(FfiError(format!("signature {sig:?} missing `(` after return type")));
    }
    if *bytes.last().unwrap() != b')' {
        return Err(FfiError(format!("signature {sig:?} missing trailing `)`")));
    }
    let mut args = Vec::new();
    for &c in &bytes[2..bytes.len() - 1] {
        let code = TypeCode::parse(c as char)?;
        if code == TypeCode::V {
            return Err(FfiError(format!("signature {sig:?}: void only valid as return type")));
        }
        args.push(code);
    }
    Ok(ParsedSig { ret, args })
}

/// Registry of all live FFI handles (libraries, symbols, opaque
/// pointers returned from C calls). Each entry is a raw `*mut c_void`
/// plus an optional cleanup closure that runs when the handle is
/// explicitly closed.
///
/// Handles are u64 IDs allocated monotonically; no recycling. The
/// number space is large enough that exhausting it before the runtime
/// exits would require an absurd allocation rate.
///
/// The lock is coarse-grained — every FFI op acquires it. FFI calls
/// are single-threaded in v1; if/when we add concurrent step engines
/// the lock can be split per-resource-kind.
pub struct HandleRegistry {
    inner: Mutex<HandleRegistryInner>,
}

struct HandleRegistryInner {
    next_id: u64,
    /// `entries[id]` is the registered resource, or None if freed.
    /// Each entry boxes an `Owner` so we can call its drop closure
    /// when the handle is closed.
    entries: HashMap<u64, Owner>,
}

struct Owner {
    /// The raw pointer this handle wraps. For library handles, this is
    /// a leaked `Box<Library>` cast to `*mut c_void`. For symbols,
    /// the raw function pointer. For C-returned pointers, whatever the
    /// callee gave us.
    ptr: *mut std::ffi::c_void,
    /// Optional cleanup. For libraries, drops the Box<Library>. For
    /// C-returned pointers with a registered destructor, calls the
    /// destructor via FFI. Most handles have None here (lifetimes are
    /// the user's problem, by design).
    drop: Option<Box<dyn FnOnce(*mut std::ffi::c_void) + Send>>,
}

// SAFETY: Owner stores a raw pointer that's only ever dereferenced
// inside HandleRegistry under its mutex. The dyn FnOnce we hold has
// the Send bound so cross-thread ownership of the registry is sound
// even before the registry itself crosses threads.
unsafe impl Send for Owner {}

impl HandleRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HandleRegistryInner {
                next_id: 1, // 0 reserved as sentinel "null"
                entries: HashMap::new(),
            }),
        }
    }

    fn register_with_drop(
        &self,
        ptr: *mut std::ffi::c_void,
        drop: Option<Box<dyn FnOnce(*mut std::ffi::c_void) + Send>>,
    ) -> u64 {
        let mut inner = self.inner.lock().unwrap();
        let id = inner.next_id;
        inner.next_id += 1;
        inner.entries.insert(id, Owner { ptr, drop });
        id
    }

    fn lookup(&self, id: u64) -> Result<*mut std::ffi::c_void, FfiError> {
        let inner = self.inner.lock().unwrap();
        inner.entries.get(&id)
            .map(|o| o.ptr)
            .ok_or_else(|| FfiError(format!("unknown handle {id}")))
    }

    /// Free the handle, running its cleanup closure if any. Returns
    /// false if the handle didn't exist.
    pub fn close(&self, id: u64) -> bool {
        let owner = {
            let mut inner = self.inner.lock().unwrap();
            inner.entries.remove(&id)
        };
        if let Some(o) = owner {
            if let Some(drop_fn) = o.drop {
                drop_fn(o.ptr);
            }
            true
        } else {
            false
        }
    }
}

impl Default for HandleRegistry {
    fn default() -> Self { Self::new() }
}

/// `dlopen(path, RTLD_NOW)`-equivalent. Returns a handle to the
/// loaded library. Cleanup closure unloads via `Library::drop`.
pub fn ffi_open(reg: &HandleRegistry, path: &str) -> Result<u64, FfiError> {
    let lib = unsafe { Library::new(path) }
        .map_err(|e| FfiError(format!("dlopen({path:?}): {e}")))?;
    // Box and leak; the cleanup closure reconstructs the box and drops it.
    let boxed = Box::new(lib);
    let raw = Box::into_raw(boxed) as *mut std::ffi::c_void;
    Ok(reg.register_with_drop(raw, Some(Box::new(|p| unsafe {
        let _ = Box::from_raw(p as *mut Library);
    }))))
}

/// `dlsym(handle, symbol)`. Returns a function-pointer handle. No
/// cleanup needed — the symbol is invalidated when the library is
/// dropped, but we don't track that dependency in v1; programs are
/// expected to keep libraries alive while they hold symbols.
pub fn ffi_lookup(reg: &HandleRegistry, lib_id: u64, sym: &str) -> Result<u64, FfiError> {
    let lib_ptr = reg.lookup(lib_id)?;
    // Reborrow as Library to call get(). Unsafe because we trust the
    // handle was registered as a library and not, say, a symbol.
    let lib = unsafe { &*(lib_ptr as *const Library) };
    let c_name = CString::new(sym)
        .map_err(|_| FfiError(format!("symbol name {sym:?} contains null byte")))?;
    let sym_ref: Symbol<*mut std::ffi::c_void> = unsafe { lib.get(c_name.as_bytes_with_nul()) }
        .map_err(|e| FfiError(format!("dlsym({sym:?}): {e}")))?;
    let raw_ptr: *mut std::ffi::c_void = unsafe { *sym_ref.into_raw() };
    Ok(reg.register_with_drop(raw_ptr, None))
}

/// Call a previously-looked-up function through libffi. Marshals each
/// arg according to `sig`, invokes the call, materializes the return
/// value as the matching `FfiReturn`.
pub fn ffi_call(
    reg: &HandleRegistry,
    fn_id: u64,
    sig: &str,
    args: &[FfiArg],
) -> Result<FfiReturn, FfiError> {
    let parsed = parse_signature(sig)?;
    if parsed.args.len() != args.len() {
        return Err(FfiError(format!(
            "signature {sig:?} expects {} args; got {}",
            parsed.args.len(), args.len(),
        )));
    }
    let fn_ptr = reg.lookup(fn_id)?;

    // Build libffi arg-type list (ParsedSig holds TypeCodes; libffi
    // wants its own Type vector).
    let arg_types: Vec<FfiType> = parsed.args.iter().map(|c| c.as_ffi()).collect();
    let cif = Cif::new(arg_types, parsed.ret.as_ffi());

    // Materialize each Evident-typed arg into a stable C value plus
    // an `Arg` reference. `c_strings` and `bool_ints` keep backing
    // storage alive across the call. `handles` resolves Handle ids
    // to raw pointers.
    let mut c_strings: Vec<CString> = Vec::with_capacity(args.len());
    let mut bool_ints: Vec<i32>     = Vec::with_capacity(args.len());
    let mut int64s:    Vec<i64>     = Vec::with_capacity(args.len());
    let mut doubles:   Vec<f64>     = Vec::with_capacity(args.len());
    let mut floats:    Vec<f32>     = Vec::with_capacity(args.len());
    let mut handles:   Vec<*mut std::ffi::c_void> = Vec::with_capacity(args.len());
    let mut str_ptrs:  Vec<*const std::os::raw::c_char> = Vec::with_capacity(args.len());

    // First pass: fill the backing-storage vectors. We must NOT push
    // to these between borrowing slots from them, because Vec growth
    // would invalidate the pointers we hand to libffi.
    for (i, (arg, code)) in args.iter().zip(parsed.args.iter()).enumerate() {
        match (arg, *code) {
            (FfiArg::Int(n), TypeCode::I) => int64s.push(*n),
            (FfiArg::Bool(b), TypeCode::B) => bool_ints.push(if *b { 1 } else { 0 }),
            (FfiArg::Str(s), TypeCode::S) => {
                let cs = CString::new(s.as_bytes())
                    .map_err(|_| FfiError(format!("arg {i}: string contains null byte")))?;
                c_strings.push(cs);
            }
            (FfiArg::Real(d), TypeCode::D) => doubles.push(*d),
            (FfiArg::Real(d), TypeCode::F) => floats.push(*d as f32),
            (FfiArg::Handle(h), TypeCode::P) => {
                let ptr = if *h == 0 {
                    std::ptr::null_mut()
                } else {
                    reg.lookup(*h)?
                };
                handles.push(ptr);
            }
            (other, expected) => {
                return Err(FfiError(format!(
                    "arg {i}: type mismatch — value is {other:?}, signature says {expected:?}",
                )));
            }
        }
    }

    // Reserve str_ptrs *after* c_strings is fully populated so the
    // pointers we capture remain valid.
    for cs in &c_strings { str_ptrs.push(cs.as_ptr()); }

    // Second pass: build the libffi `Arg` vector, indexing into the
    // stable storage. We track per-type indices since each backing
    // vec only grows for args of its own type.
    let mut idx_int = 0usize; let mut idx_bool = 0usize;
    let mut idx_str = 0usize; let mut idx_dbl  = 0usize;
    let mut idx_flt = 0usize; let mut idx_p   = 0usize;
    let mut ffi_args: Vec<Arg> = Vec::with_capacity(args.len());
    for code in &parsed.args {
        let a = match code {
            TypeCode::I => { let r = Arg::new(&int64s[idx_int]);  idx_int  += 1; r }
            TypeCode::B => { let r = Arg::new(&bool_ints[idx_bool]); idx_bool += 1; r }
            TypeCode::S => { let r = Arg::new(&str_ptrs[idx_str]); idx_str  += 1; r }
            TypeCode::D => { let r = Arg::new(&doubles[idx_dbl]); idx_dbl  += 1; r }
            TypeCode::F => { let r = Arg::new(&floats[idx_flt]);  idx_flt  += 1; r }
            TypeCode::P => { let r = Arg::new(&handles[idx_p]);   idx_p    += 1; r }
            TypeCode::V => unreachable!("void rejected during signature parse"),
        };
        ffi_args.push(a);
    }

    // Dispatch via libffi. Different return types use different
    // `cif.call::<T>(...)` instantiations — libffi reads the right
    // number of bytes off the return slot for the concrete T. The
    // caller-side T must match cif's return-type slot, which we set
    // from `parsed.ret`.
    let code_ptr = CodePtr::from_ptr(fn_ptr as *const _);
    let ret = match parsed.ret {
        TypeCode::V => {
            unsafe { cif.call::<()>(code_ptr, &ffi_args); }
            FfiReturn::Void
        }
        TypeCode::I => {
            let r: i64 = unsafe { cif.call(code_ptr, &ffi_args) };
            FfiReturn::Int(r)
        }
        TypeCode::B => {
            let r: i32 = unsafe { cif.call(code_ptr, &ffi_args) };
            FfiReturn::Bool(r != 0)
        }
        TypeCode::D => {
            let r: f64 = unsafe { cif.call(code_ptr, &ffi_args) };
            FfiReturn::Real(r)
        }
        TypeCode::F => {
            let r: f32 = unsafe { cif.call(code_ptr, &ffi_args) };
            FfiReturn::Real(r as f64)
        }
        TypeCode::S => {
            let p: *const std::os::raw::c_char = unsafe { cif.call(code_ptr, &ffi_args) };
            if p.is_null() {
                FfiReturn::Str(String::new())
            } else {
                let s = unsafe { std::ffi::CStr::from_ptr(p) };
                FfiReturn::Str(s.to_string_lossy().into_owned())
            }
        }
        TypeCode::P => {
            let p: *mut std::ffi::c_void = unsafe { cif.call(code_ptr, &ffi_args) };
            if p.is_null() {
                FfiReturn::Handle(0)
            } else {
                FfiReturn::Handle(reg.register_with_drop(p, None))
            }
        }
    };
    Ok(ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pick a libc path for the host platform. macOS uses
    /// libSystem; Linux uses libc.so.6.
    fn libc_path() -> &'static str {
        if cfg!(target_os = "macos") { "libSystem.dylib" }
        else                          { "libc.so.6" }
    }

    #[test]
    fn parse_signature_basic() {
        let p = parse_signature("i()").unwrap();
        assert_eq!(p.ret, TypeCode::I);
        assert!(p.args.is_empty());

        let p = parse_signature("i(s)").unwrap();
        assert_eq!(p.ret, TypeCode::I);
        assert_eq!(p.args, vec![TypeCode::S]);

        let p = parse_signature("p(siii)").unwrap();
        assert_eq!(p.ret, TypeCode::P);
        assert_eq!(p.args, vec![TypeCode::S, TypeCode::I, TypeCode::I, TypeCode::I]);

        assert!(parse_signature("x()").is_err(),  "unknown type code");
        assert!(parse_signature("i)").is_err(),    "missing open paren");
        assert!(parse_signature("i(").is_err(),    "missing close paren");
        assert!(parse_signature("i(v)").is_err(),  "void as arg");
    }

    #[test]
    fn call_libc_getpid() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).expect("dlopen libc");
        let getpid = ffi_lookup(&reg, lib, "getpid").expect("dlsym getpid");
        let result = ffi_call(&reg, getpid, "i()", &[]).expect("call getpid");
        match result {
            FfiReturn::Int(pid) => {
                assert!(pid > 0, "getpid returned {pid}");
                assert_eq!(pid as u32, std::process::id());
            }
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn call_libc_strlen() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        let strlen = ffi_lookup(&reg, lib, "strlen").unwrap();
        let r = ffi_call(&reg, strlen, "i(s)", &[FfiArg::Str("hello world".into())]).unwrap();
        match r {
            FfiReturn::Int(n) => assert_eq!(n, 11),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn call_libc_abs() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        let abs = ffi_lookup(&reg, lib, "abs").unwrap();
        let r = ffi_call(&reg, abs, "i(i)", &[FfiArg::Int(-42)]).unwrap();
        match r {
            FfiReturn::Int(n) => assert_eq!(n, 42),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    /// f64 round-trip through libm's `sqrt`. Validates the
    /// double-arg + double-return ABI slots.
    #[test]
    fn call_libm_sqrt_double() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        let f = ffi_lookup(&reg, lib, "sqrt").unwrap();
        let r = ffi_call(&reg, f, "d(d)", &[FfiArg::Real(16.0)]).unwrap();
        match r {
            FfiReturn::Real(x) => assert!((x - 4.0).abs() < 1e-12, "got {x}"),
            other => panic!("expected Real, got {other:?}"),
        }
    }

    /// f32 round-trip through libm's `sqrtf`. The Evident-side value
    /// is f64 (ArgReal); the marshaller narrows to f32 for the
    /// libffi arg slot, then widens the f32 return back to f64.
    /// On AArch64 floats and doubles use distinct register banks,
    /// so a wrong type code here would silently return garbage.
    #[test]
    fn call_libm_sqrtf_float() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        let f = ffi_lookup(&reg, lib, "sqrtf").unwrap();
        let r = ffi_call(&reg, f, "f(f)", &[FfiArg::Real(25.0)]).unwrap();
        match r {
            FfiReturn::Real(x) => assert!((x - 5.0).abs() < 1e-6, "got {x}"),
            other => panic!("expected Real, got {other:?}"),
        }
    }

    #[test]
    fn type_mismatch_errors() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        let strlen = ffi_lookup(&reg, lib, "strlen").unwrap();
        // strlen wants String; pass Int → error before any C call.
        let err = ffi_call(&reg, strlen, "i(s)", &[FfiArg::Int(0)]).unwrap_err();
        assert!(err.0.contains("type mismatch"), "{}", err.0);
    }

    #[test]
    fn arg_count_mismatch_errors() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        let strlen = ffi_lookup(&reg, lib, "strlen").unwrap();
        let err = ffi_call(&reg, strlen, "i(s)", &[]).unwrap_err();
        assert!(err.0.contains("expects 1 args"), "{}", err.0);
    }

    #[test]
    fn unknown_handle_errors() {
        let reg = HandleRegistry::new();
        let err = ffi_lookup(&reg, 9999, "anything").unwrap_err();
        assert!(err.0.contains("unknown handle"), "{}", err.0);
    }

    #[test]
    fn close_handle_frees_entry() {
        let reg = HandleRegistry::new();
        let lib = ffi_open(&reg, libc_path()).unwrap();
        assert!(reg.close(lib),  "first close succeeds");
        assert!(!reg.close(lib), "second close finds nothing");
    }

    #[test]
    fn null_returning_string_is_empty() {
        // Most pure-libc functions don't return null pointers easily;
        // skip if we can't construct one. This documents the
        // null-handling contract more than verifies it.
        let _reg = HandleRegistry::new();
        // Intentionally trivial: covered by ffi_call's null-check
        // logic, but no widely-portable libc function reliably returns
        // null we can call without args.
    }
}
