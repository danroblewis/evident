use std::path::Path;
use std::sync::{Arc, Mutex};

use evident_runtime::{EvidentRuntime, trampoline};
use evident_runtime::ffi::DispatchContext;

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
    let mut ctx = DispatchContext::with_streams(Box::new(SharedWrite(Arc::clone(&captured))));
    let r = trampoline::run_with_ctx(&rt, &trampoline::LoopOpts { max_steps: 5 }, &mut ctx)
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
    let mut ctx = DispatchContext::with_streams(Box::new(SharedWrite(Arc::clone(&captured))));
    let r = trampoline::run_with_ctx(&rt, &trampoline::LoopOpts { max_steps: 5 }, &mut ctx)
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
    let mut ctx = DispatchContext::with_streams(Box::new(SharedWrite(Arc::clone(&captured))));
    let _ = trampoline::run_with_ctx(&rt, &trampoline::LoopOpts { max_steps: 5 }, &mut ctx);
    let stdout = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    assert_eq!(stdout, "a\nb\n");
}
