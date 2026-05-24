//! Tokens → AST. Hand-rolled recursive-descent for the v0.1 subset.
//!
//! The parser is split across files by grammar-production group:
//!   - `program`   — file → schemas + enums + imports; enum declarations
//!   - `schema`    — schema/claim/type/subclaim bodies; first-line params
//!   - `body_item` — body-item dispatch; chained-membership desugaring
//!   - `types`     — type-name parsing incl. generics (`Edge<T>`) + pins
//!   - `exprs`     — expression precedence climbing (`⇒` … `parse_postfix`)
//!   - `atoms`     — atom level: literals, calls, tuples, set/seq literals
//!   - `patterns`  — `match` arms + constructor patterns
//!
//! `mod.rs` holds the `Parser` struct, the token-stream utilities every
//! group shares (`peek`/`bump`/`eat`/`skip_blank_newlines`), the
//! `peek_compare_op` helper, and the public `parse` entry point.

use crate::core::ast::*;
use crate::lexer::Token;

mod atoms;
mod body_item;
mod exprs;
mod patterns;
mod program;
mod schema;
mod types;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "parse error: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

type Result<T> = std::result::Result<T, ParseError>;

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Parser { toks, pos: 0 }
    }

    fn peek(&self) -> &Token { &self.toks[self.pos] }
    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        self.pos += 1;
        t
    }
    fn eat(&mut self, expected: &Token) -> Result<()> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(expected) {
            self.bump();
            Ok(())
        } else {
            Err(ParseError(format!("expected {:?}, got {:?}", expected, self.peek())))
        }
    }

    /// Skip Newline tokens that aren't followed by an indent change worth recording.
    fn skip_blank_newlines(&mut self) {
        loop {
            match self.peek() {
                Token::Newline => { self.bump(); }
                _ => break,
            }
        }
    }
}

/// Recognize a comparison operator token. Used by `parse_compare` for
/// chained-comparison detection (`20 ≤ x ≤ 740` etc.) — when the
/// token after a `lhs op rhs` parse is another comparison op, we
/// know we're in a chain and the desugaring kicks in.
fn peek_compare_op(tok: &Token) -> Option<BinOp> {
    match tok {
        Token::Eq  => Some(BinOp::Eq),
        Token::Neq => Some(BinOp::Neq),
        Token::Lt  => Some(BinOp::Lt),
        Token::Le  => Some(BinOp::Le),
        Token::Gt  => Some(BinOp::Gt),
        Token::Ge  => Some(BinOp::Ge),
        _ => None,
    }
}

pub fn parse(src: &str) -> Result<Program> {
    let toks = crate::lexer::tokenize(src).map_err(|e| ParseError(e.to_string()))?;
    Parser::new(toks).parse_program()
}
