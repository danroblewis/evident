//! One PYTHONPATH-style resolver for Evident's `stdlib/` directory.
//!
//! Every place that loads a stdlib file — `effect-run`'s `runtime.ev`, the
//! self-hosted-pass drivers' `ast.ev` + `passes/*.ev`, the portable
//! `Evident*` impls — goes through [`stdlib_dir`]. It returns the stdlib
//! *directory*; callers join `runtime.ev` / `ast.ev` / `passes/<x>.ev`
//! onto it.
//!
//! This replaces three inconsistent schemes that preceded it: CWD-relative
//! consts (`"stdlib/runtime.ev"`), a portable-only `EVIDENT_STDLIB_DIR`
//! env check duplicated per file, and hard-coded `../stdlib` in tests. A
//! relocated/installed binary now works (it finds an installed stdlib), and
//! the dev tree keeps working with zero config — without baking the files
//! into the binary (embedding would force a rebuild on every `.ev` edit,
//! killing the dogfooding workflow).
//!
//! ## Resolution order
//!
//! 1. **`EVIDENT_STDLIB`** (or the back-compat alias `EVIDENT_STDLIB_DIR`)
//!    — explicit override, the PYTHONPATH analog. Authoritative: if it's
//!    set but doesn't point at a stdlib dir, that's a hard error (a typo'd
//!    override fails loudly instead of silently falling back).
//! 2. **Install locations** relative to the executable:
//!    `<exe_dir>/../share/evident/stdlib`, `<exe_dir>/stdlib`, and the XDG
//!    data dir (`$XDG_DATA_HOME/evident/stdlib` →
//!    `~/.local/share/evident/stdlib`).
//! 3. **Dev-tree fallback**: `$CARGO_MANIFEST_DIR/../stdlib` (a compile-time
//!    constant baked into the binary — covers `cargo test` and the dev
//!    binary regardless of CWD), then a few exe-relative guesses through
//!    the `target/{debug,release}[/deps]` layout, then `./stdlib`
//!    (CWD-relative, the historical behavior).
//! 4. **Clear error** if none match: lists every path checked and names the
//!    `EVIDENT_STDLIB` override.

use std::path::{Path, PathBuf};

/// File whose presence marks a directory as Evident's stdlib root. Every
/// stdlib dir ships it (it's the most fundamental stdlib file), so it's a
/// reliable "this is the stdlib" signal — and a misdirected override that
/// lacks it is caught immediately rather than failing later on a missing
/// `runtime.ev`.
const STDLIB_MARKER: &str = "runtime.ev";

/// Canonical override env var (the PYTHONPATH analog).
pub const ENV_PRIMARY: &str = "EVIDENT_STDLIB";
/// Back-compat alias — the name the per-file portable checks used before
/// this resolver unified them. Honored so existing `EVIDENT_STDLIB_DIR=…`
/// invocations keep working.
pub const ENV_ALIAS: &str = "EVIDENT_STDLIB_DIR";

/// Resolve the stdlib directory. See the module docs for the search order.
///
/// Returns the directory (not a file); callers join `runtime.ev` /
/// `ast.ev` / `passes/<x>.ev` onto it.
pub fn stdlib_dir() -> Result<PathBuf, String> {
    resolve_candidates(env_override().as_deref(), &candidate_dirs())
}

/// The explicit override value, from `EVIDENT_STDLIB` or its alias. Empty /
/// whitespace-only values are treated as unset.
fn env_override() -> Option<String> {
    for var in [ENV_PRIMARY, ENV_ALIAS] {
        if let Ok(val) = std::env::var(var) {
            if !val.trim().is_empty() {
                return Some(val);
            }
        }
    }
    None
}

/// A directory looks like the stdlib if it contains the marker file.
fn is_stdlib_dir(p: &Path) -> bool {
    p.join(STDLIB_MARKER).is_file()
}

/// Build the ordered, non-override candidate list (install locations →
/// dev-tree fallbacks). The override is handled separately in
/// [`resolve_candidates`] because it's authoritative.
fn candidate_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();

    // 2. Install locations relative to the executable.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // FHS-style install: <prefix>/bin/evident + <prefix>/share/...
            v.push(exe_dir.join("../share/evident/stdlib"));
            // Self-contained / portable layout: stdlib next to the binary.
            v.push(exe_dir.join("stdlib"));
        }
    }
    // 2b. XDG / standard per-user data dir.
    if let Some(p) = xdg_data_stdlib() {
        v.push(p);
    }

    // 3. Dev-tree fallback: compile-time manifest dir. Absolute and
    //    CWD-independent — this is what makes `cargo test` and the dev
    //    binary resolve with zero config.
    v.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("stdlib"));
    // 3b. Exe-relative dev guesses, walking up out of
    //     target/{debug,release}[/deps] to the repo root.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            for rel in ["../stdlib", "../../stdlib", "../../../stdlib", "../../../../stdlib"] {
                v.push(exe_dir.join(rel));
            }
        }
    }
    // 3c. CWD-relative (the historical "stdlib/..." behavior).
    v.push(PathBuf::from("stdlib"));

    v
}

