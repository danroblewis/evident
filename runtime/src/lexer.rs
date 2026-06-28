/// One segment inside an f-string literal.
#[derive(Debug, Clone, PartialEq)]
pub enum FStrPart {
    /// A run of literal characters between (or around) interpolations.
    Lit(String),
    /// The raw source text of a `{expr}` interpolation (without braces).
    Interp(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {

    Ident(String),
    Int(i64),
    Real(f64),
    Str(String),
    /// f"..." format-string literal: alternating literal/interp parts.
    FStr(Vec<FStrPart>),
    True,
    False,

    Schema,
    Claim,
    Type,
    Subclaim,
    Operator,
    Fsm,
    Fti,
    External,
    Enum,
    Match,
    Matches,
    Import,
    In,
    NotIn,
    ContainsRev,

    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    PlusPlus,
    Minus,
    Star,
    Slash,
    MidDot,
    Times,

    And,
    Or,
    Not,
    Implies,
    Delta,

    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LSeq,
    RSeq,
    Hash,
    Comma,
    Pipe,
    Question,
    DotDot,
    Dot,
    Colon,
    ColonEq,
    ForAll,
    Exists,
    MapsTo,

    Newline,

    Indent(usize),

    Eof,
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "lex error at line {}, col {}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for LexError {}

/// Does the token stream end on an infix operator that REQUIRES a right operand? If so a
/// trailing newline is a line CONTINUATION, not a statement terminator — so `… ++` ⏎ `…` lexes
/// as one expression. Limited to ARITHMETIC and LOGICAL infix operators, which unambiguously
/// need a right operand and have NO block form. Deliberately EXCLUDED: `=` (and comparisons /
/// `↦`) — `=` ends the head of a multi-line `enum X =` ⏎ <variants> / `type X =` and a
/// chained-membership decl, where the trailing newline is load-bearing; `⇒`/`:` have block
/// forms too (implies-block, ternary). `¬` is prefix, and `,`/brackets are handled by paren
/// depth — none belong here.
fn ends_with_continuation_op(tokens: &[Token]) -> bool {
    matches!(
        tokens.last(),
        Some(
            Token::PlusPlus | Token::Plus | Token::Minus | Token::Star | Token::Slash
                | Token::MidDot | Token::Times | Token::And | Token::Or
        )
    )
}

/// Tokenize, dropping position info. Thin wrapper over [`tokenize_with_locs`].
pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    tokenize_with_locs(src).map(|(toks, _locs)| toks)
}

/// Tokenize, returning a `(line, col)` position parallel to each token
/// (same index; the trailing `Eof` carries the final position). The parser
/// uses these to stamp parse errors with the offending token's location.
pub fn tokenize_with_locs(src: &str) -> Result<(Vec<Token>, Vec<(usize, usize)>), LexError> {
    let mut tokens = Vec::new();
    let mut locs: Vec<(usize, usize)> = Vec::new();
    let mut chars = src.chars().peekable();
    let mut line = 1usize;
    let mut col = 1usize;

    let mut at_line_start = true;
    let mut current_indent;

    let mut paren_depth: usize = 0;

    // After each scan step we backfill `locs` so it stays index-parallel with
    // `tokens`: the position recorded is where that token *started*.
    macro_rules! sync_locs {
        ($pos:expr) => {
            while locs.len() < tokens.len() {
                locs.push($pos);
            }
        };
    }

    while let Some(&c) = chars.peek() {
        let tok_start = (line, col);
        if at_line_start {

            current_indent = 0;
            while let Some(&ch) = chars.peek() {
                match ch {
                    ' '  => { chars.next(); col += 1; current_indent += 1; }
                    '\t' => { chars.next(); col += 1; current_indent += 4; }
                    _    => break,
                }
            }

            if let Some(&ch) = chars.peek() {
                if ch == '\n' {
                    chars.next(); line += 1; col = 1;
                    at_line_start = true;
                    continue;
                }
                if ch == '-' {

                    let mut clone = chars.clone();
                    clone.next();
                    if clone.peek() == Some(&'-') {

                        while let Some(&ch) = chars.peek() {
                            if ch == '\n' { break; }
                            chars.next(); col += 1;
                        }
                        continue;
                    }
                }
            } else {

                break;
            }
            tokens.push(Token::Indent(current_indent));
            sync_locs!(tok_start);
            at_line_start = false;
            continue;
        }

        match c {
            ' ' | '\t' => { chars.next(); col += 1; }
            '\n' => {
                chars.next();
                line += 1; col = 1;
                // A line ending on an infix operator that REQUIRES a right operand (`++`, `+`,
                // `∧`, a comparison, …) continues onto the next line — suppress the Newline so a
                // multi-line `"a" ++ "b" ++` ⏎ `"c"` lexes as one expression. `⇒` is deliberately
                // NOT in the set: `cond ⇒` ⏎ <indent> is the valid implies-block form.
                if paren_depth == 0 && !ends_with_continuation_op(&tokens) {
                    tokens.push(Token::Newline);
                    at_line_start = true;
                }

            }
            '-' => {

                let mut clone = chars.clone();
                clone.next();
                if clone.peek() == Some(&'-') {

                    while let Some(&ch) = chars.peek() {
                        if ch == '\n' { break; }
                        chars.next(); col += 1;
                    }
                } else {
                    chars.next(); col += 1;
                    tokens.push(Token::Minus);
                }
            }
            '"' => {

                chars.next(); col += 1;
                let mut s = String::new();
                loop {
                    match chars.peek().copied() {
                        Some('"') => { chars.next(); col += 1; break; }
                        Some('\\') => {
                            chars.next(); col += 1;
                            match chars.peek().copied() {
                                Some('"')  => { s.push('"');  chars.next(); col += 1; }
                                Some('\\') => { s.push('\\'); chars.next(); col += 1; }
                                Some('n')  => { s.push('\n'); chars.next(); col += 1; }
                                Some('t')  => { s.push('\t'); chars.next(); col += 1; }
                                Some(c)    => return Err(LexError {
                                    message: format!("unknown escape \\{}", c), line, col }),
                                None       => return Err(LexError {
                                    message: "unterminated string escape".into(), line, col }),
                            }
                        }
                        Some('\n') => return Err(LexError {
                            message: "unterminated string literal".into(), line, col }),
                        Some(c) => { s.push(c); chars.next(); col += 1; }
                        None => return Err(LexError {
                            message: "unterminated string at EOF".into(), line, col }),
                    }
                }
                tokens.push(Token::Str(s));
            }
            '0'..='9' => {
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_digit() {
                        s.push(ch);
                        chars.next(); col += 1;
                    } else { break; }
                }

                if chars.peek() == Some(&'.') {
                    let mut clone = chars.clone();
                    clone.next();
                    if matches!(clone.peek(), Some(c) if c.is_ascii_digit()) {
                        chars.next(); col += 1;
                        s.push('.');
                        while let Some(&ch) = chars.peek() {
                            if ch.is_ascii_digit() {
                                s.push(ch);
                                chars.next(); col += 1;
                            } else { break; }
                        }
                        let v: f64 = s.parse().map_err(|e| LexError {
                            message: format!("invalid real {s:?}: {e}"),
                            line, col,
                        })?;
                        tokens.push(Token::Real(v));
                        continue;
                    }
                }
                let n: i64 = s.parse().map_err(|e| LexError {
                    message: format!("invalid integer {s:?}: {e}"),
                    line, col,
                })?;
                tokens.push(Token::Int(n));
            }
            'f' if { let mut clone = chars.clone(); clone.next(); clone.peek() == Some(&'"') } => {
                // f"..." format-string literal
                chars.next(); col += 1; // consume 'f'
                chars.next(); col += 1; // consume '"'
                let parts = lex_fstring(&mut chars, &mut line, &mut col)
                    .map_err(|e| LexError { message: e, line, col })?;
                tokens.push(Token::FStr(parts));
            }
            c if is_ident_start(c) => {
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if is_ident_continue(ch) {
                        s.push(ch);
                        chars.next(); col += 1;
                    } else { break; }
                }
                tokens.push(keyword_or_ident(s));
            }
            '+' => {
                chars.next(); col += 1;
                if chars.peek() == Some(&'+') {
                    chars.next(); col += 1;
                    tokens.push(Token::PlusPlus);
                } else {
                    tokens.push(Token::Plus);
                }
            }
            '*' => { chars.next(); col += 1; tokens.push(Token::Star); }
            '/' => { chars.next(); col += 1; tokens.push(Token::Slash); }
            '(' => { chars.next(); col += 1; tokens.push(Token::LParen);   paren_depth += 1; }
            ')' => { chars.next(); col += 1; tokens.push(Token::RParen);   paren_depth = paren_depth.saturating_sub(1); }
            '{' => { chars.next(); col += 1; tokens.push(Token::LBrace);   paren_depth += 1; }
            '}' => { chars.next(); col += 1; tokens.push(Token::RBrace);   paren_depth = paren_depth.saturating_sub(1); }
            '[' => { chars.next(); col += 1; tokens.push(Token::LBracket); paren_depth += 1; }
            ']' => { chars.next(); col += 1; tokens.push(Token::RBracket); paren_depth = paren_depth.saturating_sub(1); }
            '#' => { chars.next(); col += 1; tokens.push(Token::Hash); }
            ',' => { chars.next(); col += 1; tokens.push(Token::Comma); }
            ':' => {
                chars.next(); col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next(); col += 1;
                    tokens.push(Token::ColonEq);
                } else {
                    tokens.push(Token::Colon);
                }
            }
            '.' => {
                chars.next(); col += 1;
                if chars.peek() == Some(&'.') {
                    chars.next(); col += 1;
                    tokens.push(Token::DotDot);
                } else {
                    tokens.push(Token::Dot);
                }
            }
            '=' => {
                chars.next(); col += 1;

                if chars.peek() == Some(&'>') {
                    chars.next(); col += 1;
                    tokens.push(Token::Implies);
                } else {
                    tokens.push(Token::Eq);
                }
            }
            '<' => {
                chars.next(); col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next(); col += 1;
                    tokens.push(Token::Le);
                } else {
                    tokens.push(Token::Lt);
                }
            }
            '>' => {
                chars.next(); col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next(); col += 1;
                    tokens.push(Token::Ge);
                } else {
                    tokens.push(Token::Gt);
                }
            }
            '!' => {
                chars.next(); col += 1;
                if chars.peek() == Some(&'=') {
                    chars.next(); col += 1;
                    tokens.push(Token::Neq);
                } else {
                    return Err(LexError { message: "unexpected '!'".into(), line, col });
                }
            }
            '\u{2208}' => { chars.next(); col += 1; tokens.push(Token::In); }
            '\u{2209}' => { chars.next(); col += 1; tokens.push(Token::NotIn); }
            '\u{220B}' => { chars.next(); col += 1; tokens.push(Token::ContainsRev); }
            '\u{2227}' => { chars.next(); col += 1; tokens.push(Token::And); }
            '\u{2228}' => { chars.next(); col += 1; tokens.push(Token::Or); }
            '\u{00AC}' => { chars.next(); col += 1; tokens.push(Token::Not); }
            '\u{21D2}' => { chars.next(); col += 1; tokens.push(Token::Implies); }
            '\u{0394}' => { chars.next(); col += 1; tokens.push(Token::Delta); }
            '\u{2264}' => { chars.next(); col += 1; tokens.push(Token::Le); }
            '\u{2265}' => { chars.next(); col += 1; tokens.push(Token::Ge); }
            '\u{2260}' => { chars.next(); col += 1; tokens.push(Token::Neq); }
            '\u{2200}' => { chars.next(); col += 1; tokens.push(Token::ForAll); }
            '\u{2203}' => { chars.next(); col += 1; tokens.push(Token::Exists); }
            '\u{21A6}' => { chars.next(); col += 1; tokens.push(Token::MapsTo); }
            '\u{00B7}' => { chars.next(); col += 1; tokens.push(Token::MidDot); }
            '\u{00D7}' => { chars.next(); col += 1; tokens.push(Token::Times); }
            '\u{27E8}' => { chars.next(); col += 1; tokens.push(Token::LSeq); paren_depth += 1; }
            '\u{27E9}' => { chars.next(); col += 1; tokens.push(Token::RSeq); paren_depth = paren_depth.saturating_sub(1); }
            '|' => { chars.next(); col += 1; tokens.push(Token::Pipe); }
            '?' => { chars.next(); col += 1; tokens.push(Token::Question); }
            other => {
                return Err(LexError {
                    message: format!("unexpected character {:?}", other),
                    line, col,
                });
            }
        }
        sync_locs!(tok_start);
    }

    tokens.push(Token::Eof);
    sync_locs!((line, col));
    Ok((tokens, locs))
}

