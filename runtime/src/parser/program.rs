//! Top-level program parsing: schemas, enums, imports; enum variant bodies.

use super::*;

impl Parser {
    pub(super) fn parse_program(&mut self) -> Result<Program> {
        let mut program = Program::default();
        if matches!(self.peek(), Token::Indent(0)) {
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

    /// `enum Name = Var1 | Var2 …` — variants may have payloads; single-line or
    /// indented multi-line body. Variant names must be globally unique.
    pub(super) fn parse_enum_decl(&mut self) -> Result<crate::core::ast::EnumDecl> {
        self.bump(); // enum
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
                    // Optional leading `|` before the first variant.
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
            // Payload `(Type1, Type2, …)` — serialized as `"Seq(Int)"` etc;
            // fields are auto-named `f0`, `f1`, ….
            let mut fields: Vec<crate::core::ast::EnumField> = Vec::new();
            if matches!(self.peek(), Token::LParen) {
                self.bump();   // (
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
                                self.bump();   // indent
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

    /// Parse one enum field type: bare ident or recursive `Head(Inner)`.
    pub(super) fn parse_enum_field_type(&mut self, v_name: &str) -> Result<String> {
        let head = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected field type in variant `{}`, got {:?}",
                v_name, other))),
        };
        if matches!(self.peek(), Token::LParen) {
            self.bump();   // (
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
}