/// `$XDG_DATA_HOME/evident/stdlib`, falling back to
/// `$HOME/.local/share/evident/stdlib`.
fn xdg_data_stdlib() -> Option<PathBuf> {
    if let Ok(x) = std::env::var("XDG_DATA_HOME") {
        if !x.trim().is_empty() {
            return Some(PathBuf::from(x).join("evident").join("stdlib"));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Some(PathBuf::from(home).join(".local/share/evident/stdlib"));
        }
    }
    None
}

/// Pure resolution core: pick the override if valid, else the first
/// candidate that looks like a stdlib dir, else a clear error. Kept free of
/// env / exe lookups so it's unit-testable without process-global mutation.
fn resolve_candidates(override_dir: Option<&str>, candidates: &[PathBuf]) -> Result<PathBuf, String> {
    // 1. Explicit override — authoritative. Set-but-wrong is a hard error,
    //    NOT a fall-through, so a typo'd override surfaces immediately.
    if let Some(dir) = override_dir {
        let p = PathBuf::from(dir);
        if is_stdlib_dir(&p) {
            return Ok(p);
        }
        return Err(format!(
            "${ENV_PRIMARY}={dir} does not look like Evident's stdlib \
             (no `{STDLIB_MARKER}` at `{}`).\n\
             Point ${ENV_PRIMARY} at the directory that holds `runtime.ev` \
             and `ast.ev`.",
            p.join(STDLIB_MARKER).display(),
        ));
    }

    for p in candidates {
        if is_stdlib_dir(p) {
            return Ok(p.clone());
        }
    }

    let checked = candidates
        .iter()
        .map(|p| format!("    {}", p.display()))
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "could not locate Evident's stdlib (no directory with `{STDLIB_MARKER}` found).\n\
         Set ${ENV_PRIMARY}=<dir> to the directory containing `runtime.ev` and `ast.ev`.\n\
         Checked:\n{checked}",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory that is the real stdlib (has the marker), discovered via
    /// the compile-time manifest path. Used to build "valid" candidates
    /// without depending on CWD.
    fn real_stdlib() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("stdlib")
    }

    #[test]
    fn real_stdlib_has_marker() {
        // Sanity: the dev tree the other tests rely on actually exists.
        assert!(is_stdlib_dir(&real_stdlib()), "dev stdlib should have {STDLIB_MARKER}");
    }

    #[test]
    fn default_resolution_finds_dev_tree() {
        // No override; the live candidate list must locate the dev stdlib
        // (via CARGO_MANIFEST_DIR) regardless of CWD.
        let got = resolve_candidates(None, &candidate_dirs()).expect("should resolve dev stdlib");
        assert!(is_stdlib_dir(&got));
    }

    #[test]
    fn valid_override_wins() {
        let real = real_stdlib();
        let got = resolve_candidates(Some(real.to_str().unwrap()), &[])
            .expect("valid override should resolve");
        assert_eq!(got, real);
    }

    #[test]
    fn invalid_override_is_a_clear_error() {
        // A set-but-wrong override hard-errors (no silent fallback), and the
        // message names both the bad path and the env var.
        let err = resolve_candidates(Some("/nonexistent/evident-xyz"), &[real_stdlib()])
            .expect_err("invalid override must error");
        assert!(err.contains("/nonexistent/evident-xyz"), "names the bad path: {err}");
        assert!(err.contains(ENV_PRIMARY), "names the override env var: {err}");
    }

    #[test]
    fn missing_stdlib_lists_searched_paths_and_env_var() {
        // No override, no candidate is a real stdlib → the error lists every
        // path checked and names the override env var.
        let bogus = vec![
            PathBuf::from("/no/such/a"),
            PathBuf::from("/no/such/b"),
            PathBuf::from("/no/such/c"),
        ];
        let err = resolve_candidates(None, &bogus).expect_err("absent stdlib must error");
        for p in &bogus {
            assert!(err.contains(&p.display().to_string()), "lists {}: {err}", p.display());
        }
        assert!(err.contains(ENV_PRIMARY), "names the override env var: {err}");
    }
}
