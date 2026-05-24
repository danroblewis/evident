//! `match` expression parsing (indentation-delimited arms) and the
//! per-arm constructor / wildcard pattern parser. Shared with the
//! `e matches Pattern` recognizer in `parse_compare`.

use super::*;

impl Parser {
    /// `match scrutinee \n   Pattern ⇒ body \n   Pattern ⇒ body ...`
    /// Arms are delimited by indentation; the scrutinee is one line
    /// after `match` (no trailing colon needed). Each arm has a
    /// pattern (`Ctor(b1, b2, ...)` or `_`) then `⇒` then a body
    /// expression (single line; no implies-block on the body).
    /// Caller is `parse_atom` — match sits at atom level so it composes
    /// with arithmetic (`1 + match e ...`) and equality LHS.
    pub(super) fn parse_match(&mut self) -> Result<Expr> {
        self.bump(); // match
        let scrutinee = self.parse_or()?;
        // Require Newline → Indent at deeper level.
        if !matches!(self.peek(), Token::Newline) {
            return Err(ParseError(
                "expected newline + indented arms after `match scrutinee`".into()));
        }
        self.bump();
        while matches!(self.peek(), Token::Newline) { self.bump(); }
        let arm_indent = match self.peek() {
            Token::Indent(n) if *n > 0 => *n,
            _ => return Err(ParseError(
                "expected indented arms after `match`".into())),
        };
        let mut arms = Vec::new();
        loop {
            match self.peek() {
                Token::Indent(m) if *m == arm_indent => { self.bump(); }
                _ => break,
            }
            let pattern = self.parse_match_pattern()?;
            match self.bump() {
                Token::Implies => {}
                other => return Err(ParseError(format!(
                    "expected `⇒` after pattern, got {:?}", other))),
            }
            let body = self.parse_or()?;
            arms.push(crate::core::ast::MatchArm {
                pattern, body: Box::new(body),
            });
            // Optional Newline between arms.
            while matches!(self.peek(), Token::Newline) { self.bump(); }
        }
        if arms.is_empty() {
            return Err(ParseError("match must have at least one arm".into()));
        }
        Ok(Expr::Match(Box::new(scrutinee), arms))
    }

    /// One pattern: bare `_` (wildcard), or `Ctor(b1, b2, ...)` where
    /// each binding is an identifier or `_`.
    pub(super) fn parse_match_pattern(&mut self) -> Result<crate::core::ast::MatchPattern> {
        // Bare `_` at the top level.
        if let Token::Ident(s) = self.peek().clone() {
            if s == "_" {
                self.bump();
                return Ok(crate::core::ast::MatchPattern::Wildcard);
            }
            // Either bare nullary variant `Ctor` or `Ctor(b1, ...)`.
            self.bump();
            if !matches!(self.peek(), Token::LParen) {
                return Ok(crate::core::ast::MatchPattern::Ctor {
                    name: s, binds: Vec::new(),
                });
            }
            self.bump(); // (
            let mut binds = Vec::new();
            if !matches!(self.peek(), Token::RParen) {
                loop {
                    let bind = match self.bump() {
                        Token::Ident(b) if b == "_" => None,
                        Token::Ident(b) => Some(b),
                        other => return Err(ParseError(format!(
                            "expected identifier or `_` in pattern, got {:?}", other))),
                    };
                    binds.push(bind);
                    if matches!(self.peek(), Token::Comma) {
                        self.bump();
                        continue;
                    }
                    break;
                }
            }
            self.eat(&Token::RParen)?;
            return Ok(crate::core::ast::MatchPattern::Ctor { name: s, binds });
        }
        Err(ParseError(format!(
            "expected pattern (Ctor or `_`), got {:?}", self.peek())))
    }
}
