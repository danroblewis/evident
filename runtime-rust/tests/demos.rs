//! End-to-end driver for `programs/demos/test_*.ev`.
//!
//! For each demo file, runs:
//!   1. `evident test <file>`      — static sat_/unsat_ claims
//!   2. `evident effect-run <file>` — multi-FSM end-to-end
//!
//! Per-demo expected exit code + (optionally) expected stdout
//! lives in the `EXPECTATIONS` table below. Add a row when a new
//! demo lands; tests only fire for files that have an entry, so
//! WIP demos (or interactive ones like stdin/SDL) can be left
//! out without breaking CI.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

const EVIDENT: &str = env!("CARGO_BIN_EXE_evident");

/// (filename without .ev, expected effect-run exit, expected
/// substring in stdout, max-steps, EVIDENT_TICK_MS override or 0)
const EXPECTATIONS: &[(&str, i32, &str, usize, u64)] = &[
    ("test_01_hello",         0,  "hello from evident",      10,  0),
    ("test_02_counter",       0,  "bye",                     20,  0),
    ("test_03_seq_chain",     0,  "third",                   10,  0),
    ("test_04_parse_int",     0,  "bad: ERROR was correct",  10,  0),
    ("test_05_int_to_str",    0,  "42",                      10,  0),
    ("test_06_shell_run",     0,  "hello-from-shell",        10,  0),
    ("test_07_time",          0,  "got time",                30,  0),
    ("test_08_exit_code",     42, "exiting with code 42",    10,  0),
    ("test_09_two_fsms",      0,  "got n",                   30,  0),
    ("test_10_spawn",         0,  "parent spawned worker",   10,  0),
    ("test_11_frameclock",    0,  "3 clock ticks observed",  60,  50),
    ("test_12_hostname",      0,  "hostname known",          10,  0),
    ("test_13_timer",         0,  "3 timer ticks observed",  60,  0),
    // test_14_stdin needs piped input — tested separately if at all.
    // test_15_signal needs SIGINT — tested separately if at all.
    // test_16_sdl_red needs a display (skipped in headless CI).
    // test_17_sdl_gl_window is a known counterexample.
];

#[test]
fn static_tests_all_pass() {
    let out = Command::new(EVIDENT)
        .args(["test", "programs/demos/"])
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
    for &(name, exp_exit, exp_substr, max_steps, tick_ms) in EXPECTATIONS {
        let path = format!("programs/demos/{name}.ev");
        if !Path::new(&format!("../{path}")).exists() {
            failures.push(format!("{name}: file missing at {path}"));
            continue;
        }
        let mut cmd = Command::new(EVIDENT);
        cmd.args(["effect-run", &path, "--max-steps", &max_steps.to_string()]);
        cmd.current_dir("..");  // imports use repo-root-relative paths
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        if tick_ms > 0 {
            cmd.env("EVIDENT_TICK_MS", tick_ms.to_string());
        }
        let out = match wait_with_timeout(cmd, Duration::from_secs(15)) {
            Ok(o) => o,
            Err(e) => { failures.push(format!("{name}: {e}")); continue; }
        };
        let stdout = String::from_utf8_lossy(&out.stdout);
        let actual_exit = out.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if actual_exit != exp_exit {
            failures.push(format!(
                "{name}: expected exit {exp_exit}, got {actual_exit}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            ));
            continue;
        }
        if !stdout.contains(exp_substr) {
            failures.push(format!(
                "{name}: stdout missing {exp_substr:?}\ngot:\n{stdout}",
            ));
        }
    }
    assert!(failures.is_empty(),
        "{} demo(s) failed:\n\n{}", failures.len(), failures.join("\n\n"));
}

/// Spawn + wait with a wall-clock cap. Avoids relying on shell
/// `timeout` (which is GNU/Linux-flavored on macOS).
fn wait_with_timeout(mut cmd: Command, dur: Duration)
    -> Result<std::process::Output, String>
{
    let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
    let start = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| format!("wait: {e}"))? {
            // Collect output. try_wait already reaped; we need the
            // captured streams. Re-run via wait_with_output on a
            // freshly-spawned cmd — easier: just use output() with
            // a separate timeout watcher thread. For simplicity
            // here, we stream via wait_with_output above.
            let _ = status;
            return read_remaining(child);
        }
        if start.elapsed() > dur {
            let _ = child.kill();
            return Err(format!("timeout after {:?}", dur));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn read_remaining(child: std::process::Child) -> Result<std::process::Output, String> {
    child.wait_with_output().map_err(|e| format!("wait_with_output: {e}"))
}
