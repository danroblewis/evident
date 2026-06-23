use crate::core::ast::*;
use crate::lexer::Token;

#[derive(Debug)]
pub struct ParseError {
    pub msg: String,
    /// 1-based `(line, col)` of the offending token, when known.
    pub loc: Option<(usize, usize)>,
}

impl ParseError {
    /// A positionless parse error (e.g. wrapping a lex error that already
    /// carries its own location text).
    pub fn new(msg: impl Into<String>) -> Self {
        ParseError { msg: msg.into(), loc: None }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.loc {
            Some((line, col)) => write!(f, "parse error at line {}, col {}: {}", line, col, self.msg),
            None => write!(f, "parse error: {}", self.msg),
        }
    }
}

impl std::error::Error for ParseError {}

type Result<T> = std::result::Result<T, ParseError>;

pub struct Parser {
    toks: Vec<Token>,
    /// `(line, col)` parallel to `toks` (same index), from the lexer.
    locs: Vec<(usize, usize)>,
    pos: usize,
}

impl Parser {
    pub fn with_locs(toks: Vec<Token>, locs: Vec<(usize, usize)>) -> Self {
        Parser { toks, locs, pos: 0 }
    }

    /// Build a parse error stamped with the current token's `(line, col)`.
    fn err(&self, msg: impl Into<String>) -> ParseError {
        let loc = self.locs.get(self.pos).copied().filter(|&(l, _)| l != 0);
        ParseError { msg: msg.into(), loc }
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
            Err(self.err(format!("expected {:?}, got {:?}", expected, self.peek())))
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
                Token::Schema | Token::Claim | Token::Type | Token::Fsm | Token::Fti
                | Token::External => {
                    let s = self.parse_schema_decl()?;
                    program.schemas.push(s);
                }
                Token::Import => {
                    self.bump();
                    let path = match self.bump() {
                        Token::Str(s) => s,
                        other => return Err(self.err(format!(
                            "expected string literal after 'import', got {:?}", other))),
                    };
                    program.imports.push(path);
                }
                Token::Enum => {
                    let e = self.parse_enum_decl()?;
                    program.enums.push(e);
                }
                other => {
                    return Err(self.err(format!(
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
            other => return Err(self.err(format!(
                "expected enum name, got {:?}", other))),
        };
        match self.bump() {
            Token::Eq => {}
            other => return Err(self.err(format!(
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
                other => return Err(self.err(format!(
                    "expected variant name in enum, got {:?}", other))),
            };

            let mut fields: Vec<crate::core::ast::EnumField> = Vec::new();
            if matches!(self.peek(), Token::LParen) {
                self.bump();
                if matches!(self.peek(), Token::RParen) {
                    return Err(self.err(format!(
                        "variant `{}` has empty payload — drop the parens for nullary",
                        v_name)));
                }
                let mut idx = 0usize;
                loop {
                    let field_type = self.parse_enum_field_type(&v_name)?;
                    fields.push(crate::core::ast::EnumField {
                        // Prefix with the variant name so the accessor is UNIQUE within the datatype.
                        // Bare `f{idx}` repeats across variants (Ok.f0 AND Err.f0), which z3's C API
                        // tolerates but its SMT-LIB parser rejects ("repeated accessor f0") — so the
                        // exported encoding couldn't be re-parsed by the viz layer for ANY payload-enum
                        // / effect FSM. Variant names are globally unique, so `{Variant}_f{idx}` is too.
                        name: format!("{}_f{}", v_name, idx),
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
                    other => return Err(self.err(format!(
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
            return Err(self.err(
                "enum must have at least one variant".to_string()));
        }
        Ok(crate::core::ast::EnumDecl { name, variants })
    }

    fn parse_enum_field_type(&mut self, v_name: &str) -> Result<String> {
        let head = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(self.err(format!(
                "expected field type in variant `{}`, got {:?}",
                v_name, other))),
        };

        if matches!(self.peek(), Token::LParen) {
            self.bump();
            let inner = self.parse_enum_field_type(v_name)?;
            match self.bump() {
                Token::RParen => {}
                other => return Err(self.err(format!(
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
            other => return Err(self.err(format!(
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
                    return Err(self.err(
                        "`external schema` is not allowed — use \
                         `external type` (`schema` is deprecated anyway)".to_string()));
                }
                Keyword::Schema
            }
            Token::Claim  => Keyword::Claim,
            Token::Type   => Keyword::Type,
            Token::Fsm    => Keyword::Fsm,
            Token::Fti    => Keyword::Fti,
            other => return Err(self.err(format!(
                "expected keyword, got {:?}", other))),
        };
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(self.err(format!(
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
                    other => return Err(self.err(format!(
                        "expected param name, got {:?}", other))),
                }
                match self.peek() {
                    Token::Comma => { self.bump(); continue; }
                    Token::In => { self.bump(); break; }
                    other => return Err(self.err(format!(
                        "expected ',' or '∈' after param name, got {:?}", other))),
                }
            }

            let head = match self.bump() {
                Token::Ident(s) => s,
                other => return Err(self.err(format!(
                    "expected type name in first-line params, got {:?}", other))),
            };
            let type_name = if matches!(head.as_str(), "Seq" | "Set")
                && matches!(self.peek(), Token::LParen)
            {
                self.bump();
                let inner = match self.bump() {
                    Token::Ident(s) => s,
                    other => return Err(self.err(format!(
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
                other => return Err(self.err(format!(
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
                        other => return Err(self.err(format!(
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
                other => return Err(self.err(format!(
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
                                    other => return Err(self.err(format!(
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
    let (toks, locs) = crate::lexer::tokenize_with_locs(src).map_err(|e| ParseError::new(e.to_string()))?;
    Parser::with_locs(toks, locs).parse_program()
}
