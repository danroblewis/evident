//! Integration tests for the effect-driven step loop.
//!
//! Validates the full pipeline: load Evident → solve → decode effects
//! → dispatch → encode results → next step. Built-in effects only
//! (Print/Println/Time/Exit). FFI effects are exercised by
//! `tests/ffi.rs` at the dispatcher level; an end-to-end FFI demo
//! waits on Phase 3.3 enum-typed pattern bindings.

use std::io::{BufReader, Cursor};
use std::path::Path;
use std::sync::{Arc, Mutex};

use evident_runtime::{EvidentRuntime, effect_loop};
use evident_runtime::effect_dispatch::DispatchContext;

/// A Write impl that writes into a shared Vec — lets the test
/// inspect captured stdout after running.
struct SharedWrite(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for SharedWrite {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

fn rt_with_stdlib(user: &str) -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_source(user).unwrap();
    rt
}

#[test]
fn println_one_step_then_halt() {
    let rt = rt_with_stdlib("\
enum S = Init | Done
fsm main
    state ∈ S
    state = Init ⇒ (state_next = Done ∧ effects = ⟨Println(\"hi\")⟩)
    state = Done ⇒ (state_next = Done ∧ effects = ⟨⟩)
");
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = DispatchContext::with_streams(
        Box::new(BufReader::new(Cursor::new(Vec::<u8>::new()))),
        Box::new(SharedWrite(Arc::clone(&captured))),
    );
    let r = effect_loop::run_with_ctx(&rt, &effect_loop::LoopOpts { max_steps: 5 }, &mut ctx)
        .unwrap();
    assert!(r.halted_clean, "expected clean halt, got {r:?}");
    let stdout = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert_eq!(stdout, "hi\n");
}

#[test]
fn no_effect_program_halts_immediately() {
    let rt = rt_with_stdlib("\
enum S = Init | Done
fsm main
    state ∈ S
    state = Init ⇒ (state_next = Done ∧ effects = ⟨⟩)
    state = Done ⇒ (state_next = Done ∧ effects = ⟨⟩)
");
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = DispatchContext::with_streams(
        Box::new(BufReader::new(Cursor::new(Vec::<u8>::new()))),
        Box::new(SharedWrite(Arc::clone(&captured))),
    );
    let r = effect_loop::run_with_ctx(&rt, &effect_loop::LoopOpts { max_steps: 5 }, &mut ctx)
        .unwrap();
    assert!(r.halted_clean);
    assert_eq!(captured.lock().unwrap().len(), 0);
}

#[test]
fn multiple_println_in_one_step() {
    let rt = rt_with_stdlib("\
enum S = Init | Done
fsm main
    state ∈ S
    state = Init ⇒
        (state_next = Done ∧
         effects = ⟨Println(\"a\"), Println(\"b\")⟩)
    state = Done ⇒ (state_next = Done ∧ effects = ⟨⟩)
");
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = DispatchContext::with_streams(
        Box::new(BufReader::new(Cursor::new(Vec::<u8>::new()))),
        Box::new(SharedWrite(Arc::clone(&captured))),
    );
    let _ = effect_loop::run_with_ctx(&rt, &effect_loop::LoopOpts { max_steps: 5 }, &mut ctx);
    let stdout = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert_eq!(stdout, "a\nb\n");
}
