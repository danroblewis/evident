//! Declaration / reference extraction over the lexed token stream.
//!
//! Evident has no imports-for-resolution and composition is names-match, so
//! a "symbol" here is a NAME, and we classify each *occurrence* of that name
//! structurally:
//!
//!   * a top-level decl keyword followed by an Ident  → schema decl
//!     (`claim`/`fsm`/`type`/`schema`/`enum`/`subclaim` Name)
//!   * an Ident immediately followed (modulo a `,`-list and bounds) by `∈`
//!     → a membership = a variable/field *declaration*
//!   * an Ident on the LHS of a top-level `=` (decl with init, or a field /
//!     index assignment `r.f = …`, `xs[k] = …`)  → assignment target
//!   * an Ident followed by `(` at decl/use position → a claim/type/enum
//!     reference (call/header) — but we don't over-classify; calls and reads
//!     are both "reference" unless we can see the `∈` or `=`.
//!
//! This is deliberately a *lexical* model — it does NOT resolve names-match
//! joins across components (that needs the oracle). It is exact about token
//! boundaries (no substring bugs) and about the `_x` carry dual.

use crate::lexer::{is_keyword, lex, Tok, Token};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    Claim,
    Fsm,
    Type,
    Schema,
    Enum,
    Subclaim,
    EnumVariant,
}

impl DeclKind {
    pub fn label(&self) -> &'static str {
        match self {
            DeclKind::Claim => "claim",
            DeclKind::Fsm => "fsm",
            DeclKind::Type => "type",
            DeclKind::Schema => "schema",
            DeclKind::Enum => "enum",
            DeclKind::Subclaim => "subclaim",
            DeclKind::EnumVariant => "variant",
        }
    }
    fn from_kw(s: &str) -> Option<DeclKind> {
        match s {
            "claim" => Some(DeclKind::Claim),
            "fsm" => Some(DeclKind::Fsm),
            "type" => Some(DeclKind::Type),
            "schema" => Some(DeclKind::Schema),
            "enum" => Some(DeclKind::Enum),
            "subclaim" => Some(DeclKind::Subclaim),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    /// A top-level schema/enum/subclaim declaration (`claim Name`).
    SchemaDecl,
    /// An enum variant declaration (`Red`, `Ok(Int)` on the RHS of `enum`).
    VariantDecl,
    /// A variable/field declaration via membership (`x ∈ T`).
    MemberDecl,
    /// A header slot name (`claim R(on ∈ Bool)`), also a membership-style decl.
    HeaderSlot,
    /// LHS of an assignment `=` (not inside an expression): `x = …`, `r.f = …`.
    AssignLhs,
    /// Any other identifier occurrence (read / call / comparison operand).
    Read,
}

impl RefKind {
    pub fn label(&self) -> &'static str {
        match self {
            RefKind::SchemaDecl => "schema-decl",
            RefKind::VariantDecl => "variant-decl",
            RefKind::MemberDecl => "member-decl",
            RefKind::HeaderSlot => "header-slot",
            RefKind::AssignLhs => "assign-lhs",
            RefKind::Read => "read",
        }
    }
    pub fn is_decl(&self) -> bool {
        matches!(
            self,
            RefKind::SchemaDecl
                | RefKind::VariantDecl
                | RefKind::MemberDecl
                | RefKind::HeaderSlot
        )
    }
}

#[derive(Debug, Clone)]
pub struct Occurrence {
    pub name: String,
    /// True if this occurrence carries a leading underscore (`_name`); the
    /// stored `name` is WITHOUT the underscore so duals group with the base.
    pub is_dual: bool,
    pub file: PathBuf,
    pub line: usize,
    pub col: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    pub kind: RefKind,
    /// For decls: the enclosing schema name (top-level decl, or "" at file top).
    pub scope: String,
    /// For SchemaDecl occurrences: the decl kind.
    pub decl_kind: Option<DeclKind>,
}

#[derive(Debug, Clone)]
pub struct Decl {
    pub name: String,
    pub kind: DeclKind,
    pub file: PathBuf,
    pub line: usize,
    pub col: usize,
    /// Header slot names, in order (empty for header-less claims / non-claims).
    pub header_slots: Vec<String>,
}

#[derive(Debug, Default)]
pub struct Index {
    pub occurrences: Vec<Occurrence>,
    pub decls: Vec<Decl>,
}

/// Split a raw identifier token into (is_dual, base_name).
fn split_dual(s: &str) -> (bool, &str) {
    if let Some(rest) = s.strip_prefix('_') {
        // `_` alone, or `__x` — only a SINGLE leading underscore is the carry
        // dual; preserve the rest verbatim. `_x` → dual of `x`. `__x` → dual
        // of `_x` (rare). A bare `_` is a wildcard, not a dual.
        if !rest.is_empty() {
            return (true, rest);
        }
    }
    (false, s)
}

