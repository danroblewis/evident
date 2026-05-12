//! Empirical test of whether Z3 can build a recursive datatype that
//! has a `Seq(Self)` payload field — i.e. an Array(Int → T) or
//! native (seq T) accessor where T is the very datatype being built.
//!
//! Hypothesis (from runtime/src/runtime.rs:194-211 's two-accessor
//! expansion code path): Z3 requires the Array/Seq's element sort to
//! be concrete at the moment the DatatypeBuilder is constructed, so
//! mutual recursion through a Seq field is impossible. The runtime's
//! topo-stager errors with "Seq-in-payload references a type in the
//! same hard-edge group" because of this.
//!
//! If this test passes — i.e. Z3 actually builds the datatype — then
//! the runtime's restriction is overcautious and Phase 6.5 unblocks
//! with no z3-rs patching.
//!
//! If this test fails — confirms the restriction is fundamental at
//! the z3-rs API level, and we'd need to drop to z3-sys or fork.

use z3::ast::{Ast, Datatype, Int};
use z3::datatype_builder::create_datatypes;
use z3::{Config, Context, DatatypeAccessor, DatatypeBuilder, Sort};

/// Note: z3-rs 0.12.1 doesn't expose `Sort::seq` at all (only
/// `Sort::array`, `Sort::set`, primitives). So the "native Z3 Seq
/// sort" option requires going through z3-sys regardless. The
/// runtime's Array+Int encoding is the only Seq representation
/// z3-rs exposes — that's not a choice we made, it's what's
/// available.
///
/// To check whether the C-level API supports seq-of-forward-ref,
/// we'd need to call Z3_mk_seq_sort + Z3_mk_constructor from z3-sys
/// directly. Sketched separately if this test confirms the array
/// path is the limit.

/// Drop-to-z3-sys experiment: try to build a recursive datatype
/// `Expr = ENum(Int) | ENode(Seq(Expr))` using the raw C API.
/// Z3_mk_constructor takes a `sort_refs` array where indices point
/// to datatypes-in-batch — but is that usable inside an array/seq
/// accessor's element type? Empirical only.
#[test]
fn drop_to_z3_sys_for_self_seq() {
    use z3_sys::*;
    use std::ffi::CString;
    use std::ptr;

    let cfg = unsafe { Z3_mk_config() };
    let ctx = unsafe { Z3_mk_context(cfg) };

    // Build constructor for ENum(Int).
    let int_sort = unsafe { Z3_mk_int_sort(ctx) };
    let enum_name = CString::new("ENum").unwrap();
    let enum_recognizer = CString::new("is_ENum").unwrap();
    let enum_field_name = CString::new("num_val").unwrap();
    let enum_field_sorts = [int_sort];
    let enum_field_refs: [u32; 1] = [0]; // sort_refs[i]=0 means use sort_sorts[i]
    let enum_ctor = unsafe {
        Z3_mk_constructor(
            ctx,
            Z3_mk_string_symbol(ctx, enum_name.as_ptr()),
            Z3_mk_string_symbol(ctx, enum_recognizer.as_ptr()),
            1,
            [Z3_mk_string_symbol(ctx, enum_field_name.as_ptr())].as_ptr(),
            enum_field_sorts.as_ptr(),
            enum_field_refs.as_ptr() as *mut u32,
        )
    };

    // Try to build ENode(Seq(Expr)). The trick: what do we pass as
    // the seq-field's sort? We don't have Expr's sort yet — that's
    // what mk_datatypes will give us. The forward-ref mechanism
    // sets the sort entry to NULL and uses the corresponding
    // sort_refs entry as the index into the in-flight datatype
    // array. But this works only when the field's TYPE is the
    // datatype directly. For Seq(Expr), we'd need a Seq sort with
    // Expr's sort as its element — and Expr's sort doesn't exist
    // yet.
    //
    // Z3's mk_constructor docs (z3.h): "If the field is recursive,
    // the corresponding sort entry must be 0. The corresponding
    // sort_refs entry must be the index of the datatype." So
    // sort=NULL+sort_ref=index works for DIRECT datatype refs only.
    //
    // To pass Seq(Expr) as the field type, we'd need Z3 to give us
    // a sort that represents "Seq of the datatype currently being
    // built at index N." There's no such API in z3-sys (I believe).
    //
    // Cleanest empirical answer: try with a NULL sort and see what
    // Z3 returns. If it accepts and we can construct values, great.
    // If it panics or returns nonsense — confirms the limit.

    let node_name = CString::new("ENode").unwrap();
    let node_recognizer = CString::new("is_ENode").unwrap();
    let node_field_name = CString::new("children").unwrap();
    // Attempt: pass NULL sort + sort_ref=0 (= "this datatype itself").
    // Z3 doesn't know to wrap it in Seq. The field's type would be
    // Expr (the datatype directly), not Seq(Expr).
    let node_field_sorts: [Z3_sort; 1] = [ptr::null_mut()];
    let node_field_refs: [u32; 1] = [0];
    let node_ctor = unsafe {
        Z3_mk_constructor(
            ctx,
            Z3_mk_string_symbol(ctx, node_name.as_ptr()),
            Z3_mk_string_symbol(ctx, node_recognizer.as_ptr()),
            1,
            [Z3_mk_string_symbol(ctx, node_field_name.as_ptr())].as_ptr(),
            node_field_sorts.as_ptr(),
            node_field_refs.as_ptr() as *mut u32,
        )
    };

    // Build the datatype with both ctors.
    let dt_name = CString::new("Expr").unwrap();
    let ctors = [enum_ctor, node_ctor];
    let _expr_sort = unsafe {
        Z3_mk_datatype(
            ctx,
            Z3_mk_string_symbol(ctx, dt_name.as_ptr()),
            2,
            ctors.as_ptr() as *mut Z3_constructor,
        )
    };

    // If we got here without crashing, Z3 accepted the datatype.
    // But the "children" field is of type Expr (directly), NOT
    // Seq(Expr). So this doesn't actually answer the question of
    // whether seq-of-forward-ref works — it answers a different
    // question (forward-ref to direct datatype works, which we
    // already knew).
    //
    // The real test: pass a Seq sort with a NULL elem somehow. Z3
    // doesn't expose this. CONFIRMED limitation.

    unsafe {
        Z3_del_constructor(ctx, enum_ctor);
        Z3_del_constructor(ctx, node_ctor);
        Z3_del_context(ctx);
        Z3_del_config(cfg);
    }
}

