//! End-to-end driver for `examples/test_*.ev`.
//!
//! For each demo file, runs:
//!   1. `evident test <file>`      — static sat_/unsat_ claims
//!   2. `evident effect-run <file>` — multi-FSM end-to-end
//!
//! The expectations table below pins each demo to:
//!   * exact exit code
//!   * a sequence of stdout lines that must appear IN ORDER
//!     (not just "contains substring" — the demo must walk
//!     through the whole expected behavior, not just hit one
//!     keyword by accident through a wrong code path).
//!
//! Add a row when a new demo lands. WIP / interactive demos
//! (stdin, signals, broken counterexamples) can be left out
//! without breaking CI; document why in the comment block.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

const EVIDENT: &str = env!("CARGO_BIN_EXE_evident");

struct DemoExpect {
    name:        &'static str,
    exit:        i32,
    /// Lines that MUST appear in stdout, in this order. Other
    /// lines may appear between/around them. Empty = only check
    /// exit code.
    must_lines:  &'static [&'static str],
    /// Exact whole-line strings that must NOT appear on their
    /// own line in stdout. Catches placeholder output (e.g.
    /// the literal "tick" instead of "tick 5") that would
    /// satisfy a substring check via the wrong path.
    forbid_exact_lines: &'static [&'static str],
    max_steps:   usize,
    tick_ms:     u64,  // 0 = unset
    /// Optional stdin to pipe in.
    stdin:       Option<&'static str>,
}

