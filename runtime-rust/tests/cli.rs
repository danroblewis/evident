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
    assert!(s.contains("--width"),   "missing --width in help: {s}");
    assert!(s.contains("--height"),  "missing --height in help: {s}");
    assert!(s.contains("--title"),   "missing --title in help: {s}");
    assert!(s.contains("--host"),    "missing --host in help: {s}");
    assert!(s.contains("--port"),    "missing --port in help: {s}");
    assert!(s.contains("--quiet"),   "missing --quiet in help: {s}");
    assert!(s.contains("--explain"), "missing --explain in help: {s}");
}

/// `evident execute` on a program that UNSATs every step should warn
/// loud (one stderr line per UNSAT step) by default. This is the
/// production-mode contract: silent UNSAT is treated as a bug.
#[test]
fn cli_execute_unsat_warns_per_step() {
    // Top-level `counter < 0` plus `counter ∈ Nat` is UNSAT for every
    // value of counter. `src ∈ Stdin` makes the loop run a couple of
    // iterations before EOF.
    let path = write_tmp("execute_unsat",
        "schema main\n    src ∈ Stdin\n    counter ∈ Nat\n    counter < 0\n");
    let mut child = std::process::Command::new(bin())
        .args(["execute", path.to_str().unwrap()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"ab").unwrap();  // 2 chars + EOF flush = 3 steps
    }
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let warns = stderr.lines().filter(|l| l.starts_with("warning: step ")).count();
    assert!(warns >= 2,
        "expected ≥2 per-step UNSAT warnings, got {warns}. stderr:\n{stderr}");
    assert!(stderr.contains("UNSAT"), "stderr should mention UNSAT: {stderr}");
}

/// `--quiet` should suppress per-step UNSAT warnings entirely.
#[test]
fn cli_execute_quiet_suppresses_unsat_warning() {
    let path = write_tmp("execute_quiet",
        "schema main\n    src ∈ Stdin\n    counter ∈ Nat\n    counter < 0\n");
    let mut child = std::process::Command::new(bin())
        .args(["execute", path.to_str().unwrap(), "--quiet"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"ab").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("UNSAT"),
        "--quiet should suppress UNSAT warnings, got: {stderr}");
}

