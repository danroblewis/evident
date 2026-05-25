//! Atom-level parsing: the leaf of the expression grammar. Literals
//! (int/real/string/bool), `match`, dotted identifiers, function-call
//! and typed-constructor expressions, parenthesized groups / tuples,
//! set / range literals (`{…}`), and sequence literals (`⟨…⟩`).

use super::*;

impl Parser {
    pub(super) fn parse_atom(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            Token::Int(n)   => { self.bump(); Ok(Expr::Int(n)) }
            Token::Real(v)  => { self.bump(); Ok(Expr::Real(v)) }
            Token::Str(s)   => { self.bump(); Ok(Expr::Str(s)) }
            Token::True     => { self.bump(); Ok(Expr::Bool(true)) }
            Token::False    => { self.bump(); Ok(Expr::Bool(false)) }
            Token::Match    => self.parse_match(),
            Token::Ident(s) => {
                // `run(F, init)` — nested-FSM run-to-halt as a value
                // (tier 3, blocking-interpret). Recognized in expression
                // position, the value-producing sibling of body-item-level
                // `halts_within(F, N)`. The signature is exact: the keyword
                // `run`, `(`, a bare-identifier FSM name, a comma. Anything
                // else named `run` (`run(x)`, `run(a ↦ b)`) falls through to
                // the normal call/claim-call parse, so the hook is local and
                // never silently reinterprets an ordinary call.
                if s == "run"
                    && matches!(self.toks.get(self.pos + 1), Some(Token::LParen))
                    && matches!(self.toks.get(self.pos + 2), Some(Token::Ident(_)))
                    && matches!(self.toks.get(self.pos + 3), Some(Token::Comma))
                {
                    self.bump();                 // run
                    self.bump();                 // (
                    let fsm = match self.bump() {
                        Token::Ident(f) => f,
                        _ => unreachable!(),     // guarded by the matches! above
                    };
                    self.eat(&Token::Comma)?;    // ,
                    let init = self.parse_expr()?;
                    self.eat(&Token::RParen)?;
                    return Ok(Expr::RunFsm { fsm, init: Box::new(init) });
                }
                self.bump();
                // Greedily consume `.ident` chains (sub-schema field access)
                // and collapse into a single dotted Identifier. Done
                // BEFORE the call check so `win.renderer.set_draw_color(args)`
                // parses as `Call("win.renderer.set_draw_color", args)` —
                // method-style invocation. The inline-translator splits
                // the name on the last dot and treats the prefix as the
                // receiver (prepended to args) when the suffix is a
                // known schema.
                let mut name = s;
                while matches!(self.peek(), Token::Dot) {
                    self.bump();
                    match self.bump() {
                        Token::Ident(field) => { name.push('.'); name.push_str(&field); }
                        other => return Err(ParseError(format!(
                            "expected field name after '.', got {:?}", other))),
                    }
                }
                // Optional type-args suffix: `Edge<Rect>(args)` —
                // typed constructor for a monomorphic instance of a
                // generic type. Only accepted when immediately
                // followed by `(` (a call); anything else means
                // `<` is comparison and we rewind. Catches errors
                // from the suffix parser too — they'd mean the `<`
                // wasn't actually opening a type-args list (e.g.
                // `n < 5 + 1`).
                if matches!(self.peek(), Token::Lt) {
                    let saved = self.pos;
                    let parsed = self.try_parse_generic_args_suffix();
                    match parsed {
                        Ok(Some(args)) if matches!(self.peek(), Token::LParen) => {
                            name.push_str(&args);
                        }
                        _ => { self.pos = saved; }
                    }
                }
                // Function-call expression: `name(arg, …)`. Recognized
                // for builtins like `coindexed(A, B)` / `edges(seq)`,
                // record literals like `IVec2(0, 0)`, claim invocations
                // like `set_draw_color(ren, c, out)`, and method-style
                // `recv.claim(args)` (dispatched in inline.rs).
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
                    return Ok(Expr::Call(name, args));
                }
                Ok(Expr::Identifier(name))
            }
            Token::LParen   => {
                self.bump();
                let first = self.parse_expr()?;
                // Tuple form: `(e1, e2, …)`. Used as the LHS of
                // `∈ claim_name` (the relational invocation form). A
                // single expression in parens (`(e)`) is just a
                // grouped expression — no Tuple wrapper, to preserve
                // the natural reading of `(a + b) * c`.
                if matches!(self.peek(), Token::Comma) {
                    let mut items = vec![first];
                    while matches!(self.peek(), Token::Comma) {
                        self.bump();
                        items.push(self.parse_expr()?);
                    }
                    self.eat(&Token::RParen)?;
                    return Ok(Expr::Tuple(items));
                }
                self.eat(&Token::RParen)?;
                Ok(first)
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
