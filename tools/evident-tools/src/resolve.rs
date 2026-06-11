//! Higher-level queries the LSP server needs, kept in the library so they are
//! unit-testable without spawning a server: token-at-position resolution,
//! completion candidates, and the index-backed Seq-membership diagnostic.

use crate::index::{index_file, DeclKind, Index, RefKind};
use crate::lexer::{is_keyword, lex, Tok, Token};
use std::path::Path;

/// An identifier token resolved at a cursor position.
#[derive(Debug, Clone)]
pub struct IdentAt {
    /// Base name (dual-stripped): `_foo` resolves to `foo`.
    pub base: String,
    /// True if the token carried a leading `_` carry-dual underscore.
    pub is_dual: bool,
    /// 1-based line / char-col of the WHOLE token (incl. any leading `_`).
    pub line: usize,
    pub col: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    /// char length of the whole token text (incl. leading `_`).
    pub len_chars: usize,
}

/// Find the identifier token at our internal 1-based `(line, char-col)`.
/// `col` may point anywhere within the token (inclusive of the first char,
/// exclusive of one-past-the-end).
pub fn ident_at(text: &str, line1: usize, col1: usize) -> Option<IdentAt> {
    for t in lex(text) {
        if let Tok::Ident(w) = &t.tok {
            let len = w.chars().count();
            if t.line == line1 && col1 >= t.col && col1 < t.col + len {
                let (is_dual, base) = if let Some(rest) = w.strip_prefix('_') {
                    if rest.is_empty() {
                        (false, w.as_str())
                    } else {
                        (true, rest)
                    }
                } else {
                    (false, w.as_str())
                };
                return Some(IdentAt {
                    base: base.to_string(),
                    is_dual,
                    line: t.line,
                    col: t.col,
                    byte_start: t.byte_start,
                    byte_end: t.byte_end,
                    len_chars: len,
                });
            }
        }
    }
    None
}

/// A completion candidate.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub label: String,
    /// LSP CompletionItemKind numeric code.
    pub kind: i64,
    /// Optional detail string (e.g. header signature).
    pub detail: String,
}

/// LSP CompletionItemKind codes used here.
pub mod citem {
    pub const KEYWORD: i64 = 14;
    pub const CLASS: i64 = 7; // claim/fsm
    pub const STRUCT: i64 = 22; // type/schema
    pub const ENUM: i64 = 13;
    pub const ENUM_MEMBER: i64 = 20;
    pub const FIELD: i64 = 5;
    pub const VARIABLE: i64 = 6;
}

/// Evident keywords + the built-in type/sort names worth completing.
pub const COMPLETION_KEYWORDS: &[&str] = &[
    "claim", "type", "schema", "fsm", "enum", "subclaim", "import", "match",
    "matches", "true", "false",
];
pub const BUILTIN_TYPES: &[&str] = &[
    "Int", "Bool", "Real", "String", "Nat", "Seq", "Set", "Effect", "Result",
];

/// Build completion candidates from the workspace index plus keywords/types.
/// `prefix` filters by case-sensitive starts-with (empty = all). Deduplicated
/// by label, schema decls preferred over plain variable mentions.
pub fn completions(idx: &Index, prefix: &str) -> Vec<Candidate> {
    use std::collections::BTreeMap;
    let mut by_label: BTreeMap<String, Candidate> = BTreeMap::new();

    let mut push = |label: &str, kind: i64, detail: String| {
        if !prefix.is_empty() && !label.starts_with(prefix) {
            return;
        }
        by_label
            .entry(label.to_string())
            .or_insert_with(|| Candidate {
                label: label.to_string(),
                kind,
                detail,
            });
    };

    for kw in COMPLETION_KEYWORDS {
        push(kw, citem::KEYWORD, String::new());
    }
    for ty in BUILTIN_TYPES {
        push(ty, citem::STRUCT, "builtin type".to_string());
    }

    // schema decls + enum variants from the index
    for d in &idx.decls {
        let kind = match d.kind {
            DeclKind::Claim | DeclKind::Fsm => citem::CLASS,
            DeclKind::Type | DeclKind::Schema => citem::STRUCT,
            DeclKind::Enum => citem::ENUM,
            DeclKind::Subclaim => citem::CLASS,
            DeclKind::EnumVariant => citem::ENUM_MEMBER,
        };
        let detail = if d.header_slots.is_empty() {
            d.kind.label().to_string()
        } else {
            format!("{} ({})", d.kind.label(), d.header_slots.join(", "))
        };
        push(&d.name, kind, detail);
    }

    // member / field / header-slot names
    for o in &idx.occurrences {
        if matches!(o.kind, RefKind::MemberDecl | RefKind::HeaderSlot) {
            let kind = if matches!(o.kind, RefKind::HeaderSlot) {
                citem::FIELD
            } else {
                citem::VARIABLE
            };
            let detail = if o.scope.is_empty() {
                String::new()
            } else {
                format!("in {}", o.scope)
            };
            push(&o.name, kind, detail);
        }
    }

    by_label.into_values().collect()
}

