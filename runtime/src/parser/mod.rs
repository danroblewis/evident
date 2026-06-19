use crate::core::ast::*;
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

        if !matches!(self.peek(), Token::Indent(0)) {

        } else {
            self.bump();
        }
        loop {
            self.skip_blank_newlines();

            while let Token::Indent(_) = self.peek() {
                self.bump();
            }
            match self.peek() {
                Token::Eof => break,
                Token::Schema | Token::Claim | Token::Type | Token::Fsm
                | Token::External => {
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
                Token::Enum => {
                    let e = self.parse_enum_decl()?;
                    program.enums.push(e);
                }
                other => {
                    return Err(ParseError(format!(
                        "expected schema/claim/type/import/enum, got {:?}", other)));
                }
            }
        }
        Ok(program)
    }

    fn parse_enum_decl(&mut self) -> Result<crate::core::ast::EnumDecl> {
        self.bump();
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected enum name, got {:?}", other))),
        };
        match self.bump() {
            Token::Eq => {}
            other => return Err(ParseError(format!(
                "expected '=' after enum name, got {:?}", other))),
        }

        let mut block_indent: Option<usize> = None;
        if matches!(self.peek(), Token::Newline) {
            let saved = self.pos;
            self.bump();
            while matches!(self.peek(), Token::Newline) { self.bump(); }
            if let Token::Indent(n) = self.peek().clone() {
                if n > 0 {
                    block_indent = Some(n);
                    self.bump();

                    if matches!(self.peek(), Token::Pipe) { self.bump(); }
                } else {
                    self.pos = saved;
                }
            } else {
                self.pos = saved;
            }
        }
        let mut variants = Vec::new();
        loop {
            let v_name = match self.bump() {
                Token::Ident(s) => s,
                other => return Err(ParseError(format!(
                    "expected variant name in enum, got {:?}", other))),
            };

            let mut fields: Vec<crate::core::ast::EnumField> = Vec::new();
            if matches!(self.peek(), Token::LParen) {
                self.bump();
                if matches!(self.peek(), Token::RParen) {
                    return Err(ParseError(format!(
                        "variant `{}` has empty payload — drop the parens for nullary",
                        v_name)));
                }
                let mut idx = 0usize;
                loop {
                    let field_type = self.parse_enum_field_type(&v_name)?;
                    fields.push(crate::core::ast::EnumField {
                        name: format!("f{}", idx),
                        type_name: field_type,
                    });
                    idx += 1;
                    if matches!(self.peek(), Token::Comma) {
                        self.bump();
                        continue;
                    }
                    break;
                }
                match self.bump() {
                    Token::RParen => {}
                    other => return Err(ParseError(format!(
                        "expected ')' after variant payload, got {:?}", other))),
                }
            }
            variants.push(crate::core::ast::EnumVariant {
                name: v_name,
                fields,
            });
            if matches!(self.peek(), Token::Pipe) {
                self.bump();
                continue;
            }

            if let Some(want) = block_indent {
                if matches!(self.peek(), Token::Newline) {
                    let cont_save = self.pos;
                    self.bump();
                    while matches!(self.peek(), Token::Newline) { self.bump(); }
                    if let Token::Indent(n) = self.peek().clone() {
                        if n == want {

                            let next_kind = self.toks.get(self.pos + 1);
                            let looks_like_variant = matches!(next_kind,
                                Some(Token::Ident(_)) | Some(Token::Pipe));
                            if looks_like_variant {
                                self.bump();
                                if matches!(self.peek(), Token::Pipe) {
                                    self.bump();
                                }
                                continue;
                            }
                        }
                    }
                    self.pos = cont_save;
                }
            }
            break;
        }
        if variants.is_empty() {
            return Err(ParseError(
                "enum must have at least one variant".to_string()));
        }
        Ok(crate::core::ast::EnumDecl { name, variants })
    }

    fn parse_enum_field_type(&mut self, v_name: &str) -> Result<String> {
        let head = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected field type in variant `{}`, got {:?}",
                v_name, other))),
        };

        if matches!(self.peek(), Token::LParen) {
            self.bump();
            let inner = self.parse_enum_field_type(v_name)?;
            match self.bump() {
                Token::RParen => {}
                other => return Err(ParseError(format!(
                    "expected ')' after compound type in variant `{}`, got {:?}",
                    v_name, other))),
            }
            return Ok(format!("{}({})", head, inner));
        }
        Ok(head)
    }

    fn parse_subclaim(&mut self) -> Result<BodyItem> {
        self.bump();
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected name after subclaim, got {:?}", other))),
        };

        let mut body = Vec::new();
        if matches!(self.peek(), Token::LParen) {
            body = self.parse_first_line_params()?;
        }
        let param_count = body.len();
        body.extend(self.parse_indented_body()?);
        Ok(BodyItem::SubclaimDecl(SchemaDecl {
            keyword: Keyword::Subclaim, name,
            body, param_count, external: false,
        }))
    }

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
            let items = self.parse_body_item()?;
            body.extend(items);
            match self.peek() {
                Token::Newline => { self.bump(); }
                Token::Eof => break,
                _ => {}
            }
        }
        Ok(body)
    }

    fn parse_schema_decl(&mut self) -> Result<SchemaDecl> {

        let external = if matches!(self.peek(), Token::External) {
            self.bump();
            true
        } else {
            false
        };
        let keyword = match self.bump() {
            Token::Schema => {
                if external {
                    return Err(ParseError(
                        "`external schema` is not allowed — use \
                         `external type` (`schema` is deprecated anyway)".to_string()));
                }
                Keyword::Schema
            }
            Token::Claim  => Keyword::Claim,
            Token::Type   => Keyword::Type,
            Token::Fsm    => Keyword::Fsm,
            other => return Err(ParseError(format!(
                "expected keyword, got {:?}", other))),
        };
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected schema name, got {:?}", other))),
        };

        let mut body = Vec::new();
        if matches!(self.peek(), Token::LParen) {
            body = self.parse_first_line_params()?;
        }
        let param_count = body.len();
        body.extend(self.parse_indented_body()?);
        Ok(SchemaDecl { keyword, name, body, param_count, external })
    }

    fn parse_first_line_params(&mut self) -> Result<Vec<BodyItem>> {
        self.eat(&Token::LParen)?;
        let mut items = Vec::new();
        if matches!(self.peek(), Token::RParen) {
            self.bump();
            return Ok(items);
        }
        loop {

            let mut names = Vec::new();
            loop {
                match self.bump() {
                    Token::Ident(s) => names.push(s),
                    other => return Err(ParseError(format!(
                        "expected param name, got {:?}", other))),
                }
                match self.peek() {
                    Token::Comma => { self.bump(); continue; }
                    Token::In => { self.bump(); break; }
                    other => return Err(ParseError(format!(
                        "expected ',' or '∈' after param name, got {:?}", other))),
                }
            }

            let head = match self.bump() {
                Token::Ident(s) => s,
                other => return Err(ParseError(format!(
                    "expected type name in first-line params, got {:?}", other))),
            };
            let type_name = if matches!(head.as_str(), "Seq" | "Set")
                && matches!(self.peek(), Token::LParen)
            {
                self.bump();
                let inner = match self.bump() {
                    Token::Ident(s) => s,
                    other => return Err(ParseError(format!(
                        "expected inner type for {}, got {:?}", head, other))),
                };
                self.eat(&Token::RParen)?;
                format!("{}({})", head, inner)
            } else {
                head
            };
            for n in names {
                items.push(BodyItem::Membership {
                    name: n,
                    type_name: type_name.clone(),
                    pins: crate::core::ast::Pins::None,
                });
            }
            match self.peek() {
                Token::Comma => { self.bump(); continue; }
                Token::RParen => { self.bump(); break; }
                other => return Err(ParseError(format!(
                    "expected ',' or ')' after param group, got {:?}", other))),
            }
        }
        Ok(items)
    }

    fn try_parse_chained_membership(&mut self) -> Result<Option<Vec<BodyItem>>> {
        let saved = self.pos;

        let first = match self.parse_addsub() {
            Ok(e) => e,
            Err(_) => { self.pos = saved; return Ok(None); }
        };
        let mut operands: Vec<Expr> = vec![first];
        let mut ops: Vec<BinOp> = Vec::new();

        let mut membership_at: Option<(usize, String, Vec<String>)> = None;

        loop {

            let mut extra_names: Vec<String> = Vec::new();
            if matches!(self.peek(), Token::Comma) {
                let last_is_bare = matches!(operands.last(),
                    Some(Expr::Identifier(s)) if !s.contains('.'));
                if last_is_bare {
                    let mn_save = self.pos;
                    let mut names: Vec<String> = Vec::new();
                    while matches!(self.peek(), Token::Comma) {
                        let inner_save = self.pos;
                        self.bump();
                        if let Token::Ident(s) = self.peek().clone() {

                            let next_after = self.toks.get(self.pos + 1);
                            if matches!(next_after, Some(Token::Comma) | Some(Token::In)) {
                                self.bump();
                                names.push(s);
                                continue;
                            }
                        }
                        self.pos = inner_save;
                        break;
                    }
                    if matches!(self.peek(), Token::In) && !names.is_empty() {
                        extra_names = names;
                    } else {

                        self.pos = mn_save;
                    }
                }
            }

            if matches!(self.peek(), Token::In) {
                if membership_at.is_some() {

                    self.pos = saved;
                    return Ok(None);
                }
                self.bump();

                let head = match self.peek().clone() {
                    Token::Ident(s) => s,
                    _ => { self.pos = saved; return Ok(None); }
                };
                let after_head = self.toks.get(self.pos + 1).cloned();
                let after_chain_class = |t: &Option<Token>| matches!(t,
                    Some(Token::Newline) | Some(Token::Eof) | Some(Token::Indent(_)) | None
                ) || peek_compare_op(t.as_ref().unwrap_or(&Token::Eof)).is_some();

                let is_compound = matches!(after_head, Some(Token::LParen))
                    && matches!(self.toks.get(self.pos + 2), Some(Token::Ident(_)))
                    && matches!(self.toks.get(self.pos + 3), Some(Token::RParen));

                let type_name = if is_compound {
                    self.bump();
                    self.bump();
                    let inner = match self.bump() {
                        Token::Ident(s) => s,
                        _ => unreachable!(),
                    };
                    self.bump();
                    let after = self.toks.get(self.pos).cloned();
                    if !after_chain_class(&after) {
                        self.pos = saved;
                        return Ok(None);
                    }
                    format!("{}({})", head, inner)
                } else {
                    if !after_chain_class(&after_head) {
                        self.pos = saved;
                        return Ok(None);
                    }
                    self.bump();
                    head
                };

                let var_idx = operands.len() - 1;
                let first_name = match &operands[var_idx] {
                    Expr::Identifier(s) if !s.contains('.') => s.clone(),
                    _ => { self.pos = saved; return Ok(None); }
                };
                let mut all_names = vec![first_name];
                all_names.extend(extra_names);
                membership_at = Some((var_idx, type_name, all_names));
                continue;
            }
            if let Some(op) = peek_compare_op(self.peek()) {
                self.bump();
                let rhs = match self.parse_addsub() {
                    Ok(e) => e,
                    Err(_) => { self.pos = saved; return Ok(None); }
                };
                operands.push(rhs);
                ops.push(op);
                continue;
            }
            break;
        }

        let Some((var_idx, type_name, names)) = membership_at else {
            self.pos = saved;
            return Ok(None);
        };

        if !matches!(self.peek(),
            Token::Newline | Token::Eof | Token::Indent(_)
        ) {
            self.pos = saved;
            return Ok(None);
        }

        let mut items: Vec<BodyItem> = names.iter().map(|n| BodyItem::Membership {
            name: n.clone(),
            type_name: type_name.clone(),
            pins: crate::core::ast::Pins::None,
        }).collect();
        for name in &names {
            let var_expr = Expr::Identifier(name.clone());
            for (i, op) in ops.iter().enumerate() {
                let lhs = if i == var_idx { var_expr.clone() } else { operands[i].clone() };
                let rhs = if i + 1 == var_idx { var_expr.clone() } else { operands[i + 1].clone() };
                items.push(BodyItem::Constraint(Expr::Binary(
                    op.clone(), Box::new(lhs), Box::new(rhs),
                )));
            }
        }
        Ok(Some(items))
    }

    fn try_parse_type_and_pins(&mut self, head: &str)
        -> Result<Option<(String, crate::core::ast::Pins)>>
    {
        let after_head = self.toks.get(self.pos + 1);
        let plain_terminated = matches!(after_head,
            Some(Token::Newline) | Some(Token::Eof) | Some(Token::Indent(_)) | None);
        let has_paren = matches!(after_head, Some(Token::LParen));
        if plain_terminated {
            self.bump();
            return Ok(Some((head.to_string(), crate::core::ast::Pins::None)));
        }
        if has_paren {
            let inside_first = self.toks.get(self.pos + 2);
            let inside_second = self.toks.get(self.pos + 3);
            let is_named_pin = matches!(inside_first, Some(Token::Ident(_)))
                && matches!(inside_second, Some(Token::MapsTo));

            let looks_like_compound = matches!(inside_first, Some(Token::Ident(_)))
                && matches!(inside_second, Some(Token::RParen));
            let is_known_compound_head =
                matches!(head, "Seq" | "Set");

            if is_named_pin {
                self.bump();
                self.bump();
                let mut pins = Vec::new();
                loop {
                    let slot = match self.bump() {
                        Token::Ident(s) => s,
                        other => return Err(ParseError(format!(
                            "expected pin slot name, got {:?}", other))),
                    };
                    self.eat(&Token::MapsTo)?;
                    let value = self.parse_expr()?;
                    pins.push(crate::core::ast::Mapping { slot, value });
                    if matches!(self.peek(), Token::Comma) { self.bump(); continue; }
                    break;
                }
                self.eat(&Token::RParen)?;
                return Ok(Some((head.to_string(), crate::core::ast::Pins::Named(pins))));
            } else if is_known_compound_head && looks_like_compound {
                self.bump();
                self.bump();
                let inner = match self.bump() {
                    Token::Ident(s) => s,
                    _ => unreachable!(),
                };
                self.bump();
                let after = self.toks.get(self.pos);
                let line_end = matches!(after,
                    Some(Token::Newline) | Some(Token::Eof)
                    | Some(Token::Indent(_)) | None);
                if line_end {
                    return Ok(Some((format!("{}({})", head, inner), crate::core::ast::Pins::None)));
                }
                return Ok(None);
            } else {

                self.bump();
                self.bump();
                let mut args = Vec::new();
                if !matches!(self.peek(), Token::RParen) {
                    loop {
                        args.push(self.parse_expr()?);
                        if matches!(self.peek(), Token::Comma) {
                            self.bump(); continue;
                        }
                        break;
                    }
                }
                self.eat(&Token::RParen)?;
                return Ok(Some((head.to_string(), crate::core::ast::Pins::Positional(args))));
            }
        }
        Ok(None)
    }

    fn parse_body_item(&mut self) -> Result<Vec<BodyItem>> {

        if matches!(self.peek(), Token::DotDot) {
            self.bump();
            match self.bump() {
                Token::Ident(name) => return Ok(vec![BodyItem::Passthrough(name)]),
                other => return Err(ParseError(format!(
                    "expected claim name after '..', got {:?}", other))),
            }
        }

        if matches!(self.peek(), Token::Subclaim) {
            return Ok(vec![self.parse_subclaim()?]);
        }

        if let Token::Ident(_) = self.peek() {
            let lparen_offset: Option<usize> =
                if matches!(self.toks.get(self.pos + 1), Some(Token::LParen)) {
                    Some(1)
                } else {
                    None
                };
            if let Some(lp) = lparen_offset {
                if matches!(self.toks.get(self.pos + lp), Some(Token::LParen)) {
                    let inside_first = self.toks.get(self.pos + lp + 1);
                    let inside_second = self.toks.get(self.pos + lp + 2);
                    let is_claim_call = matches!(inside_first, Some(Token::Ident(_)))
                        && matches!(inside_second, Some(Token::MapsTo));
                    if is_claim_call {
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
                                mappings.push(crate::core::ast::Mapping { slot, value });
                                if matches!(self.peek(), Token::Comma) { self.bump(); continue; }
                                break;
                            }
                        }
                        self.eat(&Token::RParen)?;
                        return Ok(vec![BodyItem::ClaimCall { name, mappings }]);
                    }
                }

            }
        }

        if let Some(items) = self.try_parse_chained_membership()? {
            return Ok(items);
        }

        if let Token::Ident(_) = self.peek() {
            let saved = self.pos;
            let mut lhs_names = match self.bump() {
                Token::Ident(s) => vec![s],
                _ => unreachable!(),
            };

            while matches!(self.peek(), Token::Comma) {
                let inner_save = self.pos;
                self.bump();
                if let Token::Ident(next_name) = self.peek().clone() {
                    let next_after = self.toks.get(self.pos + 1);
                    if matches!(next_after, Some(Token::Comma) | Some(Token::In)) {
                        self.bump();
                        lhs_names.push(next_name);
                        continue;
                    }
                }
                self.pos = inner_save;
                break;
            }
            if matches!(self.peek(), Token::In) {
                self.bump();
                if let Token::Ident(head) = self.peek().clone() {
                    if let Some((type_name, pins)) = self.try_parse_type_and_pins(&head)? {
                        return Ok(lhs_names.into_iter().map(|n| BodyItem::Membership {
                            name: n,
                            type_name: type_name.clone(),
                            pins: pins.clone(),
                        }).collect());
                    }
                }

                self.pos = saved;
            } else {
                self.pos = saved;
            }
        }
        let e = self.parse_expr()?;
        Ok(vec![BodyItem::Constraint(e)])
    }
}

