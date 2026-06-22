//! `evident fmt` — a gofmt-style, comment-preserving source formatter.
//!
//! ## Why this is a *line/token* formatter, not an AST pretty-printer
//!
//! Evident comments (`-- …`) are discarded by the lexer and never reach the
//! AST (see `lexer.rs` — comment runs are skipped, no token is emitted). The
//! `.ev` files under `examples/` are worked examples whose comments carry real
//! meaning. A parse → AST → re-print formatter would therefore *delete every
//! comment*, which is unacceptable. So `fmt` instead normalizes the source text
//! directly: it fixes indentation, trims trailing whitespace, and collapses
//! blank-line runs, while leaving every token (and every comment) exactly where
//! the author put it.
//!
//! ## The correctness oracle
//!
//! The non-negotiable invariant is that `fmt` is *semantically equivalent* —
//! parse(src) and parse(fmt(src)) yield the same AST. We guarantee this with a
//! token-level check ([`tokens_equivalent`]): the only tokens `fmt` is allowed
//! to perturb are `Indent(n)` (re-scaled to a canonical depth) and runs of
//! `Newline` (collapsed). Every other token — every identifier, operator,
//! literal, paren — must appear in the same order with the same value. The
//! parser keys block structure off *relative* `Indent` comparisons within a
//! block, so canonicalizing each indent to its nesting depth preserves the
//! parse exactly. [`format_source`] runs this check on its own output and
//! refuses (returns `Err`) to emit anything that fails it — so a bug in the
//! formatter can never silently corrupt a program.

use crate::lexer::{tokenize, Token};

const INDENT_WIDTH: usize = 4;

/// Format Evident source text. On success returns the formatted string, which
/// is guaranteed token-equivalent to the input. On any internal inconsistency
/// (re-tokenization mismatch) returns `Err` and the caller should leave the
/// file untouched.
pub fn format_source(src: &str) -> Result<String, String> {
    // Tokenize the original up front: if it doesn't even lex, there's nothing
    // to format — surface the error.
    let orig_tokens = tokenize(src).map_err(|e| e.to_string())?;

    let formatted = reindent(src);

    // Self-check: the formatted text must lex to an equivalent token stream.
    let new_tokens = tokenize(&formatted)
        .map_err(|e| format!("internal: formatted output failed to lex: {e}"))?;
    if !tokens_equivalent(&orig_tokens, &new_tokens) {
        return Err("internal: formatter changed the token stream — refusing to \
                    emit (this is a formatter bug; file left unchanged)"
            .to_string());
    }

    // Idempotence guard: a second pass must be a fixed point. Cheap insurance.
    let twice = reindent(&formatted);
    if twice != formatted {
        return Err("internal: formatter is not idempotent — refusing to emit"
            .to_string());
    }

    Ok(formatted)
}

/// A physical line, classified.
enum LineKind {
    Blank,
    /// A line whose first non-whitespace content is `--` (a full-line comment).
    Comment,
    /// A line carrying code (possibly with a trailing comment).
    Code,
}

fn classify(line: &str) -> LineKind {
    let t = line.trim_start();
    if t.is_empty() {
        LineKind::Blank
    } else if t.starts_with("--") {
        LineKind::Comment
    } else {
        LineKind::Code
    }
}

