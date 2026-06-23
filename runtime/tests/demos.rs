use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

const EVIDENT: &str = env!("CARGO_BIN_EXE_evident");

struct DemoExpect {
    name:        &'static str,
    exit:        i32,

    must_lines:  &'static [&'static str],

    forbid_exact_lines: &'static [&'static str],
    max_steps:   usize,
    tick_ms:     u64,

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

        name: "test_02_counter", exit: 0,
        must_lines: &["starting count", "tick 5", "tick 4", "tick 3",
                      "tick 2", "tick 1", "bye"],
        forbid_exact_lines: &["tick", "tick 0"],
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
        must_lines: &["good: parsed an Int", "bad: sentinel was correct"],
        forbid_exact_lines: &["good: WRONG", "bad: WRONG"],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_05_int_to_str", exit: 0,
        must_lines: &["42", "-7"],
        forbid_exact_lines: &["?", "<no string>"],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_08_exit_code", exit: 42,
        must_lines: &["exiting with code 42"],
        forbid_exact_lines: &[],
        max_steps: 10, tick_ms: 0, stdin: None,
    },

    DemoExpect {

        name: "test_17_sdl_triangle", exit: 0,
        must_lines: &["done"],
        forbid_exact_lines: &[],
        max_steps: 5, tick_ms: 0, stdin: None,
    },
    DemoExpect {

        name: "test_19_prev_tick", exit: 0,
        must_lines: &["count = 0", "count = 1", "count = 2", "done"],
        forbid_exact_lines: &[],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {

        name: "test_20_pure_counter", exit: 0,
        must_lines: &["starting", "count = 0", "count = 1", "count = 2", "count = 3"],
        forbid_exact_lines: &["count = ?"],
        max_steps: 15, tick_ms: 0, stdin: None,
    },
    DemoExpect {

        name: "test_21_mario", exit: 0,
        must_lines: &["mario done"],
        forbid_exact_lines: &[],
        max_steps: 260, tick_ms: 0, stdin: None,
    },
    DemoExpect {

        name: "test_22_prev_record", exit: 0,
        must_lines: &["pos.x+pos.y = 0", "pos.x+pos.y = 3", "pos.x+pos.y = 6", "walker done at 9"],
        forbid_exact_lines: &["pos.x+pos.y = ?"],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {

        name: "test_23_difference", exit: 0,
        must_lines: &["x = 10", "x = 9", "x = 8", "landed at 7"],
        forbid_exact_lines: &["x = 11", "landed at 6"],
        max_steps: 10, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_24_fib", exit: 0,
        must_lines: &["fib = 1", "fib = 2", "fib = 3", "fib = 5", "fib = 8",
                      "fib = 13", "fib = 89", "done at 144"],
        forbid_exact_lines: &["fib = 4", "fib = 7"],
        max_steps: 14, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_25_oscillator", exit: 0,
        must_lines: &["milli = 60000", "milli = 58333", "milli = 55138",
                      "milli = 50590", "milli = 38257", "milli = 7198",
                      "crossed zero at milli -526"],
        forbid_exact_lines: &["milli = 60001"],
        max_steps: 14, tick_ms: 0, stdin: None,
    },
    DemoExpect {
        name: "test_26_initial", exit: 0,
        must_lines: &["sum = 0", "sum = 3", "sum = 6", "sum = 9"],
        forbid_exact_lines: &["sum = 1", "sum = 2"],
        max_steps: 10, tick_ms: 0, stdin: None,
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
