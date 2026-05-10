//! Integration test for static subscription inference. Loads the
//! real multi-FSM GL demo and asserts the read/write sets for each
//! FSM. This is the spec from `docs/design/fsm-subscriptions.md`
//! Phase 1 — Phase 2's scheduler will rely on these being correct.

use std::collections::HashSet;
use std::path::Path;

use evident_runtime::EvidentRuntime;
use evident_runtime::subscriptions::world_access_sets;

fn set(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

#[test]
fn multi_fsm_transpiled_demo_access_sets() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_file(Path::new("../programs/demos/effect_multi_fsm_transpiled.ev"))
        .expect("load demo");

    let setup  = rt.get_schema("setup").expect("setup claim missing");
    let render = rt.get_schema("render").expect("render claim missing");

    let s = world_access_sets(setup);
    let r = world_access_sets(render);

    // Setup writes all 5 handles AND reads them in the Done arm
    // (`world_next.X = world.X` passthrough). Phase 2's scheduler
    // will need to suppress self-loops where an FSM only "reads" a
    // field it itself just wrote — see fsm-subscriptions.md design.
    assert_eq!(s.writes, set(&["window", "ctx", "vao", "prog", "time_loc"]),
        "setup should write all 5 GL handle fields, got {:?}", s.writes);
    assert_eq!(s.reads, set(&["window", "ctx", "vao", "prog", "time_loc"]),
        "setup reads each field it writes, in the Done passthrough arm; got {:?}", s.reads);

    // Render reads the handles it actually uses (window for swap,
    // prog for the setup_done check, time_loc for the uniform).
    // ctx and vao are needed at GL setup time but render doesn't
    // touch them (the GL state machine on the GPU side carries them).
    assert_eq!(r.reads, set(&["window", "prog", "time_loc"]),
        "render's read-set should be exactly the handles it uses, got {:?}", r.reads);
    assert_eq!(r.writes, HashSet::<String>::new(),
        "render is a pure reader, got writes {:?}", r.writes);
}

#[test]
fn lang_test_world_handoff_access_sets() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_file(Path::new("../programs/lang_tests/multi_fsm/01_basic_world_handoff.ev"))
        .expect("load test 01");

    let game   = rt.get_schema("game").expect("game claim missing");
    let render = rt.get_schema("render").expect("render claim missing");

    let g = world_access_sets(game);
    let r = world_access_sets(render);

    assert_eq!(g.writes, set(&["tick_even"]));
    assert_eq!(r.reads,  set(&["tick_even"]));
    assert_eq!(r.writes, HashSet::<String>::new());
}
