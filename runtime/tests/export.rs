//! `export_transition` schema regression: a ΔΔ (two-tick) fsm must mark its
//! carried var `"hist": 2` and emit a top-level `is_second_tick`; a one-tick fsm
//! must stay `"hist": 1` with no `is_second_tick` field. The viz two-tick
//! reachability path gates on exactly these signals.

use evident_runtime::EvidentRuntime;

#[test]
fn two_tick_fsm_marks_hist_2_and_is_second_tick() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "fsm oscillator\n    \
         pos ∈ Real\n    \
         is_first_tick  ⇒ pos = 60\n    \
         is_second_tick ⇒ pos = 60\n    \
         (¬is_first_tick ∧ ¬is_second_tick) ⇒ ΔΔpos = (2 * __pos - 3 * _pos) / 36\n",
    )
    .unwrap();
    let (_smt2, json) = rt.export_transition("oscillator").unwrap();
    assert!(json.contains("\"hist\": 2"), "pos must be hist 2:\n{json}");
    assert!(
        json.contains("\"is_second_tick\": \"is_second_tick\""),
        "two-tick schema must emit is_second_tick:\n{json}"
    );
}

#[test]
fn one_tick_fsm_stays_hist_1_no_is_second_tick() {
    let mut rt = EvidentRuntime::new();
    rt.load_source(
        "fsm counter\n    \
         count ∈ Int\n    \
         is_first_tick  ⇒ count = 0\n    \
         ¬is_first_tick ⇒ Δcount = 1\n",
    )
    .unwrap();
    let (_smt2, json) = rt.export_transition("counter").unwrap();
    assert!(json.contains("\"hist\": 1"), "count must be hist 1:\n{json}");
    assert!(
        !json.contains("is_second_tick"),
        "one-tick schema must NOT emit is_second_tick:\n{json}"
    );
}
