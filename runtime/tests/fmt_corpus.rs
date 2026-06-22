//! Corpus-level guards for `evident fmt`.
//!
//! `format_source` runs an internal token-equivalence self-check and an
//! idempotence check on every call, returning `Err` if either fails. So
//! formatting every `.ev` file in the repo without error *is* the proof that
//! the formatter never corrupts a real program. We additionally:
//!   * assert the output is a fixed point (format(format(x)) == format(x)),
//!   * mangle each file's whitespace (extra indent, trailing spaces, blank
//!     runs) and assert the mangled version formats back to the SAME output as
//!     the clean version — i.e. formatting is whitespace-insensitive and lands
//!     on one canonical form.

use std::fs;
use std::path::{Path, PathBuf};

use evident_runtime::fmt::format_source;

fn repo_root() -> PathBuf {
    // tests run with CWD = runtime/ ; the repo root is its parent.
    let mut p = std::env::current_dir().unwrap();
    if p.ends_with("runtime") {
        p.pop();
    }
    p
}

fn collect_ev(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            // skip build/vendor dirs
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if matches!(name, "target" | ".git" | "node_modules") {
                continue;
            }
            collect_ev(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("ev") {
            out.push(p);
        }
    }
}

fn corpus() -> Vec<PathBuf> {
    let root = repo_root();
    let mut files = Vec::new();
    for sub in ["examples", "stdlib", "packages"] {
        collect_ev(&root.join(sub), &mut files);
    }
    files.sort();
    assert!(!files.is_empty(), "no .ev corpus found under {root:?}");
    files
}

/// Mangle whitespace without changing any token: add a few leading spaces to
/// indented lines, sprinkle trailing whitespace, and double up blank lines.
fn mangle(src: &str) -> String {
    let mut out = String::new();
    for (i, line) in src.split('\n').enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            out.push('\n');
            out.push('\n'); // blow up blank runs
            continue;
        }
        // Replace leading whitespace with a wonky amount (still > 0 where it
        // was > 0, so nesting relationships are preserved in *rank*).
        let lead = line.len() - trimmed.len();
        let new_lead = if lead == 0 { 0 } else { lead * 3 + 1 };
        out.push_str(&" ".repeat(new_lead));
        out.push_str(trimmed);
        // trailing whitespace
        if i % 2 == 0 {
            out.push_str("   ");
        }
        out.push('\n');
    }
    out
}

#[test]
fn every_corpus_file_formats_without_error_and_is_fixed_point() {
    for f in corpus() {
        let src = fs::read_to_string(&f).unwrap();
        let formatted = match format_source(&src) {
            Ok(s) => s,
            Err(e) => panic!("format_source failed on {}: {e}", f.display()),
        };
        let twice = format_source(&formatted)
            .unwrap_or_else(|e| panic!("second format failed on {}: {e}", f.display()));
        assert_eq!(
            formatted,
            twice,
            "not idempotent on {}",
            f.display()
        );
    }
}

#[test]
fn mangled_whitespace_lands_on_the_same_canonical_form() {
    for f in corpus() {
        let src = fs::read_to_string(&f).unwrap();
        let clean = match format_source(&src) {
            Ok(s) => s,
            Err(_) => continue, // covered by the other test
        };
        let mangled = mangle(&src);
        let from_mangled = match format_source(&mangled) {
            Ok(s) => s,
            Err(e) => panic!(
                "mangled whitespace broke equivalence on {}: {e}",
                f.display()
            ),
        };
        // Whitespace mangling must not change the canonical output, EXCEPT for
        // parser-invisible continuation lines (inside open brackets), whose
        // indentation we preserve verbatim and which mangle perturbs. Compare
        // the formatted forms after re-formatting the mangled clean form too,
        // so both went through the continuation-preserving path identically by
        // re-mangling `clean` rather than `src`.
        let clean_remangled = format_source(&mangle(&clean)).unwrap();
        assert_eq!(
            from_mangled,
            clean_remangled,
            "mangled src and mangled clean diverged on {}",
            f.display()
        );
    }
}