const EXPECTATIONS: &[DemoExpect] = &[
    DemoExpect {
        name: "test_01_hello", exit: 0,
        must_lines: &["hello from evident"],
        forbid_exact_lines: &[],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Must walk 5 → 1 in order. Catches "tick" placeholder.
        name: "test_02_counter", exit: 0,
        must_lines: &["starting count", "tick 5", "tick 4", "tick 3",
                      "tick 2", "tick 1", "bye"],
        forbid_exact_lines: &["tick", "tick 0"],  // forbid "tick" with no number
        max_steps: 30, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_03_seq_chain", exit: 0,
        must_lines: &["first", "second", "third"],
        forbid_exact_lines: &[],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_04_parse_int", exit: 0,
        must_lines: &["good: parsed an Int", "bad: ERROR was correct"],
        forbid_exact_lines: &[],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_05_int_to_str", exit: 0,
        must_lines: &["42"],
        forbid_exact_lines: &["?", "<no string>"],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // ShellRun captured `date` — should look like "2026-..".
        name: "test_06_shell_run", exit: 0,
        must_lines: &["20"],  // year prefix
        forbid_exact_lines: &["<no string>", "<no result>"],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_07_time", exit: 0,
        must_lines: &["elapsed_ms = "],
        forbid_exact_lines: &["?", "<no string>", "elapsed_ms = -1"],
        max_steps: 30, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_08_exit_code", exit: 42,
        must_lines: &["exiting with code 42"],
        forbid_exact_lines: &[],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Consumer must echo specific n values, not just "got n".
        name: "test_09_two_fsms", exit: 0,
        must_lines: &["consumer saw n = 3", "producer done"],
        forbid_exact_lines: &["got n", "consumer saw n = 0", "consumer saw n = ?"],
        max_steps: 30, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Worker must actually fire AFTER parent's spawn, not just
        // parent's own "issued spawn" line.
        name: "test_10_spawn", exit: 0,
        must_lines: &["parent issued spawn", "worker spawned with id=7", "parent done"],
        forbid_exact_lines: &[],
        max_steps: 15, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_11_frameclock", exit: 0,
        must_lines: &["3 clock ticks observed"],
        forbid_exact_lines: &[],
        max_steps: 60, tick_ms: 50, stdin: None,
    },
    DemoExpect {
        // Must contain a real hostname value, not just an
        // acknowledgement. The exact-line forbid catches the
        // "= " with nothing after it (bridge wrote empty).
        name: "test_12_hostname", exit: 0,
        must_lines: &["hostname = "],
        forbid_exact_lines: &["hostname = ", "hostname unknown"],
        max_steps: 15, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_13_timer", exit: 0,
        must_lines: &["3 timer ticks observed"],
        forbid_exact_lines: &[],
        max_steps: 60, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Stdin echo: pipe lines, expect each echoed back, then "bye".
        name: "test_14_stdin", exit: 0,
        must_lines: &["hi", "world", "bye"],
        forbid_exact_lines: &["did not halt"],
        max_steps: 100, tick_ms: 0, stdin: Some("hi\nworld\nquit\n"),
    },
    // test_15_signal — needs SIGINT, only meaningful interactive.
    // test_16_sdl_red — needs a display; renders correctly when run
    //   manually but not testable in a headless CI.
    DemoExpect {
        // SDL triangle: setup + render in ONE Seq on tick 0, halt.
        // Visible verification needs a display; here we just check
        // the program runs to its halt without error.
        name: "test_17_sdl_triangle", exit: 0,
        must_lines: &["done"],
        forbid_exact_lines: &[],
        max_steps: 5, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Reflection world-plugin: declare `program ∈ Program` in
        // the World type, runtime auto-installs the bridge, FSM
        // pattern-matches the encoded AST. Success line proves
        // the value flowed through to Z3 (Bool decided by the pin).
        name: "test_18_reflection", exit: 0,
        must_lines: &["reflected: program is loaded"],
        forbid_exact_lines: &["reflected: program missing"],
        max_steps: 5, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // `_var` time-shift convention: every var's previous-tick
        // value is available as `_var`; `is_first_tick` is auto-
        // injected when any `_var` is referenced. Counter counts
        // 0..2 via `_count + 1`, then halts.
        name: "test_19_prev_tick", exit: 0,
        must_lines: &["count = 0", "count = 1", "count = 2", "done"],
        forbid_exact_lines: &[],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Unified state model: an fsm with NO state enum, NO
        // state-pair — just `count ∈ Int` advanced via `_count
        // + 1`. Smart-inject only adds the slots that are
        // referenced (effects + last_results). Demonstrates
        // that the canonical fsm machinery is opt-in.
        name: "test_20_pure_counter", exit: 0,
        must_lines: &["starting", "count = 0", "count = 1", "count = 2", "count = 3"],
        forbid_exact_lines: &["count = ?"],
        max_steps: 15, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // First multi-tick rendering demo. Per-tick physics +
        // draw using the SDL_Window FTI bridge's persistent
        // renderer handle (win.renderer). Auto-walks across
        // screen bouncing off walls, falls under gravity to
        // ground level. 240 frames × 16ms ≈ 4s of visible
        // animation (the sdl_delay each tick paces the fsm
        // to ~60fps so SDL has time to show the window).
        // Visual verification: capture with --examples and Read
        // the PNG — should show a red square on green ground
        // against sky-blue background.
        name: "test_21_mario", exit: 0,
        must_lines: &["mario done"],
        forbid_exact_lines: &[],
        max_steps: 260, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // `_var` time-shift through RECORD types: `_pos.x` and
        // `_pos.y` get pinned from the previous tick's `pos.x`
        // and `pos.y` bindings. Diagonal walker: (0,0) → (1,2)
        // → (2,4) → (3,6), halts when pos.x ≥ 3.
        // Sums printed are pos.x + pos.y = 0, 3, 6 (the prior
        // tick's IntToStr surfaces next).
        name: "test_22_prev_record", exit: 0,
        must_lines: &["pos.x+pos.y = 0", "pos.x+pos.y = 3", "walker done at 6"],
        forbid_exact_lines: &["pos.x+pos.y = ?"],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // SDL2_mixer audio: the SDL_Mixer FTI bridge opens the audio
        // device + loads examples/assets/tone.wav at install, then
        // plays it once and idles ~1s before halting/closing/exiting.
        // Audio output isn't asserted (a headless box may lack a
        // device); the gate is the clean exit + "done" — a missing
        // device degrades to silence (Mix_LoadWAV → NULL handle),
        // not an error.
        name: "test_24_sdl_mixer", exit: 0,
        must_lines: &["done"],
        forbid_exact_lines: &[],
        max_steps: 30, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Per-component JIT showcase: the `sim` FSM decomposes into
        // SIX components; 3 (integer arithmetic) JIT-compile, 3 (Set
        // output, String-input concat, branch-selected effects) fall
        // to the scoped Z3 solve → `comp=3/6` under
        // EVIDENT_FUNCTIONIZE_STATS=1. The dot-trail grows one char
        // per tick (proving the String component runs end-to-end);
        // halts at frame 4. The growing trail (frame./frame../frame...)
        // is the visible proof; "frame" alone would mean the String
        // component never advanced.
        name: "test_25_per_component_jit", exit: 0,
        must_lines: &["frame.", "frame..", "frame...",
                      "per-component demo: done"],
        forbid_exact_lines: &[],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Cross-tick value cache showcase (session C). A trivial
        // `driver` rotates world.signal through 4 values in 8-tick
        // windows; the `expensive` consumer reads only world.signal
        // and runs a heavy nonlinear Z3 solve (three coupled integer
        // square roots), so when signal is unchanged the value cache
        // returns the prior bindings and skips the solve. Run with
        // EVIDENT_FUNCTIONIZE_STATS=1 to see vh climb (45 of 49 ticks)
        // and EVIDENT_VALUE_CACHE=0 for the ~12x-slower baseline.
        // Here we just gate the window markers + clean exit; the
        // cache win is documented in the demo's docstring.
        name: "test_26_value_cache", exit: 0,
        must_lines: &["signal=10 (window 0)", "signal=40 (window 3)", "driver done"],
        forbid_exact_lines: &[],
        max_steps: 60, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Six disjoint 16-queens boards → six independent slow
        // enum-typed components → solved on parallel Z3 contexts/threads
        // (session E's threading, extended in session S to replay enum /
        // record datatypes into each worker context). We assert only the
        // clean exit + the solved line; the speedup is a wall-clock
        // property documented in the demo's docstring, not pinned here
        // (timing isn't deterministic enough for CI).
        name: "test_27_parallel_solving", exit: 0,
        must_lines: &["solved 6 independent enum-tagged 16-queens boards in parallel"],
        forbid_exact_lines: &[],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Six disjoint dense 24-node 3-colorings (~96 edges each) with
        // enum-typed cells. Post-session-S, the enum components
        // parallelize: 1.9× wall / 3.7× per-tick steady vs PARALLEL_SLOW=0
        // (numbers in the demo's docstring). The FSM loops 20 ticks so
        // per-tick parallel savings amortize the one-shot enum-context
        // replay cost (~15 ms at tick 0).
        name: "test_28_parallel_enum_coloring", exit: 0,
        must_lines: &["step", "solved 6 independent graph 3-colorings"],
        forbid_exact_lines: &[],
        max_steps: 30, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        // Heavy-compute JIT showcase: 90 layers of branch-dependent
        // arithmetic that Z3's simplify can't fold to a polynomial.
        // Steady per-tick ≈ 0.28 ms with JIT vs ≈ 0.89 ms with
        // FUNCTIONIZE=0 → 3.2× steady-state speedup. Test gates the
        // clean run + the "heavy compute: done" line; speed is in
        // the docstring.
        name: "test_29_jit_heavy_compute", exit: 0,
        must_lines: &["step", "heavy compute: done"],
        forbid_exact_lines: &[],
        max_steps: 30, tick_ms: 0, stdin: None,
    },
];

