//! Evident **FSM** source → engine fixture (SMT-LIB + `@meta` metadata).
//!
//! Sibling of [`crate::frontend`] (which transpiles scalar *claims* into a
//! one-shot solve). This module transpiles a single `fsm` declaration into the
//! [`crate::meta`] fixture format — the embedded `; @meta` JSON block plus a
//! `; @transition <name>` SMT-LIB block — so the greenfield engine
//! ([`crate::scheduler::run`]) can run a real Evident FSM end-to-end.
//!
//! ## Supported subset (bounded to the convergence targets)
//!
//! A program with exactly one `fsm`. Recognized lines:
//!
//! * `import "..."` — ignored.
//! * `enum NAME = A | B | C` — nullary-variant enums. The FSM's state-enum's
//!   FIRST variant is its tick-0 init.
//! * `fsm NAME(state ∈ EnumType)` — enum state. Engine `prev = "state"`,
//!   `next = "state_next"`, `init = <first variant>`.
//! * `fsm NAME` with body `X ∈ Int = (is_first_tick ? INIT : EXPR(_X,…))` —
//!   scalar Int state. Engine `prev = "_X"`, `next = "X"`, `init = INIT`,
//!   transition `(assert (= X <EXPR>))`. (The `is_first_tick` branch is
//!   consumed by `init`; the engine provides no `is_first_tick` const.)
//! * `state_next = match state` with `Variant ⇒ Expr` arms →
//!   `(assert (= state_next <nested ite over (is-Variant state)>))`.
//! * `effects = match state` with `⟨…⟩` arm bodies → `(assert (= effects …))`.
//! * `effects = (cond ? ⟨…⟩ : ⟨…⟩)` → `(assert (= effects (ite …)))`.
//! * Sequence literals: `⟨a, b⟩` → `(seq.++ (seq.unit a) (seq.unit b))`,
//!   `⟨a⟩` → `(seq.unit a)`, `⟨⟩` → `(as seq.empty (Seq Effect))`.
//! * Effect ctors in `⟨⟩`: `Println("s")` → `(Println "s")`, `Exit(n)` →
//!   `(Exit n)`.
//!
//! The FSM body always emits a FIXED Effect datatype plus, for enum state, the
//! state enum, batched into one `declare-datatypes`.
//!
//! Anything outside this subset → [`FrontendError`], never silently mis-handled.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::frontend::FrontendError;

fn fe<T>(msg: impl Into<String>) -> Result<T, FrontendError> {
    Err(FrontendError(msg.into()))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Transpile one Evident `fsm` program (text) into an engine fixture string
/// (the `; @meta` JSON block + `; @transition` SMT-LIB block) that
/// [`crate::meta::load_str`] parses.
pub fn transpile_fsm(src: &str) -> Result<String, FrontendError> {
    let prog = parse_program(src)?;
    emit(&prog)
}

// ---------------------------------------------------------------------------
// Parsed program model
// ---------------------------------------------------------------------------

/// What kind of state the single FSM threads.
enum State {
    /// Enum-typed state: `fsm F(state ∈ EnumType)`. prev = the param name,
    /// next = `<param>_next`, init = first variant of the enum.
    Enum { prev: String, next: String, enum_name: String, init: String },
    /// Scalar Int state from `X ∈ Int = (is_first_tick ? INIT : EXPR(_X))`.
    /// prev = `_X`, next = `X`, init = INIT, transition expr = EXPR (lowered).
    Scalar { prev: String, next: String, init: i64, transition_rhs: String },
}

/// The body bindings that produce SMT-LIB asserts (beyond the scalar
/// transition, which lives on the State).
enum Binding {
    /// `state_next = match state` — arms over the state enum.
    StateNextMatch { arms: Vec<(String, String)>, default: Option<String> },
    /// `effects = match state` — arms whose bodies are seq literals.
    EffectsMatch { arms: Vec<(String, String)>, default: Option<String> },
    /// `effects = (cond ? thenSeq : elseSeq)`. `cond_expr` is the raw,
    /// un-lowered condition AST — lowered at `emit` time so the scalar
    /// state-var rename (current `X` → engine prev `_X`) can be applied.
    EffectsTernary { cond_expr: Expr, then_seq: String, else_seq: String },
}

struct Program {
    fsm_name: String,
    /// enum name → ordered variant list.
    enums: HashMap<String, Vec<String>>,
    state: State,
    bindings: Vec<Binding>,
    /// Whether the program references an `effects` var at all.
    has_effects: bool,
}

// ---------------------------------------------------------------------------
// Tokenizer (shared minimal lexer for body expressions)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    IntLit(i64),
    StrLit(String),
    Eq, Neq, Lt, Le, Gt, Ge,
    Plus, Minus, Star, Slash,
    And, Or, Not,
    LParen, RParen, Comma,
    Question, Colon,
    SeqOpen, SeqClose, // ⟨ ⟩
    Concat,            // ++
    Arrow,             // ⇒ / =>
    Pipe,              // |
    True, False,
}

