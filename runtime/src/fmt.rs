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
//!
//! ## Repairing ragged sibling indentation (#35 / #53)
//!
//! A *layout* formatter's whole job is to fix indentation — including the
//! common mistake of typing two sibling lines (two decls, a decl and a guard,
//! two `⇒`-guards) at different columns. The parser groups a block's members by
//! an exact-match indent column, so such *ragged* input has no parse on its own.
//! A naive indentation-ordering re-indent can't repair it — worse, it would
//! preserve the mistake as *fake nesting*: an over-indented sibling comes out
//! nested as a child of the line above it (token-equivalent, yet a structurally
//! different program — the worst #53 case).
//!
//! [`reindent`] therefore carries minimal **structural** awareness. It mirrors
//! the parser's block model: a line opens an indented child block only if it is
//! a *block opener* — a decl/`subclaim` header, a trailing-`⇒` (implies-block),
//! a trailing-`:` quantifier, or a `match` line. A deeper-indented line whose
//! predecessor is NOT an opener is a ragged sibling, and gets snapped back to
//! its block's column rather than mis-nested under the line above. This lets
//! `fmt` *repair* realistic messy-but-intended input to a uniform 4-space grid
//! instead of refusing or fake-nesting it.
//!
//! The anti-corruption guarantee is unchanged: [`format_source`] still proves
//! its output via the token oracle (every real token preserved, in order, same
//! value) AND a final re-parse. For input that already parses, it additionally
//! pins the block structure unchanged. For ragged input — which has no raw
//! parse to pin against — it requires the *repaired* output to parse, so the
//! snap is verified to produce a real program with the author's exact tokens.
//! `fmt` only refuses when the input is *un-repairable* (genuinely malformed:
//! a token-level syntax error the re-indent can't be responsible for).

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

    // Does the *input* already parse? If so, formatting must be structure-
    // preserving (the strict gofmt guarantee). If not, the usual cause is
    // ragged sibling indentation — and a layout formatter's job is to REPAIR
    // that, not refuse it. The structurally-aware `reindent` snaps ragged
    // siblings to a common column; we then verify the repaired output parses
    // and carries the author's exact tokens (below). We only refuse when the
    // input is genuinely malformed in a way the re-indent can't repair.
    let input_parsed = crate::parser::parse(src).is_ok();

    let formatted = reindent(src);

    // Self-check: the formatted text must lex to a token stream that preserves
    // every *real* token (no identifier/operator/literal added, dropped, merged,
    // split, or re-valued). This is the load-bearing anti-corruption guard — it
    // holds for both the structure-preserving path and the ragged-repair path.
    let new_tokens = tokenize(&formatted)
        .map_err(|e| format!("internal: formatted output failed to lex: {e}"))?;
    if real_tokens(&orig_tokens) != real_tokens(&new_tokens) {
        return Err("internal: re-indent perturbed the token stream — refusing \
                    to emit. This is a formatter bug, please report it. File \
                    left unchanged."
            .to_string());
    }

    if input_parsed {
        // The input was a valid program: formatting it must not change its
        // block structure. A clean re-indent only rescales `Indent` widths,
        // preserving every relative comparison the parser keys off — so the
        // structural signature is stable. If it ever isn't, that's a formatter
        // bug; refuse rather than risk emitting a structurally different program.
        if structure_signature(&orig_tokens) != structure_signature(&new_tokens) {
            return Err("internal: re-indent changed the block structure of a \
                        valid program — refusing to emit. This is a formatter \
                        bug, please report it. File left unchanged."
                .to_string());
        }
    }

    // The formatted output must parse. For valid input this is belt-and-
    // suspenders; for ragged-but-repairable input it is the proof that the
    // structural snap produced a real program (combined with the real-token
    // check above, the author's tokens in the author's structure). If even the
    // repaired output won't parse, the source was malformed beyond what
    // re-indentation can fix — surface that and leave the file untouched.
    crate::parser::parse(&formatted).map_err(|e| {
        if input_parsed {
            format!(
                "internal: formatted output failed to parse — refusing to emit. \
                 This is a formatter bug, please report it.\n  {e}"
            )
        } else {
            format!(
                "refusing to format: the source does not parse even after \
                 re-indentation, so it is malformed beyond a layout fix (a \
                 token-level syntax error, not just ragged indentation). Fix the \
                 source and re-run. File left unchanged.\n  {e}"
            )
        }
    })?;

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