/// Index a single file's tokens. `path` is stored on every occurrence.
pub fn index_file(path: &Path, src: &str, idx: &mut Index) {
    let toks: Vec<Token> = lex(src)
        .into_iter()
        .filter(|t| !matches!(t.tok, Tok::Comment(_)))
        .collect();

    // Track the current top-level scope name (last top-level schema decl).
    let mut scope = String::new();

    // Helper to know if an ident token at position p is a real name (not a
    // keyword used as a keyword). Keywords like `match`/`in`/`true` are never
    // renameable symbols.
    let n = toks.len();
    let mut p = 0usize;

    // Compute, per token index, whether it begins a logical line (preceded
    // only by Newline or start). Indentation we approximate via column.
    while p < n {
        let t = &toks[p];
        // line-leading?
        let line_leading =
            p == 0 || matches!(toks[p - 1].tok, Tok::Newline);

        if let Tok::Ident(word) = &t.tok {
            // Top-level decl keyword?
            if line_leading {
                if let Some(dk) = DeclKind::from_kw(word) {
                    // next ident is the decl name
                    if let Some((np, name_tok)) = next_ident(&toks, p + 1) {
                        let raw = name_tok.ident().unwrap();
                        let (is_dual, base) = split_dual(raw);
                        let col0 = if t.col == 1 { true } else { false };
                        // header slots if a `(` follows the name
                        let (slots, slot_occs) =
                            parse_header_slots(&toks, np + 1, base, path);
                        idx.decls.push(Decl {
                            name: base.to_string(),
                            kind: dk,
                            file: path.to_path_buf(),
                            line: name_tok.line,
                            col: name_tok.col,
                            header_slots: slots.clone(),
                        });
                        idx.occurrences.push(Occurrence {
                            name: base.to_string(),
                            is_dual,
                            file: path.to_path_buf(),
                            line: name_tok.line,
                            col: name_tok.col,
                            byte_start: name_tok.byte_start,
                            byte_end: name_tok.byte_end,
                            kind: RefKind::SchemaDecl,
                            scope: scope.clone(),
                            decl_kind: Some(dk),
                        });
                        idx.occurrences.extend(slot_occs);
                        // a top-level (col 1) decl updates scope; a nested
                        // `subclaim` keeps the parent scope as outer but we
                        // set scope to the subclaim too for member attribution.
                        if col0 || dk == DeclKind::Subclaim {
                            scope = base.to_string();
                        }
                        // For enums, harvest variant decls on the RHS.
                        if dk == DeclKind::Enum {
                            harvest_enum_variants(&toks, np + 1, &scope, path, idx);
                        }
                        p = np + 1;
                        continue;
                    }
                }
            }
        }

        // Membership declaration: IDENT (, IDENT)* ∈   — possibly inside a
        // bounds chain like `0 ≤ x ∈ Int < 10` (the IDENT directly left of ∈
        // and any comma-listed names share the decl).
        if let Tok::Ident(word) = &t.tok {
            if !is_keyword(word) {
                if let Some(names) = membership_decl_names(&toks, p) {
                    for (np, name_tok) in &names {
                        let raw = toks[*np].ident().unwrap();
                        let (is_dual, base) = split_dual(raw);
                        idx.occurrences.push(Occurrence {
                            name: base.to_string(),
                            is_dual,
                            file: path.to_path_buf(),
                            line: name_tok.line,
                            col: name_tok.col,
                            byte_start: name_tok.byte_start,
                            byte_end: name_tok.byte_end,
                            kind: RefKind::MemberDecl,
                            scope: scope.clone(),
                            decl_kind: None,
                        });
                    }
                    // Don't `continue`: still scan the rest of the line for
                    // the type-name reference etc. But skip past the names we
                    // already recorded so they aren't double-classified.
                    // We advance to just after the last consumed name.
                    let last = names.last().unwrap().0;
                    // mark intermediate idents (already pushed) — emit the
                    // remaining tokens normally by jumping to last+1, but the
                    // ∈ and type follow. Simplest: continue scanning from last+1.
                    record_line_reads(&toks, last + 1, p, &mut |_| {}); // no-op marker
                    p = last + 1;
                    // fallthrough into generic reads handling below by not continuing
                    // — but we must still classify tokens from here. Re-loop.
                    continue;
                }
            }
        }

        // Generic identifier occurrence: classify as AssignLhs if it's a
        // line-leading lvalue ending at a top-level `=`, else Read.
        if let Tok::Ident(word) = &t.tok {
            if !is_keyword(word) {
                let (is_dual, base) = split_dual(word);
                let kind = classify_lvalue(&toks, p, line_leading);
                idx.occurrences.push(Occurrence {
                    name: base.to_string(),
                    is_dual,
                    file: path.to_path_buf(),
                    line: t.line,
                    col: t.col,
                    byte_start: t.byte_start,
                    byte_end: t.byte_end,
                    kind,
                    scope: scope.clone(),
                    decl_kind: None,
                });
            }
        }
        p += 1;
    }
}

