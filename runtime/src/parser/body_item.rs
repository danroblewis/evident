//! Body-item parsing: the four body-item shapes (passthrough, subclaim,
//! claim-call, membership, expression-constraint) and the
//! chained-membership desugaring (`0 < x ∈ Int < 5` → Membership +
//! per-pair Constraints).

use super::*;

impl Parser {
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
    pub(super) fn try_parse_chained_membership(&mut self) -> Result<Option<Vec<BodyItem>>> {
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

    pub(super) fn parse_body_item(&mut self) -> Result<Vec<BodyItem>> {
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
        // Also accepts `IDENT<T>(slot mapsto value, …)` — generic claim
        // invocation. Distinguished from a parenthesized expression by
        // the IDENT (optionally followed by `<...>`) immediately
        // followed by `(`. Disambiguated from a generic function-call
        // expression (record literal like `IVec2(0, 0)`) by checking
        // that the second token inside the parens is `MapsTo` —
        // specific to ClaimCall syntax.
        if let Token::Ident(_) = self.peek() {
            // Peek past optional `<...>` to find the `(` that opens
            // the mappings list.
            let lparen_offset: Option<usize> = {
                let after = self.toks.get(self.pos + 1);
                if matches!(after, Some(Token::LParen)) {
                    Some(1)
                } else if matches!(after, Some(Token::Lt)) {
                    // Scan forward past balanced angle brackets.
                    let mut depth = 0i32;
                    let mut i = self.pos + 1;
                    loop {
                        match self.toks.get(i) {
                            Some(Token::Lt) => depth += 1,
                            Some(Token::Gt) => {
                                depth -= 1;
                                if depth == 0 { break Some(i - self.pos + 1); }
                            }
                            None => break None,
                            _ => {}
                        }
                        i += 1;
                    }
                } else {
                    None
                }
            };
            if let Some(lp) = lparen_offset {
                if matches!(self.toks.get(self.pos + lp), Some(Token::LParen)) {
                    let inside_first = self.toks.get(self.pos + lp + 1);
                    let inside_second = self.toks.get(self.pos + lp + 2);
                    let is_claim_call = matches!(inside_first, Some(Token::Ident(_)))
                        && matches!(inside_second, Some(Token::MapsTo));
                    if is_claim_call {
                        let mut name = match self.bump() {
                            Token::Ident(s) => s,
                            _ => unreachable!(),
                        };
                        // Consume optional `<args>` and append to name.
                        if matches!(self.peek(), Token::Lt) {
                            if let Some(args) = self.try_parse_generic_args_suffix()? {
                                name.push_str(&args);
                            }
                        }
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
}