fn tokenize(line: &str) -> Result<Vec<Tok>, FrontendError> {
    let chars: Vec<char> = line.chars().collect();
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < chars.len() {
        let c = chars[pos];
        if c.is_whitespace() { pos += 1; continue; }
        if c == '"' {
            pos += 1;
            let mut s = String::new();
            while pos < chars.len() && chars[pos] != '"' {
                if chars[pos] == '\\' && pos + 1 < chars.len() {
                    pos += 1;
                    match chars[pos] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        '"' => s.push('"'),
                        '\\' => s.push('\\'),
                        other => { s.push('\\'); s.push(other); }
                    }
                } else {
                    s.push(chars[pos]);
                }
                pos += 1;
            }
            if pos >= chars.len() { return fe("unterminated string literal"); }
            pos += 1;
            out.push(Tok::StrLit(s));
            continue;
        }
        // Unicode single-char tokens
        match c {
            '∈' => { out.push(Tok::Ident("∈".into())); pos += 1; continue; } // membership handled by caller
            '≠' => { out.push(Tok::Neq); pos += 1; continue; }
            '≤' => { out.push(Tok::Le); pos += 1; continue; }
            '≥' => { out.push(Tok::Ge); pos += 1; continue; }
            '∧' => { out.push(Tok::And); pos += 1; continue; }
            '∨' => { out.push(Tok::Or); pos += 1; continue; }
            '¬' => { out.push(Tok::Not); pos += 1; continue; }
            '⇒' => { out.push(Tok::Arrow); pos += 1; continue; }
            '⟨' => { out.push(Tok::SeqOpen); pos += 1; continue; }
            '⟩' => { out.push(Tok::SeqClose); pos += 1; continue; }
            '|' => { out.push(Tok::Pipe); pos += 1; continue; }
            _ => {}
        }
        if pos + 1 < chars.len() {
            let two: String = chars[pos..pos + 2].iter().collect();
            match two.as_str() {
                "!=" => { out.push(Tok::Neq); pos += 2; continue; }
                "<=" => { out.push(Tok::Le); pos += 2; continue; }
                ">=" => { out.push(Tok::Ge); pos += 2; continue; }
                "=>" => { out.push(Tok::Arrow); pos += 2; continue; }
                "++" => { out.push(Tok::Concat); pos += 2; continue; }
                _ => {}
            }
        }
        match c {
            '=' => { out.push(Tok::Eq); pos += 1; continue; }
            '<' => { out.push(Tok::Lt); pos += 1; continue; }
            '>' => { out.push(Tok::Gt); pos += 1; continue; }
            '+' => { out.push(Tok::Plus); pos += 1; continue; }
            '-' => { out.push(Tok::Minus); pos += 1; continue; }
            '*' => { out.push(Tok::Star); pos += 1; continue; }
            '/' => { out.push(Tok::Slash); pos += 1; continue; }
            '(' => { out.push(Tok::LParen); pos += 1; continue; }
            ')' => { out.push(Tok::RParen); pos += 1; continue; }
            ',' => { out.push(Tok::Comma); pos += 1; continue; }
            '?' => { out.push(Tok::Question); pos += 1; continue; }
            ':' => { out.push(Tok::Colon); pos += 1; continue; }
            _ => {}
        }
        if c.is_ascii_digit() {
            let start = pos;
            while pos < chars.len() && chars[pos].is_ascii_digit() { pos += 1; }
            let s: String = chars[start..pos].iter().collect();
            let v: i64 = s.parse().map_err(|_| FrontendError(format!("bad int literal: {s}")))?;
            out.push(Tok::IntLit(v));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = pos;
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') { pos += 1; }
            let word: String = chars[start..pos].iter().collect();
            let tok = match word.as_str() {
                "true" => Tok::True,
                "false" => Tok::False,
                "and" => Tok::And,
                "or" => Tok::Or,
                "not" => Tok::Not,
                _ => Tok::Ident(word),
            };
            out.push(tok);
            continue;
        }
        return fe(format!("unexpected character in fsm body: {c:?}"));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Line helpers
// ---------------------------------------------------------------------------

/// Strip a trailing `-- comment` (not inside a string literal).
fn strip_comment(line: &str) -> &str {
    let chars: Vec<char> = line.chars().collect();
    let mut in_str = false;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' { in_str = !in_str; i += 1; continue; }
        if !in_str && i + 1 < chars.len() && chars[i] == '-' && chars[i + 1] == '-' {
            let byte = line.char_indices().nth(i).map(|(b, _)| b).unwrap_or(line.len());
            return &line[..byte];
        }
        i += 1;
    }
    line
}

/// Leading-whitespace count (in chars) — used to detect indented match arms.
fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

// ---------------------------------------------------------------------------
// Program parser
// ---------------------------------------------------------------------------

