//! Black-box tests for the one PYTHONPATH-style stdlib resolver
//! (`evident_runtime::stdlib_path`). The pure-core search logic is unit
//! tested next to the code; this exercises the *public* `stdlib_dir()`
//! through real env + filesystem state:
//!
//!   * default resolution finds the dev-tree stdlib (no env, any CWD),
//!   * an `EVIDENT_STDLIB` override loads stdlib from a non-default
//!     location (acceptance #4),
//!   * a missing/wrong override produces a clear error naming the path and
//!     the env var (acceptance #5).
//!
//! All env mutation lives in ONE test so it runs single-threaded — env
//! vars are process-global, so splitting these across `#[test]` fns would
//! race under cargo's parallel runner.

use evident_runtime::stdlib_path::{stdlib_dir, ENV_PRIMARY};

#[test]
fn resolver_default_override_and_missing() {
    // Snapshot + clear any ambient override so "default" really is default.
    let saved_primary = std::env::var(ENV_PRIMARY).ok();
    let saved_alias = std::env::var("EVIDENT_STDLIB_DIR").ok();
    std::env::remove_var(ENV_PRIMARY);
    std::env::remove_var("EVIDENT_STDLIB_DIR");

    // 1. Default resolution: finds the dev tree with zero config, and the
    //    directory it returns really holds the stdlib files callers join.
    let dir = stdlib_dir().expect("default resolution should find the dev stdlib");
    assert!(dir.join("runtime.ev").is_file(), "resolved dir has runtime.ev: {dir:?}");
    assert!(dir.join("ast.ev").is_file(), "resolved dir has ast.ev: {dir:?}");

    // 2. Override to a non-default location: a throwaway dir with just the
    //    marker file. Proves the resolver honors EVIDENT_STDLIB pointing
    //    anywhere, not only at the repo tree.
    let tmp = std::env::temp_dir().join(format!("evident-stdlib-override-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("runtime.ev"), b"-- marker\n").unwrap();

    std::env::set_var(ENV_PRIMARY, &tmp);
    let got = stdlib_dir().expect("valid override should resolve");
    assert_eq!(got, tmp, "override path wins over the default search");

    // 3. Missing/wrong override: a clear, actionable error that names both
    //    the offending path and the override env var (no bare "not found",
    //    no silent fallback to the dev tree).
    let bogus = "/nonexistent/evident-stdlib-resolver-test";
    std::env::set_var(ENV_PRIMARY, bogus);
    let err = stdlib_dir().expect_err("a wrong override must hard-error");
    assert!(err.contains(bogus), "error names the bad path: {err}");
    assert!(err.contains(ENV_PRIMARY), "error names the override env var: {err}");

    // Restore ambient env so we don't leak state to other test binaries
    // sharing this process (defensive — each integration test is its own
    // process, but cheap insurance).
    let _ = std::fs::remove_dir_all(&tmp);
    match saved_primary {
        Some(v) => std::env::set_var(ENV_PRIMARY, v),
        None => std::env::remove_var(ENV_PRIMARY),
    }
    match saved_alias {
        Some(v) => std::env::set_var("EVIDENT_STDLIB_DIR", v),
        None => std::env::remove_var("EVIDENT_STDLIB_DIR"),
    }
}
