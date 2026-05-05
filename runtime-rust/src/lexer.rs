//! Tokenize Evident source. Handles the Unicode operators directly
//! (no separate normalization pass).
//!
//! Indentation is significant — every newline emits a `Newline` token,
//! and the parser tracks indent level by counting leading whitespace
//! on the next non-blank line. We don't emit explicit Indent/Dedent
//! tokens here; the parser handles indentation as part of statement
//! recognition.

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Identifiers and literals
    Ident(String),
    Int(i64),
    Str(String),
    True,
    False,

    // Keywords
    Schema,
    Claim,
    Type,
    Subclaim,
    In,           // ∈ or "in"

    // Operators
    Eq,           // =
    Neq,          // ≠ or "!="
    Lt,           // <
    Le,           // ≤ or "<="
    Gt,           // >
    Ge,           // ≥ or ">="
    Plus,         // +
    Minus,        // -
    Star,         // *
    Slash,        // /

    And,          // ∧
    Or,           // ∨
    Not,          // ¬
    Implies,      // ⇒

    LParen,       // (
    RParen,       // )
    LBrace,       // {  (set / range literal)
    RBrace,       // }
    LBracket,     // [  (sequence indexing)
    RBracket,     // ]
    Hash,         // #  (cardinality prefix)
    Comma,        // ,
    DotDot,       // .. (range literal)
    Dot,          // .  (sub-schema field access)
    Colon,        // :  (quantifier body separator)
    ForAll,       // ∀
    Exists,       // ∃
    MapsTo,       // ↦ or "mapsto"

    // Layout
    Newline,
    /// Number of leading-space columns on a new logical line. Emitted
    /// after a Newline (and at the start of input) so the parser can
    /// derive Indent/Dedent.
    Indent(usize),

    // Marker for end-of-input
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

pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut chars = src.chars().peekable();
    let mut line = 1usize;
    let mut col = 1usize;
    // Lexer state: at_line_start=true causes the next non-blank stretch
    // to count leading whitespace and emit an Indent(n). The initial
    // value is true so the very first line gets an Indent.
    let mut at_line_start = true;
    let mut current_indent;

    while let Some(&c) = chars.peek() {
        if at_line_start {
            // Count leading spaces (treat tab as 4 spaces).
            current_indent = 0;
            while let Some(&ch) = chars.peek() {
                match ch {
                    ' '  => { chars.next(); col += 1; current_indent += 1; }
                    '\t' => { chars.next(); col += 1; current_indent += 4; }
                    _    => break,
                }
            }
            // Skip blank lines and comment-only lines without emitting an Indent.
            if let Some(&ch) = chars.peek() {
                if ch == '\n' {
                    chars.next(); line += 1; col = 1;
                    at_line_start = true;
                    continue;
                }
                if ch == '-' {
                    // Look ahead for second '-'
                    let mut clone = chars.clone();
                    clone.next();
                    if clone.peek() == Some(&'-') {
                        // Comment: consume to newline.
                        while let Some(&ch) = chars.peek() {
                            if ch == '\n' { break; }
                            chars.next(); col += 1;
                        }
                        continue;
                    }
                }
            } else {
                // EOF after some indent.
                break;
            }
            tokens.push(Token::Indent(current_indent));
            at_line_start = false;
            continue;
        }

        match c {
            ' ' | '\t' => { chars.next(); col += 1; }
            '\n' => {
                chars.next();
                tokens.push(Token::Newline);
                line += 1; col = 1;
                at_line_start = true;
            }
            '-' => {
                // `--` comment, or unary/binary minus. Look at second char.
                let mut clone = chars.clone();
                clone.next();
                if clone.peek() == Some(&'-') {
                    // Skip to end of line (don't consume the newline).
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
                // Double-quoted string. Supports \" and \\ escapes; everything
                // else is literal. Single-line only — newlines inside are an
                // error (matches the Python grammar).
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
                let n: i64 = s.parse().map_err(|e| LexError {
                    message: format!("invalid integer {s:?}: {e}"),
                    line, col,
                })?;
                tokens.push(Token::Int(n));
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
            '+' => { chars.next(); col += 1; tokens.push(Token::Plus); }
            '*' => { chars.next(); col += 1; tokens.push(Token::Star); }
            '/' => { chars.next(); col += 1; tokens.push(Token::Slash); }
            '(' => { chars.next(); col += 1; tokens.push(Token::LParen); }
            ')' => { chars.next(); col += 1; tokens.push(Token::RParen); }
            '{' => { chars.next(); col += 1; tokens.push(Token::LBrace); }
            '}' => { chars.next(); col += 1; tokens.push(Token::RBrace); }
            '[' => { chars.next(); col += 1; tokens.push(Token::LBracket); }
            ']' => { chars.next(); col += 1; tokens.push(Token::RBracket); }
            '#' => { chars.next(); col += 1; tokens.push(Token::Hash); }
            ',' => { chars.next(); col += 1; tokens.push(Token::Comma); }
            ':' => { chars.next(); col += 1; tokens.push(Token::Colon); }
            '.' => {
                chars.next(); col += 1;
                if chars.peek() == Some(&'.') {
                    chars.next(); col += 1;
                    tokens.push(Token::DotDot);
                } else {
                    tokens.push(Token::Dot);
                }
            }
            '=' => { chars.next(); col += 1; tokens.push(Token::Eq); }
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
            '\u{2208}' => { chars.next(); col += 1; tokens.push(Token::In); }      // ∈
            '\u{2227}' => { chars.next(); col += 1; tokens.push(Token::And); }     // ∧
            '\u{2228}' => { chars.next(); col += 1; tokens.push(Token::Or); }      // ∨
            '\u{00AC}' => { chars.next(); col += 1; tokens.push(Token::Not); }     // ¬
            '\u{21D2}' => { chars.next(); col += 1; tokens.push(Token::Implies); } // ⇒
            '\u{2264}' => { chars.next(); col += 1; tokens.push(Token::Le); }      // ≤
            '\u{2265}' => { chars.next(); col += 1; tokens.push(Token::Ge); }      // ≥
            '\u{2260}' => { chars.next(); col += 1; tokens.push(Token::Neq); }     // ≠
            '\u{2200}' => { chars.next(); col += 1; tokens.push(Token::ForAll); }  // ∀
            '\u{2203}' => { chars.next(); col += 1; tokens.push(Token::Exists); }  // ∃
            '\u{21A6}' => { chars.next(); col += 1; tokens.push(Token::MapsTo); }  // ↦
            other => {
                return Err(LexError {
                    message: format!("unexpected character {:?}", other),
                    line, col,
                });
            }
        }
    }

    tokens.push(Token::Eof);
    Ok(tokens)
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
        "in"       => Token::In,
        "true"     => Token::True,
        "false"    => Token::False,
        "mapsto"   => Token::MapsTo,
        _ => Token::Ident(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_simple_schema() {
        let src = "schema SimpleNat\n    n ∈ Nat\n    n > 5\n";
        let toks = tokenize(src).unwrap();
        // Expect: Indent(0) schema SimpleNat \n Indent(4) n ∈ Nat \n Indent(4) n > 5 \n Eof
        assert!(matches!(toks[0], Token::Indent(0)));
        assert!(matches!(toks[1], Token::Schema));
        assert!(matches!(&toks[2], Token::Ident(s) if s == "SimpleNat"));
    }

    #[test]
    fn lex_unicode_operators() {
        let toks = tokenize("a ∈ Set ∧ b ≤ 5 ⇒ ¬c").unwrap();
        let kinds: Vec<_> = toks.iter().filter(|t| !matches!(t, Token::Indent(_))).cloned().collect();
        // a ∈ Set ∧ b ≤ 5 ⇒ ¬ c Eof
        assert!(matches!(kinds[1], Token::In));
        assert!(matches!(kinds[3], Token::And));
        assert!(matches!(kinds[5], Token::Le));
        assert!(matches!(kinds[7], Token::Implies));
        assert!(matches!(kinds[8], Token::Not));
    }
}