fn parse_program(src: &str) -> Result<Program, FrontendError> {
    // Pre-pass: keep raw lines (with indentation) but strip comments + blanks.
    let raw: Vec<(usize, String)> = src
        .lines()
        .map(|l| {
            let s = strip_comment(l);
            (indent_of(s), s.trim_end().to_string())
        })
        .filter(|(_, s)| !s.trim().is_empty())
        .collect();

    let mut enums: HashMap<String, Vec<String>> = HashMap::new();
    let mut fsm_header: Option<(String, Option<(String, String)>)> = None; // (name, Some((param, enumtype)))
    let mut state_scalar: Option<State> = None;
    let mut bindings: Vec<Binding> = Vec::new();
    let mut has_effects = false;

    let mut i = 0;
    while i < raw.len() {
        let (indent, line) = (&raw[i].0, raw[i].1.clone());
        let trimmed = line.trim();

        if trimmed.starts_with("import ") {
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("enum ") {
            // enum NAME = A | B | C
            let (name, variants) = parse_enum(rest)?;
            enums.insert(name, variants);
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("fsm ") {
            if fsm_header.is_some() {
                return fe("only a single `fsm` is supported in this subset");
            }
            fsm_header = Some(parse_fsm_header(rest)?);
            i += 1;
            continue;
        }
        // Bail on other top-level decls we don't model (claim/type/schema/subclaim).
        // These are the static-test blocks in example files — we ignore everything
        // after we've captured the fsm, but a `claim`/`type` at top level ends the
        // fsm body region.
        if *indent == 0
            && (trimmed.starts_with("claim ")
                || trimmed.starts_with("type ")
                || trimmed.starts_with("schema ")
                || trimmed == "claim")
        {
            // Everything from here on is a static test / unrelated decl — ignore.
            break;
        }

        // Otherwise this is an fsm body line (indented under the fsm). It only
        // makes sense once we've seen the fsm header.
        if fsm_header.is_none() {
            return fe(format!("unexpected top-level line before any `fsm`: {trimmed:?}"));
        }

        // ---- body line classification ----
        // `X ∈ Int = (is_first_tick ? INIT : EXPR)`  — scalar state
        if let Some(sc) = try_parse_scalar_state(trimmed)? {
            if state_scalar.is_some() {
                return fe("multiple scalar state declarations not supported");
            }
            state_scalar = Some(sc);
            i += 1;
            continue;
        }

        // `state_next = match state`  (multi-line block of indented arms)
        if let Some(scrut) = match_head(trimmed, "state_next") {
            let (arms, default, consumed) = parse_match_arms(&raw, i + 1, *indent, &scrut, false)?;
            bindings.push(Binding::StateNextMatch { arms, default });
            i += 1 + consumed;
            continue;
        }

        // `effects = match state`
        if let Some(scrut) = match_head(trimmed, "effects") {
            has_effects = true;
            let (arms, default, consumed) = parse_match_arms(&raw, i + 1, *indent, &scrut, true)?;
            bindings.push(Binding::EffectsMatch { arms, default });
            i += 1 + consumed;
            continue;
        }

        // `effects = (cond ? ⟨...⟩ : ⟨...⟩)`
        if let Some(rhs) = assign_rhs(trimmed, "effects") {
            has_effects = true;
            let b = parse_effects_ternary(&rhs)?;
            bindings.push(b);
            i += 1;
            continue;
        }

        return fe(format!("unsupported fsm body line: {trimmed:?}"));
    }

    let (fsm_name, param) = fsm_header.ok_or_else(|| FrontendError("no `fsm` declaration found".into()))?;

    // Resolve the state: enum (from header param) XOR scalar (from body).
    let state = match (param, state_scalar) {
        (Some((param_name, enum_type)), None) => {
            let variants = enums
                .get(&enum_type)
                .ok_or_else(|| FrontendError(format!("fsm state enum `{enum_type}` not declared")))?;
            let init = variants
                .first()
                .ok_or_else(|| FrontendError(format!("enum `{enum_type}` has no variants")))?
                .clone();
            State::Enum {
                prev: param_name.clone(),
                next: format!("{param_name}_next"),
                enum_name: enum_type,
                init,
            }
        }
        (None, Some(sc)) => sc,
        (Some(_), Some(_)) => {
            return fe("fsm has both an enum state param and a scalar state body line");
        }
        (None, None) => return fe("fsm has no recognizable state (enum param or scalar body)"),
    };

    Ok(Program { fsm_name, enums, state, bindings, has_effects })
}

/// Parse `NAME = A | B | C` into (name, [A, B, C]).
fn parse_enum(rest: &str) -> Result<(String, Vec<String>), FrontendError> {
    let eq = rest.find('=').ok_or_else(|| FrontendError(format!("malformed enum: {rest:?}")))?;
    let name = rest[..eq].trim().to_string();
    if name.is_empty() {
        return fe(format!("enum missing a name: {rest:?}"));
    }
    let variants: Vec<String> = rest[eq + 1..]
        .split('|')
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .collect();
    // Only nullary variants supported (no `(...)`).
    for v in &variants {
        if v.contains('(') {
            return fe(format!("enum variant with payload not supported in fsm subset: {v:?}"));
        }
    }
    if variants.is_empty() {
        return fe(format!("enum `{name}` has no variants"));
    }
    Ok((name, variants))
}

/// Parse `NAME` or `NAME(param ∈ EnumType)` after the `fsm ` keyword.
/// Returns (fsm_name, Some((param_name, enum_type)) | None).
fn parse_fsm_header(rest: &str) -> Result<(String, Option<(String, String)>), FrontendError> {
    let rest = rest.trim();
    match rest.find('(') {
        None => {
            // bare `fsm NAME`
            let name = rest.trim().to_string();
            if name.is_empty() || name.contains(char::is_whitespace) {
                return fe(format!("malformed fsm header: {rest:?}"));
            }
            Ok((name, None))
        }
        Some(lp) => {
            let name = rest[..lp].trim().to_string();
            let close = rest.rfind(')').ok_or_else(|| FrontendError(format!("unclosed fsm params: {rest:?}")))?;
            let params = rest[lp + 1..close].trim();
            // Subset: a single `param ∈ EnumType` param.
            let in_idx = params.find('∈').ok_or_else(|| {
                FrontendError(format!("fsm param must be `name ∈ EnumType`: {params:?}"))
            })?;
            let param_name = params[..in_idx].trim().to_string();
            let enum_type = params[in_idx + '∈'.len_utf8()..].trim().to_string();
            if param_name.is_empty() || enum_type.is_empty() || enum_type.contains(char::is_whitespace) {
                return fe(format!("fsm param must be a single `name ∈ EnumType`: {params:?}"));
            }
            Ok((name, Some((param_name, enum_type))))
        }
    }
}

/// Recognize `X ∈ Int = (is_first_tick ? INIT : EXPR)`.
/// Returns the Scalar state if it matches; `None` if the line isn't a scalar
/// state decl; `Err` if it looks like one but is malformed / out of subset.
fn try_parse_scalar_state(line: &str) -> Result<Option<State>, FrontendError> {
    // Must contain ∈ Int and a `=` and the is_first_tick idiom.
    let in_idx = match line.find('∈') {
        Some(x) => x,
        None => return Ok(None),
    };
    let name = line[..in_idx].trim().to_string();
    let after = line[in_idx + '∈'.len_utf8()..].trim_start();
    // after must start with the sort then `=`
    let eq = match after.find('=') {
        Some(x) => x,
        None => return Ok(None),
    };
    let sort = after[..eq].trim();
    if sort != "Int" && sort != "Nat" && sort != "Pos" {
        // Not a scalar-Int state line.
        return Ok(None);
    }
    if name.is_empty() || name.contains(char::is_whitespace) {
        return Ok(None);
    }
    let rhs = after[eq + 1..].trim();
    if !rhs.contains("is_first_tick") {
        return fe(format!(
            "scalar state `{name}` must use the `is_first_tick ? INIT : EXPR` idiom: {rhs:?}"
        ));
    }
    // Parse `(is_first_tick ? INIT : EXPR)`.
    let inner = strip_outer_parens(rhs);
    let q = inner.find('?').ok_or_else(|| FrontendError(format!("missing `?` in scalar state: {rhs:?}")))?;
    let cond = inner[..q].trim();
    if cond != "is_first_tick" {
        return fe(format!("scalar state condition must be `is_first_tick`: {cond:?}"));
    }
    let after_q = &inner[q + 1..];
    let colon = find_top_level_colon(after_q)
        .ok_or_else(|| FrontendError(format!("missing `:` in scalar state ternary: {rhs:?}")))?;
    let init_str = after_q[..colon].trim();
    let expr_str = after_q[colon + 1..].trim();
    let init: i64 = init_str
        .parse()
        .map_err(|_| FrontendError(format!("scalar state INIT must be an integer literal: {init_str:?}")))?;
    // Lower EXPR (an arithmetic expr over `_X`) to SMT-LIB.
    let toks = tokenize(expr_str)?;
    let (e, used) = parse_expr(&toks, 0)?;
    if used != toks.len() {
        return fe(format!("trailing tokens in scalar transition expr: {expr_str:?}"));
    }
    let transition_rhs = emit_expr(&e)?;
    Ok(Some(State::Scalar {
        prev: format!("_{name}"),
        next: name,
        init,
        transition_rhs,
    }))
}

/// `LHS = match SCRUT` → returns Some(scrut) if the line is `<lhs> = match <scrut>`.
fn match_head(line: &str, lhs: &str) -> Option<String> {
    let rhs = assign_rhs(line, lhs)?;
    let rhs = rhs.trim();
    let scrut = rhs.strip_prefix("match ")?;
    Some(scrut.trim().to_string())
}

/// If `line` is `<lhs> = <rhs>`, return the rhs (trimmed). Matches on the exact
/// lhs identifier (whitespace-insensitive around `=`).
fn assign_rhs(line: &str, lhs: &str) -> Option<String> {
    let line = line.trim();
    let rest = line.strip_prefix(lhs)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?;
    Some(rest.trim().to_string())
}

/// Parse indented `Variant ⇒ body` arms starting at `raw[start]`, all more
/// indented than `head_indent`. Returns (arms, default, lines_consumed).
/// `seq_body` = true means arm bodies are seq literals (effects); false means
/// they are enum-variant expressions (state_next).
fn parse_match_arms(
    raw: &[(usize, String)],
    start: usize,
    head_indent: usize,
    _scrut: &str,
    seq_body: bool,
) -> Result<(Vec<(String, String)>, Option<String>, usize), FrontendError> {
    let mut arms: Vec<(String, String)> = Vec::new();
    let mut default: Option<String> = None;
    let mut consumed = 0;
    let mut i = start;
    while i < raw.len() {
        let (indent, line) = (&raw[i].0, raw[i].1.clone());
        if *indent <= head_indent {
            break; // dedent → end of match block
        }
        let trimmed = line.trim();
        // `PATTERN ⇒ body`
        let toks = tokenize(trimmed)?;
        let arrow_pos = trimmed
            .find('⇒')
            .or_else(|| trimmed.find("=>"))
            .ok_or_else(|| FrontendError(format!("match arm missing `⇒`: {trimmed:?}")))?;
        let pat = trimmed[..arrow_pos].trim();
        let arrow_len = if trimmed[arrow_pos..].starts_with('⇒') { '⇒'.len_utf8() } else { 2 };
        let body = trimmed[arrow_pos + arrow_len..].trim();
        let _ = toks;

        let body_smt = if seq_body {
            emit_seq_literal_str(body)?
        } else {
            // state_next arm: a bare enum variant constructor.
            emit_enum_ctor(body)?
        };

        if pat == "_" {
            default = Some(body_smt);
        } else {
            arms.push((pat.to_string(), body_smt));
        }
        consumed += 1;
        i += 1;
    }
    if arms.is_empty() && default.is_none() {
        return fe("match block has no arms");
    }
    Ok((arms, default, consumed))
}

/// Parse `(cond ? ⟨...⟩ : ⟨...⟩)` for the effects ternary.
fn parse_effects_ternary(rhs: &str) -> Result<Binding, FrontendError> {
    let inner = strip_outer_parens(rhs);
    let q = inner
        .find('?')
        .ok_or_else(|| FrontendError(format!("effects ternary missing `?`: {rhs:?}")))?;
    let cond = inner[..q].trim();
    let after_q = &inner[q + 1..];
    let colon = find_top_level_colon(after_q)
        .ok_or_else(|| FrontendError(format!("effects ternary missing `:`: {rhs:?}")))?;
    let then_str = after_q[..colon].trim();
    let else_str = after_q[colon + 1..].trim();

    // Parse the condition expr; lowering is deferred to `emit` so the scalar
    // state-var rename can apply.
    let toks = tokenize(cond)?;
    let (cond_expr, used) = parse_expr(&toks, 0)?;
    if used != toks.len() {
        return fe(format!("trailing tokens in effects ternary condition: {cond:?}"));
    }

    Ok(Binding::EffectsTernary {
        cond_expr,
        then_seq: emit_seq_literal_str(then_str)?,
        else_seq: emit_seq_literal_str(else_str)?,
    })
}

// ---------------------------------------------------------------------------
// Small string utilities
// ---------------------------------------------------------------------------

/// Strip one layer of balanced outer parens, if the whole string is wrapped.
fn strip_outer_parens(s: &str) -> &str {
    let s = s.trim();
    let chars: Vec<char> = s.chars().collect();
    if chars.first() != Some(&'(') || chars.last() != Some(&')') {
        return s;
    }
    // Verify the first '(' matches the last ')'.
    let mut depth = 0;
    for (idx, &c) in chars.iter().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    // matched first paren at index idx; only strip if it's the last char.
                    return if idx == chars.len() - 1 {
                        let start = s.char_indices().nth(1).map(|(b, _)| b).unwrap_or(s.len());
                        let end = s.char_indices().last().map(|(b, _)| b).unwrap_or(s.len());
                        s[start..end].trim()
                    } else {
                        s
                    };
                }
            }
            _ => {}
        }
    }
    s
}

