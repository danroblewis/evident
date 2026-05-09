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

/// `evident execute --help` should print usage including the new
/// flags, without requiring a file argument.

/// `evident execute` on a program that UNSATs every step should warn
/// loud (one stderr line per UNSAT step) by default. This is the
/// production-mode contract: silent UNSAT is treated as a bug.

/// `--quiet` should suppress per-step UNSAT warnings entirely.

/// `--explain` should add the schema-body dump after each per-step
/// UNSAT warning. Verifies the pretty-printer is wired in (looks for
/// the readable form of `counter < 0`, not the AST debug form).

/// `s = ⟨10, 20, 30⟩` — Unicode angle-bracket sequence literal end-to-end
/// through the binary (lexer + parser + translator + extraction + stdout).
#[test]
fn cli_query_seq_literal() {
    let path = write_tmp("seqlit",
        "schema S\n    s ∈ Seq(Int)\n    s = ⟨10, 20, 30⟩\n");
    let out = Command::new(bin()).args(["query", path.to_str().unwrap(), "S"])
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
        .args(["query", main.to_str().unwrap(), "HasPoint"])
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
        .args(["query", main.to_str().unwrap(), "B"])
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
        .args(["query", main.to_str().unwrap(), "HasInner"])
        .output().unwrap();
    assert!(out.status.success(),
        "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("i.z=42"), "stdout: {s}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `parse` of every real demo from the parent project under
/// `programs/sdl_demo/` and `programs/balls_demo/` should succeed.
/// Locks in: `import` resolution, implies-block parsing, ⟨…⟩ seq
/// literals, and the engine schema patterns. We don't `query` /
/// `execute` here (would need an SDL display); just a parse smoke
/// to catch regressions in syntax support.
/// `evident query` on a program declaring `audio ∈ SDLAudio` should
/// resolve the type from the embedded audio stdlib (loaded by
/// cmd_execute) AND from `stdlib/sdl.ev` (which now ships SDLAudio).
/// This test runs query against a tiny synth program and verifies the
/// audio.* bindings come out arithmetically correct — proves the
/// audio-stdlib-types-aren't-dropped path works end-to-end without
/// having to actually open an audio device.
#[test]
fn cli_query_audio_bindings_resolve() {
    // Use stdlib/sdl.ev so SDLAudio resolves the same way `execute`
    // would — via the project's stdlib import.
    let src = "import \"stdlib/sdl.ev\"\n\
               type main\n    audio ∈ SDLAudio\n    \
               audio.playing = true\n    \
               audio.frequency = 440\n    \
               audio.volume = 80\n    \
               audio.waveform = 0\n";
    let path = write_tmp("audio_bindings", src);
    let out = Command::new(bin())
        .current_dir(env!("CARGO_MANIFEST_DIR").to_string() + "/..")
        .args(["query", path.to_str().unwrap(), "main"])
        .output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("unknown type SDLAudio"),
        "SDLAudio should be resolvable from stdlib/sdl.ev: {stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("audio.playing=true"),
        "missing audio.playing binding: {stdout}");
    assert!(stdout.contains("audio.frequency=440"),
        "missing audio.frequency binding: {stdout}");
    assert!(stdout.contains("audio.volume=80"),
        "missing audio.volume binding: {stdout}");
    assert!(stdout.contains("audio.waveform=0"),
        "missing audio.waveform binding: {stdout}");
}

/// `next_main = "halt"` shuts the executor down cleanly. Verifies the
/// MainCoordinator halt path: program runs at least one step, plugins
/// produce their output, then the swap-check sees "halt" and breaks.

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
        .args(["query", path.to_str().unwrap(), "S"])
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
        .args(["query", path.to_str().unwrap(), "S"])
        .env("EVIDENT_LENIENT", "1")
        .output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("warning: dropped constraint"),
        "expected warning in stderr with EVIDENT_LENIENT=1: {stderr}");
}

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

#[test]
fn cli_query_infer_types_succeeds_for_undeclared_vars() {
    // Program with no ∈ Type annotations — inference fills them in.
    let path = write_tmp("query_infer",
        "claim t\n    msg = \"hello\"\n    n = 42\n    flag = true\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin())
        .current_dir(repo_root)
        .args(["query", path.to_str().unwrap(), "t"])
        .output().unwrap();
    assert!(out.status.success(),
        "query with --infer-types should succeed; stderr: {}",
        String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    // All three bindings should appear.
    assert!(s.contains("msg=\"hello\""), "missing msg binding; got: {s}");
    assert!(s.contains("n=42"),          "missing n binding; got: {s}");
    assert!(s.contains("flag=true"),     "missing flag binding; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_query_without_infer_types_fails_for_undeclared_vars() {
    // Same source — under --strict, no inference fires and the
    // constraint `msg = "hello"` can't translate (no type for msg).
    let path = write_tmp("query_no_infer",
        "claim t\n    msg = \"hello\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin())
        .current_dir(repo_root)
        .args(["query", "--strict", path.to_str().unwrap(), "t"])
        .output().unwrap();
    assert!(!out.status.success(),
        "under --strict, undeclared vars should fail the query");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_query_infer_types_announces_added_memberships() {
    let path = write_tmp("query_infer_announce",
        "claim t\n    msg = \"hi\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin())
        .current_dir(repo_root)
        .args(["query", path.to_str().unwrap(), "t"])
        .output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("inference: added"),
        "expected announce message on stderr; got: {stderr}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_query_infer_types_skips_already_declared_vars() {
    // User already declared msg ∈ String. --infer-types should be
    // idempotent: adds nothing (or the same Membership), query
    // still works the same way.
    let path = write_tmp("query_infer_dup",
        "claim t\n    msg ∈ String\n    msg = \"hello\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin())
        .current_dir(repo_root)
        .args(["query", path.to_str().unwrap(), "t"])
        .output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("msg=\"hello\""),
        "binding should still resolve; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_query_infer_types_skips_ambiguous_vars() {
    // score declared Nat AND assigned 100 (Int) — ambiguous.
    // --infer-types should skip the ambiguous case (already
    // declared ∈ Nat, so the user's annotation wins anyway).
    let path = write_tmp("query_infer_ambig",
        "claim t\n    score ∈ Nat\n    score = 100\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin())
        .current_dir(repo_root)
        .args(["query", path.to_str().unwrap(), "t"])
        .output().unwrap();
    assert!(out.status.success(),
        "ambiguous + already-declared shouldn't break the query; \
         stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("score=100"),
        "binding should resolve; got: {s}");
    let _ = std::fs::remove_file(&path);
}
