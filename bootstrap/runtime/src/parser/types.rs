//! Type-name parsing: generic-arg suffixes and pin-clause forms (bare, named,
//! positional, compound `Seq(Int)`, generic instantiations).

use super::*;

impl Parser {
    /// Consume `<arg1, arg2, …>` and return it as a string, or `None` if next
    /// token isn't `<`. Nested generic args supported (`Edge<Pair<A, B>>`).
    pub(super) fn try_parse_generic_args_suffix(&mut self) -> Result<Option<String>> {
        if !matches!(self.peek(), Token::Lt) {
            return Ok(None);
        }
        self.bump();   // <
        let mut out = String::from("<");
        let mut first = true;
        loop {
            if !first { out.push_str(", "); }
            first = false;
            let name = match self.bump() {
                Token::Ident(s) => s,
                other => return Err(ParseError(format!(
                    "expected type argument name, got {:?}", other))),
            };
            out.push_str(&name);
            if let Some(inner) = self.try_parse_generic_args_suffix()? {
                out.push_str(&inner);
            }
            match self.peek() {
                Token::Comma => { self.bump(); }
                Token::Gt    => { self.bump(); break; }
                other => return Err(ParseError(format!(
                    "expected `,` or `>` in type arguments, got {:?}", other))),
            }
        }
        out.push('>');
        Ok(Some(out))
    }

    /// Parse a type + optional pin clause after `head`: positional pins, named
    /// pins, compound heads (`Seq/Set/Bag/Map`), or generic `Edge<Rect>`.
    pub(super) fn try_parse_type_and_pins(&mut self, head: &str)
        -> Result<Option<(String, crate::core::ast::Pins)>>
    {
        // Consume head + generic suffix, then re-examine what follows (newline,
        // `(`, etc.) without re-bumping head.
        if matches!(self.toks.get(self.pos + 1), Some(Token::Lt)) {
            self.bump();   // consume head ident
            let args = self.try_parse_generic_args_suffix()?
                .expect("Lt was peeked");
            let composite = format!("{head}{args}");
            let after = self.toks.get(self.pos).cloned();
            let plain_terminated = matches!(after,
                Some(Token::Newline) | Some(Token::Eof)
                | Some(Token::Indent(_)) | None);
            if plain_terminated {
                return Ok(Some((composite, crate::core::ast::Pins::None)));
            }
            // No pin forms on generic instantiations at use-site (v1).
            return Ok(Some((composite, crate::core::ast::Pins::None)));
        }

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
            // `Seq(Int)` or `Seq(Edge<Rect>)` — both look-like-compound.
            let looks_like_compound = matches!(inside_first, Some(Token::Ident(_)))
                && (matches!(inside_second, Some(Token::RParen))
                    || matches!(inside_second, Some(Token::Lt)));
            let is_known_compound_head =
                matches!(head, "Seq" | "Set" | "Bag" | "Map");

            if is_named_pin {
                self.bump();   // type ident
                self.bump();   // (
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
                self.bump();           // outer ident
                self.bump();           // (
                let inner_head = match self.bump() {
                    Token::Ident(s) => s,
                    _ => unreachable!(),
                };
                let inner = if let Some(args) = self.try_parse_generic_args_suffix()? {
                    format!("{inner_head}{args}")
                } else {
                    inner_head
                };
                self.bump();           // )
                let after = self.toks.get(self.pos);
                let line_end = matches!(after,
                    Some(Token::Newline) | Some(Token::Eof)
                    | Some(Token::Indent(_)) | None);
                if line_end {
                    return Ok(Some((format!("{}({})", head, inner), crate::core::ast::Pins::None)));
                }
                return Ok(None);
            } else {
                self.bump();   // type ident
                self.bump();   // (
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
}
