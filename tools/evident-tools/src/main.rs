//! evt — Evident refactoring tooling CLI.
//!
//! Subcommands: index, defs, refs, rename, symbols, families, collisions.
//! Operates on the whole `.ev` tree (default: compiler2/ + stdlib/ + the
//! compiler/ + tests, see `collect_files`), token-accurately, respecting the
//! `_x` carry dual and identifier boundaries (no substring corruption).

use evident_tools::{families, index, rename};
use index::{build_index, DeclKind, Index, RefKind};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage();
        std::process::exit(2);
    }
    let cmd = args[1].as_str();
    let rest = &args[2..];

    let code = match cmd {
        "index" => cmd_index(rest),
        "defs" => cmd_defs(rest),
        "refs" => cmd_refs(rest),
        "rename" => cmd_rename(rest),
        "symbols" => cmd_symbols(rest),
        "families" => cmd_families(rest),
        "collisions" => cmd_collisions(rest),
        "-h" | "--help" | "help" => {
            usage();
            0
        }
        _ => {
            eprintln!("evt: unknown subcommand '{cmd}'");
            usage();
            2
        }
    };
    std::process::exit(code);
}

fn usage() {
    eprintln!(
        "evt — Evident refactoring tools

USAGE:
  evt index                       Build + print the symbol index (decls).
  evt defs   <name>               Where <name> is declared.
  evt refs   <name>               All references to <name> (incl. _dual).
  evt rename <old> <new> [opts]   Tree-wide token-accurate rename.
       --dry-run                  Show the diff, write nothing.
       --force                    Rename even if <new> already exists (MERGE).
  evt symbols [file.ev]           Symbol outline (one file, or whole tree).
  evt families [--min N]          Numbered (→Seq) + prefix (→record) families.
  evt collisions [entry] [claim]  declare-fun names of emitted driver_main
                                  (authoritative collision oracle).

GLOBAL:
  --root <dir>   repo root (default: discovered from cwd upward)
  --scope <dir>  limit tree scan to this subdir (repeatable; default: the
                 standard .ev trees compiler2/ stdlib/ compiler/ tests/)
"
    );
}

// ── tree discovery ─────────────────────────────────────────────────────

fn find_root() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        if dir.join("CLAUDE.md").exists() && dir.join("compiler2").exists() {
            return dir;
        }
        if !dir.pop() {
            return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        }
    }
}

struct Opts {
    root: PathBuf,
    scopes: Vec<PathBuf>,
    positional: Vec<String>,
    flags: BTreeSet<String>,
    min: Option<usize>,
}

fn parse_opts(args: &[String]) -> Opts {
    let root = find_root();
    let mut scopes = Vec::new();
    let mut positional = Vec::new();
    let mut flags = BTreeSet::new();
    let mut min = None;
    let mut root_override: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                if i + 1 < args.len() {
                    root_override = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--scope" => {
                if i + 1 < args.len() {
                    scopes.push(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--min" => {
                if i + 1 < args.len() {
                    min = args[i + 1].parse().ok();
                    i += 1;
                }
            }
            "--dry-run" => {
                flags.insert("dry-run".into());
            }
            "--force" => {
                flags.insert("force".into());
            }
            other => positional.push(other.to_string()),
        }
        i += 1;
    }
    let root = root_override.unwrap_or(root);
    if scopes.is_empty() {
        for d in ["compiler2", "stdlib", "compiler", "tests"] {
            let p = root.join(d);
            if p.exists() {
                scopes.push(p);
            }
        }
    } else {
        scopes = scopes
            .into_iter()
            .map(|s| if s.is_absolute() { s } else { root.join(s) })
            .collect();
    }
    Opts {
        root,
        scopes,
        positional,
        flags,
        min,
    }
}

fn collect_ev_files(scopes: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for s in scopes {
        walk(s, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.is_file() {
        if dir.extension().map(|e| e == "ev").unwrap_or(false) {
            out.push(dir.to_path_buf());
        }
        return;
    }
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

fn load_files(paths: &[PathBuf]) -> Vec<(PathBuf, String)> {
    paths
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok().map(|s| (p.clone(), s)))
        .collect()
}

fn rel<'a>(root: &Path, p: &'a Path) -> &'a Path {
    p.strip_prefix(root).unwrap_or(p)
}

// ── index ──────────────────────────────────────────────────────────────

fn build(opts: &Opts) -> (Index, Vec<(PathBuf, String)>) {
    let paths = collect_ev_files(&opts.scopes);
    let files = load_files(&paths);
    let idx = build_index(&files);
    (idx, files)
}

