//! Convergence proof: the full hybrid pipeline end-to-end.
//!
//!   Evident FSM source
//!     → [`runtime_smt::transpile_fsm`]   (front-end transpiler)
//!     → SMT-LIB + metadata fixture
//!     → [`runtime_smt::meta::load_str`]  (engine loader)
//!     → [`runtime_smt::scheduler::run`]  (greenfield engine)
//!     → stdout + exit code
//!
//! For each target we assert the run's stdout AND exit code EXACTLY match the
//! ground truth captured from the legacy oracle (`evident effect-run`). This is
//! the reproducible, in-process end-to-end proof; `crosscheck.sh` does the live
//! oracle comparison on top.
//!
//! Ground truth (oracle `evident effect-run <file> --max-steps 20`):
//!
//! | source                                | stdout                          | exit |
//! |---------------------------------------|---------------------------------|------|
//! | runtime-smt/crosscheck/countdown.ev   | tick\ntick\ntick\ndone\n        | 0    |
//! | examples/test_08_exit_code.ev         | exiting with code 42\n          | 42   |
//! | examples/test_03_seq_chain.ev         | first\nsecond\nthird\n          | 0    |
//! | examples/test_05_int_to_str.ev        | 42\n                            | 0    |
//! | examples/test_04_parse_int.ev         | good: parsed an Int\n           |      |
//! |                                       |   bad: ERROR was correct\n      | 0    |
//! | examples/test_19_prev_tick.ev         | count = ?\ncount = 0\n…\ndone\n  | 0    |
//! | examples/test_20_pure_counter.ev      | starting\ncount = 0\n…count = 3 | 0    |
//!
//! Golden strings captured from `runtime/target/release/evident effect-run
//! <file> --max-steps 30` (the EXPECTATIONS contract in
//! `runtime/tests/demos.rs` uses --max-steps 10 for test_04/05 and 30 for
//! test_19/20; the engine halts on Exit / no-progress well before any cap).

use runtime_smt::driver::DEFAULT_MAX_TICKS;
use runtime_smt::meta::load_str;
use runtime_smt::scheduler::run_to_string;
use runtime_smt::transpile_fsm;

/// Repo root, derived from this crate's manifest dir (.../runtime-smt → ..).
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("runtime-smt has a parent dir")
        .to_path_buf()
}

/// Transpile `rel_path` (relative to repo root), run it through the engine, and
/// return (stdout, exit_code).
fn hybrid_run(rel_path: &str) -> (String, i32) {
    let path = repo_root().join(rel_path);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let fixture = transpile_fsm(&src)
        .unwrap_or_else(|e| panic!("transpile_fsm({rel_path}) failed: {e}"));
    let problem = load_str(&fixture)
        .unwrap_or_else(|e| panic!("load_str of transpiled {rel_path} failed: {e}\n--- fixture ---\n{fixture}"));
    let (stdout, report) = run_to_string(&problem, DEFAULT_MAX_TICKS)
        .unwrap_or_else(|e| panic!("run of {rel_path} failed: {e}"));
    (stdout, report.exit_code)
}

#[test]
fn countdown_matches_oracle() {
    let (stdout, code) = hybrid_run("runtime-smt/crosscheck/countdown.ev");
    assert_eq!(stdout, "tick\ntick\ntick\ndone\n", "stdout mismatch");
    assert_eq!(code, 0, "exit code mismatch");
}

#[test]
fn test_08_exit_code_matches_oracle() {
    let (stdout, code) = hybrid_run("examples/test_08_exit_code.ev");
    assert_eq!(stdout, "exiting with code 42\n", "stdout mismatch");
    assert_eq!(code, 42, "exit code mismatch");
}

#[test]
fn test_03_seq_chain_matches_oracle() {
    let (stdout, code) = hybrid_run("examples/test_03_seq_chain.ev");
    assert_eq!(stdout, "first\nsecond\nthird\n", "stdout mismatch");
    assert_eq!(code, 0, "exit code mismatch");
}

#[test]
fn test_05_int_to_str_matches_oracle() {
    // IntToStr(42) on tick 0 → StringResult("42") threaded → Println("42") tick 1.
    let (stdout, code) = hybrid_run("examples/test_05_int_to_str.ev");
    assert_eq!(stdout, "42\n", "stdout mismatch");
    assert_eq!(code, 0, "exit code mismatch");
}

#[test]
fn test_04_parse_int_matches_oracle() {
    // ParseInt("42")→IntResult, ParseInt("not-a-number")→ErrorResult; read back
    // on the next tick and Println'd.
    let (stdout, code) = hybrid_run("examples/test_04_parse_int.ev");
    assert_eq!(
        stdout, "good: parsed an Int\nbad: ERROR was correct\n",
        "stdout mismatch"
    );
    assert_eq!(code, 0, "exit code mismatch");
}

#[test]
fn test_19_prev_tick_matches_oracle() {
    // Enum state + scalar count; `count = " ++ prev_str` where prev_str comes
    // from last_results[1] (the prior tick's IntToStr StringResult).
    let (stdout, code) = hybrid_run("examples/test_19_prev_tick.ev");
    assert_eq!(
        stdout, "count = ?\ncount = 0\ncount = 1\ncount = 2\ndone\n",
        "stdout mismatch"
    );
    assert_eq!(code, 0, "exit code mismatch");
}

#[test]
fn test_20_pure_counter_matches_oracle() {
    // Pure scalar counter, no enum state; nested effects ternary on count.
    let (stdout, code) = hybrid_run("examples/test_20_pure_counter.ev");
    assert_eq!(
        stdout, "starting\ncount = 0\ncount = 1\ncount = 2\ncount = 3\n",
        "stdout mismatch"
    );
    assert_eq!(code, 0, "exit code mismatch");
}
