//! Workspace file discovery + index construction with open-buffer overlay.
//!
//! The engine is name-scoped over the whole `.ev` tree (the `_x` carry dual is
//! never missed only if every file is scanned — see tools/README.md trap #1),
//! so we index ALL `.ev` files under the workspace root, then overlay the
//! current text of any open buffers so edits are reflected before save.

use evident_tools::index::{build_index, Index};
use std::path::{Path, PathBuf};

/// Locate the Evident repo root from a starting dir by walking upward looking
/// for the `CLAUDE.md` + `compiler2/` marker. Falls back to the start dir.
pub fn find_root(start: &Path) -> PathBuf {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join("CLAUDE.md").exists() && dir.join("compiler2").exists() {
            return dir;
        }
        if !dir.pop() {
            return start.to_path_buf();
        }
    }
}

/// All `.ev` file paths under the standard source trees of `root`.
pub fn collect_ev_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in ["compiler2", "stdlib", "compiler", "tests"] {
        let p = root.join(d);
        if p.exists() {
            walk(&p, &mut out);
        }
    }
    // also any top-level .ev files
    if let Ok(rd) = std::fs::read_dir(root) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "ev").unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().map(|e| e == "ev").unwrap_or(false) {
            out.push(p);
        }
    }
}

/// Load every `.ev` file under `root`, overlaying `overrides` (path → text)
/// for open buffers (and adding override-only paths not on disk).
pub fn load_files_with_overlay(
    root: &Path,
    overrides: &[(PathBuf, String)],
) -> Vec<(PathBuf, String)> {
    let paths = collect_ev_files(root);
    let mut files: Vec<(PathBuf, String)> = paths
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok().map(|s| (p.clone(), s)))
        .collect();
    for (path, text) in overrides {
        if let Some(slot) = files.iter_mut().find(|(p, _)| p == path) {
            slot.1 = text.clone();
        } else {
            files.push((path.clone(), text.clone()));
        }
    }
    files
}

/// Build a fresh workspace index from disk + overlays.
pub fn build_workspace_index(root: &Path, overrides: &[(PathBuf, String)]) -> Index {
    let files = load_files_with_overlay(root, overrides);
    build_index(&files)
}