fn cmd_index(args: &[String]) -> i32 {
    let opts = parse_opts(args);
    let (idx, files) = build(&opts);
    println!(
        "# {} files, {} decls, {} occurrences",
        files.len(),
        idx.decls.len(),
        idx.occurrences.len()
    );
    let mut decls = idx.decls.clone();
    decls.sort_by(|a, b| {
        (a.file.clone(), a.line, a.col).cmp(&(b.file.clone(), b.line, b.col))
    });
    for d in &decls {
        let hdr = if d.header_slots.is_empty() {
            String::new()
        } else {
            format!("({})", d.header_slots.join(", "))
        };
        println!(
            "{}:{}:{}\t{:<8}\t{}{}",
            rel(&opts.root, &d.file).display(),
            d.line,
            d.col,
            d.kind.label(),
            d.name,
            hdr
        );
    }
    0
}

// ── defs ───────────────────────────────────────────────────────────────

fn cmd_defs(args: &[String]) -> i32 {
    let opts = parse_opts(args);
    if opts.positional.is_empty() {
        eprintln!("evt defs: need a <name>");
        return 2;
    }
    let name = strip_dual(&opts.positional[0]);
    let (idx, _files) = build(&opts);
    let mut found = false;
    for o in &idx.occurrences {
        if o.name == name && o.kind.is_decl() {
            found = true;
            print_occ(&opts.root, o);
        }
    }
    if !found {
        eprintln!("evt defs: no declaration of '{name}' found");
        return 1;
    }
    0
}

// ── refs ───────────────────────────────────────────────────────────────

fn cmd_refs(args: &[String]) -> i32 {
    let opts = parse_opts(args);
    if opts.positional.is_empty() {
        eprintln!("evt refs: need a <name>");
        return 2;
    }
    let name = strip_dual(&opts.positional[0]);
    let (idx, _files) = build(&opts);
    let mut occs: Vec<_> = idx.occurrences.iter().filter(|o| o.name == name).collect();
    occs.sort_by(|a, b| {
        (a.file.clone(), a.line, a.col).cmp(&(b.file.clone(), b.line, b.col))
    });
    if occs.is_empty() {
        eprintln!("evt refs: no occurrences of '{name}'");
        return 1;
    }
    let mut decl = 0;
    let mut asg = 0;
    let mut rd = 0;
    for o in &occs {
        print_occ(&opts.root, o);
        if o.kind.is_decl() {
            decl += 1;
        } else if o.kind == RefKind::AssignLhs {
            asg += 1;
        } else {
            rd += 1;
        }
    }
    eprintln!(
        "# {} occurrences: {} decl, {} assign-lhs, {} read/other (duals included)",
        occs.len(),
        decl,
        asg,
        rd
    );
    0
}

fn print_occ(root: &Path, o: &index::Occurrence) {
    let dual = if o.is_dual { "_" } else { "" };
    let scope = if o.scope.is_empty() {
        String::new()
    } else {
        format!("\tin {}", o.scope)
    };
    println!(
        "{}:{}:{}\t{:<12}\t{}{}{}",
        rel(root, &o.file).display(),
        o.line,
        o.col,
        o.kind.label(),
        dual,
        o.name,
        scope
    );
}

fn strip_dual(s: &str) -> String {
    s.strip_prefix('_')
        .filter(|r| !r.is_empty())
        .unwrap_or(s)
        .to_string()
}

// ── rename ─────────────────────────────────────────────────────────────

fn cmd_rename(args: &[String]) -> i32 {
    let opts = parse_opts(args);
    if opts.positional.len() < 2 {
        eprintln!("evt rename: need <old> <new>");
        return 2;
    }
    let old = strip_dual(&opts.positional[0]);
    let new = strip_dual(&opts.positional[1]);
    if !rename::valid_ident(&new) {
        eprintln!("evt rename: '{new}' is not a valid identifier");
        return 2;
    }
    if old == new {
        eprintln!("evt rename: old == new, nothing to do");
        return 2;
    }
    let (_idx, files) = build(&opts);

    let old_count = rename::count_base(&files, &old);
    if old_count == 0 {
        eprintln!("evt rename: '{old}' does not occur — nothing to rename");
        return 1;
    }
    // Collision check against the authoritative-ish source presence. The
    // declare-fun oracle is the GROUND truth (see `collisions`), but a source
    // collision is the cheap front-line guard and matches the manual recipe.
    let target_count = rename::count_base(&files, &new);
    let force = opts.flags.contains("force");
    if target_count > 0 && !force {
        eprintln!(
            "evt rename: REFUSED — target '{new}' already occurs {target_count} time(s).",
        );
        eprintln!(
            "  Renaming '{old}'→'{new}' would MERGE two distinct symbols under names-match"
        );
        eprintln!("  composition (can silently change semantics / explode the solver).");
        eprintln!("  Run `evt refs {new}` to inspect, then re-run with --force to override.");
        eprintln!("  Authoritative check: `evt collisions` (emitted declare-fun names).");
        return 3;
    }

    let edits = rename::compute(&files, &old, &new);
    let total_edits: usize = edits.iter().map(|f| f.edits.len()).sum();

    if opts.flags.contains("dry-run") {
        println!(
            "# DRY RUN: rename '{old}'→'{new}' — {total_edits} occurrence(s) in {} file(s)",
            edits.len()
        );
        if target_count > 0 {
            println!("# WARNING: target already present {target_count}× (--force MERGE)");
        }
        for fe in &edits {
            print!("{}", rename::diff_preview(rel(&opts.root, &fe.path), fe));
        }
        return 0;
    }

    for fe in &edits {
        if let Err(e) = rename::write_back(fe) {
            eprintln!("evt rename: failed to write {}: {e}", fe.path.display());
            return 1;
        }
    }
    println!(
        "renamed '{old}'→'{new}': {total_edits} occurrence(s) across {} file(s)",
        edits.len()
    );
    if force && target_count > 0 {
        println!("(--force used: MERGED with {target_count} pre-existing '{new}' occurrence(s))");
    }
    0
}

