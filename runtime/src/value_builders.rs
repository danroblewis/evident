//! JIT→Rust callbacks for constructing `Value` variants that require heap allocation.
//! ABI: output `*mut Value`, strings as `(ptr, len)`, ints as `i64`, bools as `i64` 0/1.

use std::collections::HashMap;

use crate::core::Value;

/// Reconstruct a `&str` from a JIT-passed (ptr, len) pair.
/// # Safety: `ptr` must be valid UTF-8 for `len` bytes; JIT strings are interned in `JitProgram::string_pool`.
unsafe fn str_from_raw<'a>(ptr: *const u8, len: usize) -> &'a str {
    let bytes = std::slice::from_raw_parts(ptr, len);
    std::str::from_utf8_unchecked(bytes)
}

/// Initialize an uninitialized stack slot with `Value::Int(0)` via `ptr::write` (no drop).
/// Vec output buffers are pre-initialized by the Rust wrapper, so they don't need this.
#[no_mangle]
pub unsafe extern "C" fn ev_init_slot(out: *mut Value) {
    std::ptr::write(out, Value::Int(0));
}

/// Write `Value::Int(n)` into the slot (drops the previous value).
#[no_mangle]
pub unsafe extern "C" fn ev_set_int(out: *mut Value, n: i64) {
    *out = Value::Int(n);
}

/// Write `Value::Bool(b != 0)` into the slot.
#[no_mangle]
pub unsafe extern "C" fn ev_set_bool(out: *mut Value, b: i64) {
    *out = Value::Bool(b != 0);
}

/// Write `Value::Str(...)` into the slot (copies bytes; `Value::Str` owns a `String`).
#[no_mangle]
pub unsafe extern "C" fn ev_set_str(out: *mut Value, s_ptr: *const u8, s_len: usize) {
    let s = str_from_raw(s_ptr, s_len);
    *out = Value::Str(s.to_string());
}

/// Write a nullary `Value::Enum` (no payload fields).
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

/// Write a single-Int-payload `Value::Enum` (e.g. `Exit(0)`).
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

/// Write a single-String-payload `Value::Enum` (e.g. `Println("hello")`).
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

/// Initialize a `Value::SeqEnum` with capacity `cap`; fill via `ev_seq_push_clone`.
#[no_mangle]
pub unsafe extern "C" fn ev_seq_new(out: *mut Value, cap: usize) {
    *out = Value::SeqEnum(Vec::with_capacity(cap));
}

/// Set `seq[idx]` to a clone of `*elem`, padding with `Int(0)` if needed.
/// Used to materialize a Z3 `(store arr idx val)` chain walking inner-to-outer.
#[no_mangle]
pub unsafe extern "C" fn ev_seq_set(seq: *mut Value, idx: i64, elem: *const Value) {
    let elem = (*elem).clone();
    if let Value::SeqEnum(v) = &mut *seq {
        let i = idx.max(0) as usize;
        if i >= v.len() { v.resize(i + 1, Value::Int(0)); }
        v[i] = elem;
    } else {
        eprintln!("ev_seq_set: target is not a SeqEnum: {:?}", *seq);
    }
}

/// Write a `Value::Composite` record from parallel field-name and field-value arrays.
/// The JIT builds each field into a stack slot, then passes the pointer arrays here.
#[no_mangle]
pub unsafe extern "C" fn ev_set_composite(
    out: *mut Value,
    names_ptr: *const *const u8, name_lens_ptr: *const usize,
    vals_ptr: *const *const Value, n: usize,
) {
    let name_ptrs = std::slice::from_raw_parts(names_ptr, n);
    let name_lens = std::slice::from_raw_parts(name_lens_ptr, n);
    let val_ptrs  = std::slice::from_raw_parts(vals_ptr, n);
    let mut map: HashMap<String, Value> = HashMap::with_capacity(n);
    for i in 0..n {
        let name = str_from_raw(name_ptrs[i], name_lens[i]).to_string();
        map.insert(name, (*val_ptrs[i]).clone());
    }
    *out = Value::Composite(map);
}

/// Push a clone of `*elem` onto the SeqEnum at `seq`.
#[no_mangle]
pub unsafe extern "C" fn ev_seq_push_clone(seq: *mut Value, elem: *const Value) {
    let elem = (*elem).clone();
    if let Value::SeqEnum(v) = &mut *seq {
        v.push(elem);
    } else {
        eprintln!("ev_seq_push_clone: target is not a SeqEnum: {:?}", *seq);
    }
}

/// Write a multi-field `Value::Enum` from an array of pre-built `*const Value` slots.
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
    // Flatten Cons chains in payload fields (same as z3_eval's DT_CONSTRUCTOR); skip for __Cell/__Empty variants.
    let is_cell = variant.starts_with("__Cell_") || variant.starts_with("__Empty_");
    if !is_cell {
        for f in fields.iter_mut() {
            if let Some(flat) = flatten_seq_of_chain(f) { *f = flat; }
        }
    }
    *out = Value::Enum { enum_name, variant, fields };
}

/// Flatten a `__SeqOf_*` Cons chain to `SeqEnum`/`SeqInt`/etc. Mirror of `z3_eval::flatten_seq_of_chain`.
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

/// Clone `value_pool[index]` into the output slot. Used by PreBaked JIT steps.
#[no_mangle]
pub unsafe extern "C" fn ev_clone_from_pool(
    out: *mut Value,
    pool_ptr: *const Value,
    index: usize,
) {
    let src = &*pool_ptr.add(index);
    *out = src.clone();
}

