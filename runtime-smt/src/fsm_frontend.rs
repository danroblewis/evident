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
//! * Scalar Int state, either single-line
//!   `X ∈ Int = (is_first_tick ? INIT : EXPR(_X,…))` or two lines
//!   `X ∈ Int` then `X = (is_first_tick ? INIT : EXPR(_X,…))`. Engine
//!   `prev = "_X"`, `next = "X"`, `init = INIT`, transition `(= X EXPR)`.
//!   An fsm may carry BOTH an enum `state` and a scalar (e.g. test_19).
//! * `state_next = match state` arms → nested ite over `(is-Variant state)`.
//! * `state_next = (cond ? VariantA : VariantB)` → `(ite cond VariantA VariantB)`.
//! * `effects = match state` / `effects = (cond ? ⟨…⟩ : ⟨…⟩)` (nestable).
//! * Intermediate body vars: `X ∈ Bool|String|Int` then `X = expr`, or the
//!   chained `X ∈ T = expr` form. Emitted as a declared const + defining assert.
//! * `match last_results[i]` bodies (payload-binding or `_`-ignoring arms) →
//!   bounds-guarded nested ite over `((_ is Ctor) (seq.nth last_results i))`.
//! * `#last_results` → `(seq.len last_results)`.
//! * Effect ctors in `⟨⟩`: `Println(<str-expr>)`, `Exit(n)`, `IntToStr(<int-expr>)`,
//!   `ParseInt(<str-expr>)`. `++` in an effect arg is string concat → `str.++`.
//!
//! The FSM body always emits a FIXED Effect datatype plus, for enum state, the
//! state enum, batched into one `declare-datatypes`. If the program reads
//! `last_results`, the `Result` datatype is also emitted.
//!
//! ## The prev-rename semantic
//!
//! The engine threads `next → prev` and runs the transition `next = f(prev)`
//! every tick — it does NOT honor `is_first_tick` (that init branch is consumed
//! into the StateVar `init`). So the engine's PREV var at tick K equals the
//! oracle's *current-tick* value of that var. For a scalar `X`, the .ev's
//! current `X` is therefore the engine prev `_X` in ALL other expressions
//! (effects, state_next, intermediate vars): we rename `X → _X` everywhere
//! outside the scalar transition itself. The enum `state` IS the engine prev,
//! so it is never renamed.
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

/// Enum-typed state from `fsm F(state ∈ EnumType)`.
struct EnumState {
    /// prev = the param name (the .ev's current `state`).
    prev: String,
    /// next = `<param>_next`.
    next: String,
    enum_name: String,
    /// First variant of the enum — tick-0 init.
    init: String,
}

/// Scalar Int state from `X ∈ Int = (is_first_tick ? INIT : EXPR(_X))`.
struct ScalarState {
    /// The .ev variable name (e.g. `count`). Engine `next`.
    name: String,
    /// Engine `prev` (`_<name>`).
    prev: String,
    /// Tick-0 init.
    init: i64,
    /// Lowered transition RHS (the EXPR over `_X`).
    transition_rhs: String,
}

/// A body binding that produces SMT-LIB asserts.
enum Binding {
    /// An intermediate body var: `X ∈ T` then `X = expr` (or chained). Emitted
    /// as `(declare-const X <smt-sort>)` + `(assert (= X <expr>))`. The value
    /// is either a plain expression or a `match last_results[i]`.
    IntermediateVar {
        name: String,
        smt_sort: String,
        value: BindingValue,
    },
    /// `state_next = match state` — arms over the state enum (raw variant bodies).
    StateNextMatch { arms: Vec<(String, String)>, default: Option<String> },
    /// `state_next = (cond ? VariantA : VariantB)`.
    StateNextTernary { cond_expr: Expr, then_variant: String, else_variant: String },
    /// `effects = match state` — arms whose bodies are seq literals (raw text,
    /// lowered at emit time so rename applies).
    EffectsMatch { arms: Vec<(String, String)>, default: Option<String> },
    /// `effects = <seq-expression>` (a ⟨…⟩ literal or a possibly-nested
    /// `(cond ? <seq> : <seq>)` ternary). Stored as raw text, lowered at emit.
    EffectsExpr { raw: String },
}

/// The RHS of an intermediate var binding.
enum BindingValue {
    /// A plain (possibly renamed) expression.
    Expr(Expr),
    /// `match last_results[i]` with arms producing values of the var's sort.
    MatchLastResults {
        /// `last_results` index.
        index: usize,
        /// The `last_results` var name (default "last_results").
        lr_var: String,
        /// (ctor, payload-binding-or-None, body-expr) per arm.
        arms: Vec<MatchArm>,
        /// The `_` default body (required).
        default: Box<Expr>,
    },
}

/// One arm of a `match last_results[i]`.
struct MatchArm {
    /// The Result constructor (e.g. `StringResult`).
    ctor: String,
    /// The payload binding name, if the arm binds it (`StringResult(s)` → Some("s")).
    /// `None` for `_`-payload arms (`ErrorResult(_)`).
    bind: Option<String>,
    /// The arm body expression.
    body: Expr,
}

struct Program {
    fsm_name: String,
    /// enum name → ordered variant list.
    enums: HashMap<String, Vec<String>>,
    enum_state: Option<EnumState>,
    scalar_state: Option<ScalarState>,
    bindings: Vec<Binding>,
    /// Whether the program references an `effects` var at all.
    has_effects: bool,
    /// Whether the program reads `last_results` / `#last_results` anywhere.
    reads_last_results: bool,
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
    LParen, RParen, LBracket, RBracket, Comma,
    Question, Colon,
    SeqOpen, SeqClose, // ⟨ ⟩
    Concat,            // ++
    Arrow,             // ⇒ / =>
    Pipe,              // |
    Hash,              // #
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
            '#' => { out.push(Tok::Hash); pos += 1; continue; }
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
            '[' => { out.push(Tok::LBracket); pos += 1; continue; }
            ']' => { out.push(Tok::RBracket); pos += 1; continue; }
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
    let mut scalar_state: Option<ScalarState> = None;
    // Two-line scalar declaration in flight: a bare `X ∈ Int` awaiting `X = …`.
    let mut pending_scalar_decl: Option<String> = None;
    // Intermediate var declarations in flight: name → smt_sort, awaiting `name = …`.
    let mut pending_intermediate: HashMap<String, String> = HashMap::new();
    // Track declaration order of pending intermediates so we can error helpfully.
    let mut bindings: Vec<Binding> = Vec::new();
    let mut has_effects = false;
    let mut reads_last_results = false;