/// A diagnostic produced by the library (positions in 1-based char coords;
/// the server converts to LSP).
#[derive(Debug, Clone)]
pub struct LibDiag {
    pub line: usize, // 1-based
    pub col_start: usize, // 1-based char col of the flagged token start
    pub col_end: usize, // 1-based char col, exclusive
    pub severity: i64, // 1 Error, 2 Warning, 3 Info, 4 Hint
    pub code: String,
    pub message: String,
}

/// Index-backed diagnostics for a single document.
///
/// 1. **Seq-membership footgun** (`x ∈ xs` where `xs` is declared `Seq(...)`):
///    SILENTLY DROPPED by the frozen oracle. We resolve the RHS name against
///    the document's own declarations to know it is a `Seq`, so this is far
///    more precise than a shape heuristic — no false positives on `x ∈ Set(T)`
///    or `x ∈ TypeName`.
/// 2. **`True`/`False` capitalised booleans** — parse as unbound names and the
///    constraint silently drops (CLAUDE.md footgun).
pub fn diagnostics(path: &Path, text: &str) -> Vec<LibDiag> {
    let mut idx = Index::default();
    index_file(path, text, &mut idx);

    // Names declared as `Seq(...)` in THIS document (membership decl whose
    // type annotation begins with `Seq`).
    let seq_names = seq_decl_names(text);

    let toks: Vec<Token> = lex(text)
        .into_iter()
        .filter(|t| !matches!(t.tok, Tok::Comment(_)))
        .collect();

    let mut out = Vec::new();

    // Pass 1: Seq membership `LHS ∈ RHS` at expression position (NOT a decl:
    // a decl has a Type after ∈; here the token after ∈ is a known Seq var).
    for i in 0..toks.len() {
        if toks[i].op_is("∈") {
            // RHS is the next ident
            let rhs = match toks.get(i + 1).and_then(|t| t.ident()) {
                Some(r) => r,
                None => continue,
            };
            let rhs_base = rhs.strip_prefix('_').filter(|r| !r.is_empty()).unwrap_or(rhs);
            if !seq_names.contains(rhs_base) {
                continue;
            }
            // LHS must be a single ident immediately before ∈ (an expr-position
            // membership, not a quantifier `∀ x ∈ xs` and not a decl). Guard:
            // the token two-back must not be `∀`/`∃` and the LHS ident must not
            // be itself a fresh decl. A `∀ x ∈ xs : …` is legitimate iteration.
            if i == 0 {
                continue;
            }
            let lhs_tok = &toks[i - 1];
            let lhs = match lhs_tok.ident() {
                Some(l) => l,
                None => continue,
            };
            // skip quantifier binders: look back past the ident for ∀/∃ on the
            // same logical line.
            let mut q = i as isize - 2;
            let mut quantified = false;
            while q >= 0 {
                match &toks[q as usize].tok {
                    Tok::Newline => break,
                    Tok::Op(o) if o == "∀" || o == "∃" => {
                        quantified = true;
                        break;
                    }
                    _ => {}
                }
                q -= 1;
            }
            if quantified {
                continue;
            }
            out.push(LibDiag {
                line: lhs_tok.line,
                col_start: lhs_tok.col,
                col_end: toks[i + 1].col + rhs.chars().count(),
                severity: 2,
                code: "seq-membership".to_string(),
                message: format!(
                    "Seq membership `{lhs} ∈ {rhs}` is SILENTLY DROPPED by the oracle \
                     (the constraint vanishes, claim goes vacuously SAT). \
                     Use `∃ i ∈ {{0..#{rhs}-1}} : {rhs}[i] = {lhs}` instead."
                ),
            });
        }
    }

    // Pass 2: capitalised `True`/`False` used as bare names.
    for t in &toks {
        if let Tok::Ident(w) = &t.tok {
            if (w == "True" || w == "False") && !is_keyword(w) {
                out.push(LibDiag {
                    line: t.line,
                    col_start: t.col,
                    col_end: t.col + w.chars().count(),
                    severity: 2,
                    code: "capitalised-bool".to_string(),
                    message: format!(
                        "`{w}` parses as an unbound name, not a boolean — the constraint \
                         silently drops. Use lowercase `{}`.",
                        w.to_lowercase()
                    ),
                });
            }
        }
    }

    out
}

