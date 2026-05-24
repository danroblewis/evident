//! Multi-FSM scheduler integration tests.
//!
//! Each test loads a .ev file from `tests/lang_tests/multi_fsm/`,
//! runs the effect loop with captured stdout, and asserts on the
//! output. The .ev files are the spec; these tests assert the spec
//! is met. Don't change the .ev files to fit the tests — change the
//! tests to match if the spec evolves.

use std::io::{BufReader, Cursor};
use std::path::Path;
use std::sync::{Arc, Mutex};

use evident_runtime::{EvidentRuntime, effect_loop};
use evident_runtime::effect_dispatch::DispatchContext;

struct SharedWrite(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for SharedWrite {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

fn run_program(path: &str, max_steps: usize) -> (String, effect_loop::LoopResult) {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_file(Path::new(path))
        .unwrap_or_else(|e| panic!("failed to load {path}: {e}"));
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = DispatchContext::with_streams(
        Box::new(BufReader::new(Cursor::new(Vec::<u8>::new()))),
        Box::new(SharedWrite(Arc::clone(&captured))),
    );
    let r = effect_loop::run_with_ctx(&rt, &effect_loop::LoopOpts { max_steps }, &mut ctx)
        .unwrap_or_else(|e| panic!("loop run failed for {path}: {e}"));
    let out = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    (out, r)
}

#[test]
fn basic_world_handoff() {
    // game writes world.tick_even (toggles each tick), render reads
    // it. Validates: writer-first order, world propagates within the
    // same tick, runs forever (no halt).
    let (out, r) = run_program("../tests/lang_tests/multi_fsm/01_basic_world_handoff.ev", 8);

    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.len() >= 8, "expected ≥8 lines, got {lines:?}");

    // Per-tick: writer prints "game tick", reader reads world and
    // prints "render: world even" or "render: world odd".
    for chunk in lines.chunks(2) {
        if chunk.len() == 2 {
            assert_eq!(chunk[0], "game tick", "writer should print first");
            assert!(chunk[1] == "render: world even" || chunk[1] == "render: world odd",
                    "reader line: {}", chunk[1]);
        }
    }
    // Should NOT halt — both FSMs run forever.
    assert!(!r.halted_clean, "should hit max_steps, not halt");
}

#[test]
fn setup_then_render_lifecycle() {
    // Setup prints "setup: booting" once and halts. Render prints
    // "render: ready" forever (sees world.ready = true after halt).
    // The GL killer case: setup pushes state once, halts; render
    // runs with a tiny solve.
    let (out, _r) = run_program("../tests/lang_tests/multi_fsm/02_setup_then_render_lifecycle.ev", 6);
    let lines: Vec<&str> = out.lines().collect();

    // First line is setup, rest are render: ready.
    assert_eq!(lines[0], "setup: booting");
    for line in &lines[1..] {
        assert_eq!(*line, "render: ready",
            "after setup halts, world.ready must persist (plugin sees true)");
    }
    // Setup line appears EXACTLY once — proves halt-and-drop.
    assert_eq!(lines.iter().filter(|l| **l == "setup: booting").count(), 1);
    assert!(lines.len() >= 4, "render should keep printing after setup halts");
}

#[test]
fn sibling_no_world() {
    // ticker + heartbeat, no shared world. Both run forever.
    // Validates: declaration order ("tick" before "beat"); no
    // writer/reader distinction needed when no World type exists.
    let (out, r) = run_program("../tests/lang_tests/multi_fsm/03_sibling_no_world.ev", 6);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.len() >= 6, "expected ≥6 lines, got {lines:?}");

    for chunk in lines.chunks(2) {
        if chunk.len() == 2 {
            assert_eq!(chunk[0], "tick", "ticker declared first must print first");
            assert_eq!(chunk[1], "beat", "heartbeat declared second prints second");
        }
    }
    assert!(!r.halted_clean);
}

// graceful_shutdown_via_world_and_exit lives in
// runtime/tests/scheduler_delta.rs.

#[test]
fn request_response_lang_test_11() {
    // Two user FSMs (client + server) coordinating via shared
    // world fields. Client requests; server doubles; client
    // exits after 3 requests done.
    let (out, r) = run_program("../tests/lang_tests/multi_fsm/11_request_response.ev", 30);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.contains(&"client done"),
        "should print client done; out:\n{}", out);
    assert!(r.halted_clean, "should halt cleanly via Exit; got {r:?}");
    assert_eq!(r.exit_code, Some(0));
    assert!(r.steps < 30);
}