/// Read `Value::Int` from a slot; returns 0 for non-Int (shouldn't happen for well-typed programs).
#[no_mangle]
pub unsafe extern "C" fn ev_load_int(slot: *const Value) -> i64 {
    match &*slot {
        Value::Int(n) => *n,
        _ => 0,
    }
}

/// Symbol table of `(name, addr)` pairs for `JITBuilder::symbol` registration.
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
        ("ev_seq_set",          ev_seq_set          as *const u8),
        ("ev_set_composite",    ev_set_composite    as *const u8),
        ("ev_load_int",         ev_load_int         as *const u8),
        ("ev_set_enum_multifield", ev_set_enum_multifield as *const u8),
        ("ev_clone_from_pool",  ev_clone_from_pool  as *const u8),
        ("ev_seq_extract_field", ev_seq_extract_field as *const u8),
        ("ev_extract_field",    ev_extract_field    as *const u8),
        ("ev_field_ref",        ev_field_ref        as *const u8),
        ("ev_seq_select",       ev_seq_select       as *const u8),
        ("ev_load_bool",        ev_load_bool        as *const u8),
        ("ev_str_concat",       ev_str_concat       as *const u8),
        ("ev_is_variant",       ev_is_variant       as *const u8),
    ]
}

/// Read `Value::Bool` from a slot for ITE conditions; returns 0 for non-Bool.
#[no_mangle]
pub unsafe extern "C" fn ev_load_bool(slot: *const Value) -> i64 {
    match &*slot {
        Value::Bool(b) => if *b { 1 } else { 0 },
        _ => 0,
    }
}

/// Extract a named field into `*out` from a `Value::Enum` (`f0`, `f1`, …) or `Value::Composite`.
#[no_mangle]
pub unsafe extern "C" fn ev_extract_field(
    out: *mut Value,
    src_slot: *const Value,
    name_ptr: *const u8, name_len: usize,
) {
    if src_slot.is_null() { *out = Value::Int(0); return; }
    let name = str_from_raw(name_ptr, name_len);
    if std::env::var("EVIDENT_JIT_CALL_TRACE").is_ok() {
        eprintln!("[jit/extract_field] name={name:?} src={:?}", &*src_slot);
    }
    match &*src_slot {
        Value::Enum { fields, .. } => {
            if let Some(idx_str) = name.strip_prefix('f') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let Some(v) = fields.get(idx) {
                        *out = v.clone();
                        return;
                    }
                }
            }
            // Named enum accessors must be resolved to index by the JIT; no registry at runtime.
            *out = Value::Int(0);
        }
        Value::Composite(map) => {
            if let Some(v) = map.get(name) {
                *out = v.clone();
            } else {
                *out = Value::Int(0);
            }
        }
        _ => { *out = Value::Int(0); }
    }
}

/// Borrow the named field from `*src_slot` without cloning. Returns null if the field is absent
/// or src is null; callers treat null as sentinel `Int(0)`. Avoids clone cost in accessor chains (session YY).
#[no_mangle]
pub unsafe extern "C" fn ev_field_ref(
    src_slot: *const Value,
    name_ptr: *const u8, name_len: usize,
) -> *const Value {
    if src_slot.is_null() { return std::ptr::null(); }
    let name = str_from_raw(name_ptr, name_len);
    match &*src_slot {
        Value::Enum { fields, .. } => {
            if let Some(idx_str) = name.strip_prefix('f') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let Some(v) = fields.get(idx) {
                        return v as *const Value;
                    }
                }
            }
            std::ptr::null()
        }
        Value::Composite(map) => map.get(name)
            .map(|v| v as *const Value)
            .unwrap_or(std::ptr::null()),
        _ => std::ptr::null(),
    }
}

/// Same as `ev_extract_field`; kept distinct for Seq-field call sites in JIT codegen.
#[no_mangle]
pub unsafe extern "C" fn ev_seq_extract_field(
    out: *mut Value,
    src_slot: *const Value,
    name_ptr: *const u8, name_len: usize,
) {
    ev_extract_field(out, src_slot, name_ptr, name_len);
}

/// Index into a Seq value; wraps `SeqComposite` elements as `Composite` for downstream field access.
#[no_mangle]
pub unsafe extern "C" fn ev_seq_select(
    out: *mut Value,
    arr_slot: *const Value,
    idx: i64,
) {
    if arr_slot.is_null() { *out = Value::Int(0); return; }
    let i = idx as usize;
    let v = match &*arr_slot {
        Value::SeqEnum(xs) => xs.get(i).cloned(),
        Value::SeqInt(xs)  => xs.get(i).map(|n| Value::Int(*n)),
        Value::SeqBool(xs) => xs.get(i).map(|b| Value::Bool(*b)),
        Value::SeqStr(xs)  => xs.get(i).map(|s| Value::Str(s.clone())),
        Value::SeqComposite(xs) => xs.get(i).map(|m| Value::Composite(m.clone())),
        other => {
            if std::env::var("EVIDENT_JIT_CALL_TRACE").is_ok() {
                eprintln!("[jit/seq_select] FALLBACK arr={other:?} idx={i}");
            }
            None
        }
    }.unwrap_or(Value::Int(0));
    *out = v;
}

/// Concatenate N `Value::Str` slots into `*out`.
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

/// Test whether `*src_slot` is the named enum variant; returns 1/0.
#[no_mangle]
pub unsafe extern "C" fn ev_is_variant(
    src_slot: *const Value,
    target_ptr: *const u8, target_len: usize,
) -> i64 {
    if src_slot.is_null() { return 0; }
    let target = str_from_raw(target_ptr, target_len);
    if let Value::Enum { variant, .. } = &*src_slot {
        if variant == target { 1 } else { 0 }
    } else { 0 }
}
