//! Atom-level parsing: literals, `match`, dotted idents, calls/constructors,
//! parens/tuples, set/range literals, sequence literals.

use super::*;

impl Parser {
    pub(super) fn parse_atom(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            Token::Int(n)   => { self.bump(); Ok(Expr::Int(n)) }
            Token::Real(v)  => { self.bump(); Ok(Expr::Real(v)) }
            Token::Str(s)   => { self.bump(); Ok(Expr::Str(s)) }
            Token::True     => { self.bump(); Ok(Expr::Bool(true)) }
            Token::False    => { self.bump(); Ok(Expr::Bool(false)) }
            Token::Match    => self.parse_match(),
            Token::Ident(s) => {
                self.bump();
                // Greedily consume `.ident` chains BEFORE the call check so
                // `win.renderer.set_draw_color(args)` becomes a single dotted Call.
                let mut name = s;
                while matches!(self.peek(), Token::Dot) {
                    self.bump();
                    match self.bump() {
                        Token::Ident(field) => { name.push('.'); name.push_str(&field); }
                        other => return Err(ParseError(format!(
                            "expected field name after '.', got {:?}", other))),
                    }
                }
                // Optional `<T>` suffix for generic constructors like `Edge<Rect>(args)`.
                // Only consumed when immediately followed by `(`; otherwise rewind.
                if matches!(self.peek(), Token::Lt) {
                    let saved = self.pos;
                    let parsed = self.try_parse_generic_args_suffix();
                    match parsed {
                        Ok(Some(args)) if matches!(self.peek(), Token::LParen) => {
                            name.push_str(&args);
                        }
                        _ => { self.pos = saved; }
                    }
                }
                // `name(arg, …)` — covers builtins, record literals, claim calls.
                if matches!(self.peek(), Token::LParen) {
                    self.bump();   // (
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Token::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if matches!(self.peek(), Token::Comma) {
                                self.bump();
                                continue;
                            }
                            break;
                        }
                    }
                    self.eat(&Token::RParen)?;
                    return Ok(Expr::Call(name, args));
                }
                Ok(Expr::Identifier(name))
            }
            Token::LParen   => {
                self.bump();
                let first = self.parse_expr()?;
                // Tuple `(e1, e2, …)` for relational LHS; single `(e)` is just grouping.
                if matches!(self.peek(), Token::Comma) {
                    let mut items = vec![first];
                    while matches!(self.peek(), Token::Comma) {
                        self.bump();
                        items.push(self.parse_expr()?);
                    }
                    self.eat(&Token::RParen)?;
                    return Ok(Expr::Tuple(items));
                }
                self.eat(&Token::RParen)?;
                Ok(first)
            }
            Token::LBrace => {
                self.bump();
                if matches!(self.peek(), Token::RBrace) {
                    self.bump();
                    return Ok(Expr::SetLit(vec![]));
                }
                let first = self.parse_expr()?;
                if matches!(self.peek(), Token::DotDot) {
                    self.bump();
                    let hi = self.parse_expr()?;
                    self.eat(&Token::RBrace)?;
                    return Ok(Expr::Range(Box::new(first), Box::new(hi)));
                }
                let mut items = vec![first];
                while matches!(self.peek(), Token::Comma) {
                    self.bump();
                    items.push(self.parse_expr()?);
                }
                self.eat(&Token::RBrace)?;
                Ok(Expr::SetLit(items))
            }
            Token::LSeq => {
                // `⟨e1, e2, …⟩` sequence literal (index-pinned, unlike set `{…}`).
                self.bump();
                if matches!(self.peek(), Token::RSeq) {
                    self.bump();
                    return Ok(Expr::SeqLit(vec![]));
                }
                let first = self.parse_expr()?;
                let mut items = vec![first];
                while matches!(self.peek(), Token::Comma) {
                    self.bump();
                    items.push(self.parse_expr()?);
                }
                self.eat(&Token::RSeq)?;
                Ok(Expr::SeqLit(items))
            }
            other => Err(ParseError(format!("expected expression, got {:?}", other))),
        }
    }
}
