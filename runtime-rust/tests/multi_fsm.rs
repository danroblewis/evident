//! Multi-FSM scheduler integration tests.
//!
//! Each test loads a .ev file from `programs/lang_tests/multi_fsm/`,
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
    let (out, r) = run_program("../programs/lang_tests/multi_fsm/01_basic_world_handoff.ev", 8);

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
    let (out, _r) = run_program("../programs/lang_tests/multi_fsm/02_setup_then_render_lifecycle.ev", 6);
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
    let (out, r) = run_program("../programs/lang_tests/multi_fsm/03_sibling_no_world.ev", 6);
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
// runtime-rust/tests/scheduler_delta.rs because it depends on
// delta-mode polling semantics — under legacy mode the fixpoint
// halt heuristic mistakes the consumer's polling for "done" and
// drops it before counter reaches the threshold.

#[test]
fn request_response_lang_test_11() {
    // Two user FSMs (client + server) coordinating via shared
    // world fields. Client requests; server doubles; client
    // exits after 3 requests done.
    let (out, r) = run_program("../programs/lang_tests/multi_fsm/11_request_response.ev", 30);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.contains(&"client done"),
        "should print client done; out:\n{}", out);
    assert!(r.halted_clean, "should halt cleanly via Exit; got {r:?}");
    assert_eq!(r.exit_code, Some(0));
    assert!(r.steps < 30);
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
        .env_remove("EVIDENT_SCHEDULER")
        .env("EVIDENT_TICK_MS", "30")
        .args(["effect-run",
               "programs/lang_tests/multi_fsm/09_timer_and_stdin.ev",
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
        .env_remove("EVIDENT_SCHEDULER")
        .env("EVIDENT_TICK_MS", "20")
        .args(["effect-run",
               "programs/lang_tests/multi_fsm/07_timer_demo.ev",
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
        .env_remove("EVIDENT_SCHEDULER")
        .args(["effect-run",
               "programs/lang_tests/multi_fsm/08_word_counter.ev",
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
        .env_remove("EVIDENT_SCHEDULER")  // force default (delta)
        .args(["effect-run",
               "programs/lang_tests/multi_fsm/06_echo.ev",
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
    let (out, r) = run_program("../programs/lang_tests/multi_fsm/04_halt_cascade.ev", 20);
    let lines: Vec<&str> = out.lines().collect();

    let short_count = lines.iter().filter(|l| **l == "short").count();
    let long_count  = lines.iter().filter(|l| **l == "long").count();
    assert_eq!(short_count, 3, "short_fsm should print exactly 3 lines, got {short_count}");
    assert_eq!(long_count,  5, "long_fsm should print exactly 5 lines, got {long_count}");

    assert!(r.halted_clean, "program should halt cleanly when all FSMs halt");
    assert!(r.steps < 20, "should halt before max_steps, got {} steps", r.steps);
}