/// Find the byte index of a `:` at paren/seq depth 0 (so we don't split inside
/// `Color(1,2)` or a nested seq). Skips string literals.
fn find_top_level_colon(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_str = false;
    for (b, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            _ if in_str => {}
            '(' | '⟨' => depth += 1,
            ')' | '⟩' => depth -= 1,
            ':' if depth == 0 => return Some(b),
            _ => {}
        }
    }
    None
}

/// Split a string on top-level commas (paren/seq depth 0, skipping strings).
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut last = 0usize;
    for (b, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            _ if in_str => {}
            '(' | '⟨' => depth += 1,
            ')' | '⟩' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(s[last..b].trim().to_string());
                last = b + 1;
            }
            _ => {}
        }
    }
    let tail = s[last..].trim();
    if !tail.is_empty() || !parts.is_empty() {
        parts.push(tail.to_string());
    }
    parts
}

// ---------------------------------------------------------------------------
// Effect / seq literal lowering
// ---------------------------------------------------------------------------

/// Lower a `⟨a, b, ...⟩` sequence literal (as a raw string) to SMT-LIB.
fn emit_seq_literal_str(body: &str) -> Result<String, FrontendError> {
    let body = body.trim();
    let inner = body
        .strip_prefix('⟨')
        .and_then(|s| s.strip_suffix('⟩'))
        .ok_or_else(|| FrontendError(format!("expected a ⟨…⟩ sequence literal: {body:?}")))?;
    let elems: Vec<String> = split_top_level_commas(inner)
        .into_iter()
        .filter(|e| !e.is_empty())
        .collect();
    if elems.is_empty() {
        return Ok("(as seq.empty (Seq Effect))".into());
    }
    let units: Result<Vec<String>, FrontendError> = elems
        .iter()
        .map(|e| Ok(format!("(seq.unit {})", emit_effect_ctor(e)?)))
        .collect();
    let units = units?;
    if units.len() == 1 {
        Ok(units.into_iter().next().unwrap())
    } else {
        // (seq.++ a (seq.++ b c)) — right-associated nest of binary seq.++.
        let mut iter = units.into_iter().rev();
        let mut acc = iter.next().unwrap();
        for u in iter {
            acc = format!("(seq.++ {u} {acc})");
        }
        Ok(acc)
    }
}

