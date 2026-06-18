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

/// Run the CLI against a real example file from examples/. Exercises
/// types, claims with sub-schema mapping, ClaimCall, and field access
/// — a realistic mix.

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

/// Confirms the new `--width / --height / --title / --host / --port`
/// flags parse cleanly and the program still gets to the executor
/// entry point. The .ev file is the same trivial echo automaton from
/// `cli_execute_echoes_stdin` — the SDL plugin isn't wired in yet, so
/// what we're really testing here is that arg parsing doesn't reject
/// the SDL/TCP-shaped flags.

/// `evident execute --help` should print usage including the new
/// flags, without requiring a file argument.

/// `evident execute` on a program that UNSATs every step should warn
/// loud (one stderr line per UNSAT step) by default. This is the
/// production-mode contract: silent UNSAT is treated as a bug.

/// `--quiet` should suppress per-step UNSAT warnings entirely.

/// `--explain` should add the schema-body dump after each per-step
/// UNSAT warning. Verifies the pretty-printer is wired in (looks for
/// the readable form of `counter < 0`, not the AST debug form).

// ---------------------------------------------------------------------------
// import "path"
// ---------------------------------------------------------------------------

/// `parse` of every real demo from the parent project under
/// `programs/sdl_demo/` and `programs/balls_demo/` should succeed.
/// Locks in: `import` resolution, implies-block parsing, ⟨…⟩ seq
/// literals, and the engine schema patterns. We don't `query` /
/// `execute` here (would need an SDL display); just a parse smoke
/// to catch regressions in syntax support.
// (Removed: cli_query_audio_bindings_resolve — depended on
// packages/sdl.ev's legacy SDLAudio plugin type, deleted in the
// stdlib cleanup. Audio is now expected to land via the FTI
// pattern; replace with an FTI-shaped test once an SDL_Audio
// bridge exists.)

/// `next_main = "halt"` shuts the executor down cleanly. Verifies the
/// MainCoordinator halt path: program runs at least one step, plugins
/// produce their output, then the swap-check sees "halt" and breaks.

/// `--initial-state file.json` seeds the first frame's `given` from
/// JSON. Verifies the file is parsed and the values reach the
/// constraint solver — the program echoes the seeded `world.score`
/// to stdout via dst.out, halts, expected output is the score char.

/// Real program-swap: scene_a writes "A" + sets next_main to scene_b's
/// path; scene_b writes "B" + halts. Verifies the executor loads
/// scene_b mid-run and continues stepping with the new program.


// ── Stage 2: `evident dump-ast` ─────────────────────────────────

// ── Stage 3: `evident infer-types` ──────────────────────────────





// ── Stage 4: CLI tests for new rules ────────────────────────────





// ── Stage 6: infer-types now dispatches iter_types rules too ───




// ── Stage 6 backfill ────────────────────────────────────────────





// ── Stage 7: aggregated `Inferred types:` table ────────────────




// ── Stage 8: multi-claim iteration ─────────────────────────────





// ── Stage 9: propagation rule reachable via CLI ────────────────




// ── Stage 10: --strict + consistency checks ────────────────────







// ── Stage 11: `evident lint` self-hosted lint pass ─────────────

#[test]
fn cli_lint_clean_program_exits_0() {
    let path = write_tmp("lint_clean", "claim t\n    x ∈ Int\n    y ∈ Bool\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["lint", path.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(),
        "clean program → exit 0; got {:?}", out.status.code());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("no lint issues"),
        "expected `no lint issues` message; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_lint_finds_duplicate_membership() {
    let path = write_tmp("lint_dup",
        "claim t\n    x ∈ Int\n    y ∈ Bool\n    x ∈ String\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["lint", path.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(5),
        "lint with findings → exit 5; got {:?}", out.status.code());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("duplicate_membership_in_body"),
        "expected rule name in output; got: {s}");
    assert!(s.contains("`x`"),
        "expected duplicated var name `x`; got: {s}");
    assert!(s.contains("`Int`") && s.contains("`String`"),
        "expected both type names; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_lint_finds_dup_in_specific_claim() {
    // Multi-claim: dup is in claim `b` only.
    let path = write_tmp("lint_multi_dup",
        "claim a\n    x ∈ Int\n    y ∈ Bool\n\nclaim b\n    z ∈ Int\n    z ∈ String\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["lint", path.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(5));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("in claim `b`"),
        "expected claim attribution `b`; got: {s}");
    assert!(s.contains("`z`"),
        "expected dup var `z`; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_lint_no_args_prints_usage() {
    let out = Command::new(bin()).args(["lint"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("lint"),
        "stderr should mention lint; got: {stderr}");
}

#[test]
fn cli_lint_missing_file_exits_1() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["lint", "/no/such/lint/file/exists.ev"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
}

// ── Stage 12+: --infer-types flag wires self-hosted inference ──
//                into query
