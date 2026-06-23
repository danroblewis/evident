use std::io::Write;
use std::process::Command;

fn bin() -> &'static str {

    env!("CARGO_BIN_EXE_evident")
}

fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("evident-runtime-test-{}-{}.ev", std::process::id(), name));
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(body.as_bytes()).unwrap();
    path
}

// ───────── #290: multiple fsms + multiple claims, LAST-DEFINED is the entry ─────────

const FSM_COUNTER: &str = "    count ∈ Int\n    is_first_tick ⇒ count = 0\n    ¬is_first_tick ⇒ Δcount = 1\n    last_results ∈ Seq(Result)\n    effects ∈ Seq(Effect) = ⟨⟩\n";

/// Run `evident export FILE --out PREFIX [--entry NAME]`; return the success line
/// (e.g. `wrote …  (fsm: b)`), or the joined stdout+stderr on failure.
fn export_what(name: &str, body: &str, entry: Option<&str>) -> String {
    let path = write_tmp(name, body);
    let mut prefix = std::env::temp_dir();
    prefix.push(format!("evident-export-{}-{}", std::process::id(), name));
    let mut args = vec!["export".to_string(), path.to_str().unwrap().to_string(),
                        "--out".to_string(), prefix.to_str().unwrap().to_string()];
    if let Some(e) = entry { args.push("--entry".into()); args.push(e.into()); }
    // `export` loads `stdlib/runtime.ev` relative to cwd; `cargo test` runs in `runtime/`, so
    // point cwd at the repo root (its parent) where that file lives.
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let out = Command::new(bin()).args(&args).current_dir(repo_root).output().unwrap();
    let _ = std::fs::remove_file(&path);
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

#[test]
fn export_picks_last_defined_entry_across_types() {
    // claim helper; fsm main  → main (fsm)
    let s = export_what("c1", &format!("claim helper(x ∈ Int)\n    x > 0\n\nfsm main\n{FSM_COUNTER}"), None);
    assert!(s.contains("(fsm: main)"), "claim-then-fsm should render the fsm: {s}");

    // fsm main; claim test  → test (claim)
    let s = export_what("c2", &format!("fsm main\n{FSM_COUNTER}\nclaim test(x ∈ Int)\n    0 < x < 10\n"), None);
    assert!(s.contains("(claim: test)"), "fsm-then-claim should render the claim: {s}");

    // fsm a; fsm b  → b
    let s = export_what("c3", &format!("fsm a\n{FSM_COUNTER}\nfsm b\n{FSM_COUNTER}"), None);
    assert!(s.contains("(fsm: b)"), "two fsms should render the last: {s}");

    // claim p; claim q  → q
    let s = export_what("c4", "claim p(x ∈ Int)\n    0 < x < 5\n\nclaim q(y ∈ Int)\n    10 < y < 20\n", None);
    assert!(s.contains("(claim: q)"), "two claims should render the last: {s}");
}

#[test]
fn export_single_entry_unchanged() {
    let s = export_what("c5", &format!("fsm main\n{FSM_COUNTER}"), None);
    assert!(s.contains("(fsm: main)"), "single fsm: {s}");
    // single claim alongside a helper `type` still resolves to the claim
    let s = export_what("c6", "type Edge(from, to ∈ Int)\n\nclaim solo(x ∈ Int)\n    0 < x < 5\n", None);
    assert!(s.contains("(claim: solo)"), "single claim + helper type: {s}");
}

#[test]
fn export_entry_override_wins() {
    // pick the earlier fsm `a` over the default last `b`
    let s = export_what("c3o", &format!("fsm a\n{FSM_COUNTER}\nfsm b\n{FSM_COUNTER}"), Some("a"));
    assert!(s.contains("(fsm: a)"), "--entry a should win over last-defined b: {s}");
    // pick the fsm over the later-declared claim
    let s = export_what("c2o", &format!("fsm main\n{FSM_COUNTER}\nclaim test(x ∈ Int)\n    0 < x < 10\n"), Some("main"));
    assert!(s.contains("(fsm: main)"), "--entry main should override the last claim: {s}");
    // a bogus entry is reported, not silently rendered
    let s = export_what("c2x", &format!("fsm main\n{FSM_COUNTER}\nclaim test(x ∈ Int)\n    0 < x < 10\n"), Some("nope"));
    assert!(s.contains("not found"), "bogus --entry should error: {s}");
}

#[test]
fn cli_test_runs_sat_unsat_claims() {
    let path = write_tmp("testfile",
        "claim sat_ok\n    n ∈ Nat\n    n > 0\n\
         claim unsat_bad\n    n ∈ Nat\n    n > 10\n    n < 3\n");

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
