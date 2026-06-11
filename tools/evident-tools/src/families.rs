//! "Families" analysis — productizes the two hand-run refactoring probes
//! recorded in the memories:
//!
//!   1. NUMBERED families: a set of decls `xN` (`read_acc0..5`, `int0..15`,
//!      `is_str0..7`) whose names share a common stem and differ only by a
//!      trailing integer → candidate `Seq`. We report the stem, the integer
//!      range, the count, and the files.
//!
//!   2. PREFIX families: ≥N distinct DECL names sharing a `<word>_` prefix
//!      (`parse_`, `field_`, `enum_`, …) → candidate record / namespace.
//!
//! We operate on DECLARATIONS (membership + header-slot + schema decls),
//! grouped by their *base* name (dual-stripped), so the dual never inflates
//! a count.

use crate::index::{Index, RefKind};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug)]
pub struct NumberedFamily {
    pub stem: String,
    pub indices: Vec<i64>,
    pub files: BTreeSet<String>,
}

#[derive(Debug)]
pub struct PrefixFamily {
    pub prefix: String,
    pub members: BTreeSet<String>,
    pub files: BTreeSet<String>,
}

/// Distinct decl names with the files they're declared in.
fn decl_names(idx: &Index) -> BTreeMap<String, BTreeSet<String>> {
    let mut m: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for o in &idx.occurrences {
        let is_var_decl = matches!(
            o.kind,
            RefKind::MemberDecl | RefKind::HeaderSlot
        );
        if is_var_decl {
            m.entry(o.name.clone())
                .or_default()
                .insert(o.file.display().to_string());
        }
    }
    m
}

/// Split a name into (stem, index) if it ends in digits with a non-empty,
/// non-digit-ending stem. `read_acc0` → ("read_acc", 0). `int15` → ("int", 15).
fn split_numbered(name: &str) -> Option<(String, i64)> {
    let bytes = name.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    if i == bytes.len() || i == 0 {
        return None;
    }
    let stem = &name[..i];
    let num: i64 = name[i..].parse().ok()?;
    // require the stem to not itself end in a digit-friendly junk; keep `_`
    // and letters. Stem of length 1 like "x" is fine (x0..xN).
    Some((stem.to_string(), num))
}

pub fn numbered(idx: &Index, min_count: usize) -> Vec<NumberedFamily> {
    let names = decl_names(idx);
    let mut groups: BTreeMap<String, (BTreeSet<i64>, BTreeSet<String>)> = BTreeMap::new();
    for (name, files) in &names {
        if let Some((stem, num)) = split_numbered(name) {
            let e = groups.entry(stem).or_default();
            e.0.insert(num);
            for f in files {
                e.1.insert(f.clone());
            }
        }
    }
    let mut out: Vec<NumberedFamily> = groups
        .into_iter()
        .filter(|(_, (idxs, _))| idxs.len() >= min_count)
        .map(|(stem, (idxs, files))| NumberedFamily {
            stem,
            indices: idxs.into_iter().collect(),
            files,
        })
        .collect();
    out.sort_by(|a, b| b.indices.len().cmp(&a.indices.len()));
    out
}

pub fn prefixes(idx: &Index, min_count: usize) -> Vec<PrefixFamily> {
    let names = decl_names(idx);
    // group by leading `<word>_` prefix (everything up to and including the
    // FIRST underscore that isn't a leading one).
    let mut groups: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    for (name, files) in &names {
        if let Some(us) = name.find('_') {
            if us == 0 {
                continue; // shouldn't happen (base is dual-stripped)
            }
            let prefix = &name[..us + 1]; // include the underscore
            // skip degenerate single-letter prefixes that are noise
            let e = groups.entry(prefix.to_string()).or_default();
            e.0.insert(name.clone());
            for f in files {
                e.1.insert(f.clone());
            }
        }
    }
    let mut out: Vec<PrefixFamily> = groups
        .into_iter()
        .filter(|(_, (members, _))| members.len() >= min_count)
        .map(|(prefix, (members, files))| PrefixFamily {
            prefix,
            members,
            files,
        })
        .collect();
    out.sort_by(|a, b| b.members.len().cmp(&a.members.len()));
    out
}

/// Cons-peel families: names like `after`, `next`, `tail`, `vft_rest`,
/// `vcount_rest`, or `restK`/`rest_K` chains — non-uniform list-walk clusters
/// flagged in the memory as "skip a bank" cases. We surface decl names that
/// look like list-peel residue (`*_rest`, `restN`, `rest_N`, plus the literal
/// peel words) so the user can eyeball them.
pub fn cons_peel(idx: &Index) -> Vec<(String, BTreeSet<String>)> {
    let names = decl_names(idx);
    let peel_words: BTreeSet<&str> =
        ["after", "next", "tail", "rest"].into_iter().collect();
    let mut out: Vec<(String, BTreeSet<String>)> = Vec::new();
    for (name, files) in &names {
        let hit = peel_words.contains(name.as_str())
            || name.ends_with("_rest")
            || name.starts_with("rest")
            || name.ends_with("_tail")
            || name.ends_with("_next")
            || name.ends_with("_after");
        if hit {
            out.push((name.clone(), files.clone()));
        }
    }
    out.sort();
    out
}

pub fn _files_unused(_: &[PathBuf]) {}
