//! Integration test for the SMT-LIB-driven FSM path (runtime-evolve strategy 2).
//!
//! For each fixture pair `<name>.json` (SMT-LIB) + `<name>.ev` (the Evident
//! oracle), run BOTH through the binary and assert byte-identical stdout + exit
//! code. The Evident-source path is the ground truth; the SMT-LIB path must
//! match it while reusing the same scheduler/effect engine.

use std::path::Path;
use std::process::Command;

const EVIDENT: &str = env!("CARGO_BIN_EXE_evident");

/// Run a CLI subcommand from the repo root; return (exit_code, stdout).
fn run(args: &[&str]) -> (i32, String) {
    let out = Command::new(EVIDENT)
        .args(args)
        .current_dir("..")
        .output()
        .unwrap_or_else(|e| panic!("spawn {EVIDENT} {args:?}: {e}"));
    let exit = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if !out.status.success() && exit != 0 {
        // Surface stderr for debugging non-clean exits (but Exit(0) is success).
        eprintln!("stderr for {args:?}:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    (exit, stdout)
}

/// Assert the SMT-LIB fixture and its Evident oracle produce identical output.
fn assert_paths_match(name: &str) {
    let json = format!("runtime/tests/fixtures/smtlib/{name}.json");
    let ev = format!("runtime/tests/fixtures/smtlib/{name}.ev");
    assert!(Path::new(&format!("../{json}")).exists(), "missing fixture {json}");
    assert!(Path::new(&format!("../{ev}")).exists(), "missing oracle {ev}");

    let (smt_exit, smt_out) = run(&["effect-run-smtlib", &json, "--max-steps", "100"]);
    let (ev_exit, ev_out) = run(&["effect-run", &ev, "--max-steps", "100"]);

    assert_eq!(
        smt_out, ev_out,
        "{name}: SMT-LIB stdout != Evident oracle stdout\nSMT-LIB:\n{smt_out}\nEvident:\n{ev_out}"
    );
    assert_eq!(smt_exit, ev_exit, "{name}: exit codes differ");
}

#[test]
fn countdown_matches_evident_oracle() {
    assert_paths_match("countdown");
}

#[test]
fn countdown_stdout_is_exactly_tick_tick_tick_liftoff() {
    let json = "runtime/tests/fixtures/smtlib/countdown.json";
    let (exit, out) = run(&["effect-run-smtlib", json, "--max-steps", "100"]);
    assert_eq!(exit, 0, "countdown should Exit(0)");
    assert_eq!(out, "tick\ntick\ntick\nliftoff\n");
}

#[test]
fn decr_halt_matches_evident_oracle() {
    // Halt-by-no-schedule (no Effect::Exit): the FSM stops emitting and the
    // scheduler halts cleanly. Both paths must agree on stdout AND exit.
    assert_paths_match("decr_halt");
}

#[test]
fn decr_halt_stdout_is_three_steps_clean_exit() {
    let json = "runtime/tests/fixtures/smtlib/decr_halt.json";
    let (exit, out) = run(&["effect-run-smtlib", json, "--max-steps", "100"]);
    assert_eq!(exit, 0, "clean halt (no Exit effect) is exit 0");
    assert_eq!(out, "step\nstep\nstep\n");
}

#[test]
fn clock_watcher_multi_fsm_matches_evident_oracle() {
    // Two SMT-LIB FSMs coordinated through the existing world plumbing: the
    // writer's `world_next.tick` propagates to the reader's `world.tick`, and
    // the world-access markers wake the reader. Must match the Evident oracle.
    assert_paths_match("clock_watcher");
}

#[test]
fn clock_watcher_stdout_is_three_ticks_then_done() {
    let json = "runtime/tests/fixtures/smtlib/clock_watcher.json";
    let (exit, out) = run(&["effect-run-smtlib", json, "--max-steps", "100"]);
    assert_eq!(exit, 0);
    assert_eq!(out, "tick\ntick\ntick\ndone\n");
}

#[test]
fn transpiled_pure_counter_matches_real_example() {
    // A REAL corpus example (examples/test_20_pure_counter.ev) hand-transpiled
    // to SMT-LIB+metadata. Exercises the `last_results` input binding + SMT-LIB
    // str.++ (the IntToStr -> StringResult format pattern). The SMT-LIB path must
    // match `evident effect-run examples/test_20_pure_counter.ev` exactly.
    let json = "runtime/tests/fixtures/smtlib/pure_counter.json";
    let ev = "examples/test_20_pure_counter.ev";
    let (smt_exit, smt_out) = run(&["effect-run-smtlib", json, "--max-steps", "15"]);
    let (ev_exit, ev_out) = run(&["effect-run", ev, "--max-steps", "15"]);
    assert_eq!(
        smt_out, ev_out,
        "transpiled test_20 stdout != original\nSMT-LIB:\n{smt_out}\nEvident:\n{ev_out}"
    );
    assert_eq!(smt_exit, ev_exit);
    assert_eq!(smt_out, "starting\ncount = 0\ncount = 1\ncount = 2\ncount = 3\n");
}
