//! Expression parsing via precedence climbing: quantifiers, implication,
//! ternary, boolean, comparison, arithmetic, unary, postfix. Atoms in `atoms`.

use super::*;

impl Parser {
    pub(super) fn parse_expr(&mut self) -> Result<Expr> {
        if matches!(self.peek(), Token::ForAll | Token::Exists) {
            return self.parse_quantifier();
        }
        self.parse_implies()
    }

    pub(super) fn parse_quantifier(&mut self) -> Result<Expr> {
        let is_forall = matches!(self.peek(), Token::ForAll);
        self.bump();
        // `∀ x ∈ …` (single) or `∀ (a, b, c) ∈ …` (tuple for coindexed/edges).
        let vars: Vec<String> = if matches!(self.peek(), Token::LParen) {
            self.bump();   // (
            let mut names = Vec::new();
            loop {
                match self.bump() {
                    Token::Ident(s) => names.push(s),
                    other => return Err(ParseError(format!(
                        "expected bound variable name in tuple binding, got {:?}", other))),
                }
                if matches!(self.peek(), Token::Comma) { self.bump(); continue; }
                break;
            }
            self.eat(&Token::RParen)?;
            if names.len() < 2 {
                return Err(ParseError(format!(
                    "tuple binding `(…)` must contain ≥ 2 names; got {}", names.len()
                )));
            }
            names
        } else {
            match self.bump() {
                Token::Ident(s) => vec![s],
                other => return Err(ParseError(format!(
                    "expected bound variable name, got {:?}", other))),
            }
        };
        self.eat(&Token::In)?;
        let range = self.parse_postfix()?;
        self.eat(&Token::Colon)?;
        // Block form: `∀ var ∈ range :\n    body…` AND-combines indented lines.
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

    pub(super) fn parse_implies(&mut self) -> Result<Expr> {
        // Quantifiers share `⇒` precedence; without this, `A ⇒ ∀ i : B` fails.
        if matches!(self.peek(), Token::ForAll | Token::Exists) {
            return self.parse_quantifier();
        }
        let lhs = self.parse_ternary()?;
        if matches!(self.peek(), Token::Implies) {
            self.bump();
            // Block form: `A ⇒\n    body…` AND-combines indented consequents.
            if matches!(self.peek(), Token::Newline) {
                let saved = self.pos;
                self.bump();
                while matches!(self.peek(), Token::Newline) { self.bump(); }
                if let Token::Indent(n) = self.peek().clone() {
                    let block_indent = n;
                    let mut conjuncts = Vec::new();
                    loop {
                        // Each line: Indent(block_indent) then expr then Newline.
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

    /// `cond ? then : else` — right-associative, between `⇒` and `∨` in precedence.
    pub(super) fn parse_ternary(&mut self) -> Result<Expr> {
        let cond = self.parse_or()?;
        if !matches!(self.peek(), Token::Question) {
            return Ok(cond);
        }
        self.bump(); // ?
        let then_branch = self.parse_ternary()?;
        match self.bump() {
            Token::Colon => {}
            other => return Err(ParseError(format!(
                "expected `:` after ternary then-branch, got {:?}", other,
            ))),
        }
        let else_branch = self.parse_ternary()?;
        Ok(Expr::Ternary(
            Box::new(cond),
            Box::new(then_branch),
            Box::new(else_branch),
        ))
    }

    pub(super) fn parse_or(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_and()?;
        while matches!(self.peek(), Token::Or) {
            self.bump();
            let rhs = self.parse_and()?;
            lhs = Expr::Binary(BinOp::Or, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    pub(super) fn parse_and(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_compare()?;
        while matches!(self.peek(), Token::And) {
            self.bump();
            let rhs = self.parse_compare()?;
            lhs = Expr::Binary(BinOp::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    pub(super) fn parse_compare(&mut self) -> Result<Expr> {
        let lhs = self.parse_addsub()?;
        // `e matches Pattern` at comparison precedence.
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
        // `lhs ∋ rhs` desugars to `rhs ∈ lhs`.
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
            // Chained comparisons `20 ≤ x ≤ 740`: AND-combine pairwise,
            // sharing inner operands (differs from C/Rust left-fold).
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

    pub(super) fn parse_muldiv(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star  => BinOp::Mul,
                Token::Slash => BinOp::Div,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    pub(super) fn parse_unary(&mut self) -> Result<Expr> {
        if matches!(self.peek(), Token::Not) {
            self.bump();
            let e = self.parse_unary()?;
            return Ok(Expr::Not(Box::new(e)));
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

    /// `[expr]` and `.ident` postfix chain. `.ident` after an Index (e.g. `pts[0].x`)
    /// wraps in `Field`; dotted idents on bare Identifiers are already collapsed in atoms.
    pub(super) fn parse_postfix(&mut self) -> Result<Expr> {
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
                        other => return Err(ParseError(format!(
                            "expected field name after '.', got {:?}", other))),
                    }
                }
                _ => break,
            }
        }
        Ok(e)
    }
}
