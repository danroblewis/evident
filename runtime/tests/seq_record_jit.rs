use std::collections::HashMap;
use evident_runtime::{EvidentRuntime, Value};
use evident_runtime::z3_eval::{simplify_assertions, extract_program_partial,
    recompose_record_seqs};
use evident_runtime::functionize::cranelift::compile_program;

fn jit_run(src: &str, schema: &str, given: &HashMap<String, Value>)
    -> HashMap<String, Value>
{
    let mut rt = EvidentRuntime::new();
    rt.load_source(src).unwrap();

    let ctx       = rt.z3_context();
    let datatypes = rt.datatypes_registry();
    let enums     = rt.enums_registry();
    let schemas   = rt.schemas_map();
    let empty: HashMap<String, Value> = HashMap::new();
    let cached = evident_runtime::encode::build_cache(
        rt.get_schema(schema).unwrap(),
        schemas, ctx, datatypes, Some(enums), &empty, 2);

    let assertions_local = cached.solver.get_assertions();
    let assertions: Vec<z3::ast::Bool<'static>> = unsafe {
        std::mem::transmute::<Vec<z3::ast::Bool<'_>>, Vec<z3::ast::Bool<'static>>>(
            assertions_local)
    };
    let simp = simplify_assertions(ctx, &assertions);

    let outputs: Vec<String> = cached.env.keys()
        .filter(|k| !given.contains_key(k.as_str()))
        .cloned().collect();
    let (mut program, mut missing) =
        extract_program_partial(&simp.formulas, &outputs).expect("extract_program_partial");
    recompose_record_seqs(&simp.formulas, &mut missing, &mut program, datatypes, ctx);

    let jit = compile_program(&program, enums, datatypes).expect("compile_program");
    jit.call(given).expect("jit call")
}

fn given(pairs: &[(&str, i64)]) -> HashMap<String, Value> {
    pairs.iter().map(|(k, v)| (k.to_string(), Value::Int(*v))).collect()
}

#[test]
fn jit_seq_of_flat_record() {
    let src = r#"
type IVec2(x, y ∈ Int)

claim SeqRec
    base ∈ Int
    pts ∈ Seq(IVec2)
    #pts = 3
    pts[0] = IVec2(base + 1, 2)
    pts[1] = IVec2(3, base + 4)
    pts[2] = IVec2(5, 6)
"#;
    let out = jit_run(src, "SeqRec", &given(&[("base", 10)]));
    let Some(Value::SeqComposite(v)) = out.get("pts") else {
        panic!("pts should be SeqComposite, got {:?}", out.get("pts"));
    };
    assert_eq!(v.len(), 3);
    assert_eq!(v[0].get("x"), Some(&Value::Int(11)));
    assert_eq!(v[0].get("y"), Some(&Value::Int(2)));
    assert_eq!(v[1].get("x"), Some(&Value::Int(3)));
    assert_eq!(v[1].get("y"), Some(&Value::Int(14)));
    assert_eq!(v[2].get("x"), Some(&Value::Int(5)));
    assert_eq!(v[2].get("y"), Some(&Value::Int(6)));
}

#[test]
fn jit_seq_of_nested_record() {

    let src = r#"
type IVec2(x, y ∈ Int)
type Color(r, g, b ∈ Int)
type Rect(color ∈ Color, pos ∈ IVec2, size ∈ IVec2)

claim RectSeq
    base ∈ Int
    rects ∈ Seq(Rect)
    #rects = 2
    rects[0] = Rect(Color(220, 40, 40), IVec2(base, 6), IVec2(32, 6))
    rects[1] = Rect(Color(40, 70, 200), IVec2(base + 4, 26), IVec2(24, 6))
"#;
    let out = jit_run(src, "RectSeq", &given(&[("base", 100)]));
    let Some(Value::SeqComposite(v)) = out.get("rects") else {
        panic!("rects should be SeqComposite, got {:?}", out.get("rects"));
    };
    assert_eq!(v.len(), 2);

    let Some(Value::Composite(color0)) = v[0].get("color") else { panic!("color0") };
    assert_eq!(color0.get("r"), Some(&Value::Int(220)));
    assert_eq!(color0.get("g"), Some(&Value::Int(40)));
    assert_eq!(color0.get("b"), Some(&Value::Int(40)));
    let Some(Value::Composite(pos0)) = v[0].get("pos") else { panic!("pos0") };
    assert_eq!(pos0.get("x"), Some(&Value::Int(100)));
    assert_eq!(pos0.get("y"), Some(&Value::Int(6)));

    let Some(Value::Composite(pos1)) = v[1].get("pos") else { panic!("pos1") };
    assert_eq!(pos1.get("x"), Some(&Value::Int(104)));
    let Some(Value::Composite(color1)) = v[1].get("color") else { panic!("color1") };
    assert_eq!(color1.get("b"), Some(&Value::Int(200)));
}
