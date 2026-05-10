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
