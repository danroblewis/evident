//! Schema / claim / type / subclaim declaration parsing: the `external`
//! modifier, generic type parameters, first-line param lists, and the
//! indented body shared by top-level decls and subclaims.

use super::*;

impl Parser {
    /// Parse a `subclaim Name` body item. Same indented-body shape as
    /// a top-level schema decl, but produces a `SubclaimDecl` body item
    /// so the runtime can register it for later lookup.
    pub(super) fn parse_subclaim(&mut self) -> Result<BodyItem> {
        self.bump(); // subclaim keyword
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected name after subclaim, got {:?}", other))),
        };
        // Optional first-line params — same shape as top-level
        // schemas (`subclaim foo(color ∈ Color)`). Caller-supplied
        // inputs sit on the signature; outputs and locals go in
        // the body.
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

    /// Parse zero or more body items at a single indent level. Used by
    /// both top-level schema decls and subclaims. Stops when the next
    /// Indent is at a different level (or there's no Indent at all).
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
        // Optional `external` modifier before the host keyword.
        // Legal combinations: `external type` (OS-side resource),
        // `external claim` (effect builder), `external fsm`
        // (runtime-side bridge fsm — see
        // docs/design/state-machines-as-relations.md). The only
        // illegal combination is `external schema`, since `schema`
        // is the deprecated synonym for `type`.
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
        // Optional type parameters: `type Edge<T>(...)`, `type Pair<A, B>(...)`,
        // `claim Toposort<T>`. Comma-separated capitalized identifiers
        // between angle brackets. Empty if not generic.
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
        // Optional first-line param list: `type Vec2(x, y ∈ Int)` is
        // shorthand for declaring those Memberships at the top of the
        // body. The order shown here is the canonical positional order
        // for callers using positional pins (`v ∈ Vec2(10, 20)`).
        let mut body = Vec::new();
        if matches!(self.peek(), Token::LParen) {
            body = self.parse_first_line_params()?;
        }
        let param_count = body.len();
        body.extend(self.parse_indented_body()?);
        Ok(SchemaDecl { keyword, name, type_params, body, param_count, external })
    }

    /// Parse `( name1, name2, … ∈ Type, name3 ∈ Type2, … )`. Each
    /// "group" (comma-separated names + `∈ Type`) becomes one
    /// Membership per name. Bare types only — no inline pins or
    /// compound types in this position (would be parser-noisy and
    /// I haven't seen a use case yet).
    pub(super) fn parse_first_line_params(&mut self) -> Result<Vec<BodyItem>> {
        self.eat(&Token::LParen)?;
        let mut items = Vec::new();
        if matches!(self.peek(), Token::RParen) {
            self.bump();
            return Ok(items);
        }
        loop {
            // One group: comma-separated names ending at `∈`.
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
            // Type name: bare ident, or compound `Seq(Inner)` /
            // `Set(Inner)` / etc. Pins on first-line decls would be
            // confusing (you're declaring a field, not constructing a
            // value), so we don't accept them here.
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
                // Inner type may itself carry generic args:
                // `Seq(Edge<T>)`. Consume those if present.
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