/// Names declared with a `Seq(...)` type annotation in this document.
/// Recognises `name ∈ Seq(...)` and `a, b ∈ Seq(...)` (membership decls).
fn seq_decl_names(text: &str) -> std::collections::BTreeSet<String> {
    use std::collections::BTreeSet;
    let toks: Vec<Token> = lex(text)
        .into_iter()
        .filter(|t| !matches!(t.tok, Tok::Comment(_)))
        .collect();
    let mut out = BTreeSet::new();
    for i in 0..toks.len() {
        if toks[i].op_is("∈") {
            // type after ∈ is `Seq`?
            let is_seq = toks
                .get(i + 1)
                .and_then(|t| t.ident())
                .map(|w| w == "Seq")
                .unwrap_or(false);
            if !is_seq {
                continue;
            }
            // collect the decl names: walk left over `ident (, ident)*`.
            let mut j = i as isize - 1;
            let mut names = Vec::new();
            // first the ident immediately left of ∈
            if j >= 0 {
                if let Some(n) = toks[j as usize].ident() {
                    names.push(n.to_string());
                    j -= 1;
                    // then pairs of `, ident`
                    while j >= 1 {
                        let comma = toks[j as usize].op_is(",");
                        let prev = toks[(j - 1) as usize].ident();
                        if comma {
                            if let Some(n) = prev {
                                names.push(n.to_string());
                                j -= 2;
                                continue;
                            }
                        }
                        break;
                    }
                }
            }
            for n in names {
                let base = n.strip_prefix('_').filter(|r| !r.is_empty()).unwrap_or(&n);
                out.insert(base.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn flags_direct_seq_membership() {
        let src = "claim main\n    xs ∈ Seq(Int) = ⟨1, 2, 3⟩\n    target ∈ Int = 99\n    target ∈ xs\n";
        let d = diagnostics(Path::new("t.ev"), src);
        assert!(
            d.iter().any(|d| d.code == "seq-membership" && d.line == 4),
            "expected seq-membership on line 4, got {:?}",
            d
        );
    }

    #[test]
    fn does_not_flag_quantifier_iteration() {
        let src = "claim main\n    xs ∈ Seq(Int) = ⟨1⟩\n    ∀ x ∈ xs : x > 0\n";
        let d = diagnostics(Path::new("t.ev"), src);
        assert!(d.iter().all(|d| d.code != "seq-membership"), "{:?}", d);
    }

    #[test]
    fn does_not_flag_set_membership() {
        let src = "claim main\n    s ∈ Set(Int)\n    x ∈ Int = 1\n    x ∈ s\n";
        let d = diagnostics(Path::new("t.ev"), src);
        assert!(d.iter().all(|d| d.code != "seq-membership"), "{:?}", d);
    }

    #[test]
    fn flags_capitalised_bool() {
        let src = "claim main\n    flag ∈ Bool = True\n";
        let d = diagnostics(Path::new("t.ev"), src);
        assert!(d.iter().any(|d| d.code == "capitalised-bool"));
    }

    #[test]
    fn ident_at_basic() {
        let src = "claim Foo\n    bar ∈ Int\n";
        let r = ident_at(src, 2, 6).unwrap();
        assert_eq!(r.base, "bar");
        assert!(!r.is_dual);
    }

    #[test]
    fn ident_at_dual() {
        let src = "fsm M\n    x = _x\n";
        let r = ident_at(src, 2, 10).unwrap();
        assert_eq!(r.base, "x");
        assert!(r.is_dual);
    }
}