// ── symbols ────────────────────────────────────────────────────────────

fn cmd_symbols(args: &[String]) -> i32 {
    let opts = parse_opts(args);
    let (idx, _files) = build(&opts);

    let only: Option<PathBuf> = opts.positional.first().map(|s| {
        let p = PathBuf::from(s);
        if p.is_absolute() {
            p
        } else {
            // try cwd then root
            let c = std::env::current_dir().unwrap_or_default().join(&p);
            if c.exists() {
                c
            } else {
                opts.root.join(&p)
            }
        }
    });
    let only = only.and_then(|p| std::fs::canonicalize(&p).ok());

    // Group decls by file, then list each schema decl with its member decls.
    let mut decls = idx.decls.clone();
    decls.sort_by(|a, b| {
        (a.file.clone(), a.line, a.col).cmp(&(b.file.clone(), b.line, b.col))
    });

    let mut cur_file: Option<PathBuf> = None;
    for d in &decls {
        if let Some(of) = &only {
            let df = std::fs::canonicalize(&d.file).ok();
            if df.as_ref() != Some(of) {
                continue;
            }
        }
        if cur_file.as_ref() != Some(&d.file) {
            println!("\n{}", rel(&opts.root, &d.file).display());
            cur_file = Some(d.file.clone());
        }
        let hdr = if d.header_slots.is_empty() {
            String::new()
        } else {
            format!("({})", d.header_slots.join(", "))
        };
        println!("  {} {}{}  [{}]", d.kind.label(), d.name, hdr, d.line);
        if matches!(
            d.kind,
            DeclKind::Claim | DeclKind::Fsm | DeclKind::Type | DeclKind::Schema
        ) {
            // member decls whose scope == d.name and same file
            let mut members: Vec<&index::Occurrence> = idx
                .occurrences
                .iter()
                .filter(|o| {
                    o.file == d.file
                        && o.scope == d.name
                        && matches!(o.kind, RefKind::MemberDecl | RefKind::HeaderSlot)
                })
                .collect();
            members.sort_by_key(|o| (o.line, o.col));
            let mut seen = BTreeSet::new();
            for m in members {
                if seen.insert(m.name.clone()) {
                    println!("      · {}  [{}]", m.name, m.line);
                }
            }
        }
    }
    0
}

// ── families ───────────────────────────────────────────────────────────

fn cmd_families(args: &[String]) -> i32 {
    let opts = parse_opts(args);
    let (idx, _files) = build(&opts);
    let min = opts.min.unwrap_or(3);

    println!("# NUMBERED families (xN siblings → candidate Seq), min {min}");
    for f in families::numbered(&idx, min) {
        let lo = f.indices.first().copied().unwrap_or(0);
        let hi = f.indices.last().copied().unwrap_or(0);
        let files: Vec<String> = f
            .files
            .iter()
            .map(|p| {
                rel(&opts.root, Path::new(p))
                    .display()
                    .to_string()
            })
            .collect();
        println!(
            "  {}* [{}]  {} members ({}..{})\t{}",
            f.stem,
            f.indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(","),
            f.indices.len(),
            lo,
            hi,
            files.join(" ")
        );
    }

    println!("\n# PREFIX families (<word>_ ≥{min} distinct decls → candidate record/namespace)");
    for f in families::prefixes(&idx, min) {
        let files: Vec<String> = f
            .files
            .iter()
            .map(|p| rel(&opts.root, Path::new(p)).display().to_string())
            .collect();
        let members: Vec<String> = f.members.iter().cloned().collect();
        let shown = if members.len() > 8 {
            format!("{}, …", members[..8].join(", "))
        } else {
            members.join(", ")
        };
        println!(
            "  {}  {} decls: {}\t[{}]",
            f.prefix,
            f.members.len(),
            shown,
            files.join(" ")
        );
    }

    println!("\n# CONS-PEEL residue (after/next/tail/*_rest — list-walk clusters, 'skip a bank')");
    for (name, files) in families::cons_peel(&idx) {
        let files: Vec<String> = files
            .iter()
            .map(|p| rel(&opts.root, Path::new(p)).display().to_string())
            .collect();
        println!("  {}\t[{}]", name, files.join(" "));
    }
    0
}

