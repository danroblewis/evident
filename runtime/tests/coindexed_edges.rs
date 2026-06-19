use evident_runtime::{EvidentRuntime, Value};

const VEC2: &str = "type IVec2\n    x ∈ Int\n    y ∈ Int\n";

fn ints(v: Option<&Value>) -> Vec<i64> {
    match v {
        Some(Value::SeqInt(xs)) => xs.clone(),
        other => panic!("expected SeqInt, got {:?}", other),
    }
}

#[test]
fn coindexed_two_int_seqs() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    a ∈ Seq(Int)\n    b ∈ Seq(Int)\n    #a = 4\n    #b = 4\n    a[0] = 10\n    a[1] = 20\n    a[2] = 30\n    a[3] = 40\n    ∀ (av, bv) ∈ coindexed(a, b) : bv = av + 1\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(ints(r.bindings.get("b")), vec![11, 21, 31, 41]);
}

#[test]
fn coindexed_three_int_seqs() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    a ∈ Seq(Int)\n    b ∈ Seq(Int)\n    c ∈ Seq(Int)\n    #a = 3\n    #b = 3\n    #c = 3\n    a[0] = 1\n    a[1] = 2\n    a[2] = 3\n    b[0] = 10\n    b[1] = 20\n    b[2] = 30\n    ∀ (x, y, z) ∈ coindexed(a, b, c) : z = x + y\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(ints(r.bindings.get("c")), vec![11, 22, 33]);
}

#[test]
fn coindexed_seq_of_record_with_seq_of_int() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2}schema S\n    dots ∈ Seq(IVec2)\n    yvel ∈ Seq(Int)\n    #dots = 3\n    #yvel = 3\n    dots[0].x = 10\n    dots[0].y = 20\n    dots[1].x = 30\n    dots[1].y = 40\n    dots[2].x = 50\n    dots[2].y = 60\n    ∀ (d, v) ∈ coindexed(dots, yvel) : v = d.x + d.y\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(ints(r.bindings.get("yvel")), vec![30, 70, 110]);
}

#[test]
fn edges_non_decreasing() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    items ∈ Seq(Int)\n    #items = 5\n    items[0] = 10\n    items[1] = 12\n    items[2] = 15\n    items[3] = 17\n    items[4] = 100\n    ∀ (a, b) ∈ edges(items) : a ≤ b\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
}

#[test]
fn edges_violated_is_unsat() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    items ∈ Seq(Int)\n    #items = 4\n    items[0] = 10\n    items[1] = 5\n    items[2] = 15\n    items[3] = 20\n    ∀ (a, b) ∈ edges(items) : a ≤ b\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied);
}

#[test]
fn single_var_binding_unchanged() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    items ∈ Seq(Int)\n    #items = 3\n    items[0] = 1\n    items[1] = 2\n    items[2] = 3\n    ∀ x ∈ items : x ≥ 0\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
}

#[test]
fn exists_tuple_binding() {
    let mut rt = EvidentRuntime::new();
    let src = "schema S\n    items ∈ Seq(Int)\n    #items = 4\n    items[0] = 1\n    items[1] = 1\n    items[2] = 5\n    items[3] = 5\n    flag ∈ Bool\n    flag = (∃ (a, b) ∈ edges(items) : b > a)\n";
    rt.load_source(src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    assert_eq!(r.bindings.get("flag"), Some(&Value::Bool(true)));
}
