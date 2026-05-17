//! Cranelift JIT → Rust callback helpers for constructing
//! `Value` enums. The JIT can emit native code for primitive
//! arithmetic and 0-arity enum tags directly, but `Value::Enum`
//! with payloads and `Value::SeqEnum` need Rust-managed heap
//! allocation — strings, Vecs, the tagged-union layout.
//!
//! Each `extern "C"` function in this module takes a raw
//! `*mut Value` pointing into a pre-allocated output buffer the
//! runtime owns, plus the data needed to construct the value.
//! The JIT emits `call_indirect` to these functions with
//! constant-string arg pointers and computed payload values.
//!
//! ABI:
//!   - All output pointers are `*mut Value` (no pointer
//!     arithmetic in the JIT; the runtime passes the slot
//!     pointer directly).
//!   - Strings are passed as `(ptr, len)` byte slices — UTF-8
//!     bytes, no nul terminator required.
//!   - Integer payloads are passed as `i64`.
//!   - Bool is `i64` 0/1.
//!
//! The JIT registers these with `JITBuilder::symbol(name, addr)`
//! and declares them as `Linkage::Import` to call them.

use crate::translate::Value;

/// Reconstruct a `&str` from a JIT-passed (ptr, len) pair.
///
/// # Safety
///
/// `ptr` must be a valid pointer to `len` bytes of UTF-8.
/// Strings emitted by the JIT come from interned `&'static str`
/// values stored in `JitProgram::string_pool`, so they're
/// always valid for the lifetime of the JitProgram.
unsafe fn str_from_raw<'a>(ptr: *const u8, len: usize) -> &'a str {
    let bytes = std::slice::from_raw_parts(ptr, len);
    std::str::from_utf8_unchecked(bytes)
}

/// Initialize an UNINITIALIZED slot with `Value::Int(0)`. Used
/// for stack-allocated temp slots — `*out = ...` would try to
/// drop whatever garbage is in the slot first, which is UB.
/// This helper uses `ptr::write` which does NO drop.
///
/// Output buffers (Vec<Value>) don't need this because the Rust
/// wrapper pre-initializes their slots via `vec![Value::Int(0);
/// n]` before calling the JIT.
#[no_mangle]
pub unsafe extern "C" fn ev_init_slot(out: *mut Value) {
    std::ptr::write(out, Value::Int(0));
}

/// Write `Value::Int(n)` into the slot at `out`. Assumes the
/// slot already holds a valid Value (which will be dropped).
#[no_mangle]
pub unsafe extern "C" fn ev_set_int(out: *mut Value, n: i64) {
    *out = Value::Int(n);
}

/// Write `Value::Bool(b != 0)` into the slot at `out`.
#[no_mangle]
pub unsafe extern "C" fn ev_set_bool(out: *mut Value, b: i64) {
    *out = Value::Bool(b != 0);
}

/// Write `Value::Str(...)` into the slot at `out`. Copies the
/// bytes since `Value::Str` owns a `String`.
#[no_mangle]
pub unsafe extern "C" fn ev_set_str(out: *mut Value, s_ptr: *const u8, s_len: usize) {
    let s = str_from_raw(s_ptr, s_len);
    *out = Value::Str(s.to_string());
}

/// Write `Value::Enum { enum_name, variant, fields: [] }` for a
/// 0-arity (nullary) constructor.
#[no_mangle]
pub unsafe extern "C" fn ev_set_enum_nullary(
    out: *mut Value,
    enum_ptr: *const u8, enum_len: usize,
    variant_ptr: *const u8, variant_len: usize,
) {
    let enum_name = str_from_raw(enum_ptr, enum_len).to_string();
    let variant   = str_from_raw(variant_ptr, variant_len).to_string();
    *out = Value::Enum { enum_name, variant, fields: vec![] };
}

/// Write `Value::Enum { ..., fields: [Value::Int(n)] }` for a
/// single-Int-payload variant (e.g. `Exit(0)`).
#[no_mangle]
pub unsafe extern "C" fn ev_set_enum_int(
    out: *mut Value,
    enum_ptr: *const u8, enum_len: usize,
    variant_ptr: *const u8, variant_len: usize,
    payload: i64,
) {
    let enum_name = str_from_raw(enum_ptr, enum_len).to_string();
    let variant   = str_from_raw(variant_ptr, variant_len).to_string();
    *out = Value::Enum {
        enum_name,
        variant,
        fields: vec![Value::Int(payload)],
    };
}

/// Write `Value::Enum { ..., fields: [Value::Str(payload)] }` for
/// a single-String-payload variant (e.g. `Println("hello")`).
#[no_mangle]
pub unsafe extern "C" fn ev_set_enum_str(
    out: *mut Value,
    enum_ptr: *const u8, enum_len: usize,
    variant_ptr: *const u8, variant_len: usize,
    payload_ptr: *const u8, payload_len: usize,
) {
    let enum_name = str_from_raw(enum_ptr, enum_len).to_string();
    let variant   = str_from_raw(variant_ptr, variant_len).to_string();
    let payload   = str_from_raw(payload_ptr, payload_len).to_string();
    *out = Value::Enum {
        enum_name,
        variant,
        fields: vec![Value::Str(payload)],
    };
}

/// Initialize `Value::SeqEnum(Vec::with_capacity(cap))` at the
/// output slot. The runtime calls `ev_seq_push_clone` for each
/// element afterward.
#[no_mangle]
pub unsafe extern "C" fn ev_seq_new(out: *mut Value, cap: usize) {
    *out = Value::SeqEnum(Vec::with_capacity(cap));
}

/// Append a clone of `*elem` to the SeqEnum at `seq`.
#[no_mangle]
pub unsafe extern "C" fn ev_seq_push_clone(seq: *mut Value, elem: *const Value) {
    let elem = (*elem).clone();
    if let Value::SeqEnum(v) = &mut *seq {
        v.push(elem);
    } else {
        eprintln!("ev_seq_push_clone: target is not a SeqEnum: {:?}", *seq);
    }
}

/// Read a Value::Int from a slot — used for chain steps that
/// reference an earlier output. Returns 0 if the slot isn't
/// Int-typed (shouldn't happen for well-typed programs).
#[no_mangle]
pub unsafe extern "C" fn ev_load_int(slot: *const Value) -> i64 {
    match &*slot {
        Value::Int(n) => *n,
        _ => 0,
    }
}

/// Return the function-pointer table the JIT uses to register
/// symbols with the JITBuilder. Pairs of `(name, addr)`.
pub fn symbol_table() -> Vec<(&'static str, *const u8)> {
    vec![
        ("ev_init_slot",        ev_init_slot        as *const u8),
        ("ev_set_int",          ev_set_int          as *const u8),
        ("ev_set_bool",         ev_set_bool         as *const u8),
        ("ev_set_str",          ev_set_str          as *const u8),
        ("ev_set_enum_nullary", ev_set_enum_nullary as *const u8),
        ("ev_set_enum_int",     ev_set_enum_int     as *const u8),
        ("ev_set_enum_str",     ev_set_enum_str     as *const u8),
        ("ev_seq_new",          ev_seq_new          as *const u8),
        ("ev_seq_push_clone",   ev_seq_push_clone   as *const u8),
        ("ev_load_int",         ev_load_int         as *const u8),
    ]
}
