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
                Token::Trace => {
                    let t = self.parse_trace_decl()?;
                    program.traces.push(t);
                }
                Token::Shader => {
                    let s = self.parse_shader_decl()?;
                    program.shaders.push(s);
                }
                Token::Enum => {
                    let e = self.parse_enum_decl()?;
                    program.enums.push(e);
                }
                other => {
                    return Err(ParseError(format!(
                        "expected schema/claim/type/import/trace/shader/enum, got {:?}", other)));
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
    fn parse_enum_decl(&mut self) -> Result<crate::ast::EnumDecl> {
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
            // bare Idents (no compound `Seq(Int)` etc. yet — Stage 0c).
            // Field names are auto-generated `f0`, `f1`, …; named
            // fields are a future extension.
            let mut fields: Vec<crate::ast::EnumField> = Vec::new();
            if matches!(self.peek(), Token::LParen) {
                self.bump();   // (
                if matches!(self.peek(), Token::RParen) {
                    return Err(ParseError(format!(
                        "variant `{}` has empty payload — drop the parens for nullary",
                        v_name)));
                }
                let mut idx = 0usize;
                loop {
                    let field_type = match self.bump() {
                        Token::Ident(s) => s,
                        other => return Err(ParseError(format!(
                            "expected field type in variant `{}`, got {:?}",
                            v_name, other))),
                    };
                    fields.push(crate::ast::EnumField {
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
            variants.push(crate::ast::EnumVariant {
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
        Ok(crate::ast::EnumDecl { name, variants })
    }

    /// Parse a `shader Name` top-level decl. Body is the same indented-
    /// constraint shape as a `claim` body, but the runtime treats the
    /// result as opaque (transpiled to GLSL at load time, never
    /// inlined into another schema's constraint system).
    fn parse_shader_decl(&mut self) -> Result<ShaderDecl> {
        self.bump(); // shader
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected shader name, got {:?}", other))),
        };
        let body = self.parse_indented_body()?;
        Ok(ShaderDecl { name, body })
    }

    /// Parse a `trace name "path/to/program.ev"` declaration with an
    /// indented body of `send "command" => assertion[s]` steps.
    /// Stored on the Program's `traces` list, separate from schemas;
    /// the test runner picks them up.
    fn parse_trace_decl(&mut self) -> Result<TraceDecl> {
        self.bump();   // trace
        let name = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected trace name, got {:?}", other))),
        };
        let program_path = match self.bump() {
            Token::Str(s) => s,
            other => return Err(ParseError(format!(
                "expected program path string, got {:?}", other))),
        };
        // Parse indented send-step body.
        self.skip_blank_newlines();
        let body_indent = match self.peek() {
            Token::Indent(n) if *n > 0 => *n,
            _ => return Err(ParseError(
                "expected indented body after trace declaration".into()
            )),
        };
        let mut steps = Vec::new();
        loop {
            match self.peek() {
                Token::Indent(n) if *n == body_indent => { self.bump(); }
                _ => break,
            }
            steps.push(self.parse_trace_step(body_indent)?);
            match self.peek() {
                Token::Newline => { self.bump(); }
                Token::Eof => break,
                _ => {}
            }
        }
        Ok(TraceDecl { name, program_path, steps })
    }

    /// Parse one trace-body step. Five shapes:
    ///   - `send "cmd"`                       — bare Stdin send
    ///   - `send "cmd" => assertion`          — Stdin send + inline assertion
    ///   - `send "cmd" =>\n   asserts…`       — Stdin send + assertion block
    ///   - `key_down "Right"` / `key_up "Right"` — SDL key state toggle
    ///   - `advance 0.5s [=> assertion[s]]`   — SDL clock tick
    /// Trailing-assertion forms (single inline or indented block) are
    /// shared by `Send` and `Advance` via `parse_trailing_assertions`.
    fn parse_trace_step(&mut self, parent_indent: usize) -> Result<TraceStep> {
        match self.peek() {
            Token::Send => {
                self.bump();
                let command = match self.bump() {
                    Token::Str(s) => s,
                    other => return Err(ParseError(format!(
                        "expected command string after 'send', got {:?}", other))),
                };
                let assertions = self.parse_trailing_assertions(parent_indent, "send command")?;
                Ok(TraceStep::Send { command, assertions })
            }
            Token::KeyDown => {
                self.bump();
                let key = self.parse_key_name("key_down")?;
                Ok(TraceStep::KeyDown { key })
            }
            Token::KeyUp => {
                self.bump();
                let key = self.parse_key_name("key_up")?;
                Ok(TraceStep::KeyUp { key })
            }
            Token::Advance => {
                self.bump();
                let duration_ms = self.parse_duration()?;
                let assertions = self.parse_trailing_assertions(parent_indent, "advance duration")?;
                Ok(TraceStep::Advance { duration_ms, assertions })
            }
            other => Err(ParseError(format!(
                "expected 'send' / 'key_down' / 'key_up' / 'advance' in trace body, got {:?}",
                other
            ))),
        }
    }

    /// Parse `"KeyName"` after `key_down` / `key_up`. Restricted to
    /// quoted strings (no bare identifiers) so the names are explicit
    /// and a typo doesn't silently lex as an Ident the runner ignores.
    fn parse_key_name(&mut self, ctx: &str) -> Result<String> {
        match self.bump() {
            Token::Str(s) => Ok(s),
            other => Err(ParseError(format!(
                "expected key name string after '{ctx}', got {:?}", other))),
        }
    }

    /// Parse `<number><unit>` where the number is `Int` or `Real` and
    /// the unit is `s` (seconds) or `ms` (milliseconds). Returns the
    /// duration in milliseconds, rounded down. The unit is parsed as
    /// an Ident token following the number — `0.5s` lexes as
    /// `Real(0.5) Ident("s")` because the lexer doesn't combine
    /// numeric literals with letter suffixes.
    fn parse_duration(&mut self) -> Result<u32> {
        let secs_or_ms: f64 = match self.bump() {
            Token::Int(n)  => n as f64,
            Token::Real(r) => r,
            other => return Err(ParseError(format!(
                "expected number after 'advance', got {:?}", other))),
        };
        let unit = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected duration unit ('s' or 'ms') after number, got {:?}", other))),
        };
        let ms = match unit.as_str() {
            "s"  => secs_or_ms * 1000.0,
            "ms" => secs_or_ms,
            other => return Err(ParseError(format!(
                "unknown duration unit {:?}; expected 's' or 'ms'", other))),
        };
        if ms < 0.0 {
            return Err(ParseError("advance duration must be non-negative".into()));
        }
        Ok(ms as u32)
    }

    /// Parse the optional `=> assertion` (single inline) OR
    /// `=>` newline + indented assertion block that may follow a
    /// `send` or `advance` step. Returns an empty vec when neither
    /// form is present (bare step). `ctx` names the preceding
    /// construct for error messages (e.g. `"send command"`).
    fn parse_trailing_assertions(
        &mut self, parent_indent: usize, ctx: &str,
    ) -> Result<Vec<TraceAssertion>> {
        if matches!(self.peek(), Token::Newline | Token::Eof | Token::Indent(_)) {
            return Ok(Vec::new());
        }
        if !matches!(self.peek(), Token::Implies) {
            return Err(ParseError(format!(
                "expected '=>' or end of line after {ctx}, got {:?}", self.peek()
            )));
        }
        self.bump();   // =>
        if matches!(self.peek(), Token::Newline) {
            let saved = self.pos;
            self.bump();
            while matches!(self.peek(), Token::Newline) { self.bump(); }
            if let Token::Indent(n) = self.peek().clone() {
                if n > parent_indent {
                    let block_indent = n;
                    let mut assertions = Vec::new();
                    loop {
                        match self.peek() {
                            Token::Indent(m) if *m == block_indent => { self.bump(); }
                            _ => break,
                        }
                        assertions.push(self.parse_trace_assertion()?);
                        if matches!(self.peek(), Token::Newline) { self.bump(); }
                    }
                    return Ok(assertions);
                }
            }
            self.pos = saved;
            self.bump();
            return Ok(Vec::new());
        }
        let a = self.parse_trace_assertion()?;
        Ok(vec![a])
    }

    /// Parse one assertion: `IDENT (= | ∋) <value>`. Value may be a
    /// string literal, an Int, a Real, or a Bool (`true`/`false`) —
    /// stored as the literal's `Display` form so the runner does a
    /// stringified compare against the state-field's `Display`. The
    /// `∋` operator only makes sense for strings; that's not enforced
    /// here, just convention. Unrecognized value tokens are a parse
    /// error so trace tests fail loudly rather than silently ignoring
    /// an unsupported form.
    fn parse_trace_assertion(&mut self) -> Result<TraceAssertion> {
        let mut var = match self.bump() {
            Token::Ident(s) => s,
            other => return Err(ParseError(format!(
                "expected assertion variable name, got {:?}", other))),
        };
        // Dotted-and-indexed path: `hero.pos.x`, `coins[0].collected`,
        // `dots[2].pos.y`. Stored as a single string; the runner
        // re-tokenizes (`.` and `[N]`) when looking the path up.
        loop {
            match self.peek() {
                Token::Dot => {
                    self.bump();
                    match self.bump() {
                        Token::Ident(s) => { var.push('.'); var.push_str(&s); }
                        other => return Err(ParseError(format!(
                            "expected field name after '.', got {:?}", other))),
                    }
                }
                Token::LBracket => {
                    self.bump();
                    let n = match self.bump() {
                        Token::Int(n) if n >= 0 => n,
                        other => return Err(ParseError(format!(
                            "expected non-negative integer index, got {:?}", other))),
                    };
                    match self.bump() {
                        Token::RBracket => {}
                        other => return Err(ParseError(format!(
                            "expected ']' after index, got {:?}", other))),
                    }
                    var.push('[');
                    var.push_str(&n.to_string());
                    var.push(']');
                }
                _ => break,
            }
        }
        let op = match self.bump() {
            Token::Eq          => AssertOp::Eq,
            Token::ContainsRev => AssertOp::Contains,
            other => return Err(ParseError(format!(
                "expected '=' or '∋' in trace assertion, got {:?}", other))),
        };
        // Optional leading `-` so negative numeric literals work
        // (`hero.vel.y = -700`). Evident-the-language has no unary
        // minus operator, but trace assertions are a restricted
        // grammar where a literal-with-sign reads naturally.
        let neg = matches!(self.peek(), Token::Minus);
        if neg { self.bump(); }
        let value = match self.bump() {
            Token::Str(s) if !neg => s,
            Token::Int(n)         => if neg { (-n).to_string() } else { n.to_string() },
            Token::Real(r)        => if neg { (-r).to_string() } else { r.to_string() },
            Token::True if !neg   => "true".into(),
            Token::False if !neg  => "false".into(),
            other => return Err(ParseError(format!(
                "expected string/int/real/bool literal as assertion value, got {:?}", other))),
        };
        Ok(TraceAssertion { var, op, value })
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
        // Optional first-line param list: `type Vec2(x, y ∈ Int)` is
        // shorthand for declaring those Memberships at the top of the
        // body. The order shown here is the canonical positional order
        // for callers using positional pins (`v ∈ Vec2(10, 20)`).
        let mut body = Vec::new();
        if matches!(self.peek(), Token::LParen) {
            body = self.parse_first_line_params()?;
        }
        body.extend(self.parse_indented_body()?);
        Ok(SchemaDecl { keyword, name, body })
    }

    /// Parse `( name1, name2, … ∈ Type, name3 ∈ Type2, … )`. Each
    /// "group" (comma-separated names + `∈ Type`) becomes one
    /// Membership per name. Bare types only — no inline pins or
    /// compound types in this position (would be parser-noisy and
    /// I haven't seen a use case yet).
    fn parse_first_line_params(&mut self) -> Result<Vec<BodyItem>> {
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
                    pins: crate::ast::Pins::None,
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

    /// Parse a Membership's type name and optional pin clause, given
    /// `head` is the type-name ident currently at `self.peek()`.
    /// Returns `Ok(Some(...))` on success, `Ok(None)` if the input
    /// doesn't look like a recognized Membership shape (caller backs
    /// up and tries to parse as an expression instead).
    ///
    /// Type-name shapes accepted:
    ///   - bare ident:               `Nat`
    ///   - named pins:               `IVec2 (x ↦ 0, y ↦ 0)`
    /// Body-item-level recognition of a chained-comparison expression
    /// with an embedded `∈ TypeName` step. Splits into a Membership
    /// declaration plus one Constraint per comparison pair.
    ///
    ///   `pos_x ∈ Int = 5`         →  `pos_x ∈ Int` ; `pos_x = 5`
    ///   `pos_x ∈ Int < 5`         →  `pos_x ∈ Int` ; `pos_x < 5`
    ///   `0 < pos_x ∈ Int`         →  `pos_x ∈ Int` ; `0 < pos_x`
    ///   `0 < pos_x ∈ Int < 5`     →  `pos_x ∈ Int` ; `0 < pos_x` ; `pos_x < 5`
    ///   `pos_x ∈ Seq(Int)`        →  same single-Membership case
    ///
    /// The variable being declared is the operand immediately to the
    /// left of `∈`. It must be a bare Identifier (no field access,
    /// expression). Multi-name shorthand is supported when the comma
    /// list sits at the operand position immediately to the left of
    /// `∈`: `x, y, z ∈ Int < 5` and `0 < x, y, z ∈ Int < 5` both
    /// expand to one Membership per name plus per-name copies of
    /// every comparison-pair constraint.
    ///
    /// Returns `None` (and rewinds the cursor) if the line doesn't fit
    /// this pattern. Carefully avoids consuming a regular set-membership
    /// expression like `x ∈ pts ∧ …` — the chain-end check requires
    /// a Newline / Eof / Indent immediately after the chain.
    fn try_parse_chained_membership(&mut self) -> Result<Option<Vec<BodyItem>>> {
        let saved = self.pos;

        let first = match self.parse_addsub() {
            Ok(e) => e,
            Err(_) => { self.pos = saved; return Ok(None); }
        };
        let mut operands: Vec<Expr> = vec![first];
        let mut ops: Vec<BinOp> = Vec::new();
        // (var_idx, type_name, all names — 1 element for single-name,
        //  2+ for `x, y, z ∈ Type` shorthand)
        let mut membership_at: Option<(usize, String, Vec<String>)> = None;

        loop {
            // Multi-name shorthand: if the most recent operand is a
            // bare Ident and the next tokens look like `, IDENT (, IDENT)* ∈`,
            // consume the extra names. Only valid at the operand position
            // immediately to the left of `∈`.
            let mut extra_names: Vec<String> = Vec::new();
            if matches!(self.peek(), Token::Comma) {
                let last_is_bare = matches!(operands.last(),
                    Some(Expr::Identifier(s)) if !s.contains('.'));
                if last_is_bare {
                    let mn_save = self.pos;
                    let mut names: Vec<String> = Vec::new();
                    while matches!(self.peek(), Token::Comma) {
                        let inner_save = self.pos;
                        self.bump();   // ,
                        if let Token::Ident(s) = self.peek().clone() {
                            // Same protective lookahead as the existing
                            // multi-name body-item path: only consume if
                            // the next token after the new ident is itself
                            // a `,` or `∈`. Avoids eating `x, y` from a
                            // tuple-like expression.
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
                        // Not a multi-name shorthand; rewind comma consumption.
                        self.pos = mn_save;
                    }
                }
            }

            if matches!(self.peek(), Token::In) {
                if membership_at.is_some() {
                    // Two ∈ in one chain — not a recognized form.
                    self.pos = saved;
                    return Ok(None);
                }
                self.bump();
                // The RHS of ∈ in a chained membership must be a
                // simple type name: a bare Ident, or a recognized
                // compound `Ident(Ident)` for Seq/Set/Bag/Map.
                // Followed by either a Newline-class token or
                // another comparison op. Anything else (function call,
                // pin form, expression) → bail.
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
                    self.bump();   // head
                    self.bump();   // (
                    let inner = match self.bump() {
                        Token::Ident(s) => s,
                        _ => unreachable!(),
                    };
                    self.bump();   // )
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
                    self.bump();   // head
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

        // The chain must end at a body-item boundary; otherwise the
        // user wrote something like `pos_x ∈ Int = 5 ∧ …` and we
        // should let the regular expression parser handle it.
        if !matches!(self.peek(),
            Token::Newline | Token::Eof | Token::Indent(_)
        ) {
            self.pos = saved;
            return Ok(None);
        }

        // Desugar: emit one Membership per name first, then per-name
        // copies of each comparison-pair constraint with the variable
        // position substituted to the current name.
        let mut items: Vec<BodyItem> = names.iter().map(|n| BodyItem::Membership {
            name: n.clone(),
            type_name: type_name.clone(),
            pins: crate::ast::Pins::None,
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

    ///   - positional pins:          `IVec2 (-800, -800)`
    ///   - compound `Ident(Ident)`:  `Seq(Int)` (only for hardcoded
    ///     compound heads — Seq/Set/Bag/Map — so other `Type(arg)`
    ///     reads as a positional pin).
    fn try_parse_type_and_pins(&mut self, head: &str)
        -> Result<Option<(String, crate::ast::Pins)>>
    {
        let after_head = self.toks.get(self.pos + 1);
        let plain_terminated = matches!(after_head,
            Some(Token::Newline) | Some(Token::Eof) | Some(Token::Indent(_)) | None);
        let has_paren = matches!(after_head, Some(Token::LParen));
        if plain_terminated {
            self.bump();
            return Ok(Some((head.to_string(), crate::ast::Pins::None)));
        }
        if has_paren {
            let inside_first = self.toks.get(self.pos + 2);
            let inside_second = self.toks.get(self.pos + 3);
            let is_named_pin = matches!(inside_first, Some(Token::Ident(_)))
                && matches!(inside_second, Some(Token::MapsTo));
            let looks_like_compound = matches!(inside_first, Some(Token::Ident(_)))
                && matches!(inside_second, Some(Token::RParen));
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
                    pins.push(crate::ast::Mapping { slot, value });
                    if matches!(self.peek(), Token::Comma) { self.bump(); continue; }
                    break;
                }
                self.eat(&Token::RParen)?;
                return Ok(Some((head.to_string(), crate::ast::Pins::Named(pins))));
            } else if is_known_compound_head && looks_like_compound {
                self.bump();           // outer ident
                self.bump();           // (
                let inner = match self.bump() {
                    Token::Ident(s) => s,
                    _ => unreachable!(),
                };
                self.bump();           // )
                let after = self.toks.get(self.pos);
                let line_end = matches!(after,
                    Some(Token::Newline) | Some(Token::Eof)
                    | Some(Token::Indent(_)) | None);
                if line_end {
                    return Ok(Some((format!("{}({})", head, inner), crate::ast::Pins::None)));
                }
                return Ok(None);
            } else {
                // Positional pins.
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
                return Ok(Some((head.to_string(), crate::ast::Pins::Positional(args))));
            }
        }
        Ok(None)
    }

    fn parse_body_item(&mut self) -> Result<Vec<BodyItem>> {
        // Four shapes:
        //   ..IDENT                                 → Passthrough composition
        //   IDENT IN IDENT (followed by line-end)   → Membership declaration
        //   <chain with ∈ TypeName embedded>        → Membership + Constraint(s)
        //         e.g. `pos_x ∈ Int = 5`, `0 < pos_x ∈ Int < 5`,
        //              `pos_x ∈ Int < 5`. Desugars to a Membership for
        //              the var to the left of ∈, plus per-pair Constraints
        //              for each comparison op in the chain. Single-name
        //              only; multi-name + chain not supported yet.
        //   <expr>                                  → Constraint
        // Anything else with `∈` (e.g. `x ∈ {1, 2}` or `x ∈ pts`) parses
        // as an expression and ends up as a Constraint.

        // Passthrough: `..ClaimName` at body-item start.
        if matches!(self.peek(), Token::DotDot) {
            self.bump();
            match self.bump() {
                Token::Ident(name) => return Ok(vec![BodyItem::Passthrough(name)]),
                other => return Err(ParseError(format!(
                    "expected claim name after '..', got {:?}", other))),
            }
        }

        // Subclaim: `subclaim Name` followed by an indented body. Same
        // shape as a top-level schema decl. The runtime-loader pulls
        // the inner SchemaDecl out and registers it under its name so
        // ClaimCall / passthrough can reference it.
        if matches!(self.peek(), Token::Subclaim) {
            return Ok(vec![self.parse_subclaim()?]);
        }

        // ClaimCall: `IDENT(slot mapsto value, …)` at body-item start.
        // Distinguished from a parenthesized expression by the IDENT
        // immediately followed by `(`. Disambiguated from a generic
        // function-call expression (record literal like `IVec2(0, 0)`)
        // by checking that the second token inside the parens is
        // `MapsTo` — that's specific to ClaimCall syntax.
        if let Token::Ident(_) = self.peek() {
            if matches!(self.toks.get(self.pos + 1), Some(Token::LParen)) {
                let inside_first = self.toks.get(self.pos + 2);
                let inside_second = self.toks.get(self.pos + 3);
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
                            mappings.push(crate::ast::Mapping { slot, value });
                            if matches!(self.peek(), Token::Comma) { self.bump(); continue; }
                            break;
                        }
                    }
                    self.eat(&Token::RParen)?;
                    return Ok(vec![BodyItem::ClaimCall { name, mappings }]);
                }
                // Otherwise fall through to expr-as-Constraint parsing,
                // which handles record literals like `IVec2(0, 0)`.
            }
        }
        // Chained-membership: try to parse `[lhs comp]* IDENT ∈ TypeName [comp rhs]*`
        // and split into a Membership + a Constraint per comparison pair.
        // Returns None (and rewinds) for anything that doesn't fit; the
        // existing Membership and expression branches below handle the rest.
        if let Some(items) = self.try_parse_chained_membership()? {
            return Ok(items);
        }

        if let Token::Ident(_) = self.peek() {
            let saved = self.pos;
            let mut lhs_names = match self.bump() {
                Token::Ident(s) => vec![s],
                _ => unreachable!(),
            };
            // Multi-name shorthand: `x, y, z ∈ Type …`. Each comma must
            // be followed by an Ident that's itself followed by a Comma
            // or `In` — protects against confusing `(a, b)` tuple
            // bindings or comma-in-expr from being eaten here.
            while matches!(self.peek(), Token::Comma) {
                let inner_save = self.pos;
                self.bump();   // ,
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
                // Not a recognized Membership shape — back up.
                self.pos = saved;
            } else {
                self.pos = saved;
            }
        }
        let e = self.parse_expr()?;
        Ok(vec![BodyItem::Constraint(e)])
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
        let range = self.parse_atom()?;   // expect a {lo..hi}, {a, b, c}, or builtin call
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
            Token::Real(v)  => { self.bump(); Ok(Expr::Real(v)) }
            Token::Str(s)   => { self.bump(); Ok(Expr::Str(s)) }
            Token::True     => { self.bump(); Ok(Expr::Bool(true)) }
            Token::False    => { self.bump(); Ok(Expr::Bool(false)) }
            Token::Ident(s) => {
                self.bump();
                // Function-call expression: `name(arg, …)`. Recognized
                // for builtins like `coindexed(A, B)` and `edges(seq)`
                // in quantifier source position. Translator
                // special-cases known names and errors on unknown.
                if matches!(self.peek(), Token::LParen) {
                    self.bump();   // (
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Token::RParen) {
                        loop {
                            args.push(self.parse_expr()?);
                            if matches!(self.peek(), Token::Comma) {
                                self.bump();
                                continue;
                            }
                            break;
                        }
                    }
                    self.eat(&Token::RParen)?;
                    return Ok(Expr::Call(s, args));
                }
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

/// Recognize a comparison operator token. Used by `parse_compare` for
/// chained-comparison detection (`20 ≤ x ≤ 740` etc.) — when the
/// token after a `lhs op rhs` parse is another comparison op, we
/// know we're in a chain and the desugaring kicks in.
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
        // Even though the translator doesn't run these yet, the AST
        // shape should be settled.
        let p = parse("schema S\n    s ∈ Seq(Int)\n    #s = 3\n    s[0] > 0\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 3);
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
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

    #[test]
    fn parse_chained_membership_two_sided() {
        // `0 < pos_x ∈ Int < 5` desugars to:
        //   - Membership(pos_x, Int)
        //   - Constraint(0 < pos_x)
        //   - Constraint(pos_x < 5)
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
        // `pos_x ∈ Int = 5` desugars to Membership + Constraint(=).
        let p = parse("claim t\n    pos_x ∈ Int = 5\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 2);
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "pos_x" && type_name == "Int"));
        assert!(matches!(&s.body[1], BodyItem::Constraint(Expr::Binary(BinOp::Eq, _, _))));
    }

    #[test]
    fn parse_chained_membership_compound_type() {
        // Compound type like `Seq(Int)` allowed on the RHS of ∈.
        // Tail comparison only — leading comparison would need to chain
        // against a Seq, which isn't meaningful.
        let p = parse("claim t\n    s ∈ Seq(Int)\n    #s = 3\n").unwrap();
        // This particular form doesn't trigger chained-membership (no
        // comparison op follows the type) — confirms the regular path
        // still parses.
        let s = &p.schemas[0];
        assert!(matches!(&s.body[0], BodyItem::Membership { name, type_name, .. }
            if name == "s" && type_name == "Seq(Int)"));
    }

    #[test]
    fn parse_chained_membership_does_not_eat_set_membership() {
        // `x ∈ pts ∧ x > 0` must NOT be split into a Membership +
        // Constraint — the `∧` after the Ident isn't a comparison op,
        // so the chain detector bails and the regular expression
        // parser handles it as `(x ∈ pts) ∧ (x > 0)`.
        let p = parse("claim t\n    pts ∈ Set(Int)\n    x ∈ Int\n    x ∈ pts ∧ x > 0\n").unwrap();
        let s = &p.schemas[0];
        // Last body item should be a single Constraint with a Binary(And) at top.
        match s.body.last().unwrap() {
            BodyItem::Constraint(Expr::Binary(BinOp::And, _, _)) => {}
            other => panic!("expected `(x ∈ pts) ∧ (x > 0)` constraint, got {:?}", other),
        }
    }

    #[test]
    fn parse_chained_membership_multi_name() {
        // `x, y, z ∈ Int < 5` desugars to:
        //   - Membership(x, Int), Membership(y, Int), Membership(z, Int)
        //   - Constraint(x < 5), Constraint(y < 5), Constraint(z < 5)
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
        // `0 < x, y ∈ Int < 5` → 2 Memberships + 4 Constraints (lower + upper per name).
        let p = parse("claim t\n    0 < x, y ∈ Int < 5\n").unwrap();
        let s = &p.schemas[0];
        assert_eq!(s.body.len(), 6);
        // First two are Memberships
        assert!(matches!(&s.body[0], BodyItem::Membership { name, .. } if name == "x"));
        assert!(matches!(&s.body[1], BodyItem::Membership { name, .. } if name == "y"));
        // Next four are Constraints (per-name pair)
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
        // `Expr` declared first, references `BinOp` declared later.
        // The parser doesn't validate types — that's runtime — so this
        // just confirms the AST shape: 2 enum decls, the first
        // references the second by name in a payload field.
        let p = parse(
            "enum Expr = Lit(Int) | Op(BinOp, Expr, Expr)\nenum BinOp = Add | Sub\n"
        ).unwrap();
        assert_eq!(p.enums.len(), 2);
        assert_eq!(p.enums[0].name, "Expr");
        assert_eq!(p.enums[1].name, "BinOp");
        // Op variant's first field references BinOp.
        assert_eq!(p.enums[0].variants[1].name, "Op");
        assert_eq!(p.enums[0].variants[1].fields[0].type_name, "BinOp");
    }

    #[test]
    fn parse_enum_decl_mutual_recursion_parses() {
        // Expr ↔ Stmt — each references the other in payloads.
        let p = parse(
            "enum Expr = ENum(Int) | EBlock(Stmt)\nenum Stmt = SExpr(Expr) | SSeq(Stmt, Stmt)\n"
        ).unwrap();
        assert_eq!(p.enums.len(), 2);
        // Expr.EBlock references Stmt.
        assert_eq!(p.enums[0].variants[1].fields[0].type_name, "Stmt");
        // Stmt.SExpr references Expr.
        assert_eq!(p.enums[1].variants[0].fields[0].type_name, "Expr");
    }

    #[test]
    fn parse_enum_decl_empty_payload_errors() {
        // `Variant()` is rejected — drop the parens for nullary variants.
        assert!(parse("enum X = Foo() | Bar\n").is_err());
    }

    #[test]
    fn parse_enum_decl_no_variants_errors() {
        // The grammar requires at least one variant after `=`.
        // Parser rejects "got X" where X is the unexpected token after `=`.
        assert!(parse("enum Empty =\n").is_err());
    }

    #[test]
    fn parse_chained_membership_rejects_dotted_lhs() {
        // `state.x ∈ Int < 5` would try to declare a dotted name —
        // not allowed. Falls through and errors at the schema-parse
        // level (the trailing `< 5` is unexpected).
        assert!(parse("claim t\n    state.x ∈ Int < 5\n").is_err());
    }
}