/// no-op helper retained for clarity of intent; reads are handled inline.
fn record_line_reads(_toks: &[Token], _from: usize, _decl_at: usize, _f: &mut dyn FnMut(usize)) {}

fn next_ident<'a>(toks: &'a [Token], from: usize) -> Option<(usize, &'a Token)> {
    let mut p = from;
    while p < toks.len() {
        match &toks[p].tok {
            Tok::Ident(_) => return Some((p, &toks[p])),
            Tok::Newline => return None,
            _ => p += 1,
        }
    }
    None
}

/// If a `(` follows the schema name at `from`, parse the header slot names
/// (`name ∈ Type` entries separated by `,`), returning their names and the
/// occurrences (classified HeaderSlot). Multi-name groups `a, b ∈ T` count
/// each name as a slot.
fn parse_header_slots(
    toks: &[Token],
    from: usize,
    _scope: &str,
    path: &Path,
) -> (Vec<String>, Vec<Occurrence>) {
    let mut slots = Vec::new();
    let mut occs = Vec::new();
    if from >= toks.len() || !toks[from].op_is("(") {
        return (slots, occs);
    }
    let mut p = from + 1;
    let mut depth = 1i32;
    // A header is `name (, name)* ∈ Type (, name (, name)* ∈ Type)*`. Within a
    // group, idents BEFORE the `∈` are slot names; everything from `∈` up to
    // the next depth-1 `,` is the Type (skip it). `skip_type` tracks that.
    let mut pending: Vec<usize> = Vec::new();
    let mut skip_type = false;
    while p < toks.len() && depth > 0 {
        let tk = &toks[p];
        match &tk.tok {
            Tok::Op(o) if o == "(" => depth += 1,
            Tok::Op(o) if o == ")" => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Tok::Op(o) if o == "," && depth == 1 => {
                // a depth-1 comma ends the current type and starts a new
                // name group.
                skip_type = false;
            }
            Tok::Op(o) if o == "∈" && depth == 1 => {
                // flush pending idents as slots; the rest of this group is the
                // type annotation.
                for &ip in &pending {
                    let raw = toks[ip].ident().unwrap();
                    let (is_dual, base) = {
                        let (d, b) = split_dual(raw);
                        (d, b.to_string())
                    };
                    slots.push(base.clone());
                    occs.push(Occurrence {
                        name: base,
                        is_dual,
                        file: path.to_path_buf(),
                        line: toks[ip].line,
                        col: toks[ip].col,
                        byte_start: toks[ip].byte_start,
                        byte_end: toks[ip].byte_end,
                        kind: RefKind::HeaderSlot,
                        scope: _scope.to_string(),
                        decl_kind: None,
                    });
                }
                pending.clear();
                skip_type = true;
            }
            Tok::Ident(w) if depth == 1 && !skip_type && !is_keyword(w) => {
                pending.push(p);
            }
            _ => {}
        }
        p += 1;
    }
    (slots, occs)
}

/// Harvest `enum E = A | B(Int) | C(X, E)` variant *names* as VariantDecl
/// occurrences. Scans from after the enum name to end-of-decl (until a line
/// that starts a new top-level decl or EOF). Variant names are the idents
/// directly after `=` or `|`.
fn harvest_enum_variants(
    toks: &[Token],
    from: usize,
    scope: &str,
    path: &Path,
    idx: &mut Index,
) {
    // find the `=`
    let mut p = from;
    while p < toks.len() && !toks[p].op_is("=") {
        if matches!(toks[p].tok, Tok::Newline) {
            // multi-line enums put variants on following indented lines with
            // no `=`; allow scanning to continue.
        }
        // stop if a new top-level decl starts
        if line_starts_topdecl(toks, p) && p != from {
            return;
        }
        p += 1;
    }
    if p >= toks.len() {
        return;
    }
    p += 1; // past `=`
    // Now variant names follow `=` and each `|`, and on indented continuation
    // lines (one variant per line). We accept an ident as a variant if it is
    // the first ident after `=`/`|`/Newline within this enum block.
    let mut expect_variant = true;
    while p < toks.len() {
        let tk = &toks[p];
        match &tk.tok {
            Tok::Newline => {
                // peek: does the next non-empty line start a new top decl?
                if line_starts_topdecl(toks, p + 1) {
                    return;
                }
                expect_variant = true;
            }
            Tok::Op(o) if o == "|" => expect_variant = true,
            Tok::Op(_) => { /* payload parens etc. don't reset */ }
            Tok::Ident(w) => {
                if expect_variant && !is_keyword(w) {
                    let (_d, base) = split_dual(w);
                    idx.occurrences.push(Occurrence {
                        name: base.to_string(),
                        is_dual: false,
                        file: path.to_path_buf(),
                        line: tk.line,
                        col: tk.col,
                        byte_start: tk.byte_start,
                        byte_end: tk.byte_end,
                        kind: RefKind::VariantDecl,
                        scope: scope.to_string(),
                        decl_kind: Some(DeclKind::EnumVariant),
                    });
                    idx.decls.push(Decl {
                        name: base.to_string(),
                        kind: DeclKind::EnumVariant,
                        file: path.to_path_buf(),
                        line: tk.line,
                        col: tk.col,
                        header_slots: vec![],
                    });
                }
                expect_variant = false;
            }
            _ => {}
        }
        p += 1;
    }
}