/// One entry in the nesting stack: the source column at which this depth's
/// block begins, plus whether the line that *opened* this depth is a block
/// opener (so a deeper child under it is legitimate).
struct Level {
    /// Original source column at which depth `d` begins.
    col: usize,
    /// Did the most-recent logical line at this depth open an indented block?
    /// Only when this is true may the *next* deeper line legitimately descend.
    prev_opens_block: bool,
}

/// Re-derive indentation structurally and clean up whitespace.
///
/// Strategy: walk physical lines, tracking a nesting stack. Unlike a pure
/// indentation-ordering re-indent, this mirrors the parser's block model so it
/// can *repair* ragged sibling indentation rather than mis-nesting it.
///
/// The parser opens an indented child block only after a *block opener* — a
/// decl/`subclaim` header, a trailing-`⇒` (implies-block), a trailing-`:`
/// quantifier, or a `match` line ([`opens_block`]). So a line indented deeper
/// than the current block's column descends to a new child level ONLY when the
/// preceding logical line was an opener. A line that is deeper but whose
/// predecessor is a *leaf* (a decl, a single-line `is_first_tick ⇒ x = 0`
/// guard, a constraint) is a ragged over-indented sibling: we snap it back to
/// the current block's column instead of fake-nesting it under the line above.
/// This is the #35 fix — siblings at one logical level land in one column.
///
/// Continuation lines (physical lines that start while a bracket/paren/seq is
/// still open) are *not* logical-line starts: the lexer emits no `Indent` for
/// them, so their leading whitespace is invisible to the parser. We deliberately
/// leave the author's alignment on those lines untouched (trimming only trailing
/// whitespace) — hand-aligned multi-line seq/ternary bodies read better than any
/// mechanical rule we could impose, and touching them risks nothing but loses
/// intent.
fn reindent(src: &str) -> String {
    let phys: Vec<&str> = src.split('\n').collect();
    let mut out_lines: Vec<String> = Vec::new();

    // Nesting stack. stack[0] is depth 0 at column 0. Each frame records the
    // source column its block begins at, and whether the last line seen at that
    // depth was a block opener (gate for descending into a child).
    let mut stack: Vec<Level> = vec![Level { col: 0, prev_opens_block: true }];
    // Bracket nesting carried across physical lines (paren/brace/bracket/seq).
    let mut bracket_depth: i32 = 0;
    // Pending blank lines, so we can collapse runs and drop leading/trailing.
    let mut pending_blank = false;
    let mut emitted_any = false;

    for (i, raw) in phys.iter().enumerate() {
        let line = *raw;
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
        // onto the nesting stack with structural awareness.
        let orig_indent = leading_ws_width(content);
        let trimmed = content.trim_start();
        let code = code_part(content);
        // A full-line comment is never a block opener and carries no code.
        let is_comment = matches!(classify(content), LineKind::Comment);
        // Whether this logical line opens a child block is decided from the
        // WHOLE logical line — head plus any bracket-continuation physical
        // lines — because a `⇒` / `:` that opens a block can land on a
        // continuation line (e.g. a guard whose condition spans two lines).
        // Looking only at the head would misread such an opener as a leaf and
        // pull its genuine child up a level.
        let this_opens = !is_comment && logical_line_opens_block(&phys, i);

        // Pop levels whose block column is deeper than this line — this line
        // belongs to an ancestor block.
        while stack.len() > 1 && stack.last().unwrap().col > orig_indent {
            stack.pop();
        }
        // Strictly deeper than the current block column: descend to a child
        // level ONLY if the previous line at this level opened a block.
        // Otherwise this is a ragged over-indented sibling — snap it to the
        // current block's column (no descent).
        if orig_indent > stack.last().unwrap().col {
            if stack.last().unwrap().prev_opens_block {
                stack.push(Level { col: orig_indent, prev_opens_block: this_opens });
            } else {
                // Ragged sibling: stay at the current depth, record whether THIS
                // line opens a block so a genuine child after it can descend.
                stack.last_mut().unwrap().prev_opens_block = this_opens;
            }
        } else {
            // Same column (or shallower, after popping): a sibling at this
            // depth. Update the opener flag for the next line's descent test.
            stack.last_mut().unwrap().prev_opens_block = this_opens;
        }
        let depth = stack.len() - 1;

        flush_blank(&mut out_lines, &mut pending_blank, &mut emitted_any);
        let body = respace_line(trimmed);
        out_lines.push(format!("{}{}", " ".repeat(INDENT_WIDTH * depth), body));
        emitted_any = true;

        // Update bracket depth from this logical line's *code* (ignoring its
        // trailing comment, which can contain stray brackets).
        bracket_depth += net_bracket_delta(code);
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

/// Normalize *operator spacing* on a single logical line's text, leaving its
/// trailing comment verbatim. This is the token-spacing half of `fmt` (#477):
/// a single space around binary operators (`∈ := = == ? : < > ≤ ≥ ≠ + - * /
/// ∧ ∨ ⇒ ⟸ ++ ↦ …`), and no inner space after the prefix operators `Δ` / `¬`
/// or inside `f(x)` call parens.
///
/// It works by tokenizing the *code part* (comment stripped) and re-emitting
/// the tokens with a spacing table — but it never *reconstructs* a token's
/// text from the enum (that would lose the exact form of identifiers, string
/// literals, and reals). Instead it slices each token's original source span
/// (recovered from the lexer's per-token column) and only adjusts the
/// whitespace *between* tokens. String/comment content is therefore byte-exact.
///
/// Soundness: any tokenize failure, or a token whose source span can't be
/// recovered, makes this return the line *unchanged*. The caller's
/// anti-corruption check ([`format_source`]) re-lexes the whole output and
/// rejects it if a single real token was perturbed, so even a logic bug here
/// can only cause a silent no-op, never a corruption.
fn respace_line(line: &str) -> String {
    // A full-line comment carries no code to respace.
    if line.trim_start().starts_with("--") {
        return line.to_string();
    }
    let code = code_part(line);
    let comment = &line[code.len()..]; // "" or "  -- …" (leading ws + comment)

    let respaced = match respace_code(code) {
        Some(s) => s,
        None => code.trim_end().to_string(),
    };

    // Reattach the trailing comment with a single normalizing space before it
    // (only if there is code before it; a leading-`--` line was handled above).
    let comment = comment.trim_start();
    if comment.is_empty() {
        respaced
    } else if respaced.is_empty() {
        comment.to_string()
    } else {
        format!("{respaced}  {comment}")
    }
}

/// Re-emit a comment-free code fragment with normalized operator spacing, or
/// `None` if it can't be tokenized / sliced (caller falls back to verbatim).
fn respace_code(code: &str) -> Option<String> {
    let (toks, locs) = crate::lexer::tokenize_with_locs(code).ok()?;
    // Drop the structural markers — a single-line fragment has none we want.
    let chars: Vec<char> = code.chars().collect();
    // Recover each token's exact source text by slicing [start_col .. next_start]
    // and trimming whitespace. Columns are 1-based char offsets on this line.
    let real: Vec<(usize, &Token)> = toks
        .iter()
        .enumerate()
        .filter(|(_, t)| !matches!(t, Token::Newline | Token::Indent(_) | Token::Eof))
        .map(|(i, t)| (i, t))
        .collect();

    let mut texts: Vec<String> = Vec::with_capacity(real.len());
    for (k, (i, _tok)) in real.iter().enumerate() {
        let start = locs[*i].1.saturating_sub(1); // 1-based col → 0-based char idx
        // End is the start of the next real token (or end-of-line), trimmed.
        let end = real
            .get(k + 1)
            .map(|(j, _)| locs[*j].1.saturating_sub(1))
            .unwrap_or(chars.len());
        if start > chars.len() || end > chars.len() || start > end {
            return None;
        }
        let slice: String = chars[start..end].iter().collect();
        texts.push(slice.trim_end().to_string());
    }

    let kinds: Vec<&Token> = real.iter().map(|(_, t)| *t).collect();
    Some(emit_with_spacing(&kinds, &texts))
}

/// Given the real tokens and each token's exact source text, join them with
/// normalized spacing. The rule set:
///   * binary operators get a space on each side;
///   * `Δ` / `¬` are prefix — no space after;
///   * a `-` that begins an expression (after an operator / open bracket /
///     comma / nothing) is unary — no space after; otherwise it's binary;
///   * `(`/`[`/`⟨` openers and `)`/`]`/`⟩` closers, `#`, `.`, `..`, postfix —
///     hug their neighbour;
///   * `,` hugs its left, space on its right.
fn emit_with_spacing(kinds: &[&Token], texts: &[String]) -> String {
    let mut out = String::new();
    for k in 0..kinds.len() {
        let tok = kinds[k];
        let prev = if k > 0 { Some(kinds[k - 1]) } else { None };

        let space_before = if k == 0 {
            false
        } else {
            wants_space_between(prev.unwrap(), tok, kinds, k)
        };
        if space_before {
            out.push(' ');
        }
        out.push_str(&texts[k]);
    }
    out
}

/// Should there be a space between the token ending the run so far (`prev`) and
/// the token about to be emitted (`cur`, at index `k`)? `kinds` is the full run
/// so a `-`/`+`'s unary-vs-binary nature can be read from what precedes it.
fn wants_space_between(prev: &Token, cur: &Token, kinds: &[&Token], k: usize) -> bool {
    use Token::*;

    // Hug tight after an opener and before a closer / comma — no space.
    if matches!(prev, LParen | LBracket | LSeq | LBrace | Hash) {
        return false;
    }
    if matches!(cur, RParen | RBracket | RSeq | RBrace | Comma) {
        return false;
    }
    // `.` / `..` field/range access hugs both sides: a.b , {0..n}.
    if matches!(prev, Dot | DotDot) || matches!(cur, Dot | DotDot) {
        return false;
    }
    // Prefix `Δ` / `¬` bind to the following name: Δcount, ¬cond — no space after.
    if matches!(prev, Delta | Not) {
        return false;
    }
    // A call/index immediately after a value: `f` `(` , `xs` `[` — no space, it's
    // application. After an operator/keyword, `(` starts a grouping and DOES get a
    // space (`= (a + b)`), which the operator's own right-space rule below covers.
    if matches!(cur, LParen | LBracket) && is_value_end(prev) {
        return false;
    }
    // Unary minus / plus. A `-`/`+` whose left neighbour is NOT a value end is a
    // sign on its operand (`:= -5`, `(-x)`, `, -3`), not a binary operator. The
    // sign itself is spaced from what precedes it (handled by falling through to
    // the default), but it must HUG its operand: when `prev` is that unary sign,
    // emit no space before the operand.
    if matches!(prev, Minus | Plus) && !is_value_end_lookback(kinds, k - 1) {
        return false;
    }

    // Everything else: if either side is an operator/keyword that takes spacing,
    // emit a single space. Two adjacent value tokens (rare: a keyword followed by
    // an identifier like `match state`, `∀ x`) also get one space.
    true
}

/// Does this token end a *value* (so a following `-`/`+`/`(` is binary/applied,
/// not unary/grouping)? Values: identifiers, literals, and closers.
fn is_value_end(t: &Token) -> bool {
    use Token::*;
    matches!(
        t,
        Ident(_) | Int(_) | Real(_) | Str(_) | True | False
            | RParen | RBracket | RSeq | Hash
    )
}

/// Is the token at `idx` a *unary* sign? It's unary iff what precedes IT is not
/// a value end (or it's the first token). Used to decide whether the sign hugs
/// its operand (unary) or is spaced (binary) — symmetric with the emit logic.
fn is_value_end_lookback(kinds: &[&Token], idx: usize) -> bool {
    // `kinds[idx]` is a Minus/Plus. It is BINARY (a value-end for spacing of its
    // operand) iff the token before it ends a value.
    if idx == 0 {
        return false; // leading sign → unary
    }
    is_value_end(kinds[idx - 1])
}

/// Does the logical line *starting* at physical-line index `head` open an
/// indented child block? A logical line is the head line plus every following
/// continuation line while a bracket/paren/seq stays open. We join their code
/// (stripping trailing comments) and run [`opens_block`] on the whole thing,
/// because the block-opening `⇒` / `:` can appear on a continuation line (a
/// guard whose condition wraps across lines). Blank/comment physical lines
/// inside an open bracket are still part of the logical line and are skipped.
fn logical_line_opens_block(phys: &[&str], head: usize) -> bool {
    let mut joined = String::new();
    let mut bracket: i32 = 0;
    let mut i = head;
    while i < phys.len() {
        let content = phys[i].trim_end();
        // A full-line comment contributes no code and doesn't move brackets.
        if !matches!(classify(content), LineKind::Comment) {
            let code = code_part(content);
            if !joined.is_empty() {
                joined.push(' ');
            }
            joined.push_str(code);
            bracket += net_bracket_delta(code);
            if bracket < 0 {
                bracket = 0;
            }
        }
        if bracket == 0 {
            break;
        }
        i += 1;
    }
    opens_block(&joined)
}

/// Does this logical line's code open an indented child block, mirroring the
/// parser's block-opening rules? A line opens a block iff:
///   * it is a decl/`subclaim` header (first token is a decl keyword), or
///   * its last real token is `⇒` (an implies-block: `cond ⇒` + indented body), or
///   * its last real token is `:` (a quantifier: `∀ x ∈ s :` + indented body), or
///   * it contains a top-level `match` token (a `match scrutinee` + indented arms).
///
/// A leaf line — a decl, a single-line `cond ⇒ body` guard, a constraint, a
/// names-match invocation, a `..Mixin` — opens no block. We tokenize the code
/// (not the raw text) so `⇒`/`:`/`match` inside string literals don't fool us,
/// and a re-lex failure conservatively reports "not an opener".
fn opens_block(code: &str) -> bool {
    let toks = match tokenize(code) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let real: Vec<&Token> = toks
        .iter()
        .filter(|t| !matches!(t, Token::Newline | Token::Indent(_) | Token::Eof))
        .collect();
    if real.is_empty() {
        return false;
    }

    // Decl/subclaim header: a block always follows.
    if matches!(
        real[0],
        Token::Schema
            | Token::Claim
            | Token::Type
            | Token::Fsm
            | Token::Fti
            | Token::Enum
            | Token::Subclaim
            | Token::External
    ) {
        return true;
    }

    // A top-level `match scrutinee` line opens an indented arm block. `matches`
    // (the recognizer) lexes to a distinct token, so this is not fooled by it.
    if real.iter().any(|t| matches!(t, Token::Match)) {
        return true;
    }

    // Trailing `⇒` (implies-block) or trailing `:` (quantifier body). A `⇒`
    // with content after it on the same line is a single-line guard — a leaf,
    // not an opener — so only the *last* token counts.
    matches!(real.last().unwrap(), Token::Implies | Token::Colon)
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
///
/// `format_source` checks these two conditions separately (it relaxes the
/// structure half for ragged-repair input), so the combined predicate is now a
/// test-only convenience used by the round-trip harness.
#[cfg(test)]
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

    // #35, repair case 1: a sibling OVER-indented relative to a same-block `⇒`
    // line — `count` at 8 spaces, the guard at 4. These are siblings in the fsm
    // body (a decl + a single-line guard, neither of which opens a block). A
    // *layout* formatter must SNAP them to one column, not refuse. `count` was
    // never a block opener, so the deeper guard is a ragged sibling, not a
    // child: both land at 4 spaces.
    #[test]
    fn ragged_over_indent_snaps_to_sibling_column() {
        let src = "fsm counter\n        count ∈ Int\n    is_first_tick ⇒ count = 0\n";
        let out = format_source(src).unwrap();
        assert_eq!(
            out,
            "fsm counter\n    count ∈ Int\n    is_first_tick ⇒ count = 0\n"
        );
        // Idempotent fixed point.
        assert_eq!(format_source(&out).unwrap(), out);
    }

    // #35, repair case 2 (the worst #53 case): a sibling over-indented relative
    // to the line ABOVE — `count` at 2, the guard at 6. A pure indentation-
    // ordering re-indent would fake-nest the guard UNDER `count` (a single-line
    // `⇒ count = 0` is NOT a block opener, so it can have no child). The
    // structural re-indent recognizes `count ∈ Int` as a leaf and snaps the
    // guard back to the same column — NEVER fake-nesting it.
    #[test]
    fn ragged_sibling_not_silently_renested() {
        let src = "fsm counter\n  count ∈ Int\n      is_first_tick ⇒ count = 0\n";
        let out = format_source(src).unwrap();
        assert_eq!(
            out,
            "fsm counter\n    count ∈ Int\n    is_first_tick ⇒ count = 0\n"
        );
    }

    // The canonical CLAUDE.md `accumulate` multi-delta block, with its two
    // implies-block bodies over-indented one level AND a ragged delta sibling.
    // The `⇒` lines ARE openers, so their children stay nested (snapped to one
    // level under); the ragged Δsum snaps to match its sibling Δi.
    #[test]
    fn accumulate_multidelta_block_repaired() {
        let src = "fsm accumulate\n    i ∈ Int\n    sum ∈ Int\n    \
                   is_first_tick ⇒\n            i = 0\n            sum = 0\n    \
                   ¬ is_first_tick ⇒\n        Δi = (_i < 5 ? 1 : 0)\n              \
                   Δsum = (_i < 5 ? _i : 0)\n";
        let out = format_source(src).unwrap();
        // `¬` is a prefix operator — `respace` hugs it to its operand
        // (`¬is_first_tick`), normalizing the input's `¬ is_first_tick`.
        assert_eq!(
            out,
            "fsm accumulate\n    i ∈ Int\n    sum ∈ Int\n    is_first_tick ⇒\n        \
             i = 0\n        sum = 0\n    ¬is_first_tick ⇒\n        \
             Δi = (_i < 5 ? 1 : 0)\n        Δsum = (_i < 5 ? _i : 0)\n"
        );
        assert_eq!(format_source(&out).unwrap(), out, "not a fixed point");
    }

    // A `⇒` guard line that DOES open a block keeps its genuine children nested.
    // Distinguishing this from the single-line-guard leaf case is the whole #35
    // fix: openers nest, leaves snap.
    #[test]
    fn implies_block_children_stay_nested() {
        let src = "fsm c\n    is_first_tick ⇒\n        x = 0\n        y = 1\n";
        let out = format_source(src).unwrap();
        assert_eq!(out, src);
        roundtrips(src);
    }

    // Genuinely malformed input (a token-level syntax error, not ragged layout)
    // must still be refused — the re-indent can't manufacture a valid program.
    #[test]
    fn malformed_beyond_layout_is_refused() {
        let src = "fsm c\n    count ∈ Int = = 5\n";
        let err = format_source(src).expect_err("token-level syntax error must refuse");
        assert!(
            err.contains("malformed beyond a layout fix"),
            "expected a beyond-layout refusal, got: {err}"
        );
    }

    // The clean version of the repros — both lines at the same column — is a
    // valid program; `fmt` formats it to a 4-space grid and is a fixed point.
    #[test]
    fn aligned_siblings_format_cleanly() {
        let src = "fsm counter\n  count ∈ Int\n  is_first_tick ⇒ count = 0\n";
        let out = format_source(src).unwrap();
        assert_eq!(out, "fsm counter\n    count ∈ Int\n    is_first_tick ⇒ count = 0\n");
        roundtrips(src);
    }

    // #477: operator-spacing normalization. Ana's three cramped cases must come
    // out idiomatically spaced — AND the output must round-trip (same AST) and be
    // a fixed point.
    #[test]
    fn respaces_anas_cases() {
        let src = "claim c\n    count∈Int:=0\n    Δcount=(_count<5?1:0)\n    done∈Bool=(count≥5)\n";
        let out = format_source(src).unwrap();
        assert_eq!(
            out,
            "claim c\n    count ∈ Int := 0\n    Δcount = (_count < 5 ? 1 : 0)\n    done ∈ Bool = (count ≥ 5)\n"
        );
        roundtrips(src);
        // It actually CHANGED the bytes (not a silent no-op).
        assert_ne!(out, src);
    }

    // Negative literal: `-5` is a unary sign on a literal — it must stay `-5`,
    // NOT become `- 5`. Binary subtraction `a - 5` gets spaces.
    #[test]
    fn negative_literal_hugs_but_binary_minus_spaces() {
        let src = "claim c\n    x∈Int:=-5\n    y∈Int:=a-5\n    z∈Int:=(0-w)\n";
        let out = format_source(src).unwrap();
        assert_eq!(
            out,
            "claim c\n    x ∈ Int := -5\n    y ∈ Int := a - 5\n    z ∈ Int := (0 - w)\n"
        );
        roundtrips(src);
    }

    // Prefix operators Δ / ¬ / `_` bind to their name — no inner space.
    #[test]
    fn prefix_operators_hug_their_name() {
        let src = "fsm f\n    cond∈Bool\n    ¬cond⇒x=1\n    Δx=_x\n";
        let out = format_source(src).unwrap();
        assert_eq!(
            out,
            "fsm f\n    cond ∈ Bool\n    ¬cond ⇒ x = 1\n    Δx = _x\n"
        );
        roundtrips(src);
    }

    // Operators inside a string literal or a comment are content, not code — they
    // must NOT be respaced.
    #[test]
    fn strings_and_comments_are_verbatim() {
        let src = "claim c\n    s∈String:=\"a=b<c+d\"   -- keep x=y here\n";
        let out = format_source(src).unwrap();
        assert!(out.contains("\"a=b<c+d\""), "string content perturbed: {out}");
        assert!(out.contains("-- keep x=y here"), "comment perturbed: {out}");
        // The code outside the string IS spaced.
        assert!(out.contains("s ∈ String := "), "code not respaced: {out}");
        roundtrips(src);
    }

    // Call application `f(x)` and index `xs[i]` hug — no space before the paren,
    // none inside; but a grouping `(` after `=` is spaced.
    #[test]
    fn call_and_index_hug_grouping_spaces() {
        let src = "claim c\n    r∈Int:=f(a,b)+xs[0]\n";
        let out = format_source(src).unwrap();
        assert_eq!(out, "claim c\n    r ∈ Int := f(a, b) + xs[0]\n");
        roundtrips(src);
    }

    #[test]
    fn already_formatted_is_fixed_point() {
        let src =
            "import \"stdlib/runtime.ev\"\n\nfsm gravity\n    x ∈ Int\n    is_first_tick ⇒ x = 10\n";
        let out = format_source(src).unwrap();
        assert_eq!(out, src);
    }
}

