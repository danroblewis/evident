//! Expression parsing: the precedence chain (quantifier → implies →
//! ternary → or → and → compare → addsub → muldiv → unary → postfix → atom)
//! plus match-arm / pattern parsing. One `impl Parser` block; entry points
//! parse_expr and parse_addsub are pub(super) for the declaration parser.

use crate::core::ast::*;
use crate::lexer::Token;
use super::{Parser, Result, peek_compare_op};

impl Parser {
    pub(super) fn parse_expr(&mut self) -> Result<Expr> {

        if matches!(self.peek(), Token::ForAll | Token::Exists) {
            return self.parse_quantifier();
        }
        self.parse_implies()
    }

    fn parse_quantifier(&mut self) -> Result<Expr> {
        let is_forall = matches!(self.peek(), Token::ForAll);
        self.bump();

        let vars: Vec<String> = if matches!(self.peek(), Token::LParen) {
            self.bump();
            let mut names = Vec::new();
            loop {
                match self.bump() {
                    Token::Ident(s) => names.push(s),
                    other => return Err(self.err(format!(
                        "expected bound variable name in tuple binding, got {:?}", other))),
                }
                if matches!(self.peek(), Token::Comma) { self.bump(); continue; }
                break;
            }
            self.eat(&Token::RParen)?;
            if names.len() < 2 {
                return Err(self.err(format!(
                    "tuple binding `(…)` must contain ≥ 2 names; got {}", names.len()
                )));
            }
            names
        } else {
            match self.bump() {
                Token::Ident(s) => vec![s],
                other => return Err(self.err(format!(
                    "expected bound variable name, got {:?}", other))),
            }
        };
        self.eat(&Token::In)?;

        let range = self.parse_postfix()?;
        self.eat(&Token::Colon)?;

        if matches!(self.peek(), Token::Newline) {
            let saved = self.pos;
            self.bump();
            while matches!(self.peek(), Token::Newline) { self.bump(); }
            if let Token::Indent(n) = self.peek().clone() {
                let block_indent = n;
                let mut conjuncts = Vec::new();
                loop {
                    match self.peek() {
                        Token::Indent(m) if *m == block_indent => { self.bump(); }
                        _ => break,
                    }
                    let item = self.parse_implies()?;
                    conjuncts.push(item);
                    match self.peek() {
                        Token::Newline => { self.bump(); }
                        Token::Eof => break,
                        _ => {}
                    }
                }
                if conjuncts.is_empty() {
                    self.pos = saved;
                } else {
                    let mut body = conjuncts.remove(0);
                    for c in conjuncts {
                        body = Expr::Binary(BinOp::And, Box::new(body), Box::new(c));
                    }
                    return Ok(if is_forall {
                        Expr::Forall(vars, Box::new(range), Box::new(body))
                    } else {
                        Expr::Exists(vars, Box::new(range), Box::new(body))
                    });
                }
            } else {
                self.pos = saved;
            }
        }
        let body = self.parse_expr()?;
        Ok(if is_forall {
            Expr::Forall(vars, Box::new(range), Box::new(body))
        } else {
            Expr::Exists(vars, Box::new(range), Box::new(body))
        })
    }

    fn parse_implies(&mut self) -> Result<Expr> {

        if matches!(self.peek(), Token::ForAll | Token::Exists) {
            return self.parse_quantifier();
        }
        let lhs = self.parse_ternary()?;
        if matches!(self.peek(), Token::Implies) {
            self.bump();

            if matches!(self.peek(), Token::Newline) {
                let saved = self.pos;
                self.bump();

                while matches!(self.peek(), Token::Newline) { self.bump(); }
                if let Token::Indent(n) = self.peek().clone() {
                    let block_indent = n;
                    let mut conjuncts = Vec::new();
                    loop {

                        match self.peek() {
                            Token::Indent(m) if *m == block_indent => { self.bump(); }
                            _ => break,
                        }
                        let item = self.parse_implies()?;
                        conjuncts.push(item);
                        match self.peek() {
                            Token::Newline => { self.bump(); }
                            Token::Eof => break,
                            _ => {}
                        }
                    }
                    if conjuncts.is_empty() {

                        self.pos = saved;
                    } else {

                        let mut acc = conjuncts.remove(0);
                        for c in conjuncts {
                            acc = Expr::Binary(BinOp::And, Box::new(acc), Box::new(c));
                        }
                        return Ok(Expr::Binary(BinOp::Implies, Box::new(lhs), Box::new(acc)));
                    }
                } else {
                    self.pos = saved;
                }
            }
            let rhs = self.parse_implies()?;
            return Ok(Expr::Binary(BinOp::Implies, Box::new(lhs), Box::new(rhs)));
        }
        Ok(lhs)
    }

