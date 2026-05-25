//! Integration test for static subscription inference. Loads the
//! real multi-FSM GL demo and asserts the read/write sets for each
//! FSM. This is the spec from `docs/design/fsm-subscriptions.md`
//! Phase 1 — Phase 2's scheduler will rely on these being correct.

use std::collections::HashSet;
use std::path::Path;

use evident_runtime::EvidentRuntime;
// Session XX: the canonical Rust walk is gone; the sole impl is the
// self-hosted Evident pass behind the scheduler's production entry point.
use evident_runtime::portable::subscriptions::access_sets as world_access_sets;

fn set(items: &[&str]) -> HashSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// (former `multi_fsm_transpiled_demo_access_sets` test removed —
//  the demo it referenced was deleted in the demos restart. The
//  lang_test variant below covers the same access-set inference.)

#[test]
fn lang_test_world_handoff_access_sets() {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_file(Path::new("../tests/lang_tests/multi_fsm/01_basic_world_handoff.ev"))
        .expect("load test 01");

    let game   = rt.get_schema("game").expect("game claim missing");
    let render = rt.get_schema("render").expect("render claim missing");

    let g = world_access_sets(game);
    let r = world_access_sets(render);

    assert_eq!(g.writes, set(&["tick_even"]));
    assert_eq!(r.reads,  set(&["tick_even"]));
    assert_eq!(r.writes, HashSet::<String>::new());
}
