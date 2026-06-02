//! Body-item parsing: passthrough, subclaim, claim-call, membership,
//! constraint; chained-membership desugaring (`0 < x ∈ Int < 5`).

use super::*;

impl Parser {
    /// Try `[lhs cmp]* name ∈ TypeName [cmp rhs]*` → Membership + Constraints.
    /// Returns None (rewinds) for anything that doesn't fit, including `x ∈ pts ∧ …`.
    pub(super) fn try_parse_chained_membership(&mut self) -> Result<Option<Vec<BodyItem>>> {
        let saved = self.pos;

        let first = match self.parse_addsub() {
            Ok(e) => e,
            Err(_) => { self.pos = saved; return Ok(None); }
        };
        let mut operands: Vec<Expr> = vec![first];
        let mut ops: Vec<BinOp> = Vec::new();
        let mut membership_at: Option<(usize, String, Vec<String>)> = None;

        loop {
            // Multi-name shorthand: `x, y, z ∈ Type`. Only consume extra names
            // when each ident is followed by `,` or `∈` (guards against tuples).
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
                // RHS must be bare Ident or `Ident(Ident)` (Seq/Set/etc),
                // followed by a comparison op or line-end; anything else → bail.
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

        // Must end at a body-item boundary; `pos_x ∈ Int = 5 ∧ …` falls through.
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

    pub(super) fn parse_body_item(&mut self) -> Result<Vec<BodyItem>> {
        // `halts_within(F, N)` is removed: halting is implicit in the embed
        // constraint `F(seed, fsm_state)` (an unsatisfiable settled-state IS
        // non-halting). A bare `halts_within(...)` now parses as an ordinary
        // call and fails downstream like any unknown name.

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

        // ClaimCall: `IDENT(slot ↦ value, …)`; distinguished from record literals
        // by `MapsTo` as the second token inside the parens.
        if let Token::Ident(_) = self.peek() {
            let lparen_offset: Option<usize> = {
                let after = self.toks.get(self.pos + 1);
                if matches!(after, Some(Token::LParen)) {
                    Some(1)
                } else if matches!(after, Some(Token::Lt)) {
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
            // Multi-name shorthand: each comma must be followed by Ident then `,`/`∈`.
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
                self.pos = saved;
            } else {
                self.pos = saved;
            }
        }
        let e = self.parse_expr()?;
        Ok(vec![BodyItem::Constraint(e)])
    }
}
