//! `match` expression and pattern parsing; shared with `e matches Pattern`.

use super::*;

impl Parser {
    /// `match scrutinee\n    Pattern ⇒ body\n    …` — indent-delimited arms.
    /// Sits at atom level so it composes with arithmetic and equality LHS.
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

    /// One pattern: `_` → Wildcard, lowercase → Bind, uppercase → nullary Ctor,
    /// `Ctor(p…)` → Ctor with sub-patterns. Recursive to any depth.
    pub(super) fn parse_match_pattern(&mut self) -> Result<crate::core::ast::MatchPattern> {
        use crate::core::ast::MatchPattern;
        let Token::Ident(s) = self.peek().clone() else {
            return Err(ParseError(format!(
                "expected pattern (Ctor, binding, or `_`), got {:?}", self.peek())));
        };
        self.bump();
        if s == "_" {
            return Ok(MatchPattern::Wildcard);
        }
        if matches!(self.peek(), Token::LParen) {
            self.bump(); // (
            let mut binds = Vec::new();
            if !matches!(self.peek(), Token::RParen) {
                loop {
                    binds.push(self.parse_match_pattern()?);
                    if matches!(self.peek(), Token::Comma) {
                        self.bump();
                        continue;
                    }
                    break;
                }
            }
            self.eat(&Token::RParen)?;
            return Ok(MatchPattern::Ctor { name: s, binds });
        }
        let is_ctor = s.chars().next().is_some_and(|c| c.is_uppercase());
        Ok(if is_ctor {
            MatchPattern::Ctor { name: s, binds: Vec::new() }
        } else {
            MatchPattern::Bind(s)
        })
    }
}
