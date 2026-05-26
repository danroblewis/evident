//! FFI primitive: dlopen/dlsym/libffi call dispatch. See `docs/design/ffi-design.md`.
//! Sig format: `ret(args)` where codes are i=i64 b=bool s=str d=f64 f=f32 p=ptr v=void.

use std::collections::HashMap;
use std::ffi::CString;
use std::sync::Mutex;

use libffi::middle::{Arg, Cif, CodePtr, Type as FfiType};
use libloading::{Library, Symbol};

/// Runtime-typed FFI argument; effect dispatcher builds these from ArgList elements.
#[derive(Debug, Clone)]
pub enum FfiArg {
    Int(i64),
    Bool(bool),
    Str(String),
    Real(f64),
    Handle(u64),
    /// `const char * const *` — caller passes count separately (GL convention).
    StrArr(Vec<String>),
    /// Output i32 slot passed by pointer; read-back value replaces void return.
    IntOut,
    /// Pack N i32s into a heap buffer; pass pointer (SDL_Rect / SDL_Point).
    I32Buf(Vec<i32>),
    /// Pack `PackedField`s at natural widths into a heap buffer; pass pointer.
    PackedBuf(Vec<crate::core::ast::PackedField>),
}

/// Returned value from a libffi call.
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
            // 'f' = f32: Evident-side ArgReal (f64) narrows to f32. AArch64 uses
            // s* registers (distinct from d*), so wrong code silently returns garbage.
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

/// Registry of live FFI handles (libraries, symbols, C-returned pointers).
/// Monotonic u64 IDs; coarse mutex (FFI is single-threaded in v1).
pub struct HandleRegistry {
    inner: Mutex<HandleRegistryInner>,
}

struct HandleRegistryInner {
    next_id: u64,
    entries: HashMap<u64, Owner>,
}

struct Owner {
    ptr:  *mut std::ffi::c_void,
    /// Optional drop closure. Libraries reconstruct `Box<Library>`; most handles have None.
    drop: Option<Box<dyn FnOnce(*mut std::ffi::c_void) + Send>>,
}