fn line_starts_topdecl(toks: &[Token], at: usize) -> bool {
    // at points at the first token of a line (col 1). Check col==1 + decl kw.
    let mut p = at;
    while p < toks.len() && matches!(toks[p].tok, Tok::Newline) {
        p += 1;
    }
    if p >= toks.len() {
        return false;
    }
    if toks[p].col != 1 {
        return false;
    }
    if let Tok::Ident(w) = &toks[p].tok {
        DeclKind::from_kw(w).is_some()
    } else {
        false
    }
}

/// If position `p` begins a membership decl `NAME (, NAME)* ∈`, possibly
/// preceded on the same logical scan by a lower bound (`0 ≤ x ∈ …` — here the
/// `x` is the IDENT directly left of ∈), return the list of (token-index,
/// token) decl names. We only treat the comma-group ending right before `∈`.
fn membership_decl_names<'a>(toks: &'a [Token], p: usize) -> Option<Vec<(usize, &'a Token)>> {
    // Walk forward collecting an alternating IDENT (, IDENT)* then require ∈.
    // The first ident must be at p.
    if toks[p].ident().is_none() {
        return None;
    }
    let mut names = vec![(p, &toks[p])];
    let mut q = p + 1;
    loop {
        if q >= toks.len() {
            return None;
        }
        match &toks[q].tok {
            Tok::Op(o) if o == "," => {
                // next must be an ident
                if q + 1 < toks.len() {
                    if let Tok::Ident(w) = &toks[q + 1].tok {
                        if is_keyword(w) {
                            return None;
                        }
                        names.push((q + 1, &toks[q + 1]));
                        q += 2;
                        continue;
                    }
                }
                return None;
            }
            Tok::Op(o) if o == "∈" => return Some(names),
            _ => return None,
        }
    }
}

/// Classify a non-decl identifier occurrence at `p`. If it is line-leading and
/// the lvalue (`x`, `r.f`, `xs[k]`) is terminated by a top-level `=` (not `==`
/// — Evident has no `==` — and not `≤`/`≥`/`<`/`>`), it's an AssignLhs;
/// otherwise Read.
fn classify_lvalue(toks: &[Token], p: usize, line_leading: bool) -> RefKind {
    if !line_leading {
        return RefKind::Read;
    }
    // walk the lvalue: IDENT ( . IDENT | [ ... ] )*  then expect `=`.
    let mut q = p + 1;
    loop {
        if q >= toks.len() {
            return RefKind::Read;
        }
        match &toks[q].tok {
            Tok::Op(o) if o == "." => {
                // field access — skip `.` and the field ident
                if q + 1 < toks.len() && toks[q + 1].ident().is_some() {
                    q += 2;
                    continue;
                }
                return RefKind::Read;
            }
            Tok::Op(o) if o == "[" => {
                // skip a balanced bracket group
                let mut depth = 1;
                q += 1;
                while q < toks.len() && depth > 0 {
                    if toks[q].op_is("[") {
                        depth += 1;
                    } else if toks[q].op_is("]") {
                        depth -= 1;
                    } else if matches!(toks[q].tok, Tok::Newline) {
                        return RefKind::Read;
                    }
                    q += 1;
                }
                continue;
            }
            Tok::Op(o) if o == "=" => return RefKind::AssignLhs,
            _ => return RefKind::Read,
        }
    }
}

/// Build an index over a set of files (path, source) pairs.
pub fn build_index(files: &[(PathBuf, String)]) -> Index {
    let mut idx = Index::default();
    for (path, src) in files {
        index_file(path, src, &mut idx);
    }
    idx
}