/// `--explain` should add the schema-body dump after each per-step
/// UNSAT warning. Verifies the pretty-printer is wired in (looks for
/// the readable form of `counter < 0`, not the AST debug form).
#[test]
fn cli_execute_explain_dumps_body() {
    let path = write_tmp("execute_explain",
        "schema main\n    src ∈ Stdin\n    counter ∈ Nat\n    counter < 0\n");
    let mut child = std::process::Command::new(bin())
        .args(["execute", path.to_str().unwrap(), "--explain"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"a").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("explain UNSAT step"),
        "missing explain header: {stderr}");
    assert!(stderr.contains("counter < 0"),
        "missing pretty-printed body item `counter < 0`: {stderr}");
    assert!(stderr.contains("counter ∈ Nat"),
        "missing pretty-printed membership: {stderr}");
}

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
#[test]
fn cli_execute_main_coordinator_halt() {
    let src = "schema main\n    src ∈ Stdin\n    dst ∈ Stdout\n    \
               next_main ∈ String\n    \
               dst.out = \"H\"\n    \
               next_main = \"halt\"\n";
    let path = write_tmp("mc_halt", src);
    let mut child = std::process::Command::new(bin())
        .args(["execute", path.to_str().unwrap()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        // Need at least one byte so StdinPlugin's first before_step
        // doesn't immediately halt on EOF.
        stdin.write_all(b"a").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // First step writes "H" then halts on next_main = "halt".
    assert_eq!(stdout, "H",
        "expected exactly one 'H' before halt, got {stdout:?}; stderr: {}",
        String::from_utf8_lossy(&out.stderr));
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
#[test]
fn cli_execute_initial_state_seeds_given() {
    // Program: dst.out = (world.score = 65 ? "A" : "?") essentially.
    // We use a constraint that maps an int-given to a char-output via
    // a chain of equalities.
    let src = "schema main\n    src ∈ Stdin\n    dst ∈ Stdout\n    \
               next_main ∈ String\n    score ∈ Int\n    \
               score = 65 ⇒ dst.out = \"A\"\n    \
               score = 66 ⇒ dst.out = \"B\"\n    \
               next_main = \"halt\"\n";
    let path = write_tmp("initial_state_program", src);

    let json_path = std::env::temp_dir().join(
        format!("evident-initial-state-{}.json", std::process::id()));
    std::fs::write(&json_path, r#"{"score": 66}"#).unwrap();

    let mut child = std::process::Command::new(bin())
        .args(["execute", path.to_str().unwrap(),
               "--initial-state", json_path.to_str().unwrap()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"x").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout, "B",
        "expected JSON's score=66 to drive dst.out='B', got {stdout:?}; stderr: {}",
        String::from_utf8_lossy(&out.stderr));

    let _ = std::fs::remove_file(&json_path);
}

/// Real program-swap: scene_a writes "A" + sets next_main to scene_b's
/// path; scene_b writes "B" + halts. Verifies the executor loads
/// scene_b mid-run and continues stepping with the new program.
#[test]
fn cli_execute_main_coordinator_swap_between_programs() {
    // Both files share the same temp dir so scene_a's relative
    // `next_main = "scene_b.ev"` resolves correctly via the
    // `resolve_swap_path` "join with current's parent" rule.
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let scene_a = dir.join(format!("evident-mc-swap-a-{pid}.ev"));
    let scene_b = dir.join(format!("evident-mc-swap-b-{pid}.ev"));
    let scene_b_name = scene_b.file_name().unwrap().to_str().unwrap();

    let a_src = format!("schema main\n    src ∈ Stdin\n    dst ∈ Stdout\n    \
                         next_main ∈ String\n    \
                         dst.out = \"A\"\n    \
                         next_main = \"{scene_b_name}\"\n");
    let b_src = "schema main\n    src ∈ Stdin\n    dst ∈ Stdout\n    \
                 next_main ∈ String\n    \
                 dst.out = \"B\"\n    \
                 next_main = \"halt\"\n";

    std::fs::write(&scene_a, a_src).unwrap();
    std::fs::write(&scene_b, b_src).unwrap();

    let mut child = std::process::Command::new(bin())
        .args(["execute", scene_a.to_str().unwrap()])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn().unwrap();
    {
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"ab").unwrap();
    }
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout, "AB",
        "expected swap A → B, got {stdout:?}; stderr: {}",
        String::from_utf8_lossy(&out.stderr));

    // Cleanup
    let _ = std::fs::remove_file(&scene_a);
    let _ = std::fs::remove_file(&scene_b);
}


// ── Stage 2: `evident dump-ast` ─────────────────────────────────

// ── Stage 3: `evident infer-types` ──────────────────────────────

#[test]
fn cli_infer_types_string_assignment() {
    let path = write_tmp("infer_str", "claim t\n    msg = \"hello\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin())
        .current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()])
        .output().unwrap();
    assert!(out.status.success(),
        "exit code {:?}; stderr: {}", out.status.code(),
        String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("`msg` ∈ String"),
        "expected msg inferred as String; got: {s}");
    assert!(s.contains("in claim `t`"),
        "expected claim name `t` in output; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_int_and_bool() {
    let path_int = write_tmp("infer_int", "claim n_test\n    n = 42\n");
    let path_bool = write_tmp("infer_bool", "claim flag_test\n    flag = true\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();

    let out_int = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path_int.to_str().unwrap()]).output().unwrap();
    assert!(out_int.status.success());
    let s_int = String::from_utf8_lossy(&out_int.stdout);
    assert!(s_int.contains("`n` ∈ Int"), "got: {s_int}");

    let out_bool = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path_bool.to_str().unwrap()]).output().unwrap();
    assert!(out_bool.status.success());
    let s_bool = String::from_utf8_lossy(&out_bool.stdout);
    assert!(s_bool.contains("`flag` ∈ Bool"), "got: {s_bool}");

    let _ = std::fs::remove_file(&path_int);
    let _ = std::fs::remove_file(&path_bool);
}

