use std::collections::HashMap;

use crate::core::Value;

unsafe fn str_from_raw<'a>(ptr: *const u8, len: usize) -> &'a str {
    let bytes = std::slice::from_raw_parts(ptr, len);
    std::str::from_utf8_unchecked(bytes)
}

#[no_mangle]
pub unsafe extern "C" fn ev_init_slot(out: *mut Value) {
    std::ptr::write(out, Value::Int(0));
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_int(out: *mut Value, n: i64) {
    *out = Value::Int(n);
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_bool(out: *mut Value, b: i64) {
    *out = Value::Bool(b != 0);
}

#[no_mangle]
pub unsafe extern "C" fn ev_set_str(out: *mut Value, s_ptr: *const u8, s_len: usize) {
    let s = str_from_raw(s_ptr, s_len);
    *out = Value::Str(s.to_string());
}

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

#[no_mangle]
pub unsafe extern "C" fn ev_seq_new(out: *mut Value, cap: usize) {
    *out = Value::SeqEnum(Vec::with_capacity(cap));
}

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

#[no_mangle]
pub unsafe extern "C" fn ev_seq_push_clone(seq: *mut Value, elem: *const Value) {
    let elem = (*elem).clone();
    if let Value::SeqEnum(v) = &mut *seq {
        v.push(elem);
    } else {
        eprintln!("ev_seq_push_clone: target is not a SeqEnum: {:?}", *seq);
    }
}

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

    let is_cell = variant.starts_with("__Cell_") || variant.starts_with("__Empty_");
    if !is_cell {
        for f in fields.iter_mut() {
            if let Some(flat) = flatten_seq_of_chain(f) { *f = flat; }
        }
    }
    *out = Value::Enum { enum_name, variant, fields };
}

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

#[no_mangle]
pub unsafe extern "C" fn ev_clone_from_pool(
    out: *mut Value,
    pool_ptr: *const Value,
    index: usize,
) {
    let src = &*pool_ptr.add(index);
    *out = src.clone();
}

#[no_mangle]
pub unsafe extern "C" fn ev_load_int(slot: *const Value) -> i64 {
    match &*slot {
        Value::Int(n) => *n,
        _ => 0,
    }
}

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
        ("ev_seq_select",       ev_seq_select       as *const u8),
        ("ev_load_bool",        ev_load_bool        as *const u8),
        ("ev_is_variant",       ev_is_variant       as *const u8),
    ]
}

#[no_mangle]
pub unsafe extern "C" fn ev_load_bool(slot: *const Value) -> i64 {
    match &*slot {
        Value::Bool(b) => if *b { 1 } else { 0 },
        _ => 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn ev_extract_field(
    out: *mut Value,
    src_slot: *const Value,
    name_ptr: *const u8, name_len: usize,
) {
    let name = str_from_raw(name_ptr, name_len);
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

#[no_mangle]
pub unsafe extern "C" fn ev_seq_extract_field(
    out: *mut Value,
    src_slot: *const Value,
    name_ptr: *const u8, name_len: usize,
) {
    ev_extract_field(out, src_slot, name_ptr, name_len);
}

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
        Value::SeqComposite(xs) => xs.get(i).map(|m| Value::Composite(m.clone())),
        _ => None,
    }.unwrap_or(Value::Int(0));
    *out = v;
}

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