/// Re-derive indentation structurally and clean up whitespace.
///
/// Strategy: walk physical lines. Track an indent *stack* of the original
/// leading-space widths seen at logical-line starts; map each width to its
/// position in the stack (its nesting depth) and emit `INDENT_WIDTH * depth`
/// spaces. This preserves every equal/greater/less relationship among the
/// indents the lexer would emit, so the parse is unchanged — while snapping the
/// physical indentation onto a clean 4-space grid.
///
/// Continuation lines (physical lines that start while a bracket/paren/seq is
/// still open) are *not* logical-line starts: the lexer emits no `Indent` for
/// them, so their leading whitespace is invisible to the parser. We deliberately
/// leave the author's alignment on those lines untouched (trimming only trailing
/// whitespace) — hand-aligned multi-line seq/ternary bodies read better than any
/// mechanical rule we could impose, and touching them risks nothing but loses
/// intent.
fn reindent(src: &str) -> String {
    let mut out_lines: Vec<String> = Vec::new();

    // Stack of original indent widths defining the current nesting. stack[d] is
    // the source column at which depth `d` begins. depth 0 is always column 0.
    let mut indent_stack: Vec<usize> = vec![0];
    // Bracket nesting carried across physical lines (paren/brace/bracket/seq).
    let mut bracket_depth: i32 = 0;
    // Pending blank lines, so we can collapse runs and drop leading/trailing.
    let mut pending_blank = false;
    let mut emitted_any = false;

    for raw in src.split('\n') {
        let line = raw;
        match classify(line) {
            LineKind::Blank => {
                if emitted_any {
                    pending_blank = true;
                }
                continue;
            }
            LineKind::Comment | LineKind::Code => {}
        }

        let is_continuation = bracket_depth > 0;
        let content = line.trim_end();

        if is_continuation {
            // Inside an open bracket: indentation is parser-invisible. Preserve
            // the author's alignment verbatim (only the trailing trim applied).
            flush_blank(&mut out_lines, &mut pending_blank, &mut emitted_any);
            out_lines.push(content.to_string());
            bracket_depth += net_bracket_delta(content);
            continue;
        }

        // Logical line start: compute its original indent width and re-map it
        // onto the depth stack.
        let orig_indent = leading_ws_width(content);
        let trimmed = content.trim_start();

        // Pop levels deeper than this line.
        while indent_stack.len() > 1 && *indent_stack.last().unwrap() > orig_indent {
            indent_stack.pop();
        }
        // If strictly deeper than the current top, this opens a new level.
        if orig_indent > *indent_stack.last().unwrap() {
            indent_stack.push(orig_indent);
        }
        let depth = indent_stack.len() - 1;

        flush_blank(&mut out_lines, &mut pending_blank, &mut emitted_any);
        out_lines.push(format!("{}{}", " ".repeat(INDENT_WIDTH * depth), trimmed));
        emitted_any = true;

        // Update bracket depth from this logical line's *code* (ignoring its
        // trailing comment, which can contain stray brackets).
        bracket_depth += net_bracket_delta(code_part(content));
        if bracket_depth < 0 {
            bracket_depth = 0;
        }
    }

    // Single trailing newline, no trailing blank lines.
    let mut result = out_lines.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    result
}

fn flush_blank(out: &mut Vec<String>, pending: &mut bool, emitted_any: &mut bool) {
    if *pending && *emitted_any {
        out.push(String::new());
    }
    *pending = false;
    *emitted_any = true;
}

fn leading_ws_width(s: &str) -> usize {
    let mut w = 0;
    for c in s.chars() {
        match c {
            ' ' => w += 1,
            '\t' => w += 4,
            _ => break,
        }
    }
    w
}

/// The code portion of a line: everything before a `--` that is outside a
/// string literal. Mirrors the lexer's comment rule closely enough for bracket
/// counting (we only need to avoid counting brackets inside trailing comments).
fn code_part(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_str = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == b'"' {
            in_str = true;
            i += 1;
            continue;
        }
        if c == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            return &line[..i];
        }
        i += 1;
    }
    line
}