// ── collisions (declare-fun oracle) ────────────────────────────────────

fn cmd_collisions(args: &[String]) -> i32 {
    let opts = parse_opts(args);
    let entry = opts
        .positional
        .first()
        .cloned()
        .unwrap_or_else(|| "compiler2/driver.ev".to_string());
    let claim = opts
        .positional
        .get(1)
        .cloned()
        .unwrap_or_else(|| "driver_main".to_string());

    let entry_path = if Path::new(&entry).is_absolute() {
        PathBuf::from(&entry)
    } else {
        opts.root.join(&entry)
    };
    let flatten = opts.root.join("scripts/flatten-evident.sh");
    let oracle = std::env::var("EVIDENT_ORACLE")
        .unwrap_or_else(|_| "/usr/local/bin/evident-oracle".to_string());

    if !flatten.exists() {
        eprintln!("evt collisions: {} not found", flatten.display());
        return 1;
    }
    if !Path::new(&oracle).exists() {
        eprintln!("evt collisions: oracle '{oracle}' not found (set EVIDENT_ORACLE)");
        return 1;
    }

    eprintln!(
        "# emitting {} (claim {claim}) via flatten | evident-oracle",
        rel(&opts.root, &entry_path).display()
    );

    // flatten
    let flat = Command::new("bash")
        .arg(&flatten)
        .arg(&entry_path)
        .output();
    let flat = match flat {
        Ok(o) if o.status.success() => o.stdout,
        Ok(o) => {
            eprintln!(
                "evt collisions: flatten failed: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            return 1;
        }
        Err(e) => {
            eprintln!("evt collisions: flatten exec error: {e}");
            return 1;
        }
    };

    // write flat to a temp file, emit to a temp .smt2
    let tmp_in = std::env::temp_dir().join("evt_flat.ev");
    let tmp_out = std::env::temp_dir().join("evt_dm.smt2");
    if std::fs::write(&tmp_in, &flat).is_err() {
        eprintln!("evt collisions: cannot write temp input");
        return 1;
    }
    let emit = Command::new(&oracle)
        .arg("emit")
        .arg(&tmp_in)
        .arg(&claim)
        .arg("-o")
        .arg(&tmp_out)
        .output();
    match emit {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            eprintln!(
                "evt collisions: emit failed: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            return 1;
        }
        Err(e) => {
            eprintln!("evt collisions: emit exec error: {e}");
            return 1;
        }
    }

    let smt = match std::fs::read_to_string(&tmp_out) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("evt collisions: cannot read emitted smt2: {e}");
            return 1;
        }
    };

    // Parse top-level `(declare-fun NAME (...) ...)` names and detect dups.
    let mut names: Vec<String> = Vec::new();
    for line in smt.lines() {
        let l = line.trim_start();
        if let Some(rest) = l.strip_prefix("(declare-fun ") {
            let name: String = rest
                .chars()
                .take_while(|c| !c.is_whitespace() && *c != '(')
                .collect();
            if !name.is_empty() {
                names.push(name);
            }
        }
    }
    names.sort();
    let total = names.len();
    let mut dups: Vec<(String, usize)> = Vec::new();
    {
        let mut i = 0;
        while i < names.len() {
            let mut j = i + 1;
            while j < names.len() && names[j] == names[i] {
                j += 1;
            }
            if j - i > 1 {
                dups.push((names[i].clone(), j - i));
            }
            i = j;
        }
    }
    names.dedup();
    println!("# {total} declare-fun lines, {} distinct top-level names", names.len());
    for n in &names {
        println!("{n}");
    }
    if dups.is_empty() {
        eprintln!("# COLLISIONS: none (all top-level declare-fun names distinct)");
        0
    } else {
        eprintln!("# COLLISIONS DETECTED ({}):", dups.len());
        for (n, c) in &dups {
            eprintln!("  {n}  ×{c}");
        }
        4
    }
}