/// Lower one effect constructor: `Println("s")` → `(Println "s")`,
/// `Exit(42)` → `(Exit 42)`.
fn emit_effect_ctor(e: &str) -> Result<String, FrontendError> {
    let e = e.trim();
    let lp = e.find('(').ok_or_else(|| FrontendError(format!("effect must be a constructor call: {e:?}")))?;
    let ctor = e[..lp].trim();
    let close = e.rfind(')').ok_or_else(|| FrontendError(format!("unclosed effect ctor: {e:?}")))?;
    let arg = e[lp + 1..close].trim();
    match ctor {
        "Println" => {
            let toks = tokenize(arg)?;
            match toks.as_slice() {
                [Tok::StrLit(s)] => Ok(format!("(Println {})", smt_str(s))),
                _ => fe(format!("Println argument must be a string literal: {arg:?}")),
            }
        }
        "Exit" => {
            let toks = tokenize(arg)?;
            match toks.as_slice() {
                [Tok::IntLit(n)] => Ok(format!("(Exit {})", smt_int(*n))),
                _ => fe(format!("Exit argument must be an integer literal: {arg:?}")),
            }
        }
        other => fe(format!("unsupported effect constructor `{other}` (only Println / Exit)")),
    }
}

/// Lower a bare enum variant constructor (for state_next arms): `Done` → `Done`.
/// (Nullary variants only in this subset.)
fn emit_enum_ctor(e: &str) -> Result<String, FrontendError> {
    let e = e.trim();
    if e.contains('(') {
        return fe(format!("enum constructor with payload not supported: {e:?}"));
    }
    if e.is_empty() || !e.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
        return fe(format!("expected a bare enum variant: {e:?}"));
    }
    Ok(e.to_string())
}

// ---------------------------------------------------------------------------
// Expression AST + lowering (for scalar transitions and ternary conditions)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Expr {
    Int(i64),
    Bool(bool),
    Str(String),
    Ident(String),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Eq, Neq, Lt, Le, Gt, Ge,
    And, Or,
    Add, Sub, Mul, Div,
}

type ParseResult = Result<(Expr, usize), FrontendError>;

// The bounded condition subset has no implication operator (conditions are
// comparisons joined by ∧/∨); `BinOp::Implies` remains only so `emit_expr`'s
// match is exhaustive and ready if the subset grows.
fn parse_expr(t: &[Tok], p: usize) -> ParseResult { parse_or(t, p) }