/// Lex the body of an f-string after the opening `f"` has been consumed.
/// Handles `{{`/`}}` as escaped literal braces, normal string escapes in
/// literal runs, and `{expr}` interpolation spans. Returns the alternating
/// parts list, or an error message string on malformed input.
fn lex_fstring(
    chars: &mut std::iter::Peekable<std::str::Chars>,
    _line: &mut usize,
    col: &mut usize,
) -> Result<Vec<FStrPart>, String> {
    let mut parts: Vec<FStrPart> = Vec::new();
    let mut lit = String::new();

    loop {
        match chars.peek().copied() {
            None => return Err("unterminated f-string at EOF".into()),
            Some('\n') => return Err("unterminated f-string literal".into()),
            Some('"') => {
                chars.next(); *col += 1;
                // flush any trailing literal
                if !lit.is_empty() {
                    parts.push(FStrPart::Lit(std::mem::take(&mut lit)));
                }
                return Ok(parts);
            }
            Some('{') => {
                chars.next(); *col += 1;
                if chars.peek() == Some(&'{') {
                    // `{{` → escaped literal `{`
                    chars.next(); *col += 1;
                    lit.push('{');
                } else {
                    // start of an interpolation — collect until matching `}`
                    // flush literal so far
                    if !lit.is_empty() {
                        parts.push(FStrPart::Lit(std::mem::take(&mut lit)));
                    }
                    let mut interp = String::new();
                    let mut depth: usize = 1;
                    loop {
                        match chars.peek().copied() {
                            None => return Err("unterminated f-string interpolation at EOF".into()),
                            Some('\n') => return Err("unterminated f-string interpolation".into()),
                            Some('{') => { depth += 1; interp.push('{'); chars.next(); *col += 1; }
                            Some('}') => {
                                chars.next(); *col += 1;
                                depth -= 1;
                                if depth == 0 { break; }
                                interp.push('}');
                            }
                            Some(c) => { interp.push(c); chars.next(); *col += 1; }
                        }
                    }
                    let s = interp.trim().to_string();
                    if s.is_empty() {
                        return Err("empty interpolation `{}` in f-string".into());
                    }
                    parts.push(FStrPart::Interp(s));
                }
            }
            Some('}') => {
                chars.next(); *col += 1;
                if chars.peek() == Some(&'}') {
                    // `}}` → escaped literal `}`
                    chars.next(); *col += 1;
                    lit.push('}');
                } else {
                    return Err("unexpected `}` in f-string (use `}}` for a literal `}`)".into());
                }
            }
            Some('\\') => {
                chars.next(); *col += 1;
                match chars.peek().copied() {
                    Some('"')  => { lit.push('"');  chars.next(); *col += 1; }
                    Some('\\') => { lit.push('\\'); chars.next(); *col += 1; }
                    Some('n')  => { lit.push('\n'); chars.next(); *col += 1; }
                    Some('t')  => { lit.push('\t'); chars.next(); *col += 1; }
                    Some(c)    => return Err(format!("unknown escape \\{} in f-string", c)),
                    None       => return Err("unterminated escape in f-string".into()),
                }
            }
            Some(c) => {
                lit.push(c); chars.next(); *col += 1;
            }
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn keyword_or_ident(s: String) -> Token {
    match s.as_str() {
        "schema"   => Token::Schema,
        "claim"    => Token::Claim,
        "type"     => Token::Type,
        "subclaim" => Token::Subclaim,
        "operator" => Token::Operator,
        "fsm"      => Token::Fsm,
        "fti"      => Token::Fti,
        "external" => Token::External,
        "enum"     => Token::Enum,
        "match"    => Token::Match,
        "matches"  => Token::Matches,
        "import"   => Token::Import,
        "in"       => Token::In,
        "true"     => Token::True,
        "false"    => Token::False,
        "mapsto"   => Token::MapsTo,
        _ => Token::Ident(s),
    }
}

#[cfg(test)]
mod fstr_tests {
    use super::*;

    fn first_fstr(src: &str) -> Vec<FStrPart> {
        let toks = tokenize(src).unwrap();
        toks.into_iter().find_map(|t| match t {
            Token::FStr(parts) => Some(parts),
            _ => None,
        }).expect("no FStr token found")
    }

    #[test]
    fn test_fstr_literal_only() {
        let parts = first_fstr(r#"f"hello""#);
        assert_eq!(parts, vec![FStrPart::Lit("hello".into())]);
    }

    #[test]
    fn test_fstr_interp_only() {
        let parts = first_fstr(r#"f"{a}""#);
        assert_eq!(parts, vec![FStrPart::Interp("a".into())]);
    }

    #[test]
    fn test_fstr_lit_then_interp() {
        let parts = first_fstr(r#"f"v={a}""#);
        assert_eq!(parts, vec![
            FStrPart::Lit("v=".into()),
            FStrPart::Interp("a".into()),
        ]);
    }

    #[test]
    fn test_fstr_escaped_braces() {
        let parts = first_fstr(r#"f"{{lit}}""#);
        assert_eq!(parts, vec![FStrPart::Lit("{lit}".into())]);
    }

    #[test]
    fn test_fstr_empty() {
        let parts = first_fstr(r#"f"""#);
        assert_eq!(parts, vec![]);
    }

    #[test]
    fn test_fstr_multi_interp() {
        let parts = first_fstr(r#"f"{a}-{b}""#);
        assert_eq!(parts, vec![
            FStrPart::Interp("a".into()),
            FStrPart::Lit("-".into()),
            FStrPart::Interp("b".into()),
        ]);
    }
}