/// Net change in bracket nesting contributed by a code fragment, counting
/// `(`, `{`, `[`, and the seq delimiters `⟨`/`⟩` — and skipping string
/// literals. Mirrors the lexer's `paren_depth` accounting.
fn net_bracket_delta(code: &str) -> i32 {
    let mut depth = 0i32;
    let mut chars = code.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            if c == '\\' {
                chars.next();
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' | '{' | '[' | '\u{27E8}' => depth += 1,
            ')' | '}' | ']' | '\u{27E9}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// Two token streams are *equivalent* for formatting purposes iff their "real"
/// (non-`Newline`, non-`Indent`) token sequences are identical AND their
/// structural signatures (relative indent depths interleaved with line breaks)
/// match. The real-token check guarantees no token was added, dropped, merged,
/// split, or re-valued; the structure check guarantees no two blocks were
/// merged and no line moved to a different nesting level.
fn tokens_equivalent(a: &[Token], b: &[Token]) -> bool {
    real_tokens(a) == real_tokens(b) && structure_signature(a) == structure_signature(b)
}

/// The "real" tokens: everything except `Newline` and `Indent`.
fn real_tokens(toks: &[Token]) -> Vec<&Token> {
    toks.iter()
        .filter(|t| !matches!(t, Token::Newline | Token::Indent(_)))
        .collect()
}

/// A structural signature: the sequence of `Indent` widths and `Newline`
/// markers, with the indent widths rank-normalized so only their *ordering*
/// (not absolute size) matters. `reindent` preserves that ordering, so a stable
/// signature is exactly the proof that block structure is unchanged.
fn structure_signature(toks: &[Token]) -> Vec<i64> {
    let mut widths: Vec<i64> = Vec::new();
    for t in toks {
        match t {
            Token::Indent(n) => widths.push(*n as i64),
            Token::Newline => widths.push(-1),
            _ => {}
        }
    }
    rank_normalize(&widths)
}

/// Replace each non-sentinel value by its rank among the distinct non-sentinel
/// values (ascending), leaving sentinels (`-1`) in place.
fn rank_normalize(vals: &[i64]) -> Vec<i64> {
    let mut distinct: Vec<i64> = vals.iter().copied().filter(|&v| v >= 0).collect();
    distinct.sort_unstable();
    distinct.dedup();
    vals.iter()
        .map(|&v| {
            if v < 0 {
                -1
            } else {
                distinct.binary_search(&v).unwrap() as i64
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrips(src: &str) {
        let out = format_source(src).expect("format ok");
        // Token-equivalent to input.
        let a = tokenize(src).unwrap();
        let b = tokenize(&out).unwrap();
        assert!(
            tokens_equivalent(&a, &b),
            "not token-equivalent.\n--- in ---\n{src}\n--- out ---\n{out}"
        );
        // Idempotent.
        let out2 = format_source(&out).expect("format ok 2");
        assert_eq!(out, out2, "not idempotent");
        // Ends with exactly one newline (unless empty).
        if !out.is_empty() {
            assert!(out.ends_with('\n'));
            assert!(!out.ends_with("\n\n"));
        }
    }

    #[test]
    fn fixes_indentation_to_four() {
        let src = "claim foo\n  x ∈ Int\n  x = 5\n";
        let out = format_source(src).unwrap();
        assert_eq!(out, "claim foo\n    x ∈ Int\n    x = 5\n");
        roundtrips(src);
    }

    #[test]
    fn trims_trailing_whitespace() {
        let src = "claim foo   \n    x ∈ Int\t\n";
        let out = format_source(src).unwrap();
        assert_eq!(out, "claim foo\n    x ∈ Int\n");
    }

    #[test]
    fn collapses_blank_runs_and_strips_edges() {
        let src = "\n\nclaim foo\n    x ∈ Int\n\n\n\nclaim bar\n    y ∈ Int\n\n\n";
        let out = format_source(src).unwrap();
        assert_eq!(out, "claim foo\n    x ∈ Int\n\nclaim bar\n    y ∈ Int\n");
    }

    #[test]
    fn preserves_comments() {
        let src = "-- header\nclaim foo\n    x ∈ Int   -- a field\n";
        let out = format_source(src).unwrap();
        assert!(out.contains("-- header"));
        assert!(out.contains("-- a field"));
        roundtrips(src);
    }

    #[test]
    fn nested_indentation() {
        let src = "claim foo\n        a ∈ Int\n        ¬b ⇒\n                a = 1\n";
        let out = format_source(src).unwrap();
        assert_eq!(out, "claim foo\n    a ∈ Int\n    ¬b ⇒\n        a = 1\n");
        roundtrips(src);
    }

    #[test]
    fn multiline_bracket_continuation_preserved() {
        // A seq literal split across lines is one logical line (bracket open).
        let src = "claim foo\n    xs ∈ Seq(Int)\n    effects = ⟨a,\n      b,\n      c⟩\n";
        roundtrips(src);
    }

    #[test]
    fn match_arms_roundtrip() {
        let src =
            "claim foo\n    effects = match state\n        Falling ⇒ a\n        Landed  ⇒ b\n";
        roundtrips(src);
    }

    #[test]
    fn empty_input() {
        assert_eq!(format_source("").unwrap(), "");
        assert_eq!(format_source("\n\n\n").unwrap(), "");
    }

    #[test]
    fn already_formatted_is_fixed_point() {
        let src =
            "import \"stdlib/runtime.ev\"\n\nfsm gravity\n    x ∈ Int\n    is_first_tick ⇒ x = 10\n";
        let out = format_source(src).unwrap();
        assert_eq!(out, src);
    }
}