    /// Consume any Newline / Indent tokens. Used inside the multi-line ternary so `cond ?` can put
    /// its then/else branches on the following indented lines (`cond ? \n A \n : B`).
    fn skip_layout(&mut self) {
        while matches!(self.peek(), Token::Newline | Token::Indent(_)) {
            self.bump();
        }
    }

    fn parse_ternary(&mut self) -> Result<Expr> {
        let cond = self.parse_or()?;
        if !matches!(self.peek(), Token::Question) {
            return Ok(cond);
        }
        self.bump();
        self.skip_layout();                       // multi-line: `cond ?` then the branches indented below
        let then_branch = self.parse_ternary()?;
        self.skip_layout();
        match self.bump() {
            Token::Colon => {}
            other => return Err(self.err(format!(
                "expected `:` after ternary then-branch, got {:?}", other,
            ))),
        }
        self.skip_layout();
        let else_branch = self.parse_ternary()?;
        Ok(Expr::Ternary(
            Box::new(cond),
            Box::new(then_branch),
            Box::new(else_branch),
        ))
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_and()?;
        while matches!(self.peek(), Token::Or) {
            self.bump();
            let rhs = self.parse_and()?;
            lhs = Expr::Binary(BinOp::Or, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_compare()?;
        while matches!(self.peek(), Token::And) {
            self.bump();
            let rhs = self.parse_compare()?;
            lhs = Expr::Binary(BinOp::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_compare(&mut self) -> Result<Expr> {
        let lhs = self.parse_addsub()?;

        if matches!(self.peek(), Token::Matches) {
            self.bump();
            let pattern = self.parse_match_pattern()?;
            return Ok(Expr::Matches(Box::new(lhs), pattern));
        }

        if matches!(self.peek(), Token::In) {
            self.bump();
            let rhs = self.parse_addsub()?;
            return Ok(Expr::InExpr(Box::new(lhs), Box::new(rhs)));
        }

        if matches!(self.peek(), Token::NotIn) {
            self.bump();
            let rhs = self.parse_addsub()?;
            return Ok(Expr::Not(Box::new(Expr::InExpr(Box::new(lhs), Box::new(rhs)))));
        }

        if matches!(self.peek(), Token::ContainsRev) {
            self.bump();
            let rhs = self.parse_addsub()?;
            return Ok(Expr::InExpr(Box::new(rhs), Box::new(lhs)));
        }
        let op = match self.peek() {
            Token::Eq  => Some(BinOp::Eq),
            Token::Neq => Some(BinOp::Neq),
            Token::Lt  => Some(BinOp::Lt),
            Token::Le  => Some(BinOp::Le),
            Token::Gt  => Some(BinOp::Gt),
            Token::Ge  => Some(BinOp::Ge),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let rhs = self.parse_addsub()?;

            if peek_compare_op(self.peek()).is_some() {
                let mut operands: Vec<Expr> = vec![lhs, rhs];
                let mut ops: Vec<BinOp> = vec![op];
                while let Some(next_op) = peek_compare_op(self.peek()) {
                    self.bump();
                    operands.push(self.parse_addsub()?);
                    ops.push(next_op);
                }

                let mut acc: Option<Expr> = None;
                for (i, op_i) in ops.into_iter().enumerate() {
                    let pair = Expr::Binary(
                        op_i,
                        Box::new(operands[i].clone()),
                        Box::new(operands[i + 1].clone()),
                    );
                    acc = Some(match acc {
                        None    => pair,
                        Some(a) => Expr::Binary(BinOp::And, Box::new(a), Box::new(pair)),
                    });
                }
                return Ok(acc.unwrap());
            }
            return Ok(Expr::Binary(op, Box::new(lhs), Box::new(rhs)));
        }
        Ok(lhs)
    }

    pub(super) fn parse_addsub(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_muldiv()?;
        loop {
            let op = match self.peek() {
                Token::Plus     => BinOp::Add,
                Token::PlusPlus => BinOp::Concat,
                Token::Minus    => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_muldiv()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_muldiv(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star  => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::MidDot => BinOp::UserOp("·".to_string()),
                Token::Times  => BinOp::UserOp("×".to_string()),
                _ => break,
            };
            self.bump();
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if matches!(self.peek(), Token::Not) {
            self.bump();
            let e = self.parse_unary()?;
            return Ok(Expr::Not(Box::new(e)));
        }
        if matches!(self.peek(), Token::Delta) {
            self.bump();
            let e = self.parse_unary()?;
            return Ok(Expr::Delta(Box::new(e)));
        }
        if matches!(self.peek(), Token::Minus) {
            self.bump();
            let e = self.parse_unary()?;

            return Ok(Expr::Binary(BinOp::Sub, Box::new(Expr::Int(0)), Box::new(e)));
        }
        if matches!(self.peek(), Token::Hash) {
            self.bump();
            let e = self.parse_unary()?;
            return Ok(Expr::Cardinality(Box::new(e)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut e = self.parse_atom()?;
        loop {
            match self.peek() {
                Token::LBracket => {
                    self.bump();
                    let idx = self.parse_expr()?;
                    self.eat(&Token::RBracket)?;
                    e = Expr::Index(Box::new(e), Box::new(idx));
                }
                Token::Dot => {
                    self.bump();
                    match self.bump() {
                        Token::Ident(field) => {
                            e = Expr::Field(Box::new(e), field);
                        }
                        other => return Err(self.err(format!(
                            "expected field name after '.', got {:?}", other))),
                    }
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_match(&mut self) -> Result<Expr> {
        self.bump();
        let scrutinee = self.parse_or()?;

        if !matches!(self.peek(), Token::Newline) {
            return Err(self.err(
                "expected newline + indented arms after `match scrutinee`"));
        }
        self.bump();
        while matches!(self.peek(), Token::Newline) { self.bump(); }
        let arm_indent = match self.peek() {
            Token::Indent(n) if *n > 0 => *n,
            _ => return Err(self.err(
                "expected indented arms after `match`")),
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
                other => return Err(self.err(format!(
                    "expected `⇒` after pattern, got {:?}", other))),
            }
            let body = self.parse_or()?;
            arms.push(crate::core::ast::MatchArm {
                pattern, body: Box::new(body),
            });

            while matches!(self.peek(), Token::Newline) { self.bump(); }
        }
        if arms.is_empty() {
            return Err(self.err("match must have at least one arm"));
        }
        Ok(Expr::Match(Box::new(scrutinee), arms))
    }

    fn parse_match_pattern(&mut self) -> Result<crate::core::ast::MatchPattern> {

        if let Token::Ident(s) = self.peek().clone() {
            if s == "_" {
                self.bump();
                return Ok(crate::core::ast::MatchPattern::Wildcard);
            }

            self.bump();
            if !matches!(self.peek(), Token::LParen) {
                return Ok(crate::core::ast::MatchPattern::Ctor {
                    name: s, binds: Vec::new(),
                });
            }
            self.bump();
            let mut binds = Vec::new();
            if !matches!(self.peek(), Token::RParen) {
                loop {
                    let bind = match self.bump() {
                        Token::Ident(b) if b == "_" => None,
                        Token::Ident(b) => Some(b),
                        other => return Err(self.err(format!(
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
        Err(self.err(format!(
            "expected pattern (Ctor or `_`), got {:?}", self.peek())))
    }

    fn parse_atom(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            Token::Int(n)   => { self.bump(); Ok(Expr::Int(n)) }
            Token::Real(v)  => { self.bump(); Ok(Expr::Real(v)) }
            Token::Str(s)   => { self.bump(); Ok(Expr::Str(s)) }
            Token::True     => { self.bump(); Ok(Expr::Bool(true)) }
            Token::False    => { self.bump(); Ok(Expr::Bool(false)) }
            Token::Match    => self.parse_match(),
            Token::Ident(s) => {
                self.bump();

                let mut name = s;
                while matches!(self.peek(), Token::Dot) {
                    self.bump();
                    match self.bump() {
                        Token::Ident(field) => { name.push('.'); name.push_str(&field); }
                        other => return Err(self.err(format!(
                            "expected field name after '.', got {:?}", other))),
                    }
                }

                if matches!(self.peek(), Token::LParen) {
                    self.bump();
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
            other => Err(self.err(format!("expected expression, got {:?}", other))),
        }
    }
}