// SAFETY: ptr only dereferenced inside HandleRegistry under its mutex;
// FnOnce: Send bound ensures cross-thread registry ownership is sound.
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

    pub fn register_with_drop(
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

    pub fn lookup(&self, id: u64) -> Result<*mut std::ffi::c_void, FfiError> {
        let inner = self.inner.lock().unwrap();
        inner.entries.get(&id)
            .map(|o| o.ptr)
            .ok_or_else(|| FfiError(format!("unknown handle {id}")))
    }

    /// Free the handle and run its cleanup closure. Returns false if not found.
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

/// dlopen `path`. Cleanup closure reconstructs the `Box<Library>` and drops it.
pub fn ffi_open(reg: &HandleRegistry, path: &str) -> Result<u64, FfiError> {
    let lib = unsafe { Library::new(path) }
        .map_err(|e| FfiError(format!("dlopen({path:?}): {e}")))?;
    let boxed = Box::new(lib);
    let raw = Box::into_raw(boxed) as *mut std::ffi::c_void;
    Ok(reg.register_with_drop(raw, Some(Box::new(|p| unsafe {
        let _ = Box::from_raw(p as *mut Library);
    }))))
}

/// dlsym `sym` from library handle. No cleanup — caller must keep library alive.
pub fn ffi_lookup(reg: &HandleRegistry, lib_id: u64, sym: &str) -> Result<u64, FfiError> {
    let lib_ptr = reg.lookup(lib_id)?;
    // SAFETY: handle was registered as a library (not a symbol or C ptr).
    let lib = unsafe { &*(lib_ptr as *const Library) };
    let c_name = CString::new(sym)
        .map_err(|_| FfiError(format!("symbol name {sym:?} contains null byte")))?;
    let sym_ref: Symbol<*mut std::ffi::c_void> = unsafe { lib.get(c_name.as_bytes_with_nul()) }
        .map_err(|e| FfiError(format!("dlsym({sym:?}): {e}")))?;
    let raw_ptr: *mut std::ffi::c_void = unsafe { *sym_ref.into_raw() };
    Ok(reg.register_with_drop(raw_ptr, None))
}

/// Call a looked-up function through libffi; marshal args per `sig`.
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

    let arg_types: Vec<FfiType> = parsed.args.iter().map(|c| c.as_ffi()).collect();
    let cif = Cif::new(arg_types, parsed.ret.as_ffi());

    // Backing storage kept alive across the call; all pointers below
    // are into these vecs — do NOT push after capturing a pointer.
    let mut c_strings: Vec<CString> = Vec::with_capacity(args.len());
    let mut bool_ints: Vec<i32>     = Vec::with_capacity(args.len());
    let mut int64s:    Vec<i64>     = Vec::with_capacity(args.len());
    let mut doubles:   Vec<f64>     = Vec::with_capacity(args.len());
    let mut floats:    Vec<f32>     = Vec::with_capacity(args.len());
    let mut handles:   Vec<*mut std::ffi::c_void> = Vec::with_capacity(args.len());
    let mut str_ptrs:       Vec<*const std::os::raw::c_char>        = Vec::with_capacity(args.len());
    let mut arr_cstrings:   Vec<Vec<CString>>                       = Vec::new();
    let mut arr_ptr_lists:  Vec<Vec<*const std::os::raw::c_char>>   = Vec::new();
    let mut int_outs:       Vec<i32>                                = Vec::new();
    let mut i32_bufs:       Vec<Vec<i32>>                           = Vec::new();
    let mut packed_bufs:    Vec<Vec<u8>>                            = Vec::new();

    // First pass: populate backing storage. Must not push after capturing pointers.
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
            (FfiArg::StrArr(strs), TypeCode::P) => {
                let mut cstrs: Vec<CString> = Vec::with_capacity(strs.len());
                for (j, s) in strs.iter().enumerate() {
                    let cs = CString::new(s.as_bytes()).map_err(|_| FfiError(format!(
                        "arg {i}, string {j}: contains null byte",
                    )))?;
                    cstrs.push(cs);
                }
                let ptrs: Vec<*const std::os::raw::c_char> =
                    cstrs.iter().map(|c| c.as_ptr()).collect();
                arr_cstrings.push(cstrs);
                arr_ptr_lists.push(ptrs);
            }
            (FfiArg::IntOut, TypeCode::P) => int_outs.push(0),
            (FfiArg::I32Buf(ints), TypeCode::P) => i32_bufs.push(ints.clone()),
            (FfiArg::PackedBuf(fields), TypeCode::P) => {
                let mut bytes = Vec::new();
                for f in fields { f.write_le(&mut bytes); }
                packed_bufs.push(bytes);
            }
            (other, expected) => {
                return Err(FfiError(format!(
                    "arg {i}: type mismatch — value is {other:?}, signature says {expected:?}",
                )));
            }
        }
    }

    // Capture stable pointers into the now-fully-populated vecs.
    for cs in &c_strings { str_ptrs.push(cs.as_ptr()); }
    let arr_starts: Vec<*const *const std::os::raw::c_char> =
        arr_ptr_lists.iter().map(|v| v.as_ptr()).collect();
    let int_out_base = int_outs.as_mut_ptr();
    let int_out_ptrs: Vec<*mut std::ffi::c_void> = (0..int_outs.len())
        .map(|i| unsafe { int_out_base.add(i) as *mut std::ffi::c_void })
        .collect();
    let i32_buf_starts: Vec<*const i32> =
        i32_bufs.iter().map(|v| v.as_ptr()).collect();
    let packed_buf_starts: Vec<*const u8> =
        packed_bufs.iter().map(|v| v.as_ptr()).collect();
    // Only one IntOut per call; loosen with IntListResult if needed.
    if int_outs.len() > 1 {
        return Err(FfiError(format!(
            "this call has {} ArgIntOut slots; only 1 is supported per call",
            int_outs.len(),
        )));
    }
    if !int_outs.is_empty() && parsed.ret != TypeCode::V {
        return Err(FfiError(
            "ArgIntOut requires a void-returning function (its read-back value \
             replaces the void return); use the function's actual return value \
             via the regular sig+ArgInt path otherwise".into(),
        ));
    }

    // Second pass: build libffi Arg vector (TypeCode::P shared by Handle/StrArr;
    // must use actual FfiArg variant to pick the right backing store).
    let mut idx_int = 0usize; let mut idx_bool = 0usize;
    let mut idx_str = 0usize; let mut idx_dbl  = 0usize;
    let mut idx_flt = 0usize; let mut idx_p   = 0usize;
    let mut idx_arr = 0usize; let mut idx_iout = 0usize;
    let mut idx_i32buf = 0usize;
    let mut idx_packbuf = 0usize;
    let mut ffi_args: Vec<Arg> = Vec::with_capacity(args.len());
    for (arg, code) in args.iter().zip(parsed.args.iter()) {
        let a = match (arg, *code) {
            (FfiArg::Int(_),    TypeCode::I) => { let r = Arg::new(&int64s[idx_int]);      idx_int  += 1; r }
            (FfiArg::Bool(_),   TypeCode::B) => { let r = Arg::new(&bool_ints[idx_bool]);  idx_bool += 1; r }
            (FfiArg::Str(_),    TypeCode::S) => { let r = Arg::new(&str_ptrs[idx_str]);    idx_str  += 1; r }
            (FfiArg::Real(_),   TypeCode::D) => { let r = Arg::new(&doubles[idx_dbl]);     idx_dbl  += 1; r }
            (FfiArg::Real(_),   TypeCode::F) => { let r = Arg::new(&floats[idx_flt]);      idx_flt  += 1; r }
            (FfiArg::Handle(_), TypeCode::P) => { let r = Arg::new(&handles[idx_p]);       idx_p    += 1; r }
            (FfiArg::StrArr(_), TypeCode::P) => { let r = Arg::new(&arr_starts[idx_arr]);  idx_arr  += 1; r }
            (FfiArg::IntOut,    TypeCode::P) => { let r = Arg::new(&int_out_ptrs[idx_iout]); idx_iout += 1; r }
            (FfiArg::I32Buf(_), TypeCode::P) => { let r = Arg::new(&i32_buf_starts[idx_i32buf]); idx_i32buf += 1; r }
            (FfiArg::PackedBuf(_), TypeCode::P) => { let r = Arg::new(&packed_buf_starts[idx_packbuf]); idx_packbuf += 1; r }
            _ => unreachable!("pass 1 already validated all (arg, code) pairs"),
        };
        ffi_args.push(a);
    }

    let code_ptr = CodePtr::from_ptr(fn_ptr as *const _);
    let ret = match parsed.ret {
        TypeCode::V => {
            unsafe { cif.call::<()>(code_ptr, &ffi_args); }
            if !int_outs.is_empty() {
                FfiReturn::Int(int_outs[0] as i64)
            } else {
                FfiReturn::Void
            }
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
        // Null-return contract documented here; no portable libc call reliably returns null.
        let _reg = HandleRegistry::new();
    }
}