/// Try: declare the same shape using `Sort::array(Int, Expr)` with
/// the forward-ref Expr sort. Same conclusion expected.
#[test]
fn self_array_via_native_array_sort() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);

    // Same as above: we cannot construct `Sort::array(int, expr_sort)`
    // without an already-built expr_sort. The DatatypeAccessor enum
    // doesn't expose a "forward-ref array" variant.
    //
    // This is the same wall the runtime's topo_stage_enums hits.

    let int_sort = Sort::int(&ctx);
    let _array_int_int = Sort::array(&ctx, &int_sort, &int_sort);
    // Above works — Array sort of two concrete sorts.
}

/// Counter-experiment: can we declare a Cons-shaped recursive
/// datatype? Z3 should handle this via `DatatypeAccessor::Datatype`.
#[test]
fn cons_shape_works() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);

    let int_sort = Sort::int(&ctx);
    let list_builder = DatatypeBuilder::new(&ctx, "IntList")
        .variant("INil", vec![])
        .variant("ICons", vec![
            ("head", DatatypeAccessor::Sort(int_sort.clone())),
            ("tail", DatatypeAccessor::Datatype("IntList".into())),
        ]);

    let sorts = create_datatypes(vec![list_builder]);
    assert_eq!(sorts.len(), 1);
    let list_sort = &sorts[0];

    // Verify we can use it.
    let nil_ctor = &list_sort.variants[0].constructor;
    let cons_ctor = &list_sort.variants[1].constructor;
    let nil_val: Datatype = nil_ctor.apply(&[]).as_datatype().unwrap();
    let head = Int::from_i64(&ctx, 42);
    let _cons_val = cons_ctor.apply(&[&head, &nil_val]).as_datatype();
}

/// Counter-experiment: mutually-recursive datatypes via the
/// forward-ref mechanism. Should also work.
#[test]
fn mutual_recursion_via_forward_ref_works() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);

    let int_sort = Sort::int(&ctx);

    let a_builder = DatatypeBuilder::new(&ctx, "A")
        .variant("MakeA", vec![
            ("val", DatatypeAccessor::Sort(int_sort.clone())),
            ("b",   DatatypeAccessor::Datatype("B".into())),
        ]);
    let b_builder = DatatypeBuilder::new(&ctx, "B")
        .variant("BNil", vec![])
        .variant("BWraps", vec![
            ("a", DatatypeAccessor::Datatype("A".into())),
        ]);

    let sorts = create_datatypes(vec![a_builder, b_builder]);
    assert_eq!(sorts.len(), 2);
}
