#[derive(Debug, Clone, PartialEq)]
pub enum Token {

    Ident(String),
    Int(i64),
    Real(f64),
    Str(String),
    True,
    False,

    Schema,
    Claim,
    Type,
    Subclaim,
    Fsm,
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

    And,
    Or,
    Not,
    Implies,

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

pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let mut tokens = Vec::new();
    let mut chars = src.chars().peekable();
    let mut line = 1usize;
    let mut col = 1usize;

    let mut at_line_start = true;
    let mut current_indent;

    let mut paren_depth: usize = 0;

    while let Some(&c) = chars.peek() {
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
            at_line_start = false;
            continue;
        }

        match c {
            ' ' | '\t' => { chars.next(); col += 1; }
            '\n' => {
                chars.next();
                line += 1; col = 1;
                if paren_depth == 0 {
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
            '\u{2264}' => { chars.next(); col += 1; tokens.push(Token::Le); }
            '\u{2265}' => { chars.next(); col += 1; tokens.push(Token::Ge); }
            '\u{2260}' => { chars.next(); col += 1; tokens.push(Token::Neq); }
            '\u{2200}' => { chars.next(); col += 1; tokens.push(Token::ForAll); }
            '\u{2203}' => { chars.next(); col += 1; tokens.push(Token::Exists); }
            '\u{21A6}' => { chars.next(); col += 1; tokens.push(Token::MapsTo); }
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
        "fsm"      => Token::Fsm,
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