#[test]
fn cli_infer_types_no_match_exits_3() {
    let path = write_tmp("infer_noop",
        "claim t\n    a > b\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    // v0.1 pass only matches single-body-item programs; multi-body
    // programs match no rule. Exit code 3 documents this.
    assert_eq!(out.status.code(), Some(3),
        "expected exit 3 (no match); got {:?}; stderr: {}",
        out.status.code(), String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_no_args_prints_usage() {
    let out = Command::new(bin()).args(["infer-types"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("infer-types"),
        "stderr should mention infer-types; got: {stderr}");
}

// ── Stage 4: CLI tests for new rules ────────────────────────────

#[test]
fn cli_infer_types_extract_membership() {
    // `claim t : x ∈ Int` should fire extract_first_membership.
    let path = write_tmp("infer_extract", "claim t\n    x ∈ Int\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(),
        "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("extract_first_membership"),
        "expected extract_first_membership rule to fire; got: {s}");
    assert!(s.contains("`x` ∈ Int"),
        "expected `x ∈ Int` to be extracted; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_chained_membership_desugars_to_2body() {
    // `x ∈ Int = 5` desugars (in the parser) to two body items:
    //   Membership(x, Int, None)
    //   Constraint(x = 5)
    // So the membership_plus_assignment rule should fire alongside
    // extract_first_membership.
    let path = write_tmp("infer_chain",
        "claim t\n    x ∈ Int = 5\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "stderr: {}",
            String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("extract_first_membership"),
        "extract rule should fire for the desugared Membership; got: {s}");
    assert!(s.contains("infer_int_from_membership_plus_assignment"),
        "membership+assignment Int rule should fire on 2-body desugar; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_membership_with_string_assignment() {
    let path = write_tmp("infer_mem_str",
        "claim greeting\n    msg ∈ String\n    msg = \"hello\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    // Both the extract rule and the membership+assignment String rule fire.
    assert!(s.contains("extract_first_membership"));
    assert!(s.contains("infer_string_from_membership_plus_assignment"));
    // Neither single-assignment rule should fire (program has 2 body items).
    assert!(!s.contains("infer_string_from_single_assignment"),
        "single-assignment rule must NOT fire for 2-body program; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_dispatch_order_extract_before_inferred() {
    // Output ordering: more-specific rules (extract) come first.
    let path = write_tmp("infer_order",
        "claim t\n    msg ∈ String\n    msg = \"hi\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    let extract_pos = s.find("extract_first_membership").unwrap_or(usize::MAX);
    let infer_pos   = s.find("infer_string_from_membership_plus_assignment").unwrap_or(usize::MAX);
    assert!(extract_pos < infer_pos,
        "extract rule should appear before inference rules; got: {s}");
    let _ = std::fs::remove_file(&path);
}

// ── Stage 6: infer-types now dispatches iter_types rules too ───

#[test]
fn cli_infer_types_iter_finds_in_long_body() {
    // 5-body program — beyond what literal_types.ev's 1-body /
    // 2-body rules can match. The iter_types rules find everything
    // via existential search.
    let path = write_tmp("infer_long",
        "claim t\n    a = \"first\"\n    b = 1\n    c = false\n    \
         score ∈ Nat\n    score = 100\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(), "stderr: {}",
            String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    // All four iter rules should fire on this program.
    for needle in [
        "has_membership_of_var",   "score` ∈ Nat",
        "has_string_assignment",   "a` ∈ String",
        "has_int_assignment",      "b` ∈ Int",
        "has_bool_assignment",     "c` ∈ Bool",
    ] {
        assert!(s.contains(needle),
            "expected {needle:?} in iter-rules output; got:\n{s}");
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_iter_complements_literal_rules() {
    // 1-body program — both literal_types and iter_types should
    // fire (literal_types' single_assignment rule + iter_types'
    // has_string_assignment).
    let path = write_tmp("infer_complement",
        "claim t\n    msg = \"hello\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("infer_string_from_single_assignment"),
        "literal_types rule should fire");
    assert!(s.contains("has_string_assignment"),
        "iter_types rule should also fire");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_iter_unsat_for_empty_program() {
    // Body has no items; nothing iterates; nothing matches.
    // Still exits 3 (no rule matched), not 0 / 1.
    let path = write_tmp("infer_empty", "claim t\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(3),
        "expected exit 3 (no match); got {:?}; stderr: {}",
        out.status.code(), String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_file(&path);
}

// ── Stage 6 backfill ────────────────────────────────────────────

#[test]
fn cli_infer_types_label_format_for_iter_rule() {
    // The "found via iteration" prefix is part of the contract.
    let path = write_tmp("infer_label_iter",
        "claim t\n    a = \"x\"\n    b = 1\n    c = false\n    d ∈ Int\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("found via iteration"),
        "expected `found via iteration` label; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_label_format_for_extract_rule() {
    let path = write_tmp("infer_label_extract",
        "claim t\n    x ∈ Int\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("extracted from declaration"),
        "expected `extracted from declaration` label; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_label_format_for_membership_plus_assignment() {
    let path = write_tmp("infer_label_mem",
        "claim t\n    msg ∈ String\n    msg = \"hi\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("inferred from declaration + assignment"),
        "expected `inferred from declaration + assignment` label; got: {s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_program_rules_run_before_iter_rules() {
    // Output order: PROGRAM_RULES first (extract/infer), then ITER_RULES.
    let path = write_tmp("infer_dispatch_order",
        "claim t\n    x ∈ Int\n    x = 5\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    let extract_pos = s.find("extract_first_membership").unwrap_or(usize::MAX);
    let iter_pos    = s.find("has_membership_of_var").unwrap_or(usize::MAX);
    assert!(extract_pos < iter_pos,
        "PROGRAM_RULES (extract_first_membership) should appear before \
         ITER_RULES (has_membership_of_var); got:\n{s}");
    let _ = std::fs::remove_file(&path);
}

// ── Stage 7: aggregated `Inferred types:` table ────────────────

#[test]
fn cli_infer_types_prints_aggregated_table() {
    let path = write_tmp("infer_aggregate",
        "claim t\n    a = \"hi\"\n    b = 1\n    c = false\n    \
         score ∈ Nat\n    score = 100\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8_lossy(&out.stdout);
    // The aggregated section appears after the per-rule lines.
    assert!(s.contains("Inferred types:"),
        "expected `Inferred types:` header; got:\n{s}");
    // One row per variable.
    for needle in ["a", "b", "c", "score"] {
        // Each var name should appear in the aggregated section
        // (separated from per-rule lines by the header).
        let agg_pos = s.find("Inferred types:").unwrap();
        let agg_section = &s[agg_pos..];
        assert!(agg_section.contains(needle),
            "expected var `{needle}` in aggregated section; got:\n{agg_section}");
    }
    // Aggregated section attributes types to rules via "(via ...)".
    assert!(s.matches("(via ").count() >= 4,
        "expected at least 4 `(via …)` annotations; got:\n{s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_aggregation_dedupes_by_var() {
    // For msg ∈ String / msg = "hi", FOUR rules fire (extract_first,
    // infer_string_from_membership_plus_assignment, and both iter
    // rules). The aggregation should show ONE `msg : String` row
    // with all four rules listed.
    let path = write_tmp("infer_dedupe",
        "claim t\n    msg ∈ String\n    msg = \"hi\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    let agg_pos = s.find("Inferred types:").unwrap();
    let agg_section = &s[agg_pos..];
    // msg appears EXACTLY ONCE in the aggregated section.
    let msg_count = agg_section.matches("msg").count();
    assert!(msg_count == 1,
        "expected exactly one `msg` row in aggregated section; got {msg_count}: \n{agg_section}");
    // The rule list should mention all four contributing rules.
    assert!(agg_section.contains("extract_first_membership"));
    assert!(agg_section.contains("has_membership_of_var"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_aggregation_omitted_for_no_match() {
    // No matches → no `Inferred types:` header.
    let path = write_tmp("infer_no_agg", "claim t\n    a > b\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(!s.contains("Inferred types:"),
        "no rules matched → no aggregated table; got:\n{s}");
    let _ = std::fs::remove_file(&path);
}

// ── Stage 8: multi-claim iteration ─────────────────────────────

#[test]
fn cli_infer_types_iterates_multiple_claims() {
    // Two-claim program. iter rules should fire for BOTH.
    let path = write_tmp("infer_multi",
        "claim alpha\n    msg = \"in_alpha\"\nclaim beta\n    score ∈ Nat\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(),
        "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    // Both claims should appear.
    assert!(s.contains("in claim `alpha`"),
        "expected `in claim alpha` in output; got:\n{s}");
    assert!(s.contains("in claim `beta`"),
        "expected `in claim beta` in output; got:\n{s}");
    // Both inferences should appear.
    assert!(s.contains("`msg` ∈ String"),
        "expected msg inferred; got:\n{s}");
    assert!(s.contains("`score` ∈ Nat"),
        "expected score inferred; got:\n{s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_aggregator_groups_by_claim() {
    // Multi-claim → table grouped under `in claim NAME:` headers.
    let path = write_tmp("infer_group",
        "claim a\n    x = 1\nclaim b\n    y = \"hi\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    let agg_pos = s.find("Inferred types:").unwrap();
    let agg = &s[agg_pos..];
    // Both claim group headers should be present in the aggregated section.
    assert!(agg.contains("in claim `a`:"),
        "missing `in claim a:` group; got:\n{agg}");
    assert!(agg.contains("in claim `b`:"),
        "missing `in claim b:` group; got:\n{agg}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_three_claim_program() {
    let path = write_tmp("infer_three",
        "claim a\n    x = 1\n\nclaim b\n    y = \"hi\"\n\nclaim c\n    z = true\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    for needle in ["`x` ∈ Int", "`y` ∈ String", "`z` ∈ Bool"] {
        assert!(s.contains(needle),
            "missing inference {needle:?} in:\n{s}");
    }
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_aggregator_surfaces_ambiguity() {
    // `score ∈ Nat` (extract → Nat) + `score = 100` (literal → Int).
    // Stage 7's *ambiguous* path actually fires because Nat and Int
    // are different type strings.
    let path = write_tmp("infer_ambig",
        "claim t\n    score ∈ Nat\n    score = 100\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("*ambiguous*"),
        "expected ambiguity flag for score; got:\n{s}");
    let _ = std::fs::remove_file(&path);
}

// ── Stage 9: propagation rule reachable via CLI ────────────────

#[test]
fn cli_infer_types_propagation_via_eq_chain() {
    let path = write_tmp("infer_propagate",
        "claim t\n    x = y\n    y = \"hello\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(),
        "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("propagate_string"),
        "expected propagate_string rule to fire; got:\n{s}");
    assert!(s.contains("`x` ∈ String"),
        "expected x ∈ String inference; got:\n{s}");
    assert!(s.contains("propagated through `=`"),
        "expected propagation label; got:\n{s}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_propagation_in_aggregated_table() {
    let path = write_tmp("infer_propagate_table",
        "claim t\n    a = b\n    b = c\n    c = 42\n");
    // Two-step propagation: a=b, b=c, c=42. The single-step
    // propagation rule will catch (b, c, 42) → b ∈ Int and
    // (a, b, ?) but b's literal isn't in the body — c is. So
    // the rule fires for (b ← c via int).
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    let agg_pos = s.find("Inferred types:").unwrap_or(0);
    let agg = &s[agg_pos..];
    // At minimum, c ∈ Int (from has_int_assignment) and b ∈ Int
    // (from propagate_int via b = c, c = 42).
    assert!(agg.contains("`b`") || agg.contains("b "),
        "expected b in aggregated table; got:\n{agg}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_propagation_unsat_for_one_body_item() {
    // x = "literal" — single body item; propagation needs TWO.
    // The literal-types rules will still fire for x.
    let path = write_tmp("infer_no_propagate",
        "claim t\n    x = \"hi\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    // Propagation rules should NOT fire (no second body item).
    assert!(!s.contains("propagate_string"),
        "propagate_string should NOT fire on a 1-body program; got:\n{s}");
    // Other rules still do.
    assert!(s.contains("infer_string_from_single_assignment"),
        "single-assignment rule should still fire; got:\n{s}");
    let _ = std::fs::remove_file(&path);
}

// ── Stage 10: --strict + consistency checks ────────────────────

#[test]
fn cli_infer_types_consistency_warns_on_string_int_conflict() {
    // x ∈ String; x = 5 — real bug. Default mode warns to stderr
    // but exits 0 (compatibility).
    let path = write_tmp("infer_conflict",
        "claim t\n    x ∈ String\n    x = 5\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(0),
        "default mode exits 0 even with conflicts; got {:?}", out.status.code());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("consistency"),
        "expected consistency diagnostic on stderr; got: {stderr}");
    assert!(stderr.contains("conflict_string_decl_with_int_assignment"),
        "expected named conflict rule; got: {stderr}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_strict_exits_4_on_conflict() {
    let path = write_tmp("infer_strict_conflict",
        "claim t\n    x ∈ String\n    x = 5\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", "--strict", path.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(4),
        "strict mode exits 4 on conflict; got {:?}; stderr: {}",
        out.status.code(), String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_strict_exits_4_on_ambiguity_only() {
    // No bugs but ambiguity (Nat vs Int) — strict still exits 4.
    let path = write_tmp("infer_strict_ambig",
        "claim t\n    score ∈ Nat\n    score = 100\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", "--strict", path.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(4),
        "strict mode exits 4 on ambiguity; got {:?}; stderr: {}",
        out.status.code(), String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("ambiguous"),
        "expected ambiguity diagnostic; got: {stderr}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_strict_exits_0_when_clean() {
    // No conflicts, no ambiguity — even strict mode is happy.
    let path = write_tmp("infer_strict_clean",
        "claim t\n    msg = \"hello\"\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", "--strict", path.to_str().unwrap()]).output().unwrap();
    assert!(out.status.success(),
        "strict mode should exit 0 when clean; got {:?}; stderr: {}",
        out.status.code(), String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_consistency_catches_bool_int_conflict() {
    // flag ∈ Bool; flag = 5
    let path = write_tmp("infer_bool_int",
        "claim t\n    flag ∈ Bool\n    flag = 5\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("conflict_bool_decl_with_int_assignment"),
        "expected bool/int conflict; got: {stderr}");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn cli_infer_types_consistency_silent_when_no_conflict() {
    let path = write_tmp("infer_no_conflict",
        "claim t\n    score ∈ Int\n    score = 100\n");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(manifest).parent().unwrap();
    let out = Command::new(bin()).current_dir(repo_root)
        .args(["infer-types", path.to_str().unwrap()]).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("conflict"),
        "no conflicts → no consistency warnings; got stderr: {stderr}");
    let _ = std::fs::remove_file(&path);
}

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