fn parse_or(t: &[Tok], p: usize) -> ParseResult {
    let (mut left, mut p) = parse_and(t, p)?;
    while matches!(t.get(p), Some(Tok::Or)) {
        let (r, p2) = parse_and(t, p + 1)?;
        p = p2;
        left = Expr::Bin(BinOp::Or, Box::new(left), Box::new(r));
    }
    Ok((left, p))
}
fn parse_and(t: &[Tok], p: usize) -> ParseResult {
    let (mut left, mut p) = parse_cmp(t, p)?;
    while matches!(t.get(p), Some(Tok::And)) {
        let (r, p2) = parse_cmp(t, p + 1)?;
        p = p2;
        left = Expr::Bin(BinOp::And, Box::new(left), Box::new(r));
    }
    Ok((left, p))
}
fn parse_cmp(t: &[Tok], p: usize) -> ParseResult {
    let (left, p) = parse_add(t, p)?;
    let op = match t.get(p) {
        Some(Tok::Eq) => BinOp::Eq,
        Some(Tok::Neq) => BinOp::Neq,
        Some(Tok::Lt) => BinOp::Lt,
        Some(Tok::Le) => BinOp::Le,
        Some(Tok::Gt) => BinOp::Gt,
        Some(Tok::Ge) => BinOp::Ge,
        _ => return Ok((left, p)),
    };
    let (r, p2) = parse_add(t, p + 1)?;
    Ok((Expr::Bin(op, Box::new(left), Box::new(r)), p2))
}
fn parse_add(t: &[Tok], p: usize) -> ParseResult {
    let (mut left, mut p) = parse_mul(t, p)?;
    loop {
        let op = match t.get(p) {
            Some(Tok::Plus) => BinOp::Add,
            Some(Tok::Minus) => BinOp::Sub,
            _ => break,
        };
        let (r, p2) = parse_mul(t, p + 1)?;
        p = p2;
        left = Expr::Bin(op, Box::new(left), Box::new(r));
    }
    Ok((left, p))
}
fn parse_mul(t: &[Tok], p: usize) -> ParseResult {
    let (mut left, mut p) = parse_unary(t, p)?;
    loop {
        let op = match t.get(p) {
            Some(Tok::Star) => BinOp::Mul,
            Some(Tok::Slash) => BinOp::Div,
            _ => break,
        };
        let (r, p2) = parse_unary(t, p + 1)?;
        p = p2;
        left = Expr::Bin(op, Box::new(left), Box::new(r));
    }
    Ok((left, p))
}
fn parse_unary(t: &[Tok], p: usize) -> ParseResult {
    match t.get(p) {
        Some(Tok::Not) => { let (e, p2) = parse_unary(t, p + 1)?; Ok((Expr::Not(Box::new(e)), p2)) }
        Some(Tok::Minus) => { let (e, p2) = parse_unary(t, p + 1)?; Ok((Expr::Neg(Box::new(e)), p2)) }
        _ => parse_atom(t, p),
    }
}
fn parse_atom(t: &[Tok], p: usize) -> ParseResult {
    match t.get(p) {
        Some(Tok::IntLit(i)) => Ok((Expr::Int(*i), p + 1)),
        Some(Tok::True) => Ok((Expr::Bool(true), p + 1)),
        Some(Tok::False) => Ok((Expr::Bool(false), p + 1)),
        Some(Tok::StrLit(s)) => Ok((Expr::Str(s.clone()), p + 1)),
        Some(Tok::Ident(n)) => Ok((Expr::Ident(n.clone()), p + 1)),
        Some(Tok::LParen) => {
            let (e, p2) = parse_expr(t, p + 1)?;
            match t.get(p2) {
                Some(Tok::RParen) => Ok((e, p2 + 1)),
                _ => fe("expected ')'"),
            }
        }
        other => fe(format!("unexpected token in expression: {other:?}")),
    }
}

/// Lower an expr with no identifier renaming.
fn emit_expr(e: &Expr) -> Result<String, FrontendError> {
    emit_expr_renamed(e, &HashMap::new())
}

/// Lower an expr, rewriting any identifier found in `rename` to its target.
/// Used for the scalar effects condition: the `.ev`'s current state var `X`
/// equals the engine's prev `_X`, so `X → _X` there.
fn emit_expr_renamed(e: &Expr, rename: &HashMap<String, String>) -> Result<String, FrontendError> {
    match e {
        Expr::Int(i) => Ok(smt_int(*i)),
        Expr::Bool(b) => Ok(if *b { "true".into() } else { "false".into() }),
        Expr::Str(s) => Ok(smt_str(s)),
        Expr::Ident(n) => Ok(rename.get(n).cloned().unwrap_or_else(|| n.clone())),
        Expr::Not(i) => Ok(format!("(not {})", emit_expr_renamed(i, rename)?)),
        Expr::Neg(i) => match i.as_ref() {
            Expr::Int(n) => Ok(smt_int(-n)),
            other => Ok(format!("(- {})", emit_expr_renamed(other, rename)?)),
        },
        Expr::Bin(BinOp::Neq, a, b) => Ok(format!("(not (= {} {}))", emit_expr_renamed(a, rename)?, emit_expr_renamed(b, rename)?)),
        Expr::Bin(op, a, b) => {
            let sym = match op {
                BinOp::Eq => "=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::And => "and",
                BinOp::Or => "or",
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "div",
                BinOp::Neq => unreachable!(),
            };
            Ok(format!("({sym} {} {})", emit_expr_renamed(a, rename)?, emit_expr_renamed(b, rename)?))
        }
    }
}

fn smt_int(i: i64) -> String {
    if i < 0 { format!("(- {})", (i as i128).unsigned_abs()) } else { i.to_string() }
}
fn smt_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if c == '"' { out.push('"'); }
        out.push(c);
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Emit: Program → fixture text
// ---------------------------------------------------------------------------

