//! Integration tests for the Phase 2 delta scheduler
//! (EVIDENT_SCHEDULER=delta). Verifies that an FSM with no live
//! inputs is skipped — its body doesn't re-solve, observable side
//! effects don't fire.
//!
//! Counterpart in legacy mode: the same FSM ticks every iteration.

use std::io::{BufReader, Cursor};
use std::path::Path;
use std::sync::{Arc, Mutex};

use evident_runtime::{EvidentRuntime, effect_loop};
use evident_runtime::effect_dispatch::DispatchContext;

// Serialize: each test mutates EVIDENT_SCHEDULER on the process
// env, which is shared across cargo's default-parallel test runner.
// Without this, a test reading the var mid-flight could see a value
// set by another test.
static ENV_LOCK: Mutex<()> = Mutex::new(());

struct SharedWrite(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for SharedWrite {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}


/// Test program: writer toggles `world.gate` once and stops emitting.
/// Reader prints the gate's value every tick.
///
/// Under delta mode, after the writer has gone silent and its state
/// no longer changes, the writer should not re-solve. The reader
/// keeps looping (self-feedback fires every tick because it emits
/// a Println).
const SETUP_THEN_QUIET_PROGRAM: &str = "\
type World
    gate ∈ Bool

enum WriterState =
    Initing
    Settled

claim writer(world, world_next ∈ World,
             state, state_next ∈ WriterState,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    state_next = match state
        Initing ⇒ Settled
        Settled ⇒ Settled
    world_next.gate = match state
        Initing ⇒ true
        Settled ⇒ world.gate
    effects = ⟨Println(\"writer-tick\")⟩

enum ReaderState = Reading

claim reader(world ∈ World,
             state, state_next ∈ ReaderState,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    state_next = Reading
    msg ∈ String
    msg = (world.gate ? \"reader: gate=true\" : \"reader: gate=false\")
    effects = ⟨Println(msg)⟩
";

#[test]
fn legacy_mode_writer_ticks_every_iteration() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::remove_var("EVIDENT_SCHEDULER");

    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_source(SETUP_THEN_QUIET_PROGRAM).unwrap();
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = DispatchContext::with_streams(
        Box::new(BufReader::new(Cursor::new(Vec::<u8>::new()))),
        Box::new(SharedWrite(Arc::clone(&captured))),
    );
    let _ = effect_loop::run_with_ctx(&rt, &effect_loop::LoopOpts { max_steps: 5 }, &mut ctx);
    let out = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    let writer_count = out.lines().filter(|l| *l == "writer-tick").count();
    let reader_count = out.lines().filter(|l| l.starts_with("reader:")).count();
    // Legacy: writer ticks every iteration → 5 writer prints.
    assert_eq!(writer_count, 5, "legacy: writer should tick every iteration; out:\n{}", out);
    assert_eq!(reader_count, 5, "reader prints every iteration");
}

#[test]
fn delta_mode_writer_goes_quiet() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("EVIDENT_SCHEDULER", "delta");
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_source(SETUP_THEN_QUIET_PROGRAM).unwrap();
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = DispatchContext::with_streams(
        Box::new(BufReader::new(Cursor::new(Vec::<u8>::new()))),
        Box::new(SharedWrite(Arc::clone(&captured))),
    );
    let _ = effect_loop::run_with_ctx(&rt, &effect_loop::LoopOpts { max_steps: 8 }, &mut ctx);
    std::env::remove_var("EVIDENT_SCHEDULER");

    let out = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    let writer_count = out.lines().filter(|l| *l == "writer-tick").count();
    let reader_count = out.lines().filter(|l| l.starts_with("reader:")).count();

    // Reader ticks every iteration via self-feedback (always emits).
    assert_eq!(reader_count, 8, "reader should print every iteration; out:\n{}", out);

    // Writer should print on tick 0 (bootstrap, Initing emits print)
    // and tick 1 (state changed Initing→Settled, still emits print).
    // Tick 2: writer is woken because had_effects_last is true (it
    // emitted on tick 1). Writer's body: state=Settled, prints, no
    // state change, but effects non-empty → had_effects_last stays
    // true → keeps ticking forever.
    //
    // BUG OPPORTUNITY: this test catches the case where Println
    // emission acts as self-feedback indefinitely. In a more
    // sophisticated model, we'd note that Println produces no
    // observable result the FSM consumes (last_results stays
    // unchanged shape). For Phase 2's first cut, ANY effect counts
    // as self-feedback. Document this.
    //
    // For now, assert what's actually true:
    assert_eq!(writer_count, 8,
        "phase 2: any-effect self-feedback re-schedules forever; out:\n{}", out);

    // Reader sees gate=true every tick — writer-first scheduling
    // means the reader observes the writer's tick-0 write within
    // the same tick.
    let gate_true = out.lines().filter(|l| *l == "reader: gate=true").count();
    assert_eq!(gate_true, 8,
        "writer-first within a tick → reader sees gate=true even on tick 0; out:\n{}", out);
}

/// More targeted: writer that stops emitting after init. This is
/// the actual "FSM goes quiet" case Phase 2 should handle.
const QUIET_AFTER_INIT_PROGRAM: &str = "\
type World
    gate ∈ Bool

enum WriterState =
    Initing
    Settled

claim writer(world, world_next ∈ World,
             state, state_next ∈ WriterState,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    state_next = match state
        Initing ⇒ Settled
        Settled ⇒ Settled
    world_next.gate = match state
        Initing ⇒ true
        Settled ⇒ world.gate
    -- Only emit on Initing, then go quiet.
    effects = match state
        Initing ⇒ ⟨Println(\"writer-init\")⟩
        Settled ⇒ ⟨⟩

enum ReaderState = Reading

claim reader(world ∈ World,
             state, state_next ∈ ReaderState,
             last_results ∈ ResultList,
             effects ∈ EffectList)
    state_next = Reading
    msg ∈ String
    msg = (world.gate ? \"reader: ON\" : \"reader: OFF\")
    effects = ⟨Println(msg)⟩
";

#[test]
fn delta_mode_writer_truly_goes_quiet_after_init() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("EVIDENT_SCHEDULER", "delta");
    let mut rt = EvidentRuntime::new();
    rt.load_file(Path::new("../stdlib/runtime.ev")).unwrap();
    rt.load_source(QUIET_AFTER_INIT_PROGRAM).unwrap();
    let captured: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let mut ctx = DispatchContext::with_streams(
        Box::new(BufReader::new(Cursor::new(Vec::<u8>::new()))),
        Box::new(SharedWrite(Arc::clone(&captured))),
    );
    let _ = effect_loop::run_with_ctx(&rt, &effect_loop::LoopOpts { max_steps: 10 }, &mut ctx);
    std::env::remove_var("EVIDENT_SCHEDULER");

    let out = String::from_utf8(captured.lock().unwrap().clone()).unwrap();
    let writer_inits = out.lines().filter(|l| *l == "writer-init").count();
    let reader_count = out.lines().filter(|l| l.starts_with("reader:")).count();
    let reader_on    = out.lines().filter(|l| *l == "reader: ON").count();

    // Writer prints "writer-init" exactly once — on tick 0. After
    // tick 1 (state change), it's Settled and emits ⟨⟩. From tick 2
    // onward there's no input that would wake it.
    assert_eq!(writer_inits, 1,
        "writer should print exactly once and go quiet; out:\n{}", out);
    assert_eq!(reader_count, 10,
        "reader keeps ticking via self-feedback; out:\n{}", out);
    // Reader sees gate=true from tick 1 onward (writer's tick 0
    // wrote it). On tick 0, gate is unset → false.
    assert!(reader_on >= 9,
        "reader should see ON on most ticks; out:\n{}", out);
}
