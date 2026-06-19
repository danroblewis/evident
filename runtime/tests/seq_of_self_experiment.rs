use z3::ast::{Ast, Datatype, Int};
use z3::datatype_builder::create_datatypes;
use z3::{Config, Context, DatatypeAccessor, DatatypeBuilder, Sort};

#[test]
fn drop_to_z3_sys_for_self_seq() {
    use z3_sys::*;
    use std::ffi::CString;
    use std::ptr;

    let cfg = unsafe { Z3_mk_config() };
    let ctx = unsafe { Z3_mk_context(cfg) };

    let int_sort = unsafe { Z3_mk_int_sort(ctx) };
    let enum_name = CString::new("ENum").unwrap();
    let enum_recognizer = CString::new("is_ENum").unwrap();
    let enum_field_name = CString::new("num_val").unwrap();
    let enum_field_sorts = [int_sort];
    let enum_field_refs: [u32; 1] = [0];
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

    let node_name = CString::new("ENode").unwrap();
    let node_recognizer = CString::new("is_ENode").unwrap();
    let node_field_name = CString::new("children").unwrap();

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

    unsafe {
        Z3_del_constructor(ctx, enum_ctor);
        Z3_del_constructor(ctx, node_ctor);
        Z3_del_context(ctx);
        Z3_del_config(cfg);
    }
}

#[test]
fn self_array_via_native_array_sort() {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);

    let int_sort = Sort::int(&ctx);
    let _array_int_int = Sort::array(&ctx, &int_sort, &int_sort);

}

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

    let nil_ctor = &list_sort.variants[0].constructor;
    let cons_ctor = &list_sort.variants[1].constructor;
    let nil_val: Datatype = nil_ctor.apply(&[]).as_datatype().unwrap();
    let head = Int::from_i64(&ctx, 42);
    let _cons_val = cons_ctor.apply(&[&head, &nil_val]).as_datatype();
}

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
