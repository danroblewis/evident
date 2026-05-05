//! Smoke tests for the SDL plugin.
//!
//! Tests that exercise SDL itself (open a window, render a frame) need
//! a display server, which CI doesn't have — those are gated behind
//! `#[ignore]` and the manual instructions in PROGRESS.md.
//!
//! What we test here without opening a window:
//!   - The embedded SDL stdlib (Color, SDLRect, SDLInput, SDLOutput,
//!     SDLWindow type defs) loads cleanly into a runtime.
//!   - A program declaring `∈ SDLInput` / `∈ SDLOutput` can be loaded
//!     and the type names appear in the schema list.
//!   - A simulated first-frame solve returns SAT and the bindings
//!     contain the expected `output.bg.r/g/b` field values — i.e.
//!     the same shape the real SDL plugin would consume in
//!     `after_step`.

use std::collections::HashMap;

use evident_runtime::executor;
#[allow(unused_imports)] // Plugin is only used by the #[ignore]'d manual test
use evident_runtime::executor::Plugin;
use evident_runtime::plugins::sdl as sdl_plugin;
use evident_runtime::{EvidentRuntime, Value};

fn fresh_runtime_with_stdlibs() -> EvidentRuntime {
    let mut rt = EvidentRuntime::new();
    executor::load_io_stdlib(&mut rt).expect("io stdlib loads");
    rt.load_source(sdl_plugin::STDLIB_SDL_EV).expect("sdl stdlib loads");
    rt
}

#[test]
fn sdl_stdlib_declares_expected_types() {
    let rt = fresh_runtime_with_stdlibs();
    let names: std::collections::HashSet<&str> = rt.schema_names().collect();
    for required in ["Color", "SDLRect", "SDLInput", "SDLOutput", "SDLWindow"] {
        assert!(names.contains(required), "missing SDL type: {required}");
    }
}

#[test]
fn sdl_program_loads_and_reports_main_vars() {
    // Minimal program: declare SDLInput / SDLOutput on `main`, no
    // logic. The schema list should now include `main` alongside the
    // SDL stdlib types.
    let mut rt = fresh_runtime_with_stdlibs();
    let src = "
type main
    input  ∈ SDLInput
    output ∈ SDLOutput
";
    rt.load_source(src).expect("user program parses");
    assert!(rt.get_schema("main").is_some(), "main schema present");
}

#[test]
fn sdl_first_frame_bg_solve() {
    // Mirror what the SDL plugin's first frame would do: feed the
    // input.* fields as `given`, call `query_cached("main", given)`,
    // expect SAT, and read back `output.bg.r/g/b` as Int values.
    //
    // Doesn't open an SDL window — pure constraint solve. Confirms
    // the plumbing the SDL plugin depends on is intact.
    let mut rt = fresh_runtime_with_stdlibs();
    let src = "
type main
    input  ∈ SDLInput
    output ∈ SDLOutput

    output.bg.r = 15
    output.bg.g = 15
    output.bg.b = 30
";
    rt.load_source(src).expect("user program parses");

    // Simulate the input.* fields the SDL plugin contributes per step.
    let mut given: HashMap<String, Value> = HashMap::new();
    given.insert("input.right_held".into(), Value::Bool(false));
    given.insert("input.left_held".into(), Value::Bool(false));
    given.insert("input.up_held".into(), Value::Bool(false));
    given.insert("input.down_held".into(), Value::Bool(false));
    given.insert("input.mouse_x".into(), Value::Int(100));
    given.insert("input.mouse_y".into(), Value::Int(100));
    given.insert("input.click".into(), Value::Bool(false));
    given.insert("input.quit".into(), Value::Bool(false));
    given.insert("input.time".into(), Value::Int(1_700_000_000_000));
    given.insert("input.dt".into(), Value::Int(16));

    let result = rt.query_cached("main", &given).expect("query ok");
    assert!(result.satisfied, "first-frame solve must be SAT");
    assert_eq!(result.bindings.get("output.bg.r"), Some(&Value::Int(15)));
    assert_eq!(result.bindings.get("output.bg.g"), Some(&Value::Int(15)));
    assert_eq!(result.bindings.get("output.bg.b"), Some(&Value::Int(30)));
}

#[test]
fn sdl_plugin_handles_types_matches_stdlib() {
    // The plugin's claimed type names must match the type names defined
    // in the embedded stdlib — otherwise auto-detection in `cmd_execute`
    // would silently never activate the plugin.
    let claimed: std::collections::HashSet<&str> =
        sdl_plugin::SDL_TYPES.iter().copied().collect();
    let rt = fresh_runtime_with_stdlibs();
    for t in &claimed {
        assert!(rt.get_schema(t).is_some(),
            "SDL plugin claims type {t} but it isn't in the stdlib");
    }
}

#[test]
#[ignore] // Opens an actual SDL window — only works when run on the
          // process's main thread (SDL on macOS requires it). Rust's
          // test harness spawns workers, so this aborts under
          // `cargo test`. The reliable way to verify SDL actually
          // opens is the binary path:
          //   cargo run -- execute examples/sdl_mouse_rect.ev
          // (close the window or press Escape to quit). This test
          // is kept as documentation of the API.
fn sdl_window_actually_opens() {
    let mut var_types = HashMap::new();
    var_types.insert("output".to_string(), "SDLOutput".to_string());
    let mut plugin = sdl_plugin::create_sdl_plugin(320, 240, "smoke-test", var_types);
    plugin.initialize(vec!["output".to_string()]);
    let _ = plugin.before_step();
    let bindings: HashMap<String, Value> = HashMap::new();
    let _ = plugin.after_step(&bindings);
}
