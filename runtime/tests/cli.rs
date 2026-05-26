//! End-to-end tests for the CLI binary. Spawns the compiled binary
//! and checks its stdout / exit code.
//!
//! The surviving subcommands are `sample`, `test`, and `effect-run`.
//! The single SAT/UNSAT decision the former `query` subcommand made is
//! now `sample <file> <schema> -n 1` (a one-model sample IS a SAT/UNSAT
//! decision plus a witness); the former `check` is `sample --all`.

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
fn cli_sample_sat_prints_bindings() {
    let path = write_tmp("simple",
        "schema Pair\n    a ∈ Nat\n    b ∈ Nat\n    a + b = 10\n    a > 0\n    b > 0\n");
    let out = Command::new(bin()).args(["sample", path.to_str().unwrap(), "Pair", "-n", "1"])
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
fn cli_sample_unsat_yields_no_model() {
    // UNSAT schema → `sample` produces no models. With --json that's an
    // empty array; in text form, no binding lines.
    let path = write_tmp("unsat", "schema Bad\n    n ∈ Nat\n    n > 10\n    n < 3\n");
    let out = Command::new(bin())
        .args(["sample", path.to_str().unwrap(), "Bad", "-n", "1", "--json"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "[]");
}

#[test]
fn cli_sample_with_given() {
    let path = write_tmp("given",
        "schema S\n    a ∈ Nat\n    b ∈ Nat\n    a + b = 10\n");
    let out = Command::new(bin())
        .args(["sample", path.to_str().unwrap(), "S", "-n", "1", "--given", "a=7"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("a=7"), "stdout: {s}");
    assert!(s.contains("b=3"), "stdout: {s}");
}

#[test]
fn cli_sample_all_reports_per_schema() {
    // `sample --all` subsumes the former `check`: one SAT/UNSAT line
    // per schema.
    let path = write_tmp("check",
        "schema A\n    n ∈ Nat\n    n > 0\n\
         schema B\n    n ∈ Nat\n    n > 100\n    n < 3\n");
    let out = Command::new(bin()).args(["sample", path.to_str().unwrap(), "--all"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("SAT    A"),  "stdout: {s}");
    assert!(s.contains("UNSAT  B"), "stdout: {s}");
}

#[test]
fn cli_sample_all_json_object() {
    let path = write_tmp("check_json",
        "schema A\n    n ∈ Nat\n    n > 0\n\
         schema B\n    n ∈ Nat\n    n > 100\n    n < 3\n");
    let out = Command::new(bin()).args(["sample", path.to_str().unwrap(), "--all", "--json"])
        .output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("\"A\": true"),  "stdout: {s}");
    assert!(s.contains("\"B\": false"), "stdout: {s}");
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
    let out = Command::new(bin())
        .args(["test", "-v", "--no-color", renamed.to_str().unwrap()])
        .output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stdout: {s}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(s.contains("PASS sat_ok"),    "stdout: {s}");
    assert!(s.contains("PASS unsat_bad"), "stdout: {s}");
    assert!(s.contains("2 passed"),       "stdout: {s}");
    let _ = std::fs::remove_file(&renamed);
}

#[test]
fn cli_sample_json_output() {
    let path = write_tmp("json", "schema S\n    n ∈ Nat\n    n = 7\n");
    let out = Command::new(bin())
        .args(["sample", path.to_str().unwrap(), "S", "-n", "1", "--json"])
        .output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("\"n\": 7"), "stdout: {s}");
}

/// `s = ⟨10, 20, 30⟩` — Unicode angle-bracket sequence literal end-to-end
/// through the binary (lexer + parser + translator + extraction + stdout).
#[test]
fn cli_sample_seq_literal() {
    let path = write_tmp("seqlit",
        "schema S\n    s ∈ Seq(Int)\n    s = ⟨10, 20, 30⟩\n");
    let out = Command::new(bin()).args(["sample", path.to_str().unwrap(), "S", "-n", "1"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("s=[10, 20, 30]"), "expected 's=[10, 20, 30]' in: {s}");
}

// ---------------------------------------------------------------------------
// import "path"
// ---------------------------------------------------------------------------

/// Helper: write `body` to a temp file at a specific absolute path
/// (for tests that need files at known relative locations to each
/// other). Returns the path.
fn write_at(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    p
}

/// `import "lib.ev"` from a sibling file should resolve and the
/// imported file's schemas should be queryable through the importing
/// file.
#[test]
fn cli_import_loads_referenced_file() {
    // Use a unique sub-directory under the OS temp dir so concurrent
    // test runs don't collide on file names.
    let dir = std::env::temp_dir().join(format!(
        "evident-rt-import-{}-{}", std::process::id(), "loads"));
    std::fs::create_dir_all(&dir).unwrap();
    write_at(&dir, "lib.ev",
        "type Point\n    x ∈ Int\n    y ∈ Int\n");
    let main = write_at(&dir, "main.ev",
        "import \"lib.ev\"\n\
         schema HasPoint\n    p ∈ Point\n    p.x = 3\n    p.y = 7\n");
    let out = Command::new(bin())
        .args(["sample", main.to_str().unwrap(), "HasPoint", "-n", "1"])
        .output().unwrap();
    assert!(out.status.success(),
        "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("p.x=3"), "stdout: {s}");
    assert!(s.contains("p.y=7"), "stdout: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A imports B, B imports A — the runtime should detect the cycle and
/// not infinite-loop. Both files end up loaded exactly once.
#[test]
fn cli_import_cycle_safe() {
    let dir = std::env::temp_dir().join(format!(
        "evident-rt-import-{}-{}", std::process::id(), "cycle"));
    std::fs::create_dir_all(&dir).unwrap();
    write_at(&dir, "a.ev",
        "import \"b.ev\"\n\
         schema A\n    n ∈ Nat\n    n = 1\n");
    write_at(&dir, "b.ev",
        "import \"a.ev\"\n\
         schema B\n    n ∈ Nat\n    n = 2\n");
    let main = dir.join("a.ev");
    let out = Command::new(bin())
        .args(["sample", main.to_str().unwrap(), "B", "-n", "1"])
        .output().unwrap();
    assert!(out.status.success(),
        "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("n=2"), "stdout: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `import "sub/lib.ev"` from a file at /tmp/foo/main.ev should find
/// /tmp/foo/sub/lib.ev — i.e. relative-to-file resolution works.
#[test]
fn cli_import_relative_to_file() {
    let dir = std::env::temp_dir().join(format!(
        "evident-rt-import-{}-{}", std::process::id(), "relpath"));
    std::fs::create_dir_all(&dir).unwrap();
    write_at(&dir, "sub/lib.ev",
        "type Inner\n    z ∈ Int\n");
    let main = write_at(&dir, "main.ev",
        "import \"sub/lib.ev\"\n\
         schema HasInner\n    i ∈ Inner\n    i.z = 42\n");
    let out = Command::new(bin())
        .args(["sample", main.to_str().unwrap(), "HasInner", "-n", "1"])
        .output().unwrap();
    assert!(out.status.success(),
        "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("i.z=42"), "stdout: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Dropped constraints are now hard errors by default — silently
/// dropping a constraint produces wrong models, so the runtime
/// exits non-zero. `EVIDENT_LENIENT=1` demotes to a warning for
/// mid-refactor work.
#[test]
fn cli_dropped_constraint_is_an_error() {
    // `Set(Pos)` isn't supported (the runtime warns + drops); using
    // it as the LHS of an equality forces translate_bool to fail and
    // the constraint drops. With strict default, exits non-zero.
    let path = write_tmp("dropped",
        "schema S\n    s ∈ Set(Pos)\n    s = {1, 2}\n");
    let out = Command::new(bin())
        .args(["sample", path.to_str().unwrap(), "S", "-n", "1"])
        .output().unwrap();
    assert!(!out.status.success(),
        "expected exit !=0 on dropped constraint; stdout: {}, stderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error: dropped constraint"),
        "expected error message in stderr: {stderr}");
}

#[test]
fn cli_dropped_constraint_lenient_demotes_to_warning() {
    let path = write_tmp("dropped_lenient",
        "schema S\n    s ∈ Set(Pos)\n    s = {1, 2}\n");
    let out = Command::new(bin())
        .args(["sample", path.to_str().unwrap(), "S", "-n", "1"])
        .env("EVIDENT_LENIENT", "1")
        .output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("warning: dropped constraint"),
        "expected warning in stderr with EVIDENT_LENIENT=1: {stderr}");
}