#[test]
fn spawnable_only_lang_test_14() {
    // Worker has `spawnable_only` body marker → not
    // auto-detected. Parent spawns 2 workers; output has
    // EXACTLY worker A + worker B (no "worker ?" from auto-
    // instance).
    let (out, r) = run_program("../tests/lang_tests/multi_fsm/14_spawnable_only.ev", 10);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.contains(&"worker A"), "missing worker A; out:\n{}", out);
    assert!(lines.contains(&"worker B"), "missing worker B; out:\n{}", out);
    assert!(!lines.contains(&"worker ?"),
        "should NOT have worker ? (auto-instance); out:\n{}", out);
    assert!(r.halted_clean);
}

#[test]
fn spawn_with_arg_lang_test_13() {
    // Parent spawns 3 workers each with a different ID arg.
    // Each worker prints a message keyed on its ID.
    let (out, r) = run_program("../tests/lang_tests/multi_fsm/13_spawn_with_arg.ev", 20);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.contains(&"worker 1 says hi"), "missing worker 1; out:\n{}", out);
    assert!(lines.contains(&"worker 2 says hi"), "missing worker 2; out:\n{}", out);
    assert!(lines.contains(&"worker 3 says hi"), "missing worker 3; out:\n{}", out);
    assert!(r.halted_clean, "should halt cleanly; got {r:?}");
}

// (Removed: fti_sdl_gl_render_lang_test_19. It asserted on
// stdout "render done" which the binary emits even when the
// window renders BLACK — see examples/COUNTEREXAMPLES.md #7.
// Paper assertion on a known-broken visual path. The lang_test
// 19 file remains for manual exercise via `evident effect-run`.)

