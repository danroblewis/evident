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
                other => {
                    return Err(ParseError(format!(
                        "expected schema/claim/type, got {:?}", other)));
                }
            }
        }
        Ok(program)
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
        // Optional newline + indented body.
        self.skip_blank_newlines();
        let body_indent = match self.peek() {
            Token::Indent(n) if *n > 0 => *n,
            _ => return Ok(SchemaDecl { keyword, name, body: vec![] }),
        };
        let mut body = Vec::new();
        loop {
            // Each body item starts with Indent(body_indent).
            match self.peek() {
                Token::Indent(n) if *n == body_indent => { self.bump(); }
                _ => break,
            }
            // Now parse one body line.
            let item = self.parse_body_item()?;
            body.push(item);
            // Consume the trailing newline (or EOF).
            match self.peek() {
                Token::Newline => { self.bump(); }
                Token::Eof => break,
                _ => {}
            }
        }
        Ok(SchemaDecl { keyword, name, body })
    }

    fn parse_body_item(&mut self) -> Result<BodyItem> {
        // Two shapes:
        //   IDENT IN IDENT (followed by line-end)  → Membership declaration
        //   <expr>                                  → Constraint
        // Anything else with `∈` (e.g. `x ∈ {1, 2}` or `x ∈ pts`) parses
        // as an expression and ends up as a Constraint.
        if let Token::Ident(_) = self.peek() {
            let saved = self.pos;
            let lhs_name = match self.bump() {
                Token::Ident(s) => s,
                _ => unreachable!(),
            };
            if matches!(self.peek(), Token::In) {
                self.bump();
                if let Token::Ident(type_name) = self.peek().clone() {
                    // Lookahead: is this the end of the line? If so, it's a
                    // type declaration. If the next token is `.` or any
                    // expression-continuation, treat the IDENT as the start
                    // of a constraint expression.
                    let after_type = self.toks.get(self.pos + 1);
                    let line_terminator = matches!(after_type,
                        Some(Token::Newline) | Some(Token::Eof) | Some(Token::Indent(_)) | None);
                    if line_terminator {
                        self.bump(); // consume the type ident
                        return Ok(BodyItem::Membership { name: lhs_name, type_name });
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
        let body = self.parse_expr()?;
        Ok(if is_forall {
            Expr::Forall(var, Box::new(range), Box::new(body))
        } else {
            Expr::Exists(var, Box::new(range), Box::new(body))
        })
    }

    fn parse_implies(&mut self) -> Result<Expr> {
        let lhs = self.parse_or()?;
        if matches!(self.peek(), Token::Implies) {
            self.bump();
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
                Token::Plus  => BinOp::Add,
                Token::Minus => BinOp::Sub,
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
        self.parse_atom()
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