fn emit(prog: &Program) -> Result<String, FrontendError> {
    let mut out = String::new();

    // ---- @meta JSON ----
    out.push_str("; @meta\n");
    out.push_str("; {\n");
    out.push_str(";   \"fsms\": [\n");
    out.push_str(";     { \"name\": \"");
    out.push_str(&prog.fsm_name);
    out.push_str("\",\n");

    match &prog.state {
        State::Enum { prev, next, enum_name, init } => {
            let _ = write!(
                out,
                ";       \"state\": [{{\"prev\":\"{prev}\",\"next\":\"{next}\",\"sort\":\"{enum_name}\",\"init\":\"{init}\"}}]"
            );
        }
        State::Scalar { prev, next, init, .. } => {
            let _ = write!(
                out,
                ";       \"state\": [{{\"prev\":\"{prev}\",\"next\":\"{next}\",\"sort\":\"Int\",\"init\":{init}}}]"
            );
        }
    }
    if prog.has_effects {
        out.push_str(",\n;       \"effects\": {\"var\":\"effects\"}\n");
    } else {
        out.push('\n');
    }
    out.push_str(";     }\n");
    out.push_str(";   ]\n");
    out.push_str("; }\n");
    out.push_str("; @end\n");

    // ---- transition block ----
    let _ = writeln!(out, "; @transition {}", prog.fsm_name);

    // Datatypes: fixed Effect + (for enum state) the state enum, batched.
    emit_datatypes(&mut out, prog);

    // Const declarations.
    match &prog.state {
        State::Enum { prev, next, enum_name, .. } => {
            let _ = writeln!(out, "(declare-const {prev} {enum_name})");
            let _ = writeln!(out, "(declare-const {next} {enum_name})");
        }
        State::Scalar { prev, next, .. } => {
            let _ = writeln!(out, "(declare-const {prev} Int)");
            let _ = writeln!(out, "(declare-const {next} Int)");
        }
    }
    if prog.has_effects {
        let _ = writeln!(out, "(declare-const effects (Seq Effect))");
    }

    // ---- asserts ----
    // Scalar transition (state).
    if let State::Scalar { next, transition_rhs, .. } = &prog.state {
        let _ = writeln!(out, "(assert (= {next} {transition_rhs}))");
    }

    let scrut = match &prog.state {
        State::Enum { prev, .. } => prev.clone(),     // .ev's `state` (current) = engine prev
        State::Scalar { prev, .. } => prev.clone(),   // .ev's `count` (this tick) = engine prev
    };

    // For scalar state, the .ev's current state var `X` equals the engine's
    // prev `_X` (the engine runs the transition each tick and threads next→prev,
    // so prev at tick N holds the .ev's `X` at tick N — init pins tick 0).
    // Rename `X → _X` in effects expressions so the branch reads the right value.
    let mut rename: HashMap<String, String> = HashMap::new();
    if let State::Scalar { prev, next, .. } = &prog.state {
        rename.insert(next.clone(), prev.clone());
    }

    for b in &prog.bindings {
        match b {
            Binding::StateNextMatch { arms, default } => {
                let target = match &prog.state {
                    State::Enum { next, .. } => next.clone(),
                    State::Scalar { .. } => {
                        return fe("`state_next = match` used with a scalar (non-enum) fsm");
                    }
                };
                let ite = emit_state_match(prog, &scrut, arms, default)?;
                let _ = writeln!(out, "(assert (= {target} {ite}))");
            }
            Binding::EffectsMatch { arms, default } => {
                let ite = emit_effects_match(prog, &scrut, arms, default)?;
                let _ = writeln!(out, "(assert (= effects {ite}))");
            }
            Binding::EffectsTernary { cond_expr, then_seq, else_seq } => {
                let cond = emit_expr_renamed(cond_expr, &rename)?;
                let _ = writeln!(out, "(assert (= effects (ite {cond} {then_seq} {else_seq})))");
            }
        }
    }

    Ok(out)
}

fn emit_datatypes(out: &mut String, prog: &Program) {
    let effect_decl = "(Effect 0)";
    let effect_body = "((Println (msg String)) (Exit (code Int)))";
    match &prog.state {
        State::Enum { enum_name, .. } => {
            let variants = prog.enums.get(enum_name).cloned().unwrap_or_default();
            let body: String = variants.iter().map(|v| format!("({v})")).collect::<Vec<_>>().join(" ");
            let _ = writeln!(
                out,
                "(declare-datatypes ({effect_decl} ({enum_name} 0)) ({effect_body} ({body})))"
            );
        }
        State::Scalar { .. } => {
            let _ = writeln!(out, "(declare-datatypes ({effect_decl}) ({effect_body}))");
        }
    }
}

/// Build a nested ite for a `match state` whose arms produce enum constructors.
fn emit_state_match(
    prog: &Program,
    scrut: &str,
    arms: &[(String, String)],
    default: &Option<String>,
) -> Result<String, FrontendError> {
    emit_match(prog, scrut, arms, default)
}

fn emit_effects_match(
    prog: &Program,
    scrut: &str,
    arms: &[(String, String)],
    default: &Option<String>,
) -> Result<String, FrontendError> {
    emit_match(prog, scrut, arms, default)
}

