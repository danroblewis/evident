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

/// Write `Value::Enum { enum_name, variant, fields }` where each
/// field is read from a pre-built slot. `args_ptr` points to an
/// array of `*const Value` slots; the helper clones each into the
/// constructor's `fields` Vec. The JIT writes each field into its
/// own stack slot (built via emit_write_value), then passes the
/// array of pointers to this helper.
#[no_mangle]
pub unsafe extern "C" fn ev_set_enum_multifield(
    out: *mut Value,
    enum_ptr: *const u8, enum_len: usize,
    variant_ptr: *const u8, variant_len: usize,
    args_ptr: *const *const Value, args_len: usize,
) {
    let enum_name = str_from_raw(enum_ptr, enum_len).to_string();
    let variant   = str_from_raw(variant_ptr, variant_len).to_string();
    let slice = std::slice::from_raw_parts(args_ptr, args_len);
    let mut fields: Vec<Value> = Vec::with_capacity(args_len);
    for p in slice {
        fields.push((**p).clone());
    }
    // Apply the same Cons-chain → SeqEnum normalization as
    // z3_eval's DT_CONSTRUCTOR handler. Without this, LibCall's
    // `args` field would be a Value::Enum (__SeqOf_FFIArg / __Cell)
    // and downstream decode_arg_list would reject it.
    let is_cell = variant.starts_with("__Cell_") || variant.starts_with("__Empty_");
    if !is_cell {
        for f in fields.iter_mut() {
            if let Some(flat) = flatten_seq_of_chain(f) { *f = flat; }
        }
    }
    *out = Value::Enum { enum_name, variant, fields };
}

/// Mirror of `z3_eval::flatten_seq_of_chain` — used by the
/// multifield enum helper to flatten Cons chains in payload fields
/// at construction time.
fn flatten_seq_of_chain(v: &Value) -> Option<Value> {
    let Value::Enum { enum_name, .. } = v else { return None };
    if !enum_name.starts_with("__SeqOf_") { return None; }
    let mut out: Vec<Value> = Vec::new();
    let mut cur = v;
    loop {
        let Value::Enum { variant, fields, .. } = cur else { return None };
        if variant.starts_with("__Empty_") { break; }
        if !variant.starts_with("__Cell_") { return None; }
        if fields.len() != 2 { return None; }
        let mut head = fields[0].clone();
        if let Value::Enum { variant: hv, fields: hf, .. } = &mut head {
            if !hv.starts_with("__Cell_") && !hv.starts_with("__Empty_") {
                for f in hf.iter_mut() {
                    if let Some(flat) = flatten_seq_of_chain(f) { *f = flat; }
                }
            }
        }
        out.push(head);
        cur = &fields[1];
    }
    // Classify like seq_value_from_elements: enum → SeqEnum;
    // other primitives based on first element.
    Some(match out.first() {
        None => Value::SeqEnum(vec![]),
        Some(Value::Int(_)) => Value::SeqInt(out.into_iter().filter_map(|v|
            if let Value::Int(n) = v { Some(n) } else { None }).collect()),
        Some(Value::Bool(_)) => Value::SeqBool(out.into_iter().filter_map(|v|
            if let Value::Bool(b) = v { Some(b) } else { None }).collect()),
        Some(Value::Str(_)) => Value::SeqStr(out.into_iter().filter_map(|v|
            if let Value::Str(s) = v { Some(s) } else { None }).collect()),
        _ => Value::SeqEnum(out),
    })
}