mod exprs;

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
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "n" && type_name == "Nat"));
        assert!(matches!(&s.body[1], BodyItem::Constraint(_)));
    }

    #[test]
    fn parse_cardinality_and_index() {

        let p = parse("schema S\n    s ∈ Seq(Int)\n    #s = 3\n    s[0] > 0\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 3);
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "s" && type_name == "Seq(Int)"));

        match &s.body[1] {
            BodyItem::Constraint(Expr::Binary(BinOp::Eq, lhs, _)) => {
                assert!(matches!(lhs.as_ref(), Expr::Cardinality(_)));
            }
            other => panic!("expected #s = 3 constraint, got {:?}", other),
        }

        match &s.body[2] {
            BodyItem::Constraint(Expr::Binary(BinOp::Gt, lhs, _)) => {
                assert!(matches!(lhs.as_ref(), Expr::Index(_, _)));
            }
            other => panic!("expected s[0] > 0 constraint, got {:?}", other),
        }
    }

    #[test]
    fn parse_arithmetic_constraint() {

        let p = parse("schema X\n    n ∈ Nat\n    n > 5 + 3 * 2\n").unwrap();
        let s = &p.schemas[0];
        let constraint = match &s.body[1] {
            BodyItem::Constraint(e) => e,
            _ => panic!(),
        };

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

    #[test]
    fn parse_chained_membership_two_sided() {

        let p = parse("claim t\n    0 < pos_x ∈ Int < 5\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 3, "expected 3 body items, got {}", s.body.len());
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "pos_x" && type_name == "Int"));
        assert!(matches!(&s.body[1], BodyItem::Constraint(Expr::Binary(BinOp::Lt, _, _))));
        assert!(matches!(&s.body[2], BodyItem::Constraint(Expr::Binary(BinOp::Lt, _, _))));
    }

    #[test]
    fn parse_chained_membership_pin_form() {

        let p = parse("claim t\n    pos_x ∈ Int = 5\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 2);
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "pos_x" && type_name == "Int"));
        assert!(matches!(&s.body[1], BodyItem::Constraint(Expr::Binary(BinOp::Eq, _, _))));
    }

    #[test]
    fn parse_chained_membership_compound_type() {

        let p = parse("claim t\n    s ∈ Seq(Int)\n    #s = 3\n").unwrap();

        let s = &p.schemas[0];
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "s" && type_name == "Seq(Int)"));
    }

    #[test]
    fn parse_chained_membership_does_not_eat_set_membership() {

        let p = parse("claim t\n    pts ∈ Set(Int)\n    x ∈ Int\n    x ∈ pts ∧ x > 0\n").unwrap();
        let s = &p.schemas[0];

        match s.body.last().unwrap() {
            BodyItem::Constraint(Expr::Binary(BinOp::And, _, _)) => {}
            other => panic!("expected `(x ∈ pts) ∧ (x > 0)` constraint, got {:?}", other),
        }
    }

    #[test]
    fn parse_chained_membership_multi_name() {

        let p = parse("claim t\n    x, y, z ∈ Int < 5\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 6, "expected 3 Memberships + 3 Constraints");
        for (i, name) in ["x", "y", "z"].iter().enumerate() {
            assert!(matches!(&s.body[i], BodyItem::Membership { name: n, type_name, .. }
                if n == *name && type_name == "Int"));
        }
        for i in 3..6 {
            assert!(matches!(&s.body[i], BodyItem::Constraint(Expr::Binary(BinOp::Lt, _, _))));
        }
    }

    #[test]
    fn parse_chained_membership_multi_name_two_sided() {

        let p = parse("claim t\n    0 < x, y ∈ Int < 5\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 6);

        assert!(matches!(&s.body[0], BodyItem::Membership { name, .. } if name == "x"));
        assert!(matches!(&s.body[1], BodyItem::Membership { name, .. } if name == "y"));

        for i in 2..6 {
            assert!(matches!(&s.body[i], BodyItem::Constraint(Expr::Binary(BinOp::Lt, _, _))));
        }
    }

    #[test]
    fn parse_enum_decl_basic() {
        let p = parse("enum Day = Mon | Tue | Wed\n").unwrap();
        assert_eq!(p.enums.len(), 1);
        let e = &p.enums[0];
        assert_eq!(e.name, "Day");
        let names: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["Mon", "Tue", "Wed"]);
        assert!(e.variants.iter().all(|v| v.fields.is_empty()));
    }

    #[test]
    fn parse_enum_decl_alongside_claim() {
        let p = parse("enum Color = Red | Green | Blue\n\nclaim t\n    c ∈ Color\n").unwrap();
        assert_eq!(p.enums.len(), 1);
        assert_eq!(p.schemas.len(), 1);
    }

    #[test]
    fn parse_enum_decl_single_variant_ok() {
        let p = parse("enum Singleton = Only\n").unwrap();
        assert_eq!(p.enums[0].variants.len(), 1);
        assert_eq!(p.enums[0].variants[0].name, "Only");
        assert!(p.enums[0].variants[0].fields.is_empty());
    }

    #[test]
    fn parse_enum_decl_payload_variants() {
        let p = parse("enum Result = Ok(Int) | Err(String)\n").unwrap();
        let e = &p.enums[0];
        assert_eq!(e.variants.len(), 2);
        assert_eq!(e.variants[0].name, "Ok");
        assert_eq!(e.variants[0].fields.len(), 1);
        assert_eq!(e.variants[0].fields[0].name, "f0");
        assert_eq!(e.variants[0].fields[0].type_name, "Int");
        assert_eq!(e.variants[1].name, "Err");
        assert_eq!(e.variants[1].fields[0].type_name, "String");
    }

    #[test]
    fn parse_enum_decl_recursive_self_reference() {
        let p = parse("enum LinkedList = Nil | Cons(Int, LinkedList)\n").unwrap();
        let e = &p.enums[0];
        assert_eq!(e.variants.len(), 2);
        assert_eq!(e.variants[1].name, "Cons");
        assert_eq!(e.variants[1].fields.len(), 2);
        assert_eq!(e.variants[1].fields[0].type_name, "Int");
        assert_eq!(e.variants[1].fields[1].type_name, "LinkedList");
    }

    #[test]
    fn parse_enum_decl_mixed_arities() {
        let p = parse("enum Maybe = None | Some(Int)\n").unwrap();
        let e = &p.enums[0];
        assert!(e.variants[0].fields.is_empty());
        assert_eq!(e.variants[1].fields.len(), 1);
    }

    #[test]
    fn parse_enum_decl_multiline_no_leading_pipe() {
        let p = parse(
            "enum Expr =\n    ENum(Int)\n    EVar(String)\n    EAdd(Expr, Expr)\n"
        ).unwrap();
        let e = &p.enums[0];
        assert_eq!(e.variants.len(), 3);
        assert_eq!(e.variants[0].name, "ENum");
        assert_eq!(e.variants[1].name, "EVar");
        assert_eq!(e.variants[2].name, "EAdd");
    }

    #[test]
    fn parse_enum_decl_multiline_with_leading_pipe() {
        let p = parse(
            "enum Color =\n    | Red\n    | Green\n    | Blue\n"
        ).unwrap();
        let e = &p.enums[0];
        assert_eq!(e.variants.len(), 3);
        let names: Vec<&str> = e.variants.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["Red", "Green", "Blue"]);
    }

    #[test]
    fn parse_enum_decl_forward_reference_parses() {

        let p = parse(
            "enum Expr = Lit(Int) | Op(BinOp, Expr, Expr)\nenum BinOp = Add | Sub\n"
        ).unwrap();
        assert_eq!(p.enums.len(), 2);
        assert_eq!(p.enums[0].name, "Expr");
        assert_eq!(p.enums[1].name, "BinOp");

        assert_eq!(p.enums[0].variants[1].name, "Op");
        assert_eq!(p.enums[0].variants[1].fields[0].type_name, "BinOp");
    }

    #[test]
    fn parse_enum_decl_mutual_recursion_parses() {

        let p = parse(
            "enum Expr = ENum(Int) | EBlock(Stmt)\nenum Stmt = SExpr(Expr) | SSeq(Stmt, Stmt)\n"
        ).unwrap();
        assert_eq!(p.enums.len(), 2);

        assert_eq!(p.enums[0].variants[1].fields[0].type_name, "Stmt");

        assert_eq!(p.enums[1].variants[0].fields[0].type_name, "Expr");
    }

    #[test]
    fn parse_enum_decl_empty_payload_errors() {

        assert!(parse("enum X = Foo() | Bar\n").is_err());
    }

    #[test]
    fn parse_enum_decl_no_variants_errors() {

        assert!(parse("enum Empty =\n").is_err());
    }

    #[test]
    fn parse_chained_membership_rejects_dotted_lhs() {

        assert!(parse("claim t\n    state.x ∈ Int < 5\n").is_err());
    }

    #[test]
    fn parse_comparison_ops_after_generics_removal() {

        let p = parse("claim t\n    a ∈ Int\n    b ∈ Int\n    a < b\n    b > 5\n").unwrap();
        let s = &p.schemas[0];
        match s.body[2] {
            BodyItem::Constraint(Expr::Binary(BinOp::Lt, _, _)) => {}
            ref other => panic!("expected `a < b`, got {:?}", other),
        }
        match s.body[3] {
            BodyItem::Constraint(Expr::Binary(BinOp::Gt, _, _)) => {}
            ref other => panic!("expected `b > 5`, got {:?}", other),
        }
    }

    #[test]
    fn parse_generic_type_params_no_longer_accepted() {

        assert!(parse("type Edge<T>\n    from ∈ Int\n").is_err());
    }
}