/// Nested `(ite (is-Variant scrut) body rest)`. The final arm (or `_`/default)
/// becomes the innermost else.
fn emit_match(
    prog: &Program,
    scrut: &str,
    arms: &[(String, String)],
    default: &Option<String>,
) -> Result<String, FrontendError> {
    // Validate the scrutinee is the enum state and patterns are its variants.
    let enum_variants: Option<&Vec<String>> = match &prog.state {
        State::Enum { enum_name, .. } => prog.enums.get(enum_name),
        State::Scalar { .. } => None,
    };

    // Build from the last arm backward.
    // If a default exists, it's the base else. Otherwise the LAST arm's body is
    // the base else (its guard is implied — total over the enum).
    let mut elems = arms.to_vec();
    let base_else: String = if let Some(d) = default {
        d.clone()
    } else {
        let last = elems
            .pop()
            .ok_or_else(|| FrontendError("match has no arms".into()))?;
        // Validate the popped arm's pattern is a known variant.
        if let Some(vs) = enum_variants {
            if !vs.contains(&last.0) {
                return fe(format!("match pattern `{}` is not a variant of the state enum", last.0));
            }
        }
        last.1
    };

    let mut acc = base_else;
    for (pat, body) in elems.into_iter().rev() {
        if let Some(vs) = enum_variants {
            if !vs.contains(&pat) {
                return fe(format!("match pattern `{pat}` is not a variant of the state enum"));
            }
        }
        acc = format!("(ite (is-{pat} {scrut}) {body} {acc})");
    }
    Ok(acc)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::load_str;

    const COUNTDOWN: &str = "import \"stdlib/runtime.ev\"\n\nfsm countdown\n    count ∈ Int = (is_first_tick ? 3 : _count - 1)\n    effects = (count > 0 ? ⟨Println(\"tick\")⟩ : ⟨Println(\"done\"), Exit(0)⟩)\n";

    const TEST_08: &str = "import \"stdlib/runtime.ev\"\n\nenum XState = Init | Done\n\nfsm exit_demo(state ∈ XState)\n    state_next = match state\n        Init ⇒ Done\n        Done ⇒ Done\n\n    effects = match state\n        Init ⇒ ⟨Println(\"exiting with code 42\"), Exit(42)⟩\n        Done ⇒ ⟨⟩\n\nclaim sat_init_exits_42\n    state ∈ XState\n    effects ∈ Seq(Effect)\n    state = Init\n    exit_demo\n";

    const TEST_03: &str = "import \"stdlib/runtime.ev\"\n\nenum SeqState = Init | Done\n\nfsm seq_demo(state ∈ SeqState)\n    state_next = match state\n        Init ⇒ Done\n        Done ⇒ Done\n\n    effects = match state\n        Init ⇒ ⟨Println(\"first\"), Println(\"second\"), Println(\"third\"), Exit(0)⟩\n        Done ⇒ ⟨⟩\n";

    #[test]
    fn countdown_transpiles_and_loads() {
        let fix = transpile_fsm(COUNTDOWN).expect("should transpile");
        // Scalar state with init 3, effect branches on the prev (`_count`) since
        // the .ev's current `count` == engine prev.
        assert!(fix.contains("\"prev\":\"_count\",\"next\":\"count\",\"sort\":\"Int\",\"init\":3"), "meta:\n{fix}");
        assert!(fix.contains("(assert (= count (- _count 1)))"), "transition:\n{fix}");
        assert!(fix.contains("(ite (> _count 0)"), "effect branches on prev:\n{fix}");
        assert!(fix.contains("(Println \"tick\")"), "{fix}");
        assert!(fix.contains("(Println \"done\")") && fix.contains("(Exit 0)"), "{fix}");
        // load_str must accept it.
        let prob = load_str(&fix).expect("engine loads the fixture");
        assert_eq!(prob.fsms.len(), 1);
        assert_eq!(prob.fsms[0].name, "countdown");
    }

    #[test]
    fn test_08_transpiles_and_loads() {
        let fix = transpile_fsm(TEST_08).expect("should transpile");
        assert!(fix.contains("\"prev\":\"state\",\"next\":\"state_next\",\"sort\":\"XState\",\"init\":\"Init\""), "meta:\n{fix}");
        assert!(fix.contains("(declare-datatypes ((Effect 0) (XState 0))"), "datatypes:\n{fix}");
        assert!(fix.contains("(assert (= state_next (ite (is-Init state) Done Done)))"), "state_next:\n{fix}");
        assert!(fix.contains("(is-Init state)"), "effects match:\n{fix}");
        assert!(fix.contains("(Println \"exiting with code 42\")") && fix.contains("(Exit 42)"), "{fix}");
        assert!(fix.contains("(as seq.empty (Seq Effect))"), "empty seq arm:\n{fix}");
        let prob = load_str(&fix).expect("engine loads test_08 fixture");
        assert_eq!(prob.fsms[0].name, "exit_demo");
    }

    #[test]
    fn test_03_transpiles_and_loads() {
        let fix = transpile_fsm(TEST_03).expect("should transpile");
        assert!(fix.contains("(Println \"first\")"), "{fix}");
        assert!(fix.contains("(Println \"second\")"), "{fix}");
        assert!(fix.contains("(Println \"third\")"), "{fix}");
        assert!(fix.contains("(Exit 0)"), "{fix}");
        // Four effects nested as seq.++ binary.
        assert!(fix.contains("(seq.++ (seq.unit (Println \"first\"))"), "seq nest:\n{fix}");
        let prob = load_str(&fix).expect("engine loads test_03 fixture");
        assert_eq!(prob.fsms[0].name, "seq_demo");
    }

    #[test]
    fn out_of_subset_payload_enum_is_err() {
        let src = "enum E = A(Int) | B\nfsm f(state ∈ E)\n    state_next = match state\n        A ⇒ B\n        B ⇒ B\n";
        assert!(transpile_fsm(src).is_err());
    }

    #[test]
    fn multiple_fsms_is_err() {
        let src = "fsm a\n    x ∈ Int = (is_first_tick ? 0 : _x + 1)\nfsm b\n    y ∈ Int = (is_first_tick ? 0 : _y + 1)\n";
        assert!(transpile_fsm(src).is_err());
    }

    #[test]
    fn no_fsm_is_err() {
        let src = "enum E = A | B\n";
        assert!(transpile_fsm(src).is_err());
    }
}
