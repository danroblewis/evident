//! Schema / claim / type / subclaim declaration parsing: `external` modifier,
//! generic type params, first-line param lists, and shared indented body.

use super::*;

impl Parser {
    /// Parse a `subclaim Name` body item into a `SubclaimDecl` for later lookup.
    pub(super) fn parse_subclaim(&mut self) -> Result<BodyItem> {
        self.bump(); // subclaim keyword
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
            keyword: Keyword::Subclaim, name, type_params: vec![],
            body, param_count, external: false,
        }))
    }

    /// Parse zero or more body items at a single indent level.
    pub(super) fn parse_indented_body(&mut self) -> Result<Vec<BodyItem>> {
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

    pub(super) fn parse_schema_decl(&mut self) -> Result<SchemaDecl> {
        // `external schema` is rejected; `external type/claim/fsm` are valid.
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
        // Comma-separated capitalized identifiers in `<…>`. Empty if not generic.
        let type_params = if matches!(self.peek(), Token::Lt) {
            self.bump();   // <
            let mut params = Vec::new();
            loop {
                match self.bump() {
                    Token::Ident(s) => params.push(s),
                    other => return Err(ParseError(format!(
                        "expected type parameter name, got {:?}", other))),
                }
                match self.peek() {
                    Token::Comma => { self.bump(); }
                    Token::Gt    => { self.bump(); break; }
                    other => return Err(ParseError(format!(
                        "expected `,` or `>` in type parameters, got {:?}", other))),
                }
            }
            params
        } else {
            vec![]
        };
        // `type Vec2(x, y ∈ Int)` — shorthand memberships, canonical positional order.
        let mut body = Vec::new();
        if matches!(self.peek(), Token::LParen) {
            body = self.parse_first_line_params()?;
        }
        let param_count = body.len();
        body.extend(self.parse_indented_body()?);
        Ok(SchemaDecl { keyword, name, type_params, body, param_count, external })
    }

    /// Parse `(name1, name2 ∈ Type, …)` into one `Membership` per name.
    /// Bare types only — no inline pins or compound types here.
    pub(super) fn parse_first_line_params(&mut self) -> Result<Vec<BodyItem>> {
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
            let type_name = if matches!(head.as_str(), "Seq" | "Set" | "Bag" | "Map")
                && matches!(self.peek(), Token::LParen)
            {
                self.bump();   // (
                let inner_head = match self.bump() {
                    Token::Ident(s) => s,
                    other => return Err(ParseError(format!(
                        "expected inner type for {}, got {:?}", head, other))),
                };
                // Inner type may carry generic args: `Seq(Edge<T>)`.
                let inner = if let Some(args) = self.try_parse_generic_args_suffix()? {
                    format!("{inner_head}{args}")
                } else {
                    inner_head
                };
                self.eat(&Token::RParen)?;
                format!("{}({})", head, inner)
            } else if matches!(self.peek(), Token::Lt) {
                // Bare-type with generic args: `Edge<T>`, `Pair<A, B>`.
                let args = self.try_parse_generic_args_suffix()?
                    .expect("Lt was peeked");
                format!("{head}{args}")
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
}