/// Clone a Value from a static pool slot into the output slot.
/// Used by PreBaked steps — at JIT compile time the value is
/// stashed in `JitProgram::value_pool`, and the JIT emits a call
/// to this helper with the pool index.
#[no_mangle]
pub unsafe extern "C" fn ev_clone_from_pool(
    out: *mut Value,
    pool_ptr: *const Value,
    index: usize,
) {
    let src = &*pool_ptr.add(index);
    *out = src.clone();
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
        ("ev_set_enum_multifield", ev_set_enum_multifield as *const u8),
        ("ev_clone_from_pool",  ev_clone_from_pool  as *const u8),
        ("ev_seq_extract_field", ev_seq_extract_field as *const u8),
        ("ev_extract_field",    ev_extract_field    as *const u8),
        ("ev_seq_select",       ev_seq_select       as *const u8),
        ("ev_load_bool",        ev_load_bool        as *const u8),
        ("ev_str_concat",       ev_str_concat       as *const u8),
        ("ev_is_variant",       ev_is_variant       as *const u8),
    ]
}

/// Read a Value::Bool from a slot — used for ITE conditions.
#[no_mangle]
pub unsafe extern "C" fn ev_load_bool(slot: *const Value) -> i64 {
    match &*slot {
        Value::Bool(b) => if *b { 1 } else { 0 },
        _ => 0,
    }
}

/// `*out = (*src_slot).<field_name>` where src_slot holds an
/// enum value (Value::Enum). Looks up field index by name using
/// a length-prefixed lookup table stored as compile-time constants.
/// Simpler approach: use the EnumRegistry indirectly via a name
/// stored in the JIT's value_pool... but that complicates ABI.
/// Instead, we resolve the field index at compile time and pass
/// it directly as a small integer.
#[no_mangle]
pub unsafe extern "C" fn ev_extract_field(
    out: *mut Value,
    src_slot: *const Value,
    field_idx: usize,
) {
    if let Value::Enum { fields, .. } = &*src_slot {
        if let Some(v) = fields.get(field_idx) {
            *out = v.clone();
            return;
        }
    }
    *out = Value::Int(0);
}

/// `*out = (*src_slot).<field>` where field is a Seq-typed enum
/// field whose Z3 representation is `<field>__arr`. The runtime
/// Value model stores the Seq directly as a Value::Seq* in the
/// field position, so this is equivalent to ev_extract_field.
/// Kept separate for clarity in JIT codegen.
#[no_mangle]
pub unsafe extern "C" fn ev_seq_extract_field(
    out: *mut Value,
    src_slot: *const Value,
    field_idx: usize,
) {
    ev_extract_field(out, src_slot, field_idx);
}

/// `*out = (*arr_slot)[idx]` — index into a SeqEnum/SeqInt/etc.
#[no_mangle]
pub unsafe extern "C" fn ev_seq_select(
    out: *mut Value,
    arr_slot: *const Value,
    idx: i64,
) {
    let i = idx as usize;
    let v = match &*arr_slot {
        Value::SeqEnum(xs) => xs.get(i).cloned(),
        Value::SeqInt(xs)  => xs.get(i).map(|n| Value::Int(*n)),
        Value::SeqBool(xs) => xs.get(i).map(|b| Value::Bool(*b)),
        Value::SeqStr(xs)  => xs.get(i).map(|s| Value::Str(s.clone())),
        _ => None,
    }.unwrap_or(Value::Int(0));
    *out = v;
}

/// Concatenate N String slots into the output. `args_ptr` is an
/// array of `*const Value` Str slots, `args_len` is the count.
#[no_mangle]
pub unsafe extern "C" fn ev_str_concat(
    out: *mut Value,
    args_ptr: *const *const Value, args_len: usize,
) {
    let slice = std::slice::from_raw_parts(args_ptr, args_len);
    let mut s = String::new();
    for p in slice {
        if let Value::Str(t) = &**p {
            s.push_str(t);
        }
    }
    *out = Value::Str(s);
}

/// Test whether a Value::Enum's variant equals `target`. Returns
/// 1 if so, 0 otherwise. Used by IsVariant recognizer ops.
#[no_mangle]
pub unsafe extern "C" fn ev_is_variant(
    src_slot: *const Value,
    target_ptr: *const u8, target_len: usize,
) -> i64 {
    let target = str_from_raw(target_ptr, target_len);
    if let Value::Enum { variant, .. } = &*src_slot {
        if variant == target { 1 } else { 0 }
    } else { 0 }
}