#[test]
fn static_tests_all_pass() {
    let out = Command::new(EVIDENT)
        .args(["test", "examples/"])
        .current_dir("..")
        .output()
        .expect("run evident test");
    assert!(out.status.success(),
        "evident test failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr));
}

#[test]
fn each_demo_runs_to_completion() {
    let mut failures = Vec::new();
    for d in EXPECTATIONS {
        // Demos can be either a single file (`examples/{name}.ev`) or
        // a directory (`examples/{name}/main.ev`).
        let flat = format!("examples/{}.ev", d.name);
        let dir  = format!("examples/{}/main.ev", d.name);
        let path = if Path::new(&format!("../{flat}")).exists() { flat }
                   else if Path::new(&format!("../{dir}")).exists() { dir }
                   else {
                       failures.push(format!("{}: file missing at {flat} or {dir}", d.name));
                       continue;
                   };
        let mut cmd = Command::new(EVIDENT);
        cmd.args(["effect-run", &path, "--max-steps", &d.max_steps.to_string()]);
        cmd.current_dir("..");
        cmd.stdin(if d.stdin.is_some() { Stdio::piped() } else { Stdio::null() });
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        if d.tick_ms > 0 { cmd.env("EVIDENT_TICK_MS", d.tick_ms.to_string()); }

        let out = match wait_with_timeout(cmd, d.stdin, Duration::from_secs(30)) {
            Ok(o) => o,
            Err(e) => { failures.push(format!("{}: {e}", d.name)); continue; }
        };
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let actual_exit = out.status.code().unwrap_or(-1);

        if actual_exit != d.exit {
            failures.push(format!(
                "{}: expected exit {}, got {actual_exit}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                d.name, d.exit));
            continue;
        }
        // must_lines: each must appear, in this order.
        let mut cursor = 0usize;
        for needle in d.must_lines {
            match stdout[cursor..].find(needle) {
                Some(rel) => cursor += rel + needle.len(),
                None => {
                    failures.push(format!(
                        "{}: missing {needle:?} (after position {cursor})\nstdout:\n{stdout}",
                        d.name));
                    break;
                }
            }
        }
        for forbid in d.forbid_exact_lines {
            if stdout.lines().any(|l| l == *forbid) {
                failures.push(format!(
                    "{}: forbidden EXACT line {forbid:?} appeared in stdout:\n{stdout}",
                    d.name));
            }
        }
    }
    assert!(failures.is_empty(),
        "{} demo(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
}

fn wait_with_timeout(mut cmd: Command, stdin: Option<&'static str>, dur: Duration)
    -> Result<std::process::Output, String>
{
    let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
    if let Some(s) = stdin {
        if let Some(mut sin) = child.stdin.take() {
            use std::io::Write;
            let _ = sin.write_all(s.as_bytes());
            // dropping sin closes stdin → EOF
        }
    }
    let start = std::time::Instant::now();
    loop {
        match child.try_wait().map_err(|e| format!("wait: {e}"))? {
            Some(_status) => return child.wait_with_output()
                .map_err(|e| format!("wait_with_output: {e}")),
            None => {}
        }
        if start.elapsed() > dur {
            let _ = child.kill();
            return Err(format!("timeout after {:?}", dur));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
