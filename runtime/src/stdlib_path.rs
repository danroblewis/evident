//! PYTHONPATH-style resolver for Evident's `stdlib/` directory.
//! Resolution order: `EVIDENT_STDLIB` env override → install paths → dev-tree (CARGO_MANIFEST_DIR) → `./stdlib`.

use std::path::{Path, PathBuf};

/// Presence of this file marks a directory as the stdlib root; misdirected overrides fail early.
const STDLIB_MARKER: &str = "runtime.ev";

/// Canonical override env var.
pub const ENV_PRIMARY: &str = "EVIDENT_STDLIB";
/// Back-compat alias; honored so existing `EVIDENT_STDLIB_DIR=…` invocations keep working.
pub const ENV_ALIAS: &str = "EVIDENT_STDLIB_DIR";

/// Resolve the stdlib directory (not a file); callers join filenames onto it.
pub fn stdlib_dir() -> Result<PathBuf, String> {
    resolve_candidates(env_override().as_deref(), &candidate_dirs())
}

/// Returns the override from `EVIDENT_STDLIB` / alias; empty/whitespace is treated as unset.
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

/// Build the ordered candidate list (install locations → dev-tree fallbacks).
fn candidate_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();

    // Install locations relative to the executable.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            v.push(exe_dir.join("../share/evident/stdlib")); // FHS
            v.push(exe_dir.join("stdlib")); // self-contained/portable
        }
    }
    if let Some(p) = xdg_data_stdlib() {
        v.push(p);
    }

    // Dev-tree: compile-time manifest dir (CWD-independent; covers `cargo test`).
    v.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("stdlib"));
    // Exe-relative guesses through target/{debug,release}[/deps].
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            for rel in ["../stdlib", "../../stdlib", "../../../stdlib", "../../../../stdlib"] {
                v.push(exe_dir.join(rel));
            }
        }
    }
    v.push(PathBuf::from("stdlib")); // CWD-relative historical fallback

    v
}

/// Returns `$XDG_DATA_HOME/evident/stdlib` or `$HOME/.local/share/evident/stdlib`.
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

/// Pure resolution core (no env/exe lookups — unit-testable).
/// Override is authoritative: set-but-wrong is a hard error, not a fall-through.
fn resolve_candidates(override_dir: Option<&str>, candidates: &[PathBuf]) -> Result<PathBuf, String> {
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
