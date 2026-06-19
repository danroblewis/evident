use evident_runtime::{EvidentRuntime, Value};

const VEC2: &str = "type IVec2\n    x ∈ Int\n    y ∈ Int\n";

fn check_int(v: Option<&Value>, expected: i64, label: &str) {
    match v {
        Some(Value::Int(n)) => assert_eq!(*n, expected, "{label}"),
        other => panic!("{label}: expected Int, got {:?}", other),
    }
}

#[test]
fn vec_sub_bare_identifiers() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ IVec2\n    b ∈ IVec2\n    c ∈ IVec2\n    a.x = 100\n    a.y = 200\n    b.x = 30\n    b.y = 50\n    c = a - b\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    check_int(r.bindings.get("c.x"), 70, "c.x");
    check_int(r.bindings.get("c.y"), 150, "c.y");
}

#[test]
fn vec_add_bare_identifiers() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ IVec2\n    b ∈ IVec2\n    c ∈ IVec2\n    a.x = 1\n    a.y = 2\n    b.x = 10\n    b.y = 20\n    c = a + b\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    check_int(r.bindings.get("c.x"), 11, "c.x");
    check_int(r.bindings.get("c.y"), 22, "c.y");
}

#[test]
fn vec_times_scalar() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ IVec2\n    c ∈ IVec2\n    dt ∈ Int\n    a.x = 3\n    a.y = 5\n    dt = 4\n    c = a * dt\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    check_int(r.bindings.get("c.x"), 12, "c.x");
    check_int(r.bindings.get("c.y"), 20, "c.y");
}

#[test]
fn vec_compound_subtraction() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}type Player\n    pos ∈ IVec2\nschema S\n    state ∈ Player\n    window_drag ∈ IVec2\n    dragged ∈ Player\n    state.pos.x = 380\n    state.pos.y = 280\n    window_drag.x = 25\n    window_drag.y = 10\n    dragged.pos = state.pos - window_drag\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    check_int(r.bindings.get("dragged.pos.x"), 355, "dragged.pos.x");
    check_int(r.bindings.get("dragged.pos.y"), 270, "dragged.pos.y");
}

#[test]
fn vec_field_of_index_arith() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}type Dot\n    pos ∈ IVec2\n    vel ∈ IVec2\ntype S\n    cur ∈ Seq(Dot)\n    nxt ∈ Seq(Dot)\n    #cur = 2\n    #nxt = 2\n    cur[0].pos.x = 10\n    cur[0].pos.y = 20\n    cur[0].vel.x = 1\n    cur[0].vel.y = 2\n    cur[1].pos.x = 100\n    cur[1].pos.y = 200\n    cur[1].vel.x = 3\n    cur[1].vel.y = 4\n    ∀ i ∈ {{0..#cur - 1}} : nxt[i].pos = cur[i].pos + cur[i].vel\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let Some(Value::SeqComposite(nxt)) = r.bindings.get("nxt") else {
        panic!("expected SeqComposite for nxt");
    };
    let pos0 = match nxt[0].get("pos") {
        Some(Value::Composite(m)) => m,
        other => panic!("nxt[0].pos: {:?}", other),
    };
    assert_eq!(pos0.get("x"), Some(&Value::Int(11)));
    assert_eq!(pos0.get("y"), Some(&Value::Int(22)));
    let pos1 = match nxt[1].get("pos") {
        Some(Value::Composite(m)) => m,
        other => panic!("nxt[1].pos: {:?}", other),
    };
    assert_eq!(pos1.get("x"), Some(&Value::Int(103)));
    assert_eq!(pos1.get("y"), Some(&Value::Int(204)));
}

#[test]
fn vec_index_lhs_assignment_from_record() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2}type Rect\n    pos  ∈ IVec2\n    size ∈ IVec2\nschema S\n    rects ∈ Seq(Rect)\n    src ∈ Rect\n    #rects = 1\n    src.pos.x = 7\n    src.pos.y = 8\n    src.size.x = 30\n    src.size.y = 40\n    rects[0] = src\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    let Some(Value::SeqComposite(items)) = r.bindings.get("rects") else {
        panic!("expected SeqComposite for rects");
    };
    assert_eq!(items.len(), 1);
    let pos = match items[0].get("pos") {
        Some(Value::Composite(m)) => m,
        other => panic!("rects[0].pos: {:?}", other),
    };
    assert_eq!(pos.get("x"), Some(&Value::Int(7)));
    assert_eq!(pos.get("y"), Some(&Value::Int(8)));
    let size = match items[0].get("size") {
        Some(Value::Composite(m)) => m,
        other => panic!("rects[0].size: {:?}", other),
    };
    assert_eq!(size.get("x"), Some(&Value::Int(30)));
    assert_eq!(size.get("y"), Some(&Value::Int(40)));
}

#[test]
fn vec_chained_with_offset_arith() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2}schema S\n    dot ∈ IVec2\n    lo ∈ IVec2\n    hi ∈ IVec2\n    player ∈ IVec2\n    dot.x = 100\n    dot.y = 200\n    lo.x = -10\n    lo.y = -10\n    hi.x = 10\n    hi.y = 10\n    dot + lo ≤ player ≤ dot + hi\n    player.x = 95\n    player.y = 205\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied, "player at (95,205) is inside box (90..110, 190..210)");
}

#[test]
fn vec_chained_with_offset_arith_unsat_when_outside() {
    let mut rt = EvidentRuntime::new();
    let src = format!(
        "{VEC2}schema S\n    dot ∈ IVec2\n    lo ∈ IVec2\n    hi ∈ IVec2\n    player ∈ IVec2\n    dot.x = 100\n    dot.y = 200\n    lo.x = -10\n    lo.y = -10\n    hi.x = 10\n    hi.y = 10\n    dot + lo ≤ player ≤ dot + hi\n    player.x = 95\n    player.y = 999\n"
    );
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(!r.satisfied, "player.y = 999 is outside box hi.y = 210");
}

#[test]
fn vec_eq_via_broadcast() {
    let mut rt = EvidentRuntime::new();
    let src = format!("{VEC2}schema S\n    a ∈ IVec2\n    b ∈ IVec2\n    a = b\n    a.x = 7\n    a.y = 9\n");
    rt.load_source(&src).unwrap();
    let r = rt.query_free("S").unwrap();
    assert!(r.satisfied);
    check_int(r.bindings.get("b.x"), 7, "b.x");
    check_int(r.bindings.get("b.y"), 9, "b.y");
}