    let mut i = 0;
    while i < raw.len() {
        let (indent, line) = (&raw[i].0, raw[i].1.clone());
        let trimmed = line.trim();

        if trimmed.starts_with("import ") {
            i += 1;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("enum ") {
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
        // A top-level `claim`/`type`/`schema` ends the fsm body region — the rest
        // are static-test blocks we ignore.
        if *indent == 0
            && (trimmed.starts_with("claim ")
                || trimmed.starts_with("type ")
                || trimmed.starts_with("schema ")
                || trimmed.starts_with("subclaim ")
                || trimmed == "claim")
        {
            break;
        }

        if fsm_header.is_none() {
            return fe(format!("unexpected top-level line before any `fsm`: {trimmed:?}"));
        }

        // ---- body line classification ----

        // Single-line scalar state: `X ∈ Int = (is_first_tick ? INIT : EXPR)`.
        if let Some(sc) = try_parse_scalar_state(trimmed)? {
            if scalar_state.is_some() {
                return fe("multiple scalar state declarations not supported");
            }
            scalar_state = Some(sc);
            i += 1;
            continue;
        }

        // A bare membership decl: `X ∈ T` (no `=` on this line). Could be a
        // two-line scalar state decl OR an intermediate var decl. Stash it.
        if let Some((name, sort)) = try_parse_bare_decl(trimmed)? {
            if sort == "Int" || sort == "Nat" || sort == "Pos" {
                // Could be scalar state (next line `X = (is_first_tick ? …)`) or
                // an intermediate Int. Decide when we see the `=` line.
                pending_scalar_decl = Some(name.clone());
                pending_intermediate.insert(name, smt_sort_of(&sort)?);
            } else {
                pending_intermediate.insert(name, smt_sort_of(&sort)?);
            }
            i += 1;
            continue;
        }

        // `state_next = match state`  (multi-line block of indented arms).
        if let Some(_scrut) = match_head(trimmed, "state_next") {
            let (arms, default, consumed) = parse_enum_match_arms(&raw, i + 1, *indent)?;
            bindings.push(Binding::StateNextMatch { arms, default });
            i += 1 + consumed;
            continue;
        }

        // `state_next = (cond ? VariantA : VariantB)`.
        if let Some(rhs) = assign_rhs(trimmed, "state_next") {
            let b = parse_state_next_ternary(&rhs)?;
            bindings.push(b);
            i += 1;
            continue;
        }

        // `effects = match state`.
        if let Some(_scrut) = match_head(trimmed, "effects") {
            has_effects = true;
            let (arms, default, consumed) = parse_effects_match_arms(&raw, i + 1, *indent)?;
            bindings.push(Binding::EffectsMatch { arms, default });
            i += 1 + consumed;
            continue;
        }

        // `effects = <seq-expr>` (may be a ⟨…⟩ literal or a possibly multi-line
        // nested ternary). Collect continuation lines (more-indented than head).
        if let Some(first) = assign_rhs(trimmed, "effects") {
            has_effects = true;
            let (raw_rhs, consumed) = gather_continuation(&raw, i, *indent, &first);
            if raw_rhs.contains("last_results") {
                reads_last_results = true;
            }
            bindings.push(Binding::EffectsExpr { raw: raw_rhs });
            i += 1 + consumed;
            continue;
        }

        // An intermediate var definition: `X = match last_results[i]` (multi-line)
        // or `X = <expr>` (single line). Also the two-line scalar state assign.
        if let Some((name, rhs)) = split_assign(trimmed) {
            // Two-line scalar state: `X = (is_first_tick ? INIT : EXPR)`.
            if pending_scalar_decl.as_deref() == Some(name.as_str())
                && rhs.contains("is_first_tick")
            {
                if scalar_state.is_some() {
                    return fe("multiple scalar state declarations not supported");
                }
                let sc = parse_scalar_state_rhs(&name, &rhs)?;
                scalar_state = Some(sc);
                pending_scalar_decl = None;
                pending_intermediate.remove(&name);
                i += 1;
                continue;
            }

            // An intermediate var (must have been declared on a prior `X ∈ T` line).
            let smt_sort = pending_intermediate.remove(&name).ok_or_else(|| {
                FrontendError(format!(
                    "assignment to `{name}` without a preceding `{name} ∈ <Type>` declaration"
                ))
            })?;
            pending_scalar_decl = None;

            // `X = match last_results[i]` — a multi-line indented match block.
            if let Some(inner) = rhs.strip_prefix("match ") {
                let scrut = inner.trim();
                if let Some((lr_var, index)) = parse_last_results_index(scrut) {
                    reads_last_results = true;
                    let (arms, default, consumed) =
                        parse_last_results_match_arms(&raw, i + 1, *indent)?;
                    let default = default.ok_or_else(|| {
                        FrontendError(format!(
                            "`{name} = match last_results[{index}]` needs a `_` default arm"
                        ))
                    })?;
                    bindings.push(Binding::IntermediateVar {
                        name,
                        smt_sort,
                        value: BindingValue::MatchLastResults {
                            index,
                            lr_var,
                            arms,
                            default: Box::new(default),
                        },
                    });
                    i += 1 + consumed;
                    continue;
                }
                return fe(format!("unsupported `match` scrutinee for `{name}`: {scrut:?}"));
            }

            // `X = <expr>` — a single-line expression binding.
            if rhs.contains("last_results") {
                reads_last_results = true;
            }
            let toks = tokenize(&rhs)?;
            let (e, used) = parse_expr(&toks, 0)?;
            if used != toks.len() {
                return fe(format!("trailing tokens in `{name}` definition: {rhs:?}"));
            }
            bindings.push(Binding::IntermediateVar {
                name,
                smt_sort,
                value: BindingValue::Expr(e),
            });
            i += 1;
            continue;
        }

        return fe(format!("unsupported fsm body line: {trimmed:?}"));
    }

    let (fsm_name, param) = fsm_header.ok_or_else(|| FrontendError("no `fsm` declaration found".into()))?;

    // Resolve the enum state (from the header param), if any.
    let enum_state = match param {
        Some((param_name, enum_type)) => {
            let variants = enums
                .get(&enum_type)
                .ok_or_else(|| FrontendError(format!("fsm state enum `{enum_type}` not declared")))?;
            let init = variants
                .first()
                .ok_or_else(|| FrontendError(format!("enum `{enum_type}` has no variants")))?
                .clone();
            Some(EnumState {
                prev: param_name.clone(),
                next: format!("{param_name}_next"),
                enum_name: enum_type,
                init,
            })
        }
        None => None,
    };

    if enum_state.is_none() && scalar_state.is_none() {
        return fe("fsm has no recognizable state (enum param or scalar body)");
    }

    Ok(Program {
        fsm_name,
        enums,
        enum_state,
        scalar_state,
        bindings,
        has_effects,
        reads_last_results,
    })
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
fn parse_fsm_header(rest: &str) -> Result<(String, Option<(String, String)>), FrontendError> {
    let rest = rest.trim();
    match rest.find('(') {
        None => {
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

/// Map an Evident type name to an SMT-LIB sort name (for intermediate vars).
fn smt_sort_of(sort: &str) -> Result<String, FrontendError> {
    match sort {
        "Int" | "Nat" | "Pos" => Ok("Int".into()),
        "Bool" => Ok("Bool".into()),
        "String" | "Str" => Ok("String".into()),
        other => fe(format!("unsupported intermediate var sort `{other}`")),
    }
}

/// Parse a bare `X ∈ T` declaration line (no `=`). Returns None if not this shape.
fn try_parse_bare_decl(line: &str) -> Result<Option<(String, String)>, FrontendError> {
    let in_idx = match line.find('∈') {
        Some(x) => x,
        None => return Ok(None),
    };
    let after = &line[in_idx + '∈'.len_utf8()..];
    if after.contains('=') {
        // Has an `=` — handled elsewhere (chained membership / scalar single-line).
        return Ok(None);
    }
    let name = line[..in_idx].trim().to_string();
    let sort = after.trim().to_string();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return Ok(None);
    }
    // Only bare type names (no `Seq(...)`, no payload) here.
    if sort.contains('(') || sort.contains(' ') {
        return Ok(None);
    }
    // Recognized intermediate / scalar sorts only; otherwise leave for other paths.
    match sort.as_str() {
        "Int" | "Nat" | "Pos" | "Bool" | "String" | "Str" => Ok(Some((name, sort))),
        _ => Ok(None),
    }
}

/// Recognize the single-line `X ∈ Int = (is_first_tick ? INIT : EXPR)`.
fn try_parse_scalar_state(line: &str) -> Result<Option<ScalarState>, FrontendError> {
    let in_idx = match line.find('∈') {
        Some(x) => x,
        None => return Ok(None),
    };
    let name = line[..in_idx].trim().to_string();
    let after = line[in_idx + '∈'.len_utf8()..].trim_start();
    let eq = match after.find('=') {
        Some(x) => x,
        None => return Ok(None),
    };
    let sort = after[..eq].trim();
    if sort != "Int" && sort != "Nat" && sort != "Pos" {
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
    Ok(Some(parse_scalar_state_rhs(&name, rhs)?))
}

/// Parse the `(is_first_tick ? INIT : EXPR)` RHS into a ScalarState.
fn parse_scalar_state_rhs(name: &str, rhs: &str) -> Result<ScalarState, FrontendError> {
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
    let toks = tokenize(expr_str)?;
    let (e, used) = parse_expr(&toks, 0)?;
    if used != toks.len() {
        return fe(format!("trailing tokens in scalar transition expr: {expr_str:?}"));
    }
    // The scalar transition is over the engine prev `_X` already (e.g. `_count + 1`).
    let transition_rhs = emit_expr(&e)?;
    Ok(ScalarState {
        name: name.to_string(),
        prev: format!("_{name}"),
        init,
        transition_rhs,
    })
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
    // Ensure we matched a whole identifier, not a prefix (`effects` vs `effects2`).
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?;
    Some(rest.trim().to_string())
}

/// Generic `<name> = <rhs>` split (for intermediate vars). Returns (name, rhs).
/// Only fires when the LHS is a bare identifier and the operator is `=` (not
/// `==`, `≠`, etc.).
fn split_assign(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    // Find the first top-level `=` that isn't part of `<=`, `>=`, `!=`.
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut depth = 0i32;
    let mut in_str = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => in_str = !in_str,
            _ if in_str => {}
            '(' | '⟨' | '[' => depth += 1,
            ')' | '⟩' | ']' => depth -= 1,
            '=' if depth == 0 => {
                // Reject `<=`, `>=`, `!=`, `==`.
                let prev = if i > 0 { chars[i - 1] } else { ' ' };
                let next = if i + 1 < chars.len() { chars[i + 1] } else { ' ' };
                if prev == '<' || prev == '>' || prev == '!' || next == '=' {
                    i += 1;
                    continue;
                }
                let lhs: String = chars[..i].iter().collect();
                let rhs: String = chars[i + 1..].iter().collect();
                let lhs = lhs.trim().to_string();
                let rhs = rhs.trim().to_string();
                // LHS must be a bare identifier.
                if lhs.is_empty()
                    || !lhs.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
                    || !lhs.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    return None;
                }
                return Some((lhs, rhs));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Parse `last_results[i]` → (var_name, index). None if not that shape.
fn parse_last_results_index(scrut: &str) -> Option<(String, usize)> {
    let scrut = scrut.trim();
    let lb = scrut.find('[')?;
    let rb = scrut.rfind(']')?;
    if rb < lb {
        return None;
    }
    let var = scrut[..lb].trim().to_string();
    let idx_str = scrut[lb + 1..rb].trim();
    let index: usize = idx_str.parse().ok()?;
    Some((var, index))
}

/// Parse indented enum-variant `Variant ⇒ Variant` arms (state_next match).
fn parse_enum_match_arms(
    raw: &[(usize, String)],
    start: usize,
    head_indent: usize,
) -> Result<(Vec<(String, String)>, Option<String>, usize), FrontendError> {
    let mut arms: Vec<(String, String)> = Vec::new();
    let mut default: Option<String> = None;
    let mut consumed = 0;
    let mut i = start;
    while i < raw.len() {
        let (indent, line) = (&raw[i].0, raw[i].1.clone());
        if *indent <= head_indent {
            break;
        }
        let trimmed = line.trim();
        let (pat, body) = split_arm(trimmed)?;
        let body_smt = emit_enum_ctor(body)?;
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

/// Parse indented `Variant ⇒ <seq-literal>` arms (effects match). Bodies are
/// kept as RAW seq-literal text and lowered at emit time (so rename applies).
fn parse_effects_match_arms(
    raw: &[(usize, String)],
    start: usize,
    head_indent: usize,
) -> Result<(Vec<(String, String)>, Option<String>, usize), FrontendError> {
    let mut arms: Vec<(String, String)> = Vec::new();
    let mut default: Option<String> = None;
    let mut consumed = 0;
    let mut i = start;
    while i < raw.len() {
        let (indent, line) = (&raw[i].0, raw[i].1.clone());
        if *indent <= head_indent {
            break;
        }
        let trimmed = line.trim();
        let (pat, body) = split_arm(trimmed)?;
        let body_raw = body.to_string();
        if pat == "_" {
            default = Some(body_raw);
        } else {
            arms.push((pat.to_string(), body_raw));
        }
        consumed += 1;
        i += 1;
    }
    if arms.is_empty() && default.is_none() {
        return fe("match block has no arms");
    }
    Ok((arms, default, consumed))
}

/// Parse indented `Ctor(bind) ⇒ body` / `Ctor(_) ⇒ body` / `_ ⇒ body` arms for
/// a `match last_results[i]` block.
fn parse_last_results_match_arms(
    raw: &[(usize, String)],
    start: usize,
    head_indent: usize,
) -> Result<(Vec<MatchArm>, Option<Expr>, usize), FrontendError> {
    let mut arms: Vec<MatchArm> = Vec::new();
    let mut default: Option<Expr> = None;
    let mut consumed = 0;
    let mut i = start;
    while i < raw.len() {
        let (indent, line) = (&raw[i].0, raw[i].1.clone());
        if *indent <= head_indent {
            break;
        }
        let trimmed = line.trim();
        let (pat, body) = split_arm(trimmed)?;
        // Parse the body expression.
        let toks = tokenize(body)?;
        let (body_expr, used) = parse_expr(&toks, 0)?;
        if used != toks.len() {
            return fe(format!("trailing tokens in match arm body: {body:?}"));
        }
        if pat == "_" {
            default = Some(body_expr);
        } else {
            // `Ctor(bind)` or `Ctor(_)`.
            let lp = pat.find('(').ok_or_else(|| {
                FrontendError(format!("match arm pattern must be `Ctor(x)` or `_`: {pat:?}"))
            })?;
            let ctor = pat[..lp].trim().to_string();
            let rp = pat.rfind(')').ok_or_else(|| {
                FrontendError(format!("unclosed pattern in match arm: {pat:?}"))
            })?;
            let bind_raw = pat[lp + 1..rp].trim();
            let bind = if bind_raw == "_" || bind_raw.is_empty() {
                None
            } else {
                Some(bind_raw.to_string())
            };
            arms.push(MatchArm { ctor, bind, body: body_expr });
        }
        consumed += 1;
        i += 1;
    }
    if arms.is_empty() && default.is_none() {
        return fe("match block has no arms");
    }
    Ok((arms, default, consumed))
}

/// Split `PATTERN ⇒ body` into (pattern, body).
fn split_arm(trimmed: &str) -> Result<(&str, &str), FrontendError> {
    let arrow_pos = trimmed
        .find('⇒')
        .or_else(|| trimmed.find("=>"))
        .ok_or_else(|| FrontendError(format!("match arm missing `⇒`: {trimmed:?}")))?;
    let pat = trimmed[..arrow_pos].trim();
    let arrow_len = if trimmed[arrow_pos..].starts_with('⇒') { '⇒'.len_utf8() } else { 2 };
    let body = trimmed[arrow_pos + arrow_len..].trim();
    Ok((pat, body))
}

/// Parse `(cond ? VariantA : VariantB)` for the state_next ternary.
fn parse_state_next_ternary(rhs: &str) -> Result<Binding, FrontendError> {
    let inner = strip_outer_parens(rhs);
    let q = inner
        .find('?')
        .ok_or_else(|| FrontendError(format!("state_next must be `match` or `(cond ? A : B)`: {rhs:?}")))?;
    let cond = inner[..q].trim();
    let after_q = &inner[q + 1..];
    let colon = find_top_level_colon(after_q)
        .ok_or_else(|| FrontendError(format!("state_next ternary missing `:`: {rhs:?}")))?;
    let then_v = after_q[..colon].trim();
    let else_v = after_q[colon + 1..].trim();

    let toks = tokenize(cond)?;
    let (cond_expr, used) = parse_expr(&toks, 0)?;
    if used != toks.len() {
        return fe(format!("trailing tokens in state_next ternary condition: {cond:?}"));
    }
    Ok(Binding::StateNextTernary {
        cond_expr,
        then_variant: emit_enum_ctor(then_v)?,
        else_variant: emit_enum_ctor(else_v)?,
    })
}

/// Collect `raw[start]`'s assignment RHS plus any more-indented continuation
/// lines into one whitespace-joined string. Returns (joined_rhs, lines_consumed
/// beyond the head line).
fn gather_continuation(
    raw: &[(usize, String)],
    start: usize,
    head_indent: usize,
    first: &str,
) -> (String, usize) {
    let mut acc = first.to_string();
    let mut consumed = 0;
    let mut i = start + 1;
    while i < raw.len() {
        let (indent, line) = (&raw[i].0, &raw[i].1);
        if *indent <= head_indent {
            break;
        }
        acc.push(' ');
        acc.push_str(line.trim());
        consumed += 1;
        i += 1;
    }
    (acc, consumed)
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
    let mut depth = 0;
    for (idx, &c) in chars.iter().enumerate() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
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

/// Find the byte index of a `:` at paren/seq/bracket depth 0. Skips strings.
fn find_top_level_colon(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_str = false;
    for (b, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            _ if in_str => {}
            '(' | '⟨' | '[' => depth += 1,
            ')' | '⟩' | ']' => depth -= 1,
            ':' if depth == 0 => return Some(b),
            _ => {}
        }
    }
    None
}

/// Find the byte index of a `?` at paren/seq/bracket depth 0. Skips strings.
fn find_top_level_question(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_str = false;
    for (b, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            _ if in_str => {}
            '(' | '⟨' | '[' => depth += 1,
            ')' | '⟩' | ']' => depth -= 1,
            '?' if depth == 0 => return Some(b),
            _ => {}
        }
    }
    None
}

/// Split a string on top-level commas (paren/seq/bracket depth 0, skipping strings).
fn split_top_level_commas(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut last = 0usize;
    for (b, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            _ if in_str => {}
            '(' | '⟨' | '[' => depth += 1,
            ')' | '⟩' | ']' => depth -= 1,
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
// Effect / seq literal lowering (rename-aware, deferred to emit time)
// ---------------------------------------------------------------------------

/// Lower an effects RHS expression to an SMT-LIB `(Seq Effect)` term. The RHS is
/// either a `⟨…⟩` literal or a (possibly nested) `(cond ? <seq> : <seq>)`
/// ternary. Identifiers are renamed per `rename` (scalar var → prev).
fn emit_effects_rhs(rhs: &str, rename: &HashMap<String, String>) -> Result<String, FrontendError> {
    let s = strip_outer_parens(rhs).trim().to_string();
    // Ternary? `cond ? then : else`.
    if let Some(q) = find_top_level_question(&s) {
        let cond = s[..q].trim();
        let after_q = &s[q + 1..];
        let colon = find_top_level_colon(after_q)
            .ok_or_else(|| FrontendError(format!("effects ternary missing `:`: {rhs:?}")))?;
        let then_str = after_q[..colon].trim();
        let else_str = after_q[colon + 1..].trim();
        let toks = tokenize(cond)?;
        let (cond_expr, used) = parse_expr(&toks, 0)?;
        if used != toks.len() {
            return fe(format!("trailing tokens in effects ternary condition: {cond:?}"));
        }
        let cond_smt = emit_expr_renamed(&cond_expr, rename)?;
        let then_smt = emit_effects_rhs(then_str, rename)?;
        let else_smt = emit_effects_rhs(else_str, rename)?;
        return Ok(format!("(ite {cond_smt} {then_smt} {else_smt})"));
    }
    // Otherwise a seq literal.
    emit_seq_literal(&s, rename)
}

/// Lower a `⟨a, b, ...⟩` sequence literal to SMT-LIB, renaming identifiers.
fn emit_seq_literal(body: &str, rename: &HashMap<String, String>) -> Result<String, FrontendError> {
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
        .map(|e| Ok(format!("(seq.unit {})", emit_effect_ctor(e, rename)?)))
        .collect();
    let units = units?;
    if units.len() == 1 {
        Ok(units.into_iter().next().unwrap())
    } else {
        let mut iter = units.into_iter().rev();
        let mut acc = iter.next().unwrap();
        for u in iter {
            acc = format!("(seq.++ {u} {acc})");
        }
        Ok(acc)
    }
}

/// Lower one effect constructor: `Println(<str-expr>)`, `Exit(n)`,
/// `IntToStr(<int-expr>)`, `ParseInt(<str-expr>)`. Identifiers are renamed.
fn emit_effect_ctor(e: &str, rename: &HashMap<String, String>) -> Result<String, FrontendError> {
    let e = e.trim();
    let lp = e.find('(').ok_or_else(|| FrontendError(format!("effect must be a constructor call: {e:?}")))?;
    let ctor = e[..lp].trim();
    let close = e.rfind(')').ok_or_else(|| FrontendError(format!("unclosed effect ctor: {e:?}")))?;
    let arg = e[lp + 1..close].trim();
    match ctor {
        "Println" => {
            let arg_smt = emit_arg_expr(arg, rename)?;
            Ok(format!("(Println {arg_smt})"))
        }
        "Exit" => {
            let toks = tokenize(arg)?;
            match toks.as_slice() {
                [Tok::IntLit(n)] => Ok(format!("(Exit {})", smt_int(*n))),
                _ => {
                    // Allow an expression (e.g. a renamed var) for the code.
                    let arg_smt = emit_arg_expr(arg, rename)?;
                    Ok(format!("(Exit {arg_smt})"))
                }
            }
        }
        "IntToStr" => {
            let arg_smt = emit_arg_expr(arg, rename)?;
            Ok(format!("(IntToStr {arg_smt})"))
        }
        "ParseInt" => {
            let arg_smt = emit_arg_expr(arg, rename)?;
            Ok(format!("(ParseInt {arg_smt})"))
        }
        other => fe(format!(
            "unsupported effect constructor `{other}` (Println / Exit / IntToStr / ParseInt)"
        )),
    }
}

/// Lower an effect-argument expression (a string-concat, a var, or a literal).
fn emit_arg_expr(arg: &str, rename: &HashMap<String, String>) -> Result<String, FrontendError> {
    let toks = tokenize(arg)?;
    let (e, used) = parse_expr(&toks, 0)?;
    if used != toks.len() {
        return fe(format!("trailing tokens in effect argument: {arg:?}"));
    }
    emit_expr_renamed(&e, rename)
}

/// Lower a bare enum variant constructor (for state_next arms): `Done` → `Done`.
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
// Expression AST + lowering (for scalar transitions, conditions, intermediate
// var defs, and effect args)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Expr {
    Int(i64),
    Bool(bool),
    Str(String),
    Ident(String),
    /// `#last_results` — sequence length.
    SeqLen(String),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// String concat (`++`).
    Concat(Box<Expr>, Box<Expr>),
    /// Ternary `(cond ? then : else)` → `(ite cond then else)`.
    Ite(Box<Expr>, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Eq, Neq, Lt, Le, Gt, Ge,
    And, Or,
    Add, Sub, Mul, Div,
}

type ParseResult = Result<(Expr, usize), FrontendError>;

fn parse_expr(t: &[Tok], p: usize) -> ParseResult { parse_ternary(t, p) }

/// Ternary `cond ? then : else` — lowest precedence, right-associative.
fn parse_ternary(t: &[Tok], p: usize) -> ParseResult {
    let (cond, p) = parse_or(t, p)?;
    if matches!(t.get(p), Some(Tok::Question)) {
        let (then_e, p2) = parse_ternary(t, p + 1)?;
        match t.get(p2) {
            Some(Tok::Colon) => {
                let (else_e, p3) = parse_ternary(t, p2 + 1)?;
                Ok((Expr::Ite(Box::new(cond), Box::new(then_e), Box::new(else_e)), p3))
            }
            other => fe(format!("ternary missing `:`, got {other:?}")),
        }
    } else {
        Ok((cond, p))
    }
}

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
    let (left, p) = parse_concat(t, p)?;
    let op = match t.get(p) {
        Some(Tok::Eq) => BinOp::Eq,
        Some(Tok::Neq) => BinOp::Neq,
        Some(Tok::Lt) => BinOp::Lt,
        Some(Tok::Le) => BinOp::Le,
        Some(Tok::Gt) => BinOp::Gt,
        Some(Tok::Ge) => BinOp::Ge,
        _ => return Ok((left, p)),
    };
    let (r, p2) = parse_concat(t, p + 1)?;
    Ok((Expr::Bin(op, Box::new(left), Box::new(r)), p2))
}
fn parse_concat(t: &[Tok], p: usize) -> ParseResult {
    let (mut left, mut p) = parse_add(t, p)?;
    while matches!(t.get(p), Some(Tok::Concat)) {
        let (r, p2) = parse_add(t, p + 1)?;
        p = p2;
        left = Expr::Concat(Box::new(left), Box::new(r));
    }
    Ok((left, p))
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
        Some(Tok::Hash) => {
            // `#ident` — sequence length.
            match t.get(p + 1) {
                Some(Tok::Ident(n)) => Ok((Expr::SeqLen(n.clone()), p + 2)),
                other => fe(format!("`#` must be followed by a sequence var: {other:?}")),
            }
        }
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
fn emit_expr_renamed(e: &Expr, rename: &HashMap<String, String>) -> Result<String, FrontendError> {
    match e {
        Expr::Int(i) => Ok(smt_int(*i)),
        Expr::Bool(b) => Ok(if *b { "true".into() } else { "false".into() }),
        Expr::Str(s) => Ok(smt_str(s)),
        Expr::Ident(n) => Ok(rename.get(n).cloned().unwrap_or_else(|| n.clone())),
        Expr::SeqLen(n) => {
            let name = rename.get(n).cloned().unwrap_or_else(|| n.clone());
            Ok(format!("(seq.len {name})"))
        }
        Expr::Not(i) => Ok(format!("(not {})", emit_expr_renamed(i, rename)?)),
        Expr::Neg(i) => match i.as_ref() {
            Expr::Int(n) => Ok(smt_int(-n)),
            other => Ok(format!("(- {})", emit_expr_renamed(other, rename)?)),
        },
        Expr::Concat(a, b) => Ok(format!(
            "(str.++ {} {})",
            emit_expr_renamed(a, rename)?,
            emit_expr_renamed(b, rename)?
        )),
        Expr::Ite(c, then_e, else_e) => Ok(format!(
            "(ite {} {} {})",
            emit_expr_renamed(c, rename)?,
            emit_expr_renamed(then_e, rename)?,
            emit_expr_renamed(else_e, rename)?
        )),
        Expr::Bin(BinOp::Neq, a, b) => Ok(format!(
            "(not (= {} {}))",
            emit_expr_renamed(a, rename)?,
            emit_expr_renamed(b, rename)?
        )),
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

    // The scalar var rename: the .ev's current `X` is the engine prev `_X`.
    let mut rename: HashMap<String, String> = HashMap::new();
    if let Some(sc) = &prog.scalar_state {
        rename.insert(sc.name.clone(), sc.prev.clone());
    }

    // ---- @meta JSON ----
    out.push_str("; @meta\n");
    out.push_str("; {\n");
    out.push_str(";   \"fsms\": [\n");
    out.push_str(";     { \"name\": \"");
    out.push_str(&prog.fsm_name);
    out.push_str("\",\n");

    // state array — enum state first (if any), then scalar (if any).
    let mut state_entries: Vec<String> = Vec::new();
    if let Some(es) = &prog.enum_state {
        state_entries.push(format!(
            "{{\"prev\":\"{}\",\"next\":\"{}\",\"sort\":\"{}\",\"init\":\"{}\"}}",
            es.prev, es.next, es.enum_name, es.init
        ));
    }
    if let Some(sc) = &prog.scalar_state {
        state_entries.push(format!(
            "{{\"prev\":\"{}\",\"next\":\"{}\",\"sort\":\"Int\",\"init\":{}}}",
            sc.prev, sc.name, sc.init
        ));
    }
    let _ = write!(out, ";       \"state\": [{}]", state_entries.join(", "));

    if prog.has_effects {
        out.push_str(",\n;       \"effects\": {\"var\":\"effects\"}");
    }
    if prog.reads_last_results {
        out.push_str(",\n;       \"last_results\": {\"var\":\"last_results\",\"elem_sort\":\"Result\"}");
    }
    out.push('\n');
    out.push_str(";     }\n");
    out.push_str(";   ]\n");
    out.push_str("; }\n");
    out.push_str("; @end\n");

    // ---- transition block ----
    let _ = writeln!(out, "; @transition {}", prog.fsm_name);

    // Datatypes: fixed Effect + (for enum state) the state enum, batched.
    emit_datatypes(&mut out, prog);
    if prog.reads_last_results {
        out.push_str(
            "(declare-datatypes ((Result 0))\n  \
             (((NoResult) (IntResult (IntResult_0 Int)) \
             (StringResult (StringResult_0 String)) \
             (ErrorResult (ErrorResult_0 String)))))\n",
        );
    }

    // Const declarations.
    if let Some(es) = &prog.enum_state {
        let _ = writeln!(out, "(declare-const {} {})", es.prev, es.enum_name);
        let _ = writeln!(out, "(declare-const {} {})", es.next, es.enum_name);
    }
    if let Some(sc) = &prog.scalar_state {
        let _ = writeln!(out, "(declare-const {} Int)", sc.prev);
        let _ = writeln!(out, "(declare-const {} Int)", sc.name);
    }
    if prog.reads_last_results {
        out.push_str("(declare-const last_results (Seq Result))\n");
    }
    if prog.has_effects {
        out.push_str("(declare-const effects (Seq Effect))\n");
    }

    // Intermediate var declarations (forward declarations, all up front so a
    // later binding can reference an earlier one in any order).
    for b in &prog.bindings {
        if let Binding::IntermediateVar { name, smt_sort, .. } = b {
            let _ = writeln!(out, "(declare-const {name} {smt_sort})");
        }
    }

    // ---- asserts ----
    // Scalar transition (state).
    if let Some(sc) = &prog.scalar_state {
        let _ = writeln!(out, "(assert (= {} {}))", sc.name, sc.transition_rhs);
    }

    // The enum match scrutinee (.ev's current `state` = engine prev).
    let enum_scrut = prog.enum_state.as_ref().map(|es| es.prev.clone());

    for b in &prog.bindings {
        match b {
            Binding::IntermediateVar { name, value, .. } => {
                let rhs = match value {
                    BindingValue::Expr(e) => emit_expr_renamed(e, &rename)?,
                    BindingValue::MatchLastResults { index, lr_var, arms, default } => {
                        emit_last_results_match(*index, lr_var, arms, default, &rename)?
                    }
                };
                let _ = writeln!(out, "(assert (= {name} {rhs}))");
            }
            Binding::StateNextMatch { arms, default } => {
                let es = prog.enum_state.as_ref().ok_or_else(|| {
                    FrontendError("`state_next = match` used without an enum state".into())
                })?;
                let scrut = enum_scrut.as_ref().unwrap();
                let ite = emit_enum_match(prog, scrut, arms, default)?;
                let _ = writeln!(out, "(assert (= {} {ite}))", es.next);
            }
            Binding::StateNextTernary { cond_expr, then_variant, else_variant } => {
                let es = prog.enum_state.as_ref().ok_or_else(|| {
                    FrontendError("`state_next = (cond ? A : B)` used without an enum state".into())
                })?;
                let cond = emit_expr_renamed(cond_expr, &rename)?;
                let _ = writeln!(
                    out,
                    "(assert (= {} (ite {cond} {then_variant} {else_variant})))",
                    es.next
                );
            }
            Binding::EffectsMatch { arms, default } => {
                let scrut = enum_scrut.as_ref().ok_or_else(|| {
                    FrontendError("`effects = match state` used without an enum state".into())
                })?;
                // Lower each arm body (raw seq text) at emit time so rename applies.
                let lowered_arms: Result<Vec<(String, String)>, FrontendError> = arms
                    .iter()
                    .map(|(p, body)| Ok((p.clone(), emit_effects_rhs(body, &rename)?)))
                    .collect();
                let lowered_arms = lowered_arms?;
                let lowered_default = match default {
                    Some(d) => Some(emit_effects_rhs(d, &rename)?),
                    None => None,
                };
                let ite = emit_enum_match(prog, scrut, &lowered_arms, &lowered_default)?;
                let _ = writeln!(out, "(assert (= effects {ite}))");
            }
            Binding::EffectsExpr { raw } => {
                let term = emit_effects_rhs(raw, &rename)?;
                let _ = writeln!(out, "(assert (= effects {term}))");
            }
        }
    }

    Ok(out)
}

fn emit_datatypes(out: &mut String, prog: &Program) {
    let effect_decl = "(Effect 0)";
    let effect_body =
        "((Println (Println_0 String)) (Exit (Exit_0 Int)) (IntToStr (IntToStr_0 Int)) (ParseInt (ParseInt_0 String)))";
    match &prog.enum_state {
        Some(es) => {
            let variants = prog.enums.get(&es.enum_name).cloned().unwrap_or_default();
            let body: String = variants.iter().map(|v| format!("({v})")).collect::<Vec<_>>().join(" ");
            let _ = writeln!(
                out,
                "(declare-datatypes ({effect_decl} ({} 0)) ({effect_body} ({body})))",
                es.enum_name
            );
        }
        None => {
            let _ = writeln!(out, "(declare-datatypes ({effect_decl}) ({effect_body}))");
        }
    }
}

/// Build a nested `(ite (is-Variant scrut) body rest)` over an enum scrutinee.
fn emit_enum_match(
    prog: &Program,
    scrut: &str,
    arms: &[(String, String)],
    default: &Option<String>,
) -> Result<String, FrontendError> {
    let enum_variants: Option<&Vec<String>> = prog
        .enum_state
        .as_ref()
        .and_then(|es| prog.enums.get(&es.enum_name));

    let mut elems = arms.to_vec();
    let base_else: String = if let Some(d) = default {
        d.clone()
    } else {
        let last = elems
            .pop()
            .ok_or_else(|| FrontendError("match has no arms".into()))?;
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

/// Build a bounds-guarded nested ite for `match last_results[i]`.
///
/// Each non-default arm becomes
/// `(ite (and (> (seq.len LR) i) ((_ is Ctor) (seq.nth LR i))) BODY rest)`,
/// with the payload binding (if any) substituted by the field accessor
/// `(Ctor_0 (seq.nth LR i))`. The `_` default is the innermost else, so an
/// out-of-bounds / unmatched element falls through to it (matching the oracle).
fn emit_last_results_match(
    index: usize,
    lr_var: &str,
    arms: &[MatchArm],
    default: &Expr,
    rename: &HashMap<String, String>,
) -> Result<String, FrontendError> {
    // The element accessor expression, e.g. `(seq.nth last_results 0)`.
    let nth = format!("(seq.nth {lr_var} {index})");
    let mut acc = emit_expr_renamed(default, rename)?;
    for arm in arms.iter().rev() {
        // Substitute the bound payload (if any) with the field accessor.
        let mut arm_rename = rename.clone();
        if let Some(bind) = &arm.bind {
            arm_rename.insert(bind.clone(), format!("({}_0 {nth})", arm.ctor));
        }
        let body = emit_expr_renamed(&arm.body, &arm_rename)?;
        let guard = format!(
            "(and (> (seq.len {lr_var}) {index}) ((_ is {}) {nth}))",
            arm.ctor
        );
        acc = format!("(ite {guard} {body} {acc})");
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
        assert!(fix.contains("\"prev\":\"_count\",\"next\":\"count\",\"sort\":\"Int\",\"init\":3"), "meta:\n{fix}");
        assert!(fix.contains("(assert (= count (- _count 1)))"), "transition:\n{fix}");
        assert!(fix.contains("(ite (> _count 0)"), "effect branches on prev:\n{fix}");
        assert!(fix.contains("(Println \"tick\")"), "{fix}");
        assert!(fix.contains("(Println \"done\")") && fix.contains("(Exit 0)"), "{fix}");
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

    // ---- new-lowering tests -------------------------------------------------

    const TEST_05: &str = "import \"stdlib/runtime.ev\"\n\nenum FmtState = Issue | Show | Halt\n\nfsm fmt_demo(state ∈ FmtState)\n    state_next = match state\n        Issue ⇒ Show\n        Show ⇒ Halt\n        Halt  ⇒ Halt\n\n    formatted ∈ String\n    formatted = match last_results[0]\n        StringResult(s) ⇒ s\n        _               ⇒ \"<no string>\"\n\n    effects = match state\n        Issue ⇒ ⟨IntToStr(42)⟩\n        Show ⇒ ⟨Println(formatted), Exit(0)⟩\n        Halt  ⇒ ⟨⟩\n";

    #[test]
    fn test_05_int_to_str_lowering() {
        let fix = transpile_fsm(TEST_05).expect("transpile test_05");
        // last_results metadata + Result datatype + decl.
        assert!(fix.contains("\"last_results\": {\"var\":\"last_results\",\"elem_sort\":\"Result\"}"), "meta:\n{fix}");
        assert!(fix.contains("(declare-datatypes ((Result 0))"), "Result dt:\n{fix}");
        assert!(fix.contains("(declare-const last_results (Seq Result))"), "lr decl:\n{fix}");
        // IntToStr effect ctor + datatype.
        assert!(fix.contains("(IntToStr (IntToStr_0 Int))"), "Effect dt has IntToStr:\n{fix}");
        assert!(fix.contains("(IntToStr 42)"), "IntToStr ctor:\n{fix}");
        // formatted intermediate var: bounds-guarded recognizer + accessor.
        assert!(fix.contains("(declare-const formatted String)"), "formatted decl:\n{fix}");
        assert!(
            fix.contains("((_ is StringResult) (seq.nth last_results 0))"),
            "recognizer:\n{fix}"
        );
        assert!(
            fix.contains("(StringResult_0 (seq.nth last_results 0))"),
            "accessor:\n{fix}"
        );
        assert!(fix.contains("(> (seq.len last_results) 0)"), "bounds guard:\n{fix}");
        // Println(formatted) — a bare var arg.
        assert!(fix.contains("(Println formatted)"), "println var arg:\n{fix}");
        let prob = load_str(&fix).expect("engine loads test_05 fixture");
        assert_eq!(prob.fsms[0].name, "fmt_demo");
        assert!(prob.fsms[0].last_results.is_some());
    }

    const TEST_04: &str = "import \"stdlib/runtime.ev\"\n\nenum PState = Issue | Read | Done\n\nfsm parse_demo(state ∈ PState)\n    state_next = match state\n        Issue ⇒ Read\n        Read  ⇒ Done\n        Done  ⇒ Done\n\n    good ∈ String\n    good = match last_results[0]\n        IntResult(n)    ⇒ \"good: parsed an Int\"\n        ErrorResult(_)  ⇒ \"good: ERROR was expected to be success\"\n        _               ⇒ \"good: unknown result\"\n\n    bad ∈ String\n    bad = match last_results[1]\n        IntResult(_)    ⇒ \"bad: parsed but expected error\"\n        ErrorResult(_)  ⇒ \"bad: ERROR was correct\"\n        _               ⇒ \"bad: unknown\"\n\n    effects = match state\n        Issue ⇒ ⟨ParseInt(\"42\"), ParseInt(\"not-a-number\")⟩\n        Read  ⇒ ⟨Println(good), Println(bad), Exit(0)⟩\n        Done  ⇒ ⟨⟩\n";

    #[test]
    fn test_04_parse_int_lowering() {
        let fix = transpile_fsm(TEST_04).expect("transpile test_04");
        assert!(fix.contains("(ParseInt (ParseInt_0 String))"), "Effect dt:\n{fix}");
        assert!(fix.contains("(ParseInt \"42\")"), "ParseInt 42:\n{fix}");
        assert!(fix.contains("(ParseInt \"not-a-number\")"), "ParseInt err:\n{fix}");
        // two intermediate string vars indexing last_results[0] and [1].
        assert!(fix.contains("(declare-const good String)"), "good decl:\n{fix}");
        assert!(fix.contains("(declare-const bad String)"), "bad decl:\n{fix}");
        assert!(fix.contains("(seq.nth last_results 0)"), "index 0:\n{fix}");
        assert!(fix.contains("(seq.nth last_results 1)"), "index 1:\n{fix}");
        // payload-ignoring arm (ErrorResult(_)) still emits the recognizer.
        assert!(fix.contains("((_ is ErrorResult) (seq.nth last_results 1))"), "err recognizer:\n{fix}");
        let prob = load_str(&fix).expect("engine loads test_04 fixture");
        assert_eq!(prob.fsms[0].name, "parse_demo");
    }

    const TEST_19: &str = "import \"stdlib/runtime.ev\"\n\nenum CounterState = Counting | Done\n\nfsm counter(state ∈ CounterState)\n    count ∈ Int\n    count = (is_first_tick ? 0 : _count + 1)\n\n    state_next = (count ≥ 3 ? Done : Counting)\n\n    has_result ∈ Bool\n    has_result = (#last_results > 0)\n\n    first_str ∈ String\n    first_str = match last_results[1]\n        StringResult(s) ⇒ s\n        _               ⇒ \"?\"\n\n    prev_str ∈ String\n    prev_str = (has_result ? first_str : \"?\")\n\n    effects = match state\n        Counting ⇒ ⟨Println(\"count = \" ++ prev_str), IntToStr(count)⟩\n        Done     ⇒ ⟨Println(\"done\"), Exit(0)⟩\n";

    #[test]
    fn test_19_prev_tick_lowering() {
        let fix = transpile_fsm(TEST_19).expect("transpile test_19");
        // BOTH state vars in the meta state array.
        assert!(fix.contains("\"prev\":\"state\",\"next\":\"state_next\",\"sort\":\"CounterState\",\"init\":\"Counting\""), "enum state:\n{fix}");
        assert!(fix.contains("\"prev\":\"_count\",\"next\":\"count\",\"sort\":\"Int\",\"init\":0"), "scalar state:\n{fix}");
        // Two-line scalar transition.
        assert!(fix.contains("(assert (= count (+ _count 1)))"), "scalar transition:\n{fix}");
        // state_next ternary renames count → _count.
        assert!(fix.contains("(assert (= state_next (ite (>= _count 3) Done Counting)))"), "state_next ternary:\n{fix}");
        // #last_results → seq.len.
        assert!(fix.contains("(> (seq.len last_results) 0)"), "seq.len:\n{fix}");
        // prev_str references has_result and first_str (intermediate vars).
        assert!(fix.contains("(assert (= prev_str (ite has_result first_str \"?\")))"), "prev_str ite:\n{fix}");
        // Effects: string concat + IntToStr(count→_count).
        assert!(fix.contains("(str.++ \"count = \" prev_str)"), "concat:\n{fix}");
        assert!(fix.contains("(IntToStr _count)"), "IntToStr renamed:\n{fix}");
        let prob = load_str(&fix).expect("engine loads test_19 fixture");
        assert_eq!(prob.fsms[0].state.len(), 2, "two state vars");
    }

    const TEST_20: &str = "import \"stdlib/runtime.ev\"\n\nfsm counter\n    count ∈ Int\n    count = (is_first_tick ? 0 : _count + 1)\n\n    fmt_str ∈ String\n    fmt_str = match last_results[0]\n        StringResult(s) ⇒ s\n        _               ⇒ \"?\"\n\n    effects = (count = 0   ? ⟨IntToStr(count), Println(\"starting\")⟩\n            : (count ≤ 3   ? ⟨IntToStr(count), Println(\"count = \" ++ fmt_str)⟩\n                           : ⟨Println(\"count = \" ++ fmt_str), Exit(0)⟩))\n";

    #[test]
    fn test_20_pure_counter_lowering() {
        let fix = transpile_fsm(TEST_20).expect("transpile test_20");
        // Pure scalar — no enum state in the meta state array.
        assert!(fix.contains("\"prev\":\"_count\",\"next\":\"count\",\"sort\":\"Int\",\"init\":0"), "scalar state:\n{fix}");
        assert!(!fix.contains("CounterState"), "no enum state:\n{fix}");
        // Nested effects ternary, count→_count renamed in conditions and args.
        assert!(fix.contains("(ite (= _count 0)"), "outer ternary:\n{fix}");
        assert!(fix.contains("(ite (<= _count 3)"), "inner ternary:\n{fix}");
        assert!(fix.contains("(IntToStr _count)"), "IntToStr renamed:\n{fix}");
        assert!(fix.contains("(Println \"starting\")"), "starting:\n{fix}");
        assert!(fix.contains("(str.++ \"count = \" fmt_str)"), "concat:\n{fix}");
        let prob = load_str(&fix).expect("engine loads test_20 fixture");
        assert_eq!(prob.fsms[0].state.len(), 1, "one scalar state var");
        assert!(prob.fsms[0].last_results.is_some());
    }
}
