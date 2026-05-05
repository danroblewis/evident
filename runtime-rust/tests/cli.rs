//! End-to-end tests for the CLI binary. Spawns the compiled binary
//! and checks its stdout / exit code.

use std::io::Write;
use std::process::Command;

fn bin() -> &'static str {
    // cargo test sets CARGO_BIN_EXE_<name> for binaries in the package.
    env!("CARGO_BIN_EXE_evident")
}

fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("evident-runtime-test-{}-{}.ev", std::process::id(), name));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    path
}

#[test]
fn cli_query_sat_prints_bindings() {
    let path = write_tmp("simple",
        "schema Pair\n    a ∈ Nat\n    b ∈ Nat\n    a + b = 10\n    a > 0\n    b > 0\n");
    let out = Command::new(bin()).args(["query", path.to_str().unwrap(), "Pair"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    // Both bindings present; values satisfy a + b = 10.
    let mut a = 0; let mut b = 0;
    for line in s.lines() {
        if let Some(v) = line.strip_prefix("a=") { a = v.parse::<i64>().unwrap(); }
        if let Some(v) = line.strip_prefix("b=") { b = v.parse::<i64>().unwrap(); }
    }
    assert_eq!(a + b, 10);
}

#[test]
fn cli_query_unsat_exits_1() {
    let path = write_tmp("unsat", "schema Bad\n    n ∈ Nat\n    n > 10\n    n < 3\n");
    let out = Command::new(bin()).args(["query", path.to_str().unwrap(), "Bad"])
        .output().unwrap();
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("UNSAT"));
}

#[test]
fn cli_query_with_given() {
    let path = write_tmp("given",
        "schema S\n    a ∈ Nat\n    b ∈ Nat\n    a + b = 10\n");
    let out = Command::new(bin())
        .args(["query", path.to_str().unwrap(), "S", "--given", "a=7"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("a=7"));
    assert!(s.contains("b=3"));
}

/// Run the CLI against a real example file from examples/. Exercises
/// types, claims with sub-schema mapping, ClaimCall, and field access
/// — a realistic mix.
#[test]
fn cli_query_examples_scheduling() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = format!("{}/examples/scheduling.ev", manifest);
    let out = Command::new(bin())
        .args(["query", &path, "FitTwoSlots"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    let mut found = std::collections::HashMap::new();
    for line in s.lines() {
        if let Some((k, v)) = line.split_once('=') {
            found.insert(k.to_string(), v.to_string());
        }
    }
    // Pinned-by-constraint values:
    assert_eq!(found.get("a.start").map(|s| s.as_str()), Some("10"));
    assert_eq!(found.get("a.duration").map(|s| s.as_str()), Some("30"));
    assert_eq!(found.get("a.duration").map(|s| s.as_str()), Some("30"));
    assert_eq!(found.get("b.duration").map(|s| s.as_str()), Some("25"));
    assert_eq!(found.get("deadline").map(|s| s.as_str()), Some("100"));
    // b.start should satisfy a.start + a.duration ≤ b.start ≤ 100 - 25.
    let bs: i64 = found["b.start"].parse().unwrap();
    assert!(bs >= 40 && bs <= 75, "b.start = {bs}");
}

#[test]
fn cli_check_reports_per_schema() {
    let path = write_tmp("check",
        "schema A\n    n ∈ Nat\n    n > 0\n\
         schema B\n    n ∈ Nat\n    n > 100\n    n < 3\n");
    let out = Command::new(bin()).args(["check", path.to_str().unwrap()])
        .output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("SAT    A"),  "stdout: {s}");
    assert!(s.contains("UNSAT  B"), "stdout: {s}");
    // Exit nonzero because at least one schema was UNSAT.
    assert!(!out.status.success());
}

#[test]
fn cli_test_runs_sat_unsat_claims() {
    let path = write_tmp("testfile",
        "claim sat_ok\n    n ∈ Nat\n    n > 0\n\
         claim unsat_bad\n    n ∈ Nat\n    n > 10\n    n < 3\n");
    // Need the file name to start with `test_` so the discovery picks
    // it up when given the directory. Move it.
    let parent = path.parent().unwrap();
    let renamed = parent.join(format!("test_{}.ev", std::process::id()));
    std::fs::rename(&path, &renamed).unwrap();
    let out = Command::new(bin()).args(["test", renamed.to_str().unwrap()])
        .output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stdout: {s}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(s.contains("PASS  sat_ok"),    "stdout: {s}");
    assert!(s.contains("PASS  unsat_bad"), "stdout: {s}");
    assert!(s.contains("2 passed"),         "stdout: {s}");
    let _ = std::fs::remove_file(&renamed);
}

#[test]
fn cli_query_json_output() {
    let path = write_tmp("json", "schema S\n    n ∈ Nat\n    n = 7\n");
    let out = Command::new(bin())
        .args(["query", path.to_str().unwrap(), "S", "--json"])
        .output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("\"satisfied\": true"));
    assert!(s.contains("\"n\": 7"));
}

#[test]
fn cli_execute_echoes_stdin() {
    // Tiny echo automaton — copy each char from src.char to dst.out.
    // Feed "hi\n" on stdin via the CLI, expect "hi\n" on stdout.
    let path = write_tmp("execute_echo",
        "schema main\n    src ∈ Stdin\n    dst ∈ Stdout\n    dst.out = src.char\n");
    let mut child = std::process::Command::new(bin())
        .args(["execute", path.to_str().unwrap()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"hi\n").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(),
        "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
}

#[test]
fn cli_batch_says_parked() {
    // batch / repl still print the parked message and exit 2.
    let out = Command::new(bin()).args(["batch", "ignored.ev"])
        .output().unwrap();
    assert!(!out.status.success());
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("not yet implemented"));
}

/// Confirms the new `--width / --height / --title / --host / --port`
/// flags parse cleanly and the program still gets to the executor
/// entry point. The .ev file is the same trivial echo automaton from
/// `cli_execute_echoes_stdin` — the SDL plugin isn't wired in yet, so
/// what we're really testing here is that arg parsing doesn't reject
/// the SDL/TCP-shaped flags.
#[test]
fn cli_execute_accepts_sdl_and_tcp_flags() {
    let path = write_tmp("execute_flags",
        "schema main\n    src ∈ Stdin\n    dst ∈ Stdout\n    dst.out = src.char\n");
    let mut child = std::process::Command::new(bin())
        .args(["execute", path.to_str().unwrap(),
               "--width", "1024", "--height", "768",
               "--title", "Test Window",
               "--host", "0.0.0.0", "--port", "9090"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"x\n").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(),
        "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // Echo automaton mirrors stdin to stdout; flags are stored but
    // not consumed by the headless plugins yet.
    assert_eq!(String::from_utf8_lossy(&out.stdout), "x\n");
}

/// `evident execute --help` should print usage including the new
/// flags, without requiring a file argument.
#[test]
fn cli_execute_help_lists_flags() {
    let out = Command::new(bin()).args(["execute", "--help"])
        .output().unwrap();
    assert!(out.status.success(),
        "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // usage() writes to stderr.
    let s = String::from_utf8_lossy(&out.stderr);
    assert!(s.contains("--width"),  "missing --width in help: {s}");
    assert!(s.contains("--height"), "missing --height in help: {s}");
    assert!(s.contains("--title"),  "missing --title in help: {s}");
    assert!(s.contains("--host"),   "missing --host in help: {s}");
    assert!(s.contains("--port"),   "missing --port in help: {s}");
}

#[test]
fn cli_parse_lists_schema_names() {
    let path = write_tmp("multi",
        "type T\n    x ∈ Int\n\
         schema S\n    n ∈ Nat\n");
    let out = Command::new(bin()).args(["parse", path.to_str().unwrap()])
        .output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    let names: std::collections::HashSet<_> = s.lines().collect();
    assert!(names.contains("T"));
    assert!(names.contains("S"));
}
