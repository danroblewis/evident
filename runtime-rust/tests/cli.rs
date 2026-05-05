//! End-to-end tests for the CLI binary. Spawns the compiled binary
//! and checks its stdout / exit code.

use std::io::Write;
use std::process::Command;

fn bin() -> &'static str {
    // cargo test sets CARGO_BIN_EXE_<name> for binaries in the package.
    env!("CARGO_BIN_EXE_evident-runtime")
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
