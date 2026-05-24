//! Expression parsing via precedence climbing, from the top-level
//! `parse_expr` (quantifiers + implication) down through ternary,
//! boolean, comparison, arithmetic, unary, and the postfix
//! `[…]` / `.ident` suffix chain. Atom-level parsing lives in `atoms`.

use super::*;

impl Parser {
    // Operator precedence (low → high):
    //   implies        : right-assoc
    //   or             : left
    //   and            : left
    //   compare        : non-assoc (=, ≠, <, ≤, >, ≥)
    //   add/sub        : left
    //   mul/div        : left
    //   unary not / -
    //   atoms          : ident, int, paren

    pub(super) fn parse_expr(&mut self) -> Result<Expr> {
        // Quantifier expressions are right at the top so the colon-separated
        // body picks up everything to the right.
        if matches!(self.peek(), Token::ForAll | Token::Exists) {
            return self.parse_quantifier();
        }
        self.parse_implies()
    }

    pub(super) fn parse_quantifier(&mut self) -> Result<Expr> {
        let is_forall = matches!(self.peek(), Token::ForAll);
        self.bump();
        // Binding shapes:
        //   `∀ x ∈ …`               — single-var (1-element Vec)
        //   `∀ (a, b, c) ∈ …`       — tuple binding for pair/N-tuple
        //                            iteration (`coindexed`, `edges`)
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
        // The range can be:
        //   - `{lo..hi}` or `{a, b, c}` — set / range literal (parsed
        //     by parse_atom's `{` branch).
        //   - `coindexed(A, B)` / `edges(seq)` — builtin call.
        //   - A Seq expression: a bare Identifier OR a Field/Index
        //     chain like `groups[0].items` reaching a Seq via a
        //     SeqField on a composite element. parse_postfix is the
        //     entry that consumes the postfix `[…]` / `.field` chain.
        let range = self.parse_postfix()?;
        self.eat(&Token::Colon)?;
        // Quantifier-block form: `∀ var ∈ range :` followed by Newline +
        // Indent at a deeper level. Parse a stack of body items at that
        // indent and AND-combine them as the quantifier body. Mirrors
        // the implies-block pattern in parse_implies. Lets users write
        //
        //   ∀ i ∈ {0..3} :
        //       state.dots[i].pos_x ≥ 20
        //       state.dots[i].pos_x ≤ 740
        //
        // instead of repeating `∀ i ∈ {0..3} : …` per constraint.
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
        // Quantifiers are valid wherever an `⇒` operand is — they live
        // at the same precedence as `⇒`. Without this, `A ⇒ ∀ i : B`
        // and the implies-block form `A ⇒\n    ∀ i : B` both fail
        // (the body iteration recurses through parse_implies).
        if matches!(self.peek(), Token::ForAll | Token::Exists) {
            return self.parse_quantifier();
        }
        let lhs = self.parse_ternary()?;
        if matches!(self.peek(), Token::Implies) {
            self.bump();
            // Implies-block form: `A ⇒` followed by Newline + Indent at a
            // deeper level than the line `A ⇒` started on. Parse a stack
            // of body items at that indent and AND them as the consequent.
            // Mirrors the Python `implies_block` grammar rule.
            if matches!(self.peek(), Token::Newline) {
                let saved = self.pos;
                self.bump();
                // Skip blank newlines.
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
                        // No body — restore and fall through to the
                        // expression-RHS branch below (will likely error).
                        self.pos = saved;
                    } else {
                        // Combine into a left-associative AND chain.
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

    /// `cond ? then : else` — C-style ternary. Sits between `⇒` and
    /// `∨` in precedence so:
    ///   `a ⇒ b ? c : d`  parses as `a ⇒ (b ? c : d)`
    ///   `a ∨ b ? c : d`  parses as `(a ∨ b) ? c : d`
    /// Right-associative (`a ? b : c ? d : e` is `a ? b : (c ? d : e)`).
    /// Both branches recursively call `parse_ternary` so nested
    /// ternaries on either side work without parens.
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
        // `e matches Pattern` — constructor recognizer. Sits at
        // comparison level so `e matches A ∨ e matches B` parses as
        // `(matches A) ∨ (matches B)`.
        if matches!(self.peek(), Token::Matches) {
            self.bump();
            let pattern = self.parse_match_pattern()?;
            return Ok(Expr::Matches(Box::new(lhs), pattern));
        }
        // ∈ binds at the same level as =, <, etc.
        if matches!(self.peek(), Token::In) {
            self.bump();
            let rhs = self.parse_addsub()?;
            return Ok(Expr::InExpr(Box::new(lhs), Box::new(rhs)));
        }
        // ∉ — desugar `lhs ∉ rhs` to `¬(lhs ∈ rhs)`.
        if matches!(self.peek(), Token::NotIn) {
            self.bump();
            let rhs = self.parse_addsub()?;
            return Ok(Expr::Not(Box::new(Expr::InExpr(Box::new(lhs), Box::new(rhs)))));
        }
        // ∋ — reverse membership: `lhs ∋ rhs` means `rhs ∈ lhs`.
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
            // Chained comparison: if another comparison op follows
            // (`20 ≤ x ≤ 740`, `a < b ≤ c`, etc.), collect the rest
            // and AND-combine pairwise. Standard math notation;
            // matches the Python parser's `arith_chain` rule. The
            // inner operands are shared between adjacent
            // comparisons (the `x` in `20 ≤ x ≤ 740` appears in both
            // (20 ≤ x) AND (x ≤ 740)) — semantics match Python and
            // mainstream math; differs from C/Rust's left-fold which
            // would type-error here.
            if peek_compare_op(self.peek()).is_some() {
                let mut operands: Vec<Expr> = vec![lhs, rhs];
                let mut ops: Vec<BinOp> = vec![op];
                while let Some(next_op) = peek_compare_op(self.peek()) {
                    self.bump();
                    operands.push(self.parse_addsub()?);
                    ops.push(next_op);
                }
                // Build (operands[0] op[0] operands[1]) ∧ (operands[1] op[1] operands[2]) ∧ …
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
            // Treat -x as 0 - x.
            return Ok(Expr::Binary(BinOp::Sub, Box::new(Expr::Int(0)), Box::new(e)));
        }
        if matches!(self.peek(), Token::Hash) {
            self.bump();
            let e = self.parse_unary()?;
            return Ok(Expr::Cardinality(Box::new(e)));
        }
        self.parse_postfix()
    }

    /// Atom followed by zero or more `[expr]` indexing suffixes and/or
    /// `.ident` field-access suffixes. Both bind tighter than any binary
    /// op. The `.ident` chain on a bare Identifier is already collapsed
    /// into a dotted name at the atom level (see `parse_atom`), so this
    /// loop only sees `.ident` after a non-Ident receiver — typically
    /// after an Index suffix like `pts[0].x`. We wrap it in `Field`,
    /// which the runtime resolves through Datatype accessors instead of
    /// env-key lookup.
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
