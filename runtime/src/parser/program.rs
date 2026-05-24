//! Top-level program parsing: file → schemas + enums + imports, plus
//! enum declarations (single-line and multi-line variant bodies, with
//! optional payloads).

use super::*;

impl Parser {
    pub(super) fn parse_program(&mut self) -> Result<Program> {
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

    /// Parse a top-level enum declaration:
    ///   `enum Name = Variant1 | Variant2 | … | VariantN`
    ///
    /// Variants are nullary (no payloads in v0.1). Variant names must be
    /// idents — they become the constructor names in the underlying Z3
    /// datatype and must be globally unique across all enums in the
    /// program (the existing datatypes.rs registry enforces this).
    /// Whitespace and newlines around `|` are tolerated; the body lives
    /// on a single logical line by default but parens/brackets aren't
    /// required to span multiple lines because Pipe doesn't open a group.
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
        // Multi-line variant body: after `=`, if a Newline + deeper
        // Indent follows, accept variants on separate lines with
        // optional leading `|`. End of body = dedent / EOF / next
        // top-level decl. Single-line form (variants joined by `|`
        // on the same logical line) still works as before.
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
            // Optional payload `(Type1, Type2, …)`. Inner types are
            // either bare Idents (`Int`, `MyEnum`) or compound types
            // (`Seq(Int)`, `Set(String)`) — the parser accepts both
            // and serializes to a flat String like `"Seq(Int)"`; the
            // translator dispatches on the leading prefix at
            // enum-load time. Field names are auto-generated `f0`,
            // `f1`, …; named fields are a future extension.
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
            // Multi-line continuation: a Newline followed by Indent at
            // exactly `block_indent` introduces another variant. The
            // optional `|` between variants is allowed but not
            // required in the multi-line form.
            if let Some(want) = block_indent {
                if matches!(self.peek(), Token::Newline) {
                    let cont_save = self.pos;
                    self.bump();
                    while matches!(self.peek(), Token::Newline) { self.bump(); }
                    if let Token::Indent(n) = self.peek().clone() {
                        if n == want {
                            // Same indent → another variant follows.
                            // Peek one token past the indent: must be
                            // an Ident (variant name) or `|`.
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

    /// Parse one enum-variant payload field type. Accepts bare idents
    /// (`Int`, `MyEnum`) and one level of compound type with a single
    /// inner type (`Seq(Int)`, `Set(String)`, `Seq(Color)`). Nested
    /// compounds (`Seq(Seq(Int))`) parse recursively. The serialized
    /// String round-trips through the translator's
    /// `s.starts_with("Seq(")` dispatch at enum-load time.
    pub(super) fn parse_enum_field_type(&mut self, v_name: &str) -> Result<String> {
        let head = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected field type in variant `{}`, got {:?}",
                v_name, other))),
        };
        // Compound form `Head(InnerType)` — recurse.
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
