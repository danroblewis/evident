//! End-to-end correctness of the inject partial cutover (session
//! REVIVE-inject).
//!
//! `inject_fsm_params` and `inject_prev_tick_decls` self-host in
//! `stdlib/passes/inject.ev` + `crate::portable::inject`; the production
//! load path (`runtime/src/runtime/load.rs`) routes those two sub-passes
//! through the Evident engine. These tests load real corpus programs through
//! the **production `EvidentRuntime::load_file` path** and assert the FSMs
//! gained the implicit memberships the self-hosted passes are responsible
//! for.
//!
//! This complements the unit-level `portable::inject::tests::matches_golden_
//! on_corpus`, which does the exact per-sub-pass before/after diff against a
//! golden captured from the canonical Rust pass. Here we verify the WHOLE
//! load pipeline end-to-end (desugar → fsm_params → lhs_eq → prev_tick →
//! claim_arg) on the binary's own load path — note this runs
//! `unify_world_syntax` BEFORE inject, so `_world.X` is already rewritten to
//! the legacy `world` / `world_next` pair; inject therefore injects the
//! non-world `_var` slots (`_game_clock`, `_frame`, …), `is_first_tick`, and
//! the `effects` / `last_results` / `state_next` fsm-params slots.

use std::collections::HashSet;
use std::path::Path;

use evident_runtime::{ast::BodyItem, EvidentRuntime};

/// Membership names declared in a loaded schema's body.
fn membership_names(rt: &EvidentRuntime, name: &str) -> HashSet<String> {
    let s = rt.get_schema(name)
        .unwrap_or_else(|| panic!("schema `{name}` not loaded"));
    s.body.iter().filter_map(|i| match i {
        BodyItem::Membership { name, .. } => Some(name.clone()),
        _ => None,
    }).collect()
}

fn load(file: &str) -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new(file))
        .unwrap_or_else(|e| panic!("load {file}: {e}"));
    rt
}

fn assert_has(got: &HashSet<String>, who: &str, wants: &[&str]) {
    for want in wants {
        assert!(got.contains(*want),
            "{who} should have injected `{want}`; got {got:?}");
    }
}

fn assert_lacks(got: &HashSet<String>, who: &str, unwanted: &[&str]) {
    for bad in unwanted {
        assert!(!got.contains(*bad),
            "{who} should NOT have injected `{bad}` (over-injection); got {got:?}");
    }
}

/// Mario's `game`: prev-tick (`_game_clock`, `is_first_tick`) + fsm_params
/// (`effects`) — exercises BOTH self-hosted sub-passes on the repo's most
/// complex demo, through the production load path.
#[test]
fn mario_game_injected() {
    let got = membership_names(&load("../examples/test_21_mario/main.ev"), "game");
    assert_has(&got, "game", &["_game_clock", "is_first_tick", "effects"]);
}

/// Mario's `keyboard`: prev-tick + fsm_params with `last_results` (it reads
/// the SDL input results) — the only corpus FSM that injects last_results
/// alongside a `_var` slot.
#[test]
fn mario_keyboard_injects_last_results() {
    let got = membership_names(&load("../examples/test_21_mario/main.ev"), "keyboard");
    assert_has(&got, "keyboard", &["_kb_frame", "is_first_tick", "effects", "last_results"]);
}

/// Mario's `display`: a prev-tick FSM that emits no `effects` and has no
/// state pair. Confirms the fsm_params decision computes FALSE here — only
/// `_frame` + `is_first_tick` are injected, NOT `effects` / `state_next` /
/// `last_results`. This is the in-Evident `(r ∧ ¬h …)` decision (the #18
/// keystone) producing an empty fsm_params list, observed end-to-end.
#[test]
fn mario_display_prev_tick_only() {
    let got = membership_names(&load("../examples/test_21_mario/main.ev"), "display");
    assert_has(&got, "display", &["_frame", "is_first_tick"]);
    assert_lacks(&got, "display", &["effects", "state_next", "last_results"]);
}

/// test_09's `producer`/`consumer`: fsm_params injects the state pair +
/// effects (+ last_results for the consumer), no prev-tick. A small
/// multi-FSM file that isolates the fsm_params sub-pass.
#[test]
fn two_fsms_state_and_effects() {
    let rt = load("../examples/test_09_two_fsms.ev");
    let producer = membership_names(&rt, "producer");
    assert_has(&producer, "producer", &["state_next", "effects"]);
    assert_lacks(&producer, "producer", &["last_results"]);
    let consumer = membership_names(&rt, "consumer");
    assert_has(&consumer, "consumer", &["state_next", "effects", "last_results"]);
}
