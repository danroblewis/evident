//! Tokens → AST. Hand-rolled recursive-descent for the v0.1 subset.

use crate::ast::*;
use crate::lexer::Token;

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

    pub fn parse_program(&mut self) -> Result<Program> {
        let mut program = Program::default();
        // Initial Indent(0) at the start of the file.
        if !matches!(self.peek(), Token::Indent(0)) {
            // Allow either Indent(0) explicit (set by lexer) or no indent.
        } else {
            self.bump();
        }
        loop {
            self.skip_blank_newlines();
            // Skip leading Indent tokens at the top level (we expect Indent(0)
            // before each top-level decl; the lexer emits one per logical line).
            while let Token::Indent(_) = self.peek() {
                self.bump();
            }
            match self.peek() {
                Token::Eof => break,
                Token::Schema | Token::Claim | Token::Type => {
                    let s = self.parse_schema_decl()?;
                    program.schemas.push(s);
                }
                Token::Import => {
                    self.bump();
                    let path = match self.bump() {
                        Token::Str(s) => s,
                        other => return Err(ParseError(format!(
                            "expected string literal after 'import', got {:?}", other))),
                    };
                    program.imports.push(path);
                }
                other => {
                    return Err(ParseError(format!(
                        "expected schema/claim/type/import, got {:?}", other)));
                }
            }
        }
        Ok(program)
    }

    /// Parse a `subclaim Name` body item. Same indented-body shape as
    /// a top-level schema decl, but produces a `SubclaimDecl` body item
    /// so the runtime can register it for later lookup.
    fn parse_subclaim(&mut self) -> Result<BodyItem> {
        self.bump(); // subclaim keyword
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected name after subclaim, got {:?}", other))),
        };
        let body = self.parse_indented_body()?;
        Ok(BodyItem::SubclaimDecl(SchemaDecl {
            keyword: Keyword::Subclaim, name, body,
        }))
    }

    /// Parse zero or more body items at a single indent level. Used by
    /// both top-level schema decls and subclaims. Stops when the next
    /// Indent is at a different level (or there's no Indent at all).
    fn parse_indented_body(&mut self) -> Result<Vec<BodyItem>> {
        self.skip_blank_newlines();
        let body_indent = match self.peek() {
            Token::Indent(n) if *n > 0 => *n,
            _ => return Ok(vec![]),
        };
        let mut body = Vec::new();
        loop {
            match self.peek() {
                Token::Indent(n) if *n == body_indent => { self.bump(); }
                _ => break,
            }
            let item = self.parse_body_item()?;
            body.push(item);
            match self.peek() {
                Token::Newline => { self.bump(); }
                Token::Eof => break,
                _ => {}
            }
        }
        Ok(body)
    }

    fn parse_schema_decl(&mut self) -> Result<SchemaDecl> {
        let keyword = match self.bump() {
            Token::Schema => Keyword::Schema,
            Token::Claim  => Keyword::Claim,
            Token::Type   => Keyword::Type,
            other => return Err(ParseError(format!(
                "expected keyword, got {:?}", other))),
        };
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected schema name, got {:?}", other))),
        };
        let body = self.parse_indented_body()?;
        Ok(SchemaDecl { keyword, name, body })
    }

    fn parse_body_item(&mut self) -> Result<BodyItem> {
        // Three shapes:
        //   ..IDENT                                 → Passthrough composition
        //   IDENT IN IDENT (followed by line-end)   → Membership declaration
        //   <expr>                                  → Constraint
        // Anything else with `∈` (e.g. `x ∈ {1, 2}` or `x ∈ pts`) parses
        // as an expression and ends up as a Constraint.

        // Passthrough: `..ClaimName` at body-item start.
        if matches!(self.peek(), Token::DotDot) {
            self.bump();
            match self.bump() {
                Token::Ident(name) => return Ok(BodyItem::Passthrough(name)),
                other => return Err(ParseError(format!(
                    "expected claim name after '..', got {:?}", other))),
            }
        }

        // Subclaim: `subclaim Name` followed by an indented body. Same
        // shape as a top-level schema decl. The runtime-loader pulls
        // the inner SchemaDecl out and registers it under its name so
        // ClaimCall / passthrough can reference it.
        if matches!(self.peek(), Token::Subclaim) {
            return self.parse_subclaim();
        }

        // ClaimCall: `IDENT(slot mapsto value, …)` at body-item start.
        // Distinguished from a parenthesized expression by the IDENT
        // immediately followed by `(`.
        if let Token::Ident(_) = self.peek() {
            if matches!(self.toks.get(self.pos + 1), Some(Token::LParen)) {
                let name = match self.bump() {
                    Token::Ident(s) => s,
                    _ => unreachable!(),
                };
                self.eat(&Token::LParen)?;
                let mut mappings = Vec::new();
                if !matches!(self.peek(), Token::RParen) {
                    loop {
                        let slot = match self.bump() {
                            Token::Ident(s) => s,
                            other => return Err(ParseError(format!(
                                "expected mapping slot name, got {:?}", other))),
                        };
                        self.eat(&Token::MapsTo)?;
                        let value = self.parse_expr()?;
                        mappings.push(crate::ast::Mapping { slot, value });
                        if matches!(self.peek(), Token::Comma) { self.bump(); continue; }
                        break;
                    }
                }
                self.eat(&Token::RParen)?;
                return Ok(BodyItem::ClaimCall { name, mappings });
            }
        }
        if let Token::Ident(_) = self.peek() {
            let saved = self.pos;
            let lhs_name = match self.bump() {
                Token::Ident(s) => s,
                _ => unreachable!(),
            };
            if matches!(self.peek(), Token::In) {
                self.bump();
                if let Token::Ident(head) = self.peek().clone() {
                    // Two type-name shapes accepted:
                    //   - bare ident followed by line-end:  `n ∈ Nat`
                    //   - compound `Ident(Ident)` followed by line-end:
                    //     `s ∈ Seq(Int)` or `t ∈ Set(Bool)` etc.
                    let after_head = self.toks.get(self.pos + 1);
                    let plain_terminated = matches!(after_head,
                        Some(Token::Newline) | Some(Token::Eof) | Some(Token::Indent(_)) | None);
                    let compound = matches!(after_head, Some(Token::LParen));
                    if plain_terminated {
                        self.bump();
                        return Ok(BodyItem::Membership { name: lhs_name, type_name: head });
                    }
                    if compound {
                        // Greedy: consume `Ident ( inner )` and stitch a
                        // single string. Inner can be Ident or another
                        // compound — recursive shape, but for v0.5 we
                        // only support one level deep (Seq(Int), Set(String)).
                        let saved2 = self.pos;
                        self.bump();           // outer ident
                        self.bump();           // (
                        if let Token::Ident(inner) = self.peek().clone() {
                            let after_inner = self.toks.get(self.pos + 1);
                            if matches!(after_inner, Some(Token::RParen)) {
                                self.bump();   // inner ident
                                self.bump();   // )
                                let after = self.toks.get(self.pos);
                                let line_end = matches!(after,
                                    Some(Token::Newline) | Some(Token::Eof)
                                    | Some(Token::Indent(_)) | None);
                                if line_end {
                                    let type_name = format!("{}({})", head, inner);
                                    return Ok(BodyItem::Membership { name: lhs_name, type_name });
                                }
                            }
                        }
                        self.pos = saved2;
                    }
                }
                // Not a Membership — back up and parse the whole line as expr.
                self.pos = saved;
            } else {
                self.pos = saved;
            }
        }
        let e = self.parse_expr()?;
        Ok(BodyItem::Constraint(e))
    }

    // Operator precedence (low → high):
    //   implies        : right-assoc
    //   or             : left
    //   and            : left
    //   compare        : non-assoc (=, ≠, <, ≤, >, ≥)
    //   add/sub        : left
    //   mul/div        : left
    //   unary not / -
    //   atoms          : ident, int, paren

    fn parse_expr(&mut self) -> Result<Expr> {
        // Quantifier expressions are right at the top so the colon-separated
        // body picks up everything to the right.
        if matches!(self.peek(), Token::ForAll | Token::Exists) {
            return self.parse_quantifier();
        }
        self.parse_implies()
    }

    fn parse_quantifier(&mut self) -> Result<Expr> {
        let is_forall = matches!(self.peek(), Token::ForAll);
        self.bump();
        let var = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected bound variable name, got {:?}", other))),
        };
        self.eat(&Token::In)?;
        let range = self.parse_atom()?;   // expect a {lo..hi} or {a, b, c}
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
                        Expr::Forall(var, Box::new(range), Box::new(body))
                    } else {
                        Expr::Exists(var, Box::new(range), Box::new(body))
                    });
                }
            } else {
                self.pos = saved;
            }
        }
        let body = self.parse_expr()?;
        Ok(if is_forall {
            Expr::Forall(var, Box::new(range), Box::new(body))
        } else {
            Expr::Exists(var, Box::new(range), Box::new(body))
        })
    }

    fn parse_implies(&mut self) -> Result<Expr> {
        // Quantifiers are valid wherever an `⇒` operand is — they live
        // at the same precedence as `⇒`. Without this, `A ⇒ ∀ i : B`
        // and the implies-block form `A ⇒\n    ∀ i : B` both fail
        // (the body iteration recurses through parse_implies).
        if matches!(self.peek(), Token::ForAll | Token::Exists) {
            return self.parse_quantifier();
        }
        let lhs = self.parse_or()?;
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
            return Ok(Expr::Binary(op, Box::new(lhs), Box::new(rhs)));
        }
        Ok(lhs)
    }

    fn parse_addsub(&mut self) -> Result<Expr> {
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
                        other => return Err(ParseError(format!(
                            "expected field name after '.', got {:?}", other))),
                    }
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_atom(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            Token::Int(n)   => { self.bump(); Ok(Expr::Int(n)) }
            Token::Str(s)   => { self.bump(); Ok(Expr::Str(s)) }
            Token::True     => { self.bump(); Ok(Expr::Bool(true)) }
            Token::False    => { self.bump(); Ok(Expr::Bool(false)) }
            Token::Ident(s) => {
                self.bump();
                // Greedily consume `.ident` chains (sub-schema field access)
                // and collapse into a single dotted Identifier.
                let mut name = s;
                while matches!(self.peek(), Token::Dot) {
                    self.bump();
                    match self.bump() {
                        Token::Ident(field) => { name.push('.'); name.push_str(&field); }
                        other => return Err(ParseError(format!(
                            "expected field name after '.', got {:?}", other))),
                    }
                }
                Ok(Expr::Identifier(name))
            }
            Token::LParen   => {
                self.bump();
                let e = self.parse_expr()?;
                self.eat(&Token::RParen)?;
                Ok(e)
            }
            Token::LBrace => {
                self.bump();
                // Empty `{}` is a (vacuous) set literal.
                if matches!(self.peek(), Token::RBrace) {
                    self.bump();
                    return Ok(Expr::SetLit(vec![]));
                }
                let first = self.parse_expr()?;
                // Decide between range `{lo..hi}` and set `{a, b, c}`.
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
                // `⟨e1, e2, …⟩` sequence literal. Distinct from `{…}` set
                // literal — pinned by element index, not membership-only.
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

pub fn parse(src: &str) -> Result<Program> {
    let toks = crate::lexer::tokenize(src).map_err(|e| ParseError(e.to_string()))?;
    Parser::new(toks).parse_program()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_nat() {
        let p = parse("schema SimpleNat\n    n ∈ Nat\n    n > 5\n").unwrap();
        assert_eq!(p.schemas.len(), 1);
        let s = &p.schemas[0];
        assert_eq!(s.name, "SimpleNat");
        assert!(matches!(s.keyword, Keyword::Schema));
        assert_eq!(s.body.len(), 2);
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name }
            if name == "n" && type_name == "Nat"));
        assert!(matches!(&s.body[1], BodyItem::Constraint(_)));
    }

    #[test]
    fn parse_cardinality_and_index() {
        // Even though the translator doesn't run these yet, the AST
        // shape should be settled.
        let p = parse("schema S\n    s ∈ Seq(Int)\n    #s = 3\n    s[0] > 0\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 3);
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name }
            if name == "s" && type_name == "Seq(Int)"));
        // #s = 3
        match &s.body[1] {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, _)) => {
                assert!(matches!(lhs.as_ref(), Expr::Cardinality(_)));
            }
            other => panic!("expected #s = 3 constraint, got {:?}", other),
        }
        // s[0] > 0
        match &s.body[2] {
            BodyItem::Constraint(Expr::Binary(BinOp::Gt, lhs, _)) => {
                assert!(matches!(lhs.as_ref(), Expr::Index(_, _)));
            }
            other => panic!("expected s[0] > 0 constraint, got {:?}", other),
        }
    }

    #[test]
    fn parse_arithmetic_constraint() {
        // n > 5 + 3 * 2  →  n > (5 + (3 * 2))
        let p = parse("schema X\n    n ∈ Nat\n    n > 5 + 3 * 2\n").unwrap();
        let s = &p.schemas[0];
        let constraint = match &s.body[1] {
            BodyItem::Constraint(e) => e,
            _ => panic!(),
        };
        // Top should be a > comparison; right side should be 5 + (3*2)
        match constraint {
            Expr::Binary(BinOp::Gt, _, rhs) => match rhs.as_ref() {
                Expr::Binary(BinOp::Add, _, r2) => match r2.as_ref() {
                    Expr::Binary(BinOp::Mul, _, _) => {}
                    other => panic!("expected Mul on rhs, got {:?}", other),
                }
                other => panic!("expected Add at top, got {:?}", other),
            }
            other => panic!("expected Gt, got {:?}", other),
        }
    }
}