#[test]
fn fti_configurable_timer_lang_test_17() {
    // FTI v2: per-instance interval via type-use pin.
    use std::process::{Command, Stdio};
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evident"))
        .current_dir(repo_root)
        .args(["effect-run",
               "tests/lang_tests/multi_fsm/17_fti_configurable_timer.ev",
               "--max-steps", "1000"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("fast: 5 ticks @ 20ms"), "out:\n{}", out);
    assert!(out.contains("slow: 3 ticks @ 100ms"), "out:\n{}", out);
    assert!(output.status.success(),
        "expected exit 0; stderr:\n{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn fti_per_instance_lang_test_16() {
    // FTI v1.5: two FSMs each with own clock instance. Each
    // sees its own tick_count (FSM-prefixed pins).
    use std::process::{Command, Stdio};
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evident"))
        .current_dir(repo_root)
        .env("EVIDENT_TICK_MS", "20")
        .args(["effect-run",
               "tests/lang_tests/multi_fsm/16_fti_per_instance.ev",
               "--max-steps", "300"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("fast: 5 ticks"), "out:\n{}", out);
    assert!(out.contains("slow: 8 ticks"), "out:\n{}", out);
    assert!(output.status.success(),
        "expected exit 0; stderr:\n{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn fti_frameclock_lang_test_15() {
    // FTI v1: `clock ∈ FrameClock` parameter; runtime auto-
    // installs a bridge that pins clock.tick_count.
    use std::process::{Command, Stdio};
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evident"))
        .current_dir(repo_root)
        .env("EVIDENT_TICK_MS", "30")
        .args(["effect-run",
               "tests/lang_tests/multi_fsm/15_fti_frameclock.ev",
               "--max-steps", "200"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("3 ticks observed via FTI"),
        "FTI bridge should pin clock.tick_count and reach 3; out:\n{}", out);
    assert!(output.status.success());
}

#[test]
fn wallclock_lang_test_12() {
    // WallClock auto-installs because World declares now_ms.
    // Demo snapshots start time, exits after 200ms elapsed.
    use std::process::{Command, Stdio};
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evident"))
        .current_dir(repo_root)
        .env("EVIDENT_CLOCK_MS", "30")
        .args(["effect-run",
               "tests/lang_tests/multi_fsm/12_wallclock.ev",
               "--max-steps", "200"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("clock works"), "out:\n{}", out);
    assert!(output.status.success(),
        "expected exit 0; stderr:\n{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn timer_and_stdin_lang_test_09_multi_plugin() {
    // Multi-plugin demo: World declares both tick_count + stdin
    // fields → both FrameTimer and StdinSource auto-install.
    // Watcher subscribes to both, halts after 2 events.
    use std::process::{Command, Stdio};
    use std::io::Write;
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_evident"))
        .current_dir(repo_root)
        .env("EVIDENT_TICK_MS", "30")
        .args(["effect-run",
               "tests/lang_tests/multi_fsm/09_timer_and_stdin.ev",
               "--max-steps", "100"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    {
        // Pipe two lines fast — likely both arrive before any tick.
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin.write_all(b"alpha\nbeta\n").unwrap();
    }
    let output = child.wait_with_output().expect("wait");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("threshold reached"),
        "should reach threshold; out:\n{}", out);
    assert!(output.status.success(),
        "expected exit 0; got {:?}; stderr:\n{}",
        output.status, String::from_utf8_lossy(&output.stderr));
}

#[test]
fn timer_lang_test_07_plugin_as_writer() {
    // Timer demo: FrameTimer auto-installed (because World has
    // tick_count: Int). Plugin writes incrementing tick counts.
    // Counter FSM gates on world.tick_count > last_seen, emits
    // "tick" each new tick, "five ticks observed" + Exit(0) at 5.
    use std::process::{Command, Stdio};
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evident"))
        .current_dir(repo_root)
        .env("EVIDENT_TICK_MS", "20")
        .args(["effect-run",
               "tests/lang_tests/multi_fsm/07_timer_demo.ev",
               "--max-steps", "100"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn");
    let out = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = out.lines().collect();
    let tick_count = lines.iter().filter(|l| **l == "tick").count();
    let done_count = lines.iter().filter(|l| **l == "five ticks observed").count();
    assert!(tick_count >= 1 && tick_count <= 5,
        "expected 1..=5 tick lines (timing-sensitive); got {tick_count}; out:\n{}", out);
    assert_eq!(done_count, 1, "exactly one 'five ticks observed' line; out:\n{}", out);
    assert!(output.status.success(),
        "expected exit 0; got {:?}; stderr:\n{}",
        output.status, String::from_utf8_lossy(&output.stderr));
}

#[test]
fn word_counter_lang_test_08_payload_state() {
    // Variant of the echo demo using payload state instead of
    // world-tracked counter. Verifies payload first-variant
    // works alongside StdinSource auto-install.
    use std::process::{Command, Stdio};
    use std::io::Write;
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_evident"))
        .current_dir(repo_root)
        .args(["effect-run",
               "tests/lang_tests/multi_fsm/08_word_counter.ev",
               "--max-steps", "30"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin.write_all(b"alpha\nbeta\ngamma\n").unwrap();
    }
    let output = child.wait_with_output().expect("wait");
    let out = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.contains(&"got: alpha"), "missing alpha; out:\n{}", out);
    assert!(lines.contains(&"got: beta"),  "missing beta; out:\n{}",  out);
    assert!(lines.contains(&"got: gamma"), "missing gamma; out:\n{}", out);
    assert!(output.status.success(),
        "expected exit 0; got {:?}; stderr:\n{}",
        output.status, String::from_utf8_lossy(&output.stderr));
}

#[test]
fn echo_lang_test_06_plugin_as_writer() {
    // The echo demo: StdinSource auto-installed (because World
    // has stdin_line + stdin_seq). Plugin reads piped stdin
    // lines, writes them to world fields. Echo FSM gates on
    // stdin_seq > last_echoed_seq → emits each line exactly
    // once. Halts after EOF.
    //
    // StdinSource reads from the real fd 0, not from a
    // DispatchContext stream — so we exercise the binary
    // directly via `cargo run`.
    use std::process::{Command, Stdio};
    use std::io::Write;
    // Run from repo root so the binary's relative paths to
    // stdlib/runtime.ev resolve.
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_evident"))
        .current_dir(repo_root)
        .args(["effect-run",
               "tests/lang_tests/multi_fsm/06_echo.ev",
               "--max-steps", "50"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn evident");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin.write_all(b"hello\nworld\nfoo\n").unwrap();
        // Closing stdin happens when child.stdin is dropped at scope exit.
    }
    let output = child.wait_with_output().expect("wait");
    let out = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = out.lines().collect();

    assert!(lines.contains(&"hello"), "missing hello; out:\n{}", out);
    assert!(lines.contains(&"world"), "missing world; out:\n{}", out);
    assert!(lines.contains(&"foo"),   "missing foo; out:\n{}",   out);
    assert_eq!(lines.iter().filter(|l| **l == "hello").count(), 1);
    assert_eq!(lines.iter().filter(|l| **l == "world").count(), 1);
    assert_eq!(lines.iter().filter(|l| **l == "foo").count(),   1);
    assert!(output.status.success(),
        "expected exit 0; got {:?}; stderr:\n{}",
        output.status, String::from_utf8_lossy(&output.stderr));
}

#[test]
fn halt_cascade() {
    // short_fsm halts after 3 prints; long_fsm after 5. Then the
    // program exits cleanly. Validates: per-FSM halt + drop;
    // program-level all-halt → exit; halted FSM doesn't re-solve.
    let (out, r) = run_program("../tests/lang_tests/multi_fsm/04_halt_cascade.ev", 20);
    let lines: Vec<&str> = out.lines().collect();

    let short_count = lines.iter().filter(|l| **l == "short").count();
    let long_count  = lines.iter().filter(|l| **l == "long").count();
    assert_eq!(short_count, 3, "short_fsm should print exactly 3 lines, got {short_count}");
    assert_eq!(long_count,  5, "long_fsm should print exactly 5 lines, got {long_count}");

    assert!(r.halted_clean, "program should halt cleanly when all FSMs halt");
    assert!(r.steps < 20, "should halt before max_steps, got {} steps", r.steps);
}

// ── Multi-writer single-owner enforcement ─────────────────────
//
// Two FSMs writing the SAME world field is a silent data race
// (second write wins non-deterministically). The runtime rejects
// it at load time. These tests pin both the direct form and the
// `..Passthrough` form — the latter was previously undetected
// because write-set inference treated passthroughs as opaque,
// letting the conflict slip past the check AND making the
// scheduler silently drop the writes (program hung). The fix:
// `fsm::full_world_access` resolves passthroughs transitively.

/// Like `run_program` but returns the loop Result instead of
/// panicking, so a load-time rejection can be asserted on.
fn try_run_program(path: &str, max_steps: usize) -> Result<effect_loop::LoopResult, String> {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_file(Path::new(path))
        .unwrap_or_else(|e| panic!("failed to load {path}: {e}"));
    let mut ctx = DispatchContext::with_streams(
        Box::new(BufReader::new(Cursor::new(Vec::<u8>::new()))),
        Box::new(Vec::<u8>::new()),
    );
    effect_loop::run_with_ctx(&rt, &effect_loop::LoopOpts { max_steps }, &mut ctx)
}

#[test]
fn multiwriter_conflict_direct_rejected() {
    let err = try_run_program(
        "../tests/lang_tests/multi_fsm/21_multiwriter_conflict_direct.ev", 20)
        .expect_err("two writers of `score` must be rejected at load");
    assert!(err.contains("both write") && err.contains("score")
            && err.contains("single-owner"),
        "expected single-owner-rule rejection naming `score`; got: {err}");
}

#[test]
fn multiwriter_conflict_passthrough_rejected() {
    // Regression: both writers write `score` only inside a
    // `..WritesScore` passthrough. Before the transitive write-set
    // fix this was NOT caught (and the program hung). It must now
    // be rejected exactly like the direct form.
    let err = try_run_program(
        "../tests/lang_tests/multi_fsm/22_multiwriter_conflict_passthrough.ev", 20)
        .expect_err("passthrough writers of `score` must be rejected at load");
    assert!(err.contains("both write") && err.contains("score")
            && err.contains("single-owner"),
        "expected single-owner-rule rejection naming `score`; got: {err}");
}

#[test]
fn passthrough_writer_takes_effect() {
    // Positive companion: a writer whose `world_next.count` write
    // lives in a `..WritesCount` passthrough, observed by a reader.
    // On tick 0 the writer sets count = 0 + 1 = 1; the reader sees
    // it the same tick → "write took effect", Exit(0). Before the
    // transitive write-set fix the scheduler dropped the passthrough
    // write and the reader saw count = 0 → "write dropped", Exit(1).
    let (out, r) = run_program(
        "../tests/lang_tests/multi_fsm/23_passthrough_writer_ok.ev", 30);
    assert!(out.lines().any(|l| l == "write took effect"),
        "passthrough writer's world write must take effect; out:\n{}", out);
    assert!(!out.lines().any(|l| l == "write dropped"),
        "passthrough write was dropped (the bug); out:\n{}", out);
    assert!(r.halted_clean, "should halt cleanly via Exit; got {r:?}");
    assert_eq!(r.exit_code, Some(0));
}
