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
//! A program with one OR MORE `fsm`s, optionally sharing a `type World`.
//! Recognized lines:
//!
//! * `import "..."` — ignored.
//! * `type World` with `field ∈ Int` lines → the shared `world` array in
//!   `@meta` (one `{"name":…,"sort":"Int","init":0}` per field). World fields
//!   are seeded `init:0` (the writer is scheduled first and writes tick 0, so
//!   this matches the oracle).
//! * `fsm NAME(world ∈ World, state ∈ EnumType)` — world-reading FSM. A param
//!   `world ∈ World` (read-only) or `world, world_next ∈ World` (read+write)
//!   declares the FSM touches the shared world. `world_next.X = expr` in the
//!   body is a `world_writes` of `X`; `world.X` referenced anywhere is a
//!   `world_reads` of `X`, lowered to the bare const name `X`.
//! * `enum NAME = A | B | C` — enums with nullary and/or payload-carrying
//!   variants (`Count(Int)`, `Format(Int)`). The FSM's state-enum's FIRST
//!   variant is its tick-0 init and MUST be nullary (the engine seeds it as a
//!   bare constructor). Payload arg sorts are scalar (Int / Bool / String).
//! * `fsm NAME(state ∈ EnumType)` — enum state. Engine `prev = "state"`,
//!   `next = "state_next"`, `init = <first variant>`.
//! * Scalar Int state, either single-line
//!   `X ∈ Int = (is_first_tick ? INIT : EXPR(_X,…))` or two lines
//!   `X ∈ Int` then `X = (is_first_tick ? INIT : EXPR(_X,…))`. Engine
//!   `prev = "_X"`, `next = "X"`, `init = INIT`, transition `(= X EXPR)`.
//!   An fsm may carry BOTH an enum `state` and a scalar (e.g. test_19).
//! * `state_next = match state` arms → nested ite over `(is-Variant state)`.
//!   Arm patterns may bind a payload (`Count(n)`) — the binding resolves to the
//!   field accessor `(Count_0 state)` inside the arm body. Arm bodies may be a
//!   bare variant (`Done`), an applied constructor (`Count(5)`, `Format(n)`,
//!   `Count(n - 1)`), or a ternary over those.
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

/// One enum variant: its constructor name plus the SMT-LIB sorts of its
/// payload args (empty for a nullary variant). Field accessors are named
/// `<ctor>_<argindex>` (0-based), matching the fixture convention.
#[derive(Debug, Clone)]
struct Variant {
    name: String,
    /// SMT-LIB sort name per payload arg, in declaration order.
    arg_sorts: Vec<String>,
}

/// Enum-typed state from `fsm F(state ∈ EnumType)`.
struct EnumState {
    /// prev = the param name (the .ev's current `state`).
    prev: String,
    /// next = `<param>_next`.
    next: String,
    enum_name: String,
    /// First variant of the enum — tick-0 init (must be nullary).
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
    /// `state_next = match state` — arms over the state enum. Each arm carries a
    /// `ctor` + optional payload binding + a RAW body (a bare/applied variant or
    /// ternary), lowered at emit time so payload bindings + variant sorts resolve.
    StateNextMatch { arms: Vec<EnumArm>, default: Option<String> },
    /// `state_next = (cond ? VariantA : VariantB)`.
    StateNextTernary { cond_expr: Expr, then_variant: String, else_variant: String },
    /// `effects = match state` — arms whose bodies are seq literals (raw text,
    /// lowered at emit time so rename + payload bindings apply).
    EffectsMatch { arms: Vec<EnumArm>, default: Option<String> },
    /// `effects = <seq-expression>` (a ⟨…⟩ literal or a possibly-nested
    /// `(cond ? <seq> : <seq>)` ternary). Stored as raw text, lowered at emit.
    EffectsExpr { raw: String },
    /// `world_next.FIELD = expr` (N3): a write of the shared world field. Emitted
    /// as `(declare-const FIELD <sort>)` + `(assert (= FIELD <expr>))`, with the
    /// bare `FIELD` const as the write target the engine decodes.
    WorldWrite { field: String, value: Expr },
    /// A bare two-var (in)equality constraint between body vars: `a ≠ b` →
    /// `(assert (not (= a b)))`, `a = b` → `(assert (= a b))`. These constrain
    /// fresh per-tick witness vars (e.g. the graph-coloring enum vars) so the
    /// transition is SAT exactly as the oracle solves it. They don't define a
    /// var (no decl) — both sides reference vars declared elsewhere.
    Constraint { lhs: Expr, op: BinOp, rhs: Expr },
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
    /// `X = match state` — an intermediate var defined by a match over the FSM's
    /// enum state. Each arm's body is a scalar expression (Int/String/Bool); the
    /// payload binding (`PTick(k)`) resolves to the field accessor at emit time.
    /// (e.g. the producer's `next_n = match state` selecting an Int per state.)
    MatchEnumState { arms: Vec<EnumArm>, default: Option<String> },
}

/// One arm of a `match state` (state_next / effects). The pattern is a state
/// enum variant, optionally binding its payload; the body is kept as raw text
/// and lowered at emit time so the payload binding resolves to the field
/// accessor `(Ctor_0 state)`.
#[derive(Clone)]
struct EnumArm {
    /// The state enum variant matched (e.g. `Count`).
    ctor: String,
    /// The payload binding name, if any. `Count(n)` → `Some("n")`; `Format(_)`
    /// and bare `Start` → `None`.
    bind: Option<String>,
    /// Raw arm body text — an enum constructor / ternary (state_next) or a
    /// `⟨…⟩` seq literal / ternary (effects).
    body: String,
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

/// One shared-world field from `type World` (e.g. `n ∈ Int`). The hybrid only
/// supports Int world fields for now; each is seeded `init:0`.
struct WorldField {
    name: String,
    /// SMT-LIB sort name (currently always `"Int"`).
    smt_sort: String,
}

/// One parsed `fsm` declaration — its own state, body bindings, and world I/O.
struct FsmDef {
    fsm_name: String,
    enum_state: Option<EnumState>,
    scalar_state: Option<ScalarState>,
    bindings: Vec<Binding>,
    /// Whether this FSM references an `effects` var at all.
    has_effects: bool,
    /// Whether this FSM reads `last_results` / `#last_results` anywhere.
    reads_last_results: bool,
    /// World fields this FSM writes (`world_next.X = …` in its body). Ordered,
    /// deduplicated.
    world_writes: Vec<String>,
    /// World fields this FSM reads (`world.X` anywhere in its body). Ordered,
    /// deduplicated.
    world_reads: Vec<String>,
    /// Fresh per-tick witness vars (multi-name decls like `a, b ∈ Color`):
    /// (name, smt_sort). Declared as bare consts in the transition block.
    fresh_vars: Vec<(String, String)>,
}

struct Program {
    /// enum name → ordered variant list (each with payload arg sorts). Shared
    /// across all FSMs (enum variant names are globally unique in Evident).
    enums: HashMap<String, Vec<Variant>>,
    /// The shared world fields (from `type World`), if any.
    world: Vec<WorldField>,
    /// One or more FSMs, in declaration order.
    fsms: Vec<FsmDef>,
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

/// The mutable per-FSM accumulators a body region builds up while it is the
/// "current" FSM. Flushed into a [`FsmDef`] when the next `fsm` (or end of
/// program) is reached.
struct FsmBuilder {
    header: FsmHeader,
    scalar_state: Option<ScalarState>,
    bindings: Vec<Binding>,
    has_effects: bool,
    reads_last_results: bool,
    /// Ordered, deduplicated world fields written (`world_next.X = …`).
    world_writes: Vec<String>,
    /// Ordered, deduplicated world fields read (`world.X`).
    world_reads: Vec<String>,
    // Two-line scalar declaration in flight: a bare `X ∈ Int` awaiting `X = …`.
    pending_scalar_decl: Option<String>,
    // Intermediate var declarations in flight: name → smt_sort, awaiting `name = …`.
    pending_intermediate: HashMap<String, String>,
    /// Fresh per-tick witness vars from multi-name decls (`a, b, c ∈ Color`):
    /// (name, smt_sort). NOT threaded state — declared as bare consts, the
    /// solver finds values each tick subject to the body's constraints.
    fresh_vars: Vec<(String, String)>,
}

impl FsmBuilder {
    fn new(header: FsmHeader) -> FsmBuilder {
        FsmBuilder {
            header,
            scalar_state: None,
            bindings: Vec::new(),
            has_effects: false,
            reads_last_results: false,
            world_writes: Vec::new(),
            world_reads: Vec::new(),
            pending_scalar_decl: None,
            pending_intermediate: HashMap::new(),
            fresh_vars: Vec::new(),
        }
    }

    fn note_read(&mut self, field: &str) {
        if !self.world_reads.iter().any(|f| f == field) {
            self.world_reads.push(field.to_string());
        }
    }
    fn note_write(&mut self, field: &str) {
        if !self.world_writes.iter().any(|f| f == field) {
            self.world_writes.push(field.to_string());
        }
    }
}

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

    let mut enums: HashMap<String, Vec<Variant>> = HashMap::new();
    let mut world: Vec<WorldField> = Vec::new();
    let mut fsms: Vec<FsmDef> = Vec::new();
    let mut cur: Option<FsmBuilder> = None;

    let mut i = 0;
    while i < raw.len() {
        let (indent, line) = (raw[i].0, raw[i].1.clone());
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
        // `type World` (at indent 0) opens the shared-world declaration: parse
        // the indented `field ∈ Int` lines. Any OTHER top-level `type` ends the
        // FSM-body region (it's a static-test struct we ignore).
        if indent == 0 && (trimmed == "type World" || trimmed.starts_with("type World ")) {
            if cur.is_some() {
                return fe("`type World` must precede the `fsm` declarations");
            }
            let consumed = parse_world_fields(&raw, i + 1, indent, &mut world)?;
            i += 1 + consumed;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("fsm ") {
            // Flush the previous FSM (if any) and start a fresh builder.
            if let Some(b) = cur.take() {
                fsms.push(finish_fsm(b, &enums)?);
            }
            // The header's param list may span continuation lines (open `(` not
            // closed on the `fsm` line). Gather until parens balance.
            let (header_text, consumed) = gather_fsm_header(&raw, i, rest);
            cur = Some(FsmBuilder::new(parse_fsm_header(&header_text)?));
            i += 1 + consumed;
            continue;
        }
        // A top-level `claim`/`type`/`schema` ends the fsm body region — the rest
        // are static-test blocks we ignore.
        if indent == 0
            && (trimmed.starts_with("claim ")
                || trimmed.starts_with("type ")
                || trimmed.starts_with("schema ")
                || trimmed.starts_with("subclaim ")
                || trimmed == "claim")
        {
            break;
        }

        let b = cur.as_mut().ok_or_else(|| {
            FrontendError(format!("unexpected top-level line before any `fsm`: {trimmed:?}"))
        })?;

        // ---- body line classification ----
        // World aliases for this FSM: `world.X` → bare `X` (a read); the
        // `world_next.X = …` write target is recognized below before any other
        // classification.
        let read_alias = b.header.world_read.clone();
        let write_alias = b.header.world_write.clone();

        // `world_next.X = expr` — a world write. Recognize before generic assign.
        if let Some(wa) = &write_alias {
            if let Some((field, rhs)) = parse_world_write(trimmed, wa) {
                b.note_write(&field);
                // The RHS may reference `world.X` reads; rewrite those to bare names.
                let rhs = rewrite_world_reads(&rhs, read_alias.as_deref(), b);
                if rhs.contains("last_results") {
                    b.reads_last_results = true;
                }
                let toks = tokenize(&rhs)?;
                let (e, used) = parse_expr(&toks, 0)?;
                if used != toks.len() {
                    return fe(format!("trailing tokens in world write `{field}`: {rhs:?}"));
                }
                b.bindings.push(Binding::WorldWrite { field, value: e });
                i += 1;
                continue;
            }
        }

        // Single-line scalar state: `X ∈ Int = (is_first_tick ? INIT : EXPR)`.
        if let Some(sc) = try_parse_scalar_state(trimmed)? {
            if b.scalar_state.is_some() {
                return fe("multiple scalar state declarations not supported");
            }
            b.scalar_state = Some(sc);
            i += 1;
            continue;
        }

        // Chained-membership intermediate var: `X ∈ T = expr` whose RHS is NOT
        // the `is_first_tick` scalar-state idiom (e.g. `seed_a ∈ Int = tick * 7
        // + 3`, `a01 ∈ Int = (seed_a > 50 ? … : …)`). A per-tick derived value,
        // emitted as a declared const + defining assert with the scalar rename.
        if let Some((name, smt_sort, rhs)) = try_parse_chained_intermediate(trimmed)? {
            let rhs = rewrite_world_reads(&rhs, read_alias.as_deref(), b);
            if rhs.contains("last_results") {
                b.reads_last_results = true;
            }
            let toks = tokenize(&rhs)?;
            let (e, used) = parse_expr(&toks, 0)?;
            if used != toks.len() {
                return fe(format!("trailing tokens in `{name}` definition: {rhs:?}"));
            }
            b.bindings.push(Binding::IntermediateVar {
                name,
                smt_sort,
                value: BindingValue::Expr(e),
            });
            i += 1;
            continue;
        }

        // Multi-name declaration: `a, b, c ∈ T` (no `=`). Each name becomes a
        // fresh per-tick witness const of sort `T` (Color / Int / Bool). These
        // are NOT threaded state — the solver picks values each tick subject to
        // the body's constraints (`a ≠ b`, …).
        if let Some((names, sort)) = try_parse_multiname_decl(trimmed)? {
            let smt_sort = enum_or_scalar_sort(&sort, &enums)?;
            for n in names {
                if b.fresh_vars.iter().any(|(fv, _)| fv == &n) {
                    return fe(format!("duplicate fresh var declaration `{n}`"));
                }
                b.fresh_vars.push((n, smt_sort.clone()));
            }
            i += 1;
            continue;
        }

        // A bare membership decl: `X ∈ T` (no `=` on this line). Could be a
        // two-line scalar state decl OR an intermediate var decl. Stash it.
        if let Some((name, sort)) = try_parse_bare_decl(trimmed)? {
            if sort == "Int" || sort == "Nat" || sort == "Pos" {
                // Could be scalar state (next line `X = (is_first_tick ? …)`) or
                // an intermediate Int. Decide when we see the `=` line.
                b.pending_scalar_decl = Some(name.clone());
                b.pending_intermediate.insert(name, smt_sort_of(&sort)?);
            } else {
                b.pending_intermediate.insert(name, smt_sort_of(&sort)?);
            }
            i += 1;
            continue;
        }

        // `state_next = match state`  (multi-line block of indented arms).
        if match_head(trimmed, "state_next").is_some() {
            let (arms, default, consumed) = parse_enum_state_match_arms_w(
                &raw, i + 1, indent, read_alias.as_deref(), b,
            )?;
            b.bindings.push(Binding::StateNextMatch { arms, default });
            i += 1 + consumed;
            continue;
        }

        // `state_next = (cond ? VariantA : VariantB)`.
        if let Some(rhs) = assign_rhs(trimmed, "state_next") {
            let rhs = rewrite_world_reads(&rhs, read_alias.as_deref(), b);
            let bind = parse_state_next_ternary(&rhs)?;
            b.bindings.push(bind);
            i += 1;
            continue;
        }

        // `effects = match state`.
        if match_head(trimmed, "effects").is_some() {
            b.has_effects = true;
            let (arms, default, consumed) = parse_enum_state_match_arms_w(
                &raw, i + 1, indent, read_alias.as_deref(), b,
            )?;
            if arms.iter().any(|a| a.body.contains("last_results"))
                || default.as_deref().map(|d| d.contains("last_results")).unwrap_or(false)
            {
                b.reads_last_results = true;
            }
            b.bindings.push(Binding::EffectsMatch { arms, default });
            i += 1 + consumed;
            continue;
        }

        // `effects = <seq-expr>` (may be a ⟨…⟩ literal or a possibly multi-line
        // nested ternary). Collect continuation lines (more-indented than head).
        if let Some(first) = assign_rhs(trimmed, "effects") {
            b.has_effects = true;
            let (raw_rhs, consumed) = gather_continuation(&raw, i, indent, &first);
            let raw_rhs = rewrite_world_reads(&raw_rhs, read_alias.as_deref(), b);
            if raw_rhs.contains("last_results") {
                b.reads_last_results = true;
            }
            b.bindings.push(Binding::EffectsExpr { raw: raw_rhs });
            i += 1 + consumed;
            continue;
        }

        // A bare two-var (in)equality constraint: `a ≠ b` / `a = b` (no decl,
        // no assignment). A `≠` line is unambiguously a constraint. An `=` line
        // is a constraint only when its LHS is NOT a pending intermediate /
        // scalar declaration (otherwise `split_assign` below claims it as an
        // assignment). Both sides are renamed (scalar var → engine prev).
        if let Some((lhs, op, rhs)) = try_parse_binary_constraint(trimmed)? {
            let is_assignment = matches!(&lhs, Expr::Ident(n)
                if b.pending_intermediate.contains_key(n)
                    || b.pending_scalar_decl.as_deref() == Some(n.as_str()));
            if op == BinOp::Neq || !is_assignment {
                b.bindings.push(Binding::Constraint { lhs, op, rhs });
                i += 1;
                continue;
            }
        }

        // An intermediate var definition: `X = match last_results[i]` (multi-line)
        // or `X = <expr>` (single line). Also the two-line scalar state assign.
        if let Some((name, rhs)) = split_assign(trimmed) {
            // Two-line scalar state: `X = (is_first_tick ? INIT : EXPR)`.
            if b.pending_scalar_decl.as_deref() == Some(name.as_str())
                && rhs.contains("is_first_tick")
            {
                if b.scalar_state.is_some() {
                    return fe("multiple scalar state declarations not supported");
                }
                let sc = parse_scalar_state_rhs(&name, &rhs)?;
                b.scalar_state = Some(sc);
                b.pending_scalar_decl = None;
                b.pending_intermediate.remove(&name);
                i += 1;
                continue;
            }

            // An intermediate var (must have been declared on a prior `X ∈ T` line).
            let smt_sort = b.pending_intermediate.remove(&name).ok_or_else(|| {
                FrontendError(format!(
                    "assignment to `{name}` without a preceding `{name} ∈ <Type>` declaration"
                ))
            })?;
            b.pending_scalar_decl = None;

            // `X = match last_results[i]` — a multi-line indented match block.
            if let Some(inner) = rhs.strip_prefix("match ") {
                let scrut = inner.trim();
                if let Some((lr_var, index)) = parse_last_results_index(scrut) {
                    b.reads_last_results = true;
                    let (arms, default, consumed) =
                        parse_last_results_match_arms(&raw, i + 1, indent)?;
                    let default = default.ok_or_else(|| {
                        FrontendError(format!(
                            "`{name} = match last_results[{index}]` needs a `_` default arm"
                        ))
                    })?;
                    b.bindings.push(Binding::IntermediateVar {
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
                // `X = match state` — match over the FSM's enum state. Each arm's
                // body is a scalar expression of the var's sort.
                let enum_param = b.header.enum_state.as_ref().map(|(p, _)| p.clone());
                if enum_param.as_deref() == Some(scrut) {
                    let (arms, default, consumed) = parse_enum_state_match_arms_w(
                        &raw, i + 1, indent, read_alias.as_deref(), b,
                    )?;
                    b.bindings.push(Binding::IntermediateVar {
                        name,
                        smt_sort,
                        value: BindingValue::MatchEnumState { arms, default },
                    });
                    i += 1 + consumed;
                    continue;
                }
                return fe(format!("unsupported `match` scrutinee for `{name}`: {scrut:?}"));
            }

            // `X = <expr>` — a single-line expression binding.
            let rhs = rewrite_world_reads(&rhs, read_alias.as_deref(), b);
            if rhs.contains("last_results") {
                b.reads_last_results = true;
            }
            let toks = tokenize(&rhs)?;
            let (e, used) = parse_expr(&toks, 0)?;
            if used != toks.len() {
                return fe(format!("trailing tokens in `{name}` definition: {rhs:?}"));
            }
            b.bindings.push(Binding::IntermediateVar {
                name,
                smt_sort,
                value: BindingValue::Expr(e),
            });
            i += 1;
            continue;
        }

        return fe(format!("unsupported fsm body line: {trimmed:?}"));
    }

    // Flush the last FSM in flight.
    if let Some(b) = cur.take() {
        fsms.push(finish_fsm(b, &enums)?);
    }

    if fsms.is_empty() {
        return fe("no `fsm` declaration found");
    }

    Ok(Program { enums, world, fsms })
}

/// Finalize a [`FsmBuilder`] into a [`FsmDef`]: resolve its enum state from the
/// header param (validating the enum is declared + its first variant nullary).
fn finish_fsm(b: FsmBuilder, enums: &HashMap<String, Vec<Variant>>) -> Result<FsmDef, FrontendError> {
    let FsmBuilder {
        header,
        scalar_state,
        bindings,
        has_effects,
        reads_last_results,
        world_writes,
        world_reads,
        fresh_vars,
        ..
    } = b;

    let enum_state = match header.enum_state {
        Some((param_name, enum_type)) => {
            let variants = enums
                .get(&enum_type)
                .ok_or_else(|| FrontendError(format!("fsm state enum `{enum_type}` not declared")))?;
            let first = variants
                .first()
                .ok_or_else(|| FrontendError(format!("enum `{enum_type}` has no variants")))?;
            // The init variant is pinned to `prev` on tick 0 as a bare ctor, so
            // it must be nullary — payload variants can't be seeded from a name.
            if !first.arg_sorts.is_empty() {
                return fe(format!(
                    "fsm state enum `{enum_type}`'s first variant `{}` carries a payload; \
                     the tick-0 init variant must be nullary",
                    first.name
                ));
            }
            Some(EnumState {
                prev: param_name.clone(),
                next: format!("{param_name}_next"),
                enum_name: enum_type,
                init: first.name.clone(),
            })
        }
        None => None,
    };

    if enum_state.is_none() && scalar_state.is_none() {
        return fe(format!(
            "fsm `{}` has no recognizable state (enum param or scalar body)",
            header.name
        ));
    }

    Ok(FsmDef {
        fsm_name: header.name,
        enum_state,
        scalar_state,
        bindings,
        has_effects,
        reads_last_results,
        world_writes,
        world_reads,
        fresh_vars,
    })
}

/// Parse the indented `field ∈ Int` lines of a `type World` block into
/// [`WorldField`]s, appending to `world`. Returns the number of lines consumed
/// beyond the `type World` head. Only `Int`-sorted fields are supported today.
fn parse_world_fields(
    raw: &[(usize, String)],
    start: usize,
    head_indent: usize,
    world: &mut Vec<WorldField>,
) -> Result<usize, FrontendError> {
    let mut consumed = 0;
    let mut i = start;
    while i < raw.len() {
        let (indent, line) = (raw[i].0, raw[i].1.clone());
        if indent <= head_indent {
            break;
        }
        let trimmed = line.trim();
        let in_idx = trimmed
            .find('∈')
            .ok_or_else(|| FrontendError(format!("`type World` field must be `name ∈ Int`: {trimmed:?}")))?;
        let name = trimmed[..in_idx].trim().to_string();
        let sort = trimmed[in_idx + '∈'.len_utf8()..].trim();
        if name.is_empty() || name.contains(char::is_whitespace) {
            return fe(format!("malformed `type World` field: {trimmed:?}"));
        }
        let smt_sort = smt_sort_of(sort)?;
        if smt_sort != "Int" {
            return fe(format!(
                "`type World` field `{name}` has sort `{sort}`; only Int world fields are supported"
            ));
        }
        // Reserved async-source plugin fields (CLAUDE.md "plugin-as-writer"):
        // the legacy runtime auto-installs an event source (SigintSource,
        // FrameTimer, StdinSource) to write these. The hybrid engine has no
        // async event sources, so an FSM reading one would never see a non-init
        // value, and its run is not comparable to the oracle's (which blocks on
        // the source). Reject as an honest gap rather than silently diverging.
        if matches!(name.as_str(), "signal_received" | "tick_count" | "stdin_seq") {
            return fe(format!(
                "`type World` field `{name}` is a reserved async-source plugin field \
                 (no event source in the hybrid engine)"
            ));
        }
        if world.iter().any(|w| w.name == name) {
            return fe(format!("duplicate `type World` field `{name}`"));
        }
        world.push(WorldField { name, smt_sort });
        consumed += 1;
        i += 1;
    }
    if world.is_empty() {
        return fe("`type World` has no fields");
    }
    Ok(consumed)
}

/// Recognize `<write_alias>.FIELD = RHS` → `Some((FIELD, RHS))`. The alias is
/// the FSM's `world_next` param name. Returns None if the line is not a world
/// write for this alias.
fn parse_world_write(line: &str, write_alias: &str) -> Option<(String, String)> {
    let line = line.trim();
    let prefix = format!("{write_alias}.");
    let rest = line.strip_prefix(&prefix)?;
    // `FIELD = RHS`.
    let eq = rest.find('=')?;
    let field = rest[..eq].trim();
    let after = &rest[eq + 1..];
    // Reject `<=`, `>=`, `!=`, `==`.
    if after.starts_with('=') {
        return None;
    }
    if let Some(prev) = rest[..eq].chars().last() {
        if prev == '<' || prev == '>' || prev == '!' {
            return None;
        }
    }
    if field.is_empty()
        || !field.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
        || !field.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
        return None;
    }
    Some((field.to_string(), after.trim().to_string()))
}

/// Rewrite every `<read_alias>.FIELD` occurrence in `text` to the bare `FIELD`
/// const name, recording each FIELD as a world read on the builder. The alias
/// match respects identifier boundaries so `world` doesn't match inside another
/// identifier (`world_next.` never matches `world.`, since the char after
/// `world` there is `_`). Returns the rewritten text.
fn rewrite_world_reads(text: &str, read_alias: Option<&str>, b: &mut FsmBuilder) -> String {
    let alias = match read_alias {
        Some(a) => a,
        None => return text.to_string(),
    };
    let needle = format!("{alias}.");
    let chars: Vec<char> = text.chars().collect();
    let needle_chars: Vec<char> = needle.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut idx = 0;
    while idx < chars.len() {
        // Try to match `alias.` at a word boundary (char before is not part of
        // an identifier).
        let boundary_ok = idx == 0 || !(chars[idx - 1].is_alphanumeric() || chars[idx - 1] == '_');
        if boundary_ok && chars[idx..].starts_with(needle_chars.as_slice()) {
            let mut j = idx + needle_chars.len();
            let field_start = j;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            if j > field_start {
                let field: String = chars[field_start..j].iter().collect();
                b.note_read(&field);
                out.push_str(&field);
                idx = j;
                continue;
            }
        }
        out.push(chars[idx]);
        idx += 1;
    }
    out
}

/// Wrapper over [`parse_enum_state_match_arms`] that additionally rewrites any
/// `<read_alias>.FIELD` in each arm body to the bare `FIELD` const, recording
/// the world reads on the builder. Used for `state_next`/`effects` match blocks
/// whose arms may read shared world (the consumer's `world.n > 0` ternary arm).
fn parse_enum_state_match_arms_w(
    raw: &[(usize, String)],
    start: usize,
    head_indent: usize,
    read_alias: Option<&str>,
    b: &mut FsmBuilder,
) -> Result<(Vec<EnumArm>, Option<String>, usize), FrontendError> {
    let (mut arms, default, consumed) = parse_enum_state_match_arms(raw, start, head_indent)?;
    for arm in &mut arms {
        arm.body = rewrite_world_reads(&arm.body, read_alias, b);
    }
    let default = default.map(|d| rewrite_world_reads(&d, read_alias, b));
    Ok((arms, default, consumed))
}

/// Parse `NAME = A | B(Int) | C` into (name, [Variant{A}, Variant{B, [Int]}, …]).
/// Nullary variants have an empty `arg_sorts`; payload variants carry the
/// SMT-LIB sort of each arg (Int / Bool / String).
fn parse_enum(rest: &str) -> Result<(String, Vec<Variant>), FrontendError> {
    let eq = rest.find('=').ok_or_else(|| FrontendError(format!("malformed enum: {rest:?}")))?;
    let name = rest[..eq].trim().to_string();
    if name.is_empty() {
        return fe(format!("enum missing a name: {rest:?}"));
    }
    let mut variants: Vec<Variant> = Vec::new();
    for raw in rest[eq + 1..].split('|') {
        let v = raw.trim();
        if v.is_empty() {
            continue;
        }
        variants.push(parse_variant(v)?);
    }
    if variants.is_empty() {
        return fe(format!("enum `{name}` has no variants"));
    }
    Ok((name, variants))
}

/// Parse one enum variant spelling: `Done` (nullary) or `Count(Int)` /
/// `Pair(Int, String)` (payload). Arg sorts must be scalar.
fn parse_variant(v: &str) -> Result<Variant, FrontendError> {
    match v.find('(') {
        None => {
            if !v.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
                || !v.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                return fe(format!("malformed enum variant: {v:?}"));
            }
            Ok(Variant { name: v.to_string(), arg_sorts: Vec::new() })
        }
        Some(lp) => {
            let name = v[..lp].trim().to_string();
            let close = v.rfind(')').ok_or_else(|| {
                FrontendError(format!("unclosed payload in enum variant: {v:?}"))
            })?;
            if close < lp {
                return fe(format!("malformed enum variant payload: {v:?}"));
            }
            if name.is_empty()
                || !name.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
                || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                return fe(format!("malformed enum variant name: {v:?}"));
            }
            let inner = v[lp + 1..close].trim();
            let mut arg_sorts = Vec::new();
            for arg in split_top_level_commas(inner) {
                let arg = arg.trim();
                if arg.is_empty() {
                    return fe(format!("empty payload arg in enum variant: {v:?}"));
                }
                arg_sorts.push(smt_sort_of(arg)?);
            }
            if arg_sorts.is_empty() {
                return fe(format!("payload-style variant `{v}` has no args (use `{name}` for nullary)"));
            }
            Ok(Variant { name, arg_sorts })
        }
    }
}

/// A parsed `fsm` header: its name, optional enum-state `(param, EnumType)`, and
/// the world-param names — `world_read` is the read alias (`world`), present
/// when the FSM has a `... ∈ World` param; `world_write` is the write alias
/// (`world_next`), present when the FSM has a second `... ∈ World` param.
struct FsmHeader {
    name: String,
    enum_state: Option<(String, String)>,
    /// The read alias for `world.X` (e.g. `"world"`), if any.
    world_read: Option<String>,
    /// The write alias for `world_next.X = …` (e.g. `"world_next"`), if any.
    world_write: Option<String>,
}

/// Parse `NAME`, `NAME(param ∈ EnumType)`, or a richer header carrying world
/// params, after the `fsm ` keyword. The full param list (already joined across
/// continuation lines) is `rest`. Recognized params, in any order:
///   * `state ∈ EnumType` — the FSM's enum state (at most one).
///   * `world ∈ World` — read alias for the shared world.
///   * `world, world_next ∈ World` — a multi-name group: the FIRST name is the
///     read alias, the SECOND the write alias (both bind `World`).
///   * `world_next ∈ World` (alone) — a write alias.
fn parse_fsm_header(rest: &str) -> Result<FsmHeader, FrontendError> {
    let rest = rest.trim();
    let lp = match rest.find('(') {
        None => {
            let name = rest.trim().to_string();
            if name.is_empty() || name.contains(char::is_whitespace) {
                return fe(format!("malformed fsm header: {rest:?}"));
            }
            return Ok(FsmHeader { name, enum_state: None, world_read: None, world_write: None });
        }
        Some(lp) => lp,
    };
    let name = rest[..lp].trim().to_string();
    let close = rest
        .rfind(')')
        .ok_or_else(|| FrontendError(format!("unclosed fsm params: {rest:?}")))?;
    if close < lp {
        return fe(format!("malformed fsm params: {rest:?}"));
    }
    let params = rest[lp + 1..close].trim();
    if name.is_empty() || name.contains(char::is_whitespace) {
        return fe(format!("malformed fsm header name: {rest:?}"));
    }

    let mut enum_state: Option<(String, String)> = None;
    let mut world_read: Option<String> = None;
    let mut world_write: Option<String> = None;

    for group in split_top_level_commas(params) {
        let group = group.trim();
        if group.is_empty() {
            continue;
        }
        // A param group is `names ∈ Type`, where `names` may be a comma-joined
        // multi-name list (e.g. `world, world_next`). But our top-level comma
        // split already broke those apart, so re-join: a group with `∈` carries
        // the type; the preceding type-less groups are leading names of the same
        // multi-name declaration. We instead handle the common shapes directly.
        let in_idx = match group.find('∈') {
            Some(x) => x,
            None => {
                // A bare name with no `∈` — part of a multi-name group whose
                // `∈ Type` follows in the NEXT comma segment. Defer: we collect
                // these by scanning the whole params string instead. Fall through
                // to the structured scan below.
                return parse_fsm_header_grouped(name, params);
            }
        };
        let names_part = group[..in_idx].trim();
        let ty = group[in_idx + '∈'.len_utf8()..].trim();
        if names_part.is_empty() || ty.is_empty() || ty.contains(char::is_whitespace) {
            return fe(format!("malformed fsm param: {group:?}"));
        }
        let names: Vec<String> = names_part
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        apply_param_group(&names, ty, &mut enum_state, &mut world_read, &mut world_write)?;
    }

    Ok(FsmHeader { name, enum_state, world_read, world_write })
}

/// Fallback header parse for the multi-name-spanning-commas case (e.g.
/// `world, world_next ∈ World, state ∈ PState`). Rebuilds `(names, ty)` groups
/// by scanning the comma-separated tokens and binding leading type-less names to
/// the next `∈ Type`.
fn parse_fsm_header_grouped(name: String, params: &str) -> Result<FsmHeader, FrontendError> {
    let mut enum_state: Option<(String, String)> = None;
    let mut world_read: Option<String> = None;
    let mut world_write: Option<String> = None;

    let mut pending_names: Vec<String> = Vec::new();
    for seg in split_top_level_commas(params) {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        match seg.find('∈') {
            None => {
                // A leading name with no type yet — belongs to the next typed seg.
                pending_names.push(seg.to_string());
            }
            Some(in_idx) => {
                let last_name = seg[..in_idx].trim();
                let ty = seg[in_idx + '∈'.len_utf8()..].trim();
                if last_name.is_empty() || ty.is_empty() || ty.contains(char::is_whitespace) {
                    return fe(format!("malformed fsm param segment: {seg:?}"));
                }
                let mut names = std::mem::take(&mut pending_names);
                names.push(last_name.to_string());
                apply_param_group(&names, ty, &mut enum_state, &mut world_read, &mut world_write)?;
            }
        }
    }
    if !pending_names.is_empty() {
        return fe(format!("fsm param names without a type: {pending_names:?}"));
    }
    Ok(FsmHeader { name, enum_state, world_read, world_write })
}

/// Apply one `(names, ty)` param group to the header accumulators. `World`-typed
/// groups set the read alias (first name) and write alias (second name);
/// other (capitalized) types are treated as the enum state (single name).
fn apply_param_group(
    names: &[String],
    ty: &str,
    enum_state: &mut Option<(String, String)>,
    world_read: &mut Option<String>,
    world_write: &mut Option<String>,
) -> Result<(), FrontendError> {
    if ty == "World" {
        // `world ∈ World` → read alias; `world, world_next ∈ World` → read+write.
        if names.len() > 2 {
            return fe(format!("World param group has too many names: {names:?}"));
        }
        if let Some(first) = names.first() {
            if world_read.is_some() {
                return fe("multiple World read params not supported".to_string());
            }
            *world_read = Some(first.clone());
        }
        if let Some(second) = names.get(1) {
            *world_write = Some(second.clone());
        }
        Ok(())
    } else {
        // Treat as the enum-state param: `state ∈ EnumType`.
        if names.len() != 1 {
            return fe(format!("enum-state param must be a single name: {names:?}"));
        }
        if enum_state.is_some() {
            return fe("multiple enum-state params not supported".to_string());
        }
        *enum_state = Some((names[0].clone(), ty.to_string()));
        Ok(())
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

/// Recognize the single-line scalar state `X ∈ Int = (is_first_tick ? INIT :
/// EXPR)`. An `X ∈ Int = …` whose RHS does NOT use the `is_first_tick` idiom is
/// NOT scalar state — it is an intermediate Int var (a per-tick derived value).
/// We return `Ok(None)` for that case so the caller falls through to the
/// chained-membership intermediate-var path; only a genuine `is_first_tick`
/// shape produces a [`ScalarState`].
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
    // Reject `==`, `<=`, `>=`, `!=` — only the membership `=` (a definition).
    if rhs.starts_with('=') {
        return Ok(None);
    }
    if !rhs.contains("is_first_tick") {
        // An intermediate Int var (`seed_a ∈ Int = tick * 7 + 3`), NOT scalar
        // state. Not our shape — let the caller handle it as a chained
        // intermediate-var definition.
        return Ok(None);
    }
    Ok(Some(parse_scalar_state_rhs(&name, rhs)?))
}

/// Recognize a chained-membership intermediate var: `X ∈ T = expr` where `T` is
/// a scalar sort (Int/Nat/Pos/Bool/String) and the RHS is NOT the
/// `is_first_tick` scalar-state idiom (that case is consumed by
/// [`try_parse_scalar_state`] first). Returns `(name, smt_sort, rhs)`. None if
/// the line is not this shape.
fn try_parse_chained_intermediate(line: &str) -> Result<Option<(String, String, String)>, FrontendError> {
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
    // Only bare scalar sorts (no `Seq(...)`, no payload, no enum types here —
    // enum-typed multi-name decls are handled separately).
    if sort.contains('(') || sort.contains(char::is_whitespace) {
        return Ok(None);
    }
    let smt_sort = match sort {
        "Int" | "Nat" | "Pos" => "Int",
        "Bool" => "Bool",
        "String" | "Str" => "String",
        _ => return Ok(None),
    };
    if name.is_empty() || name.contains(char::is_whitespace) {
        return Ok(None);
    }
    let rhs = after[eq + 1..].trim();
    // Reject comparison/eq operators glued to `∈ T =` (e.g. `X ∈ Int == …`).
    if rhs.starts_with('=') {
        return Ok(None);
    }
    Ok(Some((name, smt_sort.to_string(), rhs.to_string())))
}

/// Recognize a multi-name declaration: `a, b, c ∈ T` (no `=`). Returns
/// `(names, type-name)`. None if the line isn't a comma-list of bare identifiers
/// before `∈ <bare-type>`. (A single-name `X ∈ T` is left for
/// [`try_parse_bare_decl`]; this fires only when there's ≥ 2 names.)
fn try_parse_multiname_decl(line: &str) -> Result<Option<(Vec<String>, String)>, FrontendError> {
    let in_idx = match line.find('∈') {
        Some(x) => x,
        None => return Ok(None),
    };
    let names_part = line[..in_idx].trim();
    let after = line[in_idx + '∈'.len_utf8()..].trim();
    // No `=` on this line — a bare decl (a `… ∈ T = expr` is a chained
    // intermediate, handled earlier).
    if after.contains('=') {
        return Ok(None);
    }
    // Only bare type names (no `Seq(...)`, no payload, no whitespace).
    if after.is_empty() || after.contains('(') || after.contains(char::is_whitespace) {
        return Ok(None);
    }
    if !names_part.contains(',') {
        return Ok(None); // single-name decl — not our shape
    }
    let names: Vec<String> = names_part
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if names.len() < 2 {
        return Ok(None);
    }
    for n in &names {
        if !n.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
            || !n.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            return Ok(None);
        }
    }
    Ok(Some((names, after.to_string())))
}

/// Resolve a type name (from a multi-name decl) to its SMT-LIB sort name. A
/// scalar (`Int`/`Nat`/`Pos`/`Bool`/`String`) maps to its SMT sort; a declared
/// enum maps to its own name (the datatype sort). Anything else is rejected.
fn enum_or_scalar_sort(
    sort: &str,
    enums: &HashMap<String, Vec<Variant>>,
) -> Result<String, FrontendError> {
    match sort {
        "Int" | "Nat" | "Pos" => Ok("Int".into()),
        "Bool" => Ok("Bool".into()),
        "String" | "Str" => Ok("String".into()),
        other if enums.contains_key(other) => Ok(other.to_string()),
        other => fe(format!(
            "multi-name declaration type `{other}` is not a scalar sort or a declared enum"
        )),
    }
}

/// Recognize a bare two-operand (in)equality constraint line: `a ≠ b` or
/// `a = b` (the operands may be any expressions, e.g. `g1_0 ≠ g1_1`). Returns
/// `(lhs, op, rhs)` with op ∈ {Eq, Neq}. None if the line has no top-level
/// `≠`/`=`, or if it parses as something other than a single comparison.
fn try_parse_binary_constraint(line: &str) -> Result<Option<(Expr, BinOp, Expr)>, FrontendError> {
    let toks = tokenize(line.trim())?;
    let (e, used) = match parse_expr(&toks, 0) {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };
    if used != toks.len() {
        return Ok(None);
    }
    match e {
        Expr::Bin(op @ (BinOp::Eq | BinOp::Neq), a, b) => Ok(Some((*a, op, *b))),
        _ => Ok(None),
    }
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

/// Parse indented `Pattern ⇒ body` arms of a `match state` block (used for both
/// `state_next` and `effects`). The pattern is a state enum variant, optionally
/// binding its payload (`Count(n)`), ignoring it (`Format(_)`), or nullary
/// (`Start`); `_` is the catch-all default. Bodies are kept RAW and lowered at
/// emit time so payload bindings + the scalar-var rename apply.
fn parse_enum_state_match_arms(
    raw: &[(usize, String)],
    start: usize,
    head_indent: usize,
) -> Result<(Vec<EnumArm>, Option<String>, usize), FrontendError> {
    let mut arms: Vec<EnumArm> = Vec::new();
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
        if pat == "_" {
            default = Some(body.to_string());
        } else {
            let (ctor, bind) = parse_enum_pattern(pat)?;
            arms.push(EnumArm { ctor, bind, body: body.to_string() });
        }
        consumed += 1;
        i += 1;
    }
    if arms.is_empty() && default.is_none() {
        return fe("match block has no arms");
    }
    Ok((arms, default, consumed))
}

/// Parse a state-match pattern: `Start` → (`"Start"`, None); `Count(n)` →
/// (`"Count"`, Some("n")); `Format(_)` → (`"Format"`, None).
fn parse_enum_pattern(pat: &str) -> Result<(String, Option<String>), FrontendError> {
    let pat = pat.trim();
    match pat.find('(') {
        None => {
            if !pat.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
                || !pat.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                return fe(format!("malformed match pattern: {pat:?}"));
            }
            Ok((pat.to_string(), None))
        }
        Some(lp) => {
            let ctor = pat[..lp].trim().to_string();
            let rp = pat.rfind(')').ok_or_else(|| {
                FrontendError(format!("unclosed pattern in match arm: {pat:?}"))
            })?;
            let bind_raw = pat[lp + 1..rp].trim();
            let bind = if bind_raw == "_" || bind_raw.is_empty() {
                None
            } else {
                if bind_raw.contains(',') {
                    return fe(format!(
                        "multi-arg payload binding not supported in match pattern: {pat:?}"
                    ));
                }
                Some(bind_raw.to_string())
            };
            if ctor.is_empty()
                || !ctor.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
            {
                return fe(format!("malformed match pattern ctor: {pat:?}"));
            }
            Ok((ctor, bind))
        }
    }
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
        then_variant: emit_enum_value(then_v, &HashMap::new())?,
        else_variant: emit_enum_value(else_v, &HashMap::new())?,
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

/// Gather a (possibly multi-line) `fsm` header param list. `first` is the text
/// after the `fsm ` keyword on line `start`. If its parens are unbalanced (an
/// open `(` with no matching `)`), append subsequent lines (whitespace-joined)
/// until they balance. Returns (joined_header_text, lines_consumed_beyond_start).
fn gather_fsm_header(raw: &[(usize, String)], start: usize, first: &str) -> (String, usize) {
    let mut acc = first.to_string();
    let mut consumed = 0;
    // Count paren balance, ignoring string literals.
    let balanced = |s: &str| -> bool {
        let mut depth: i32 = 0;
        let mut in_str = false;
        for c in s.chars() {
            match c {
                '"' => in_str = !in_str,
                '(' if !in_str => depth += 1,
                ')' if !in_str => depth -= 1,
                _ => {}
            }
        }
        depth <= 0
    };
    let mut i = start + 1;
    while !balanced(&acc) && i < raw.len() {
        acc.push(' ');
        acc.push_str(raw[i].1.trim());
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

/// Lower a value-position enum expression (a `state_next` arm body or ternary
/// branch): a bare nullary variant (`Done`), an applied constructor
/// (`Count(5)`, `Format(n)`, `Count(n - 1)`), or a `(cond ? A : B)` ternary over
/// those. Identifiers (payload bindings, scalar vars) are renamed per `rename`.
fn emit_enum_value(e: &str, rename: &HashMap<String, String>) -> Result<String, FrontendError> {
    let e = strip_outer_parens(e).trim().to_string();

    // Ternary over enum values: `cond ? A : B`.
    if let Some(q) = find_top_level_question(&e) {
        let cond = e[..q].trim();
        let after_q = &e[q + 1..];
        let colon = find_top_level_colon(after_q)
            .ok_or_else(|| FrontendError(format!("enum-value ternary missing `:`: {e:?}")))?;
        let then_str = after_q[..colon].trim();
        let else_str = after_q[colon + 1..].trim();
        let toks = tokenize(cond)?;
        let (cond_expr, used) = parse_expr(&toks, 0)?;
        if used != toks.len() {
            return fe(format!("trailing tokens in enum-value ternary condition: {cond:?}"));
        }
        let cond_smt = emit_expr_renamed(&cond_expr, rename)?;
        let then_smt = emit_enum_value(then_str, rename)?;
        let else_smt = emit_enum_value(else_str, rename)?;
        return Ok(format!("(ite {cond_smt} {then_smt} {else_smt})"));
    }

    // Applied constructor `Ctor(arg, …)`.
    if let Some(lp) = e.find('(') {
        let ctor = e[..lp].trim().to_string();
        let close = e
            .rfind(')')
            .ok_or_else(|| FrontendError(format!("unclosed enum constructor: {e:?}")))?;
        if ctor.is_empty() || !ctor.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
            return fe(format!("malformed enum constructor: {e:?}"));
        }
        let inner = e[lp + 1..close].trim();
        let mut parts = Vec::new();
        for arg in split_top_level_commas(inner) {
            let arg = arg.trim();
            if arg.is_empty() {
                return fe(format!("empty arg in enum constructor: {e:?}"));
            }
            parts.push(emit_arg_expr(arg, rename)?);
        }
        if parts.is_empty() {
            return fe(format!("applied constructor `{e}` has no args"));
        }
        return Ok(format!("({} {})", ctor, parts.join(" ")));
    }

    // Bare nullary variant.
    if e.is_empty() || !e.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
        return fe(format!("expected an enum variant: {e:?}"));
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
    /// A function-call expression `f(a, b, …)`. The string-ops `index_of` /
    /// `substr` / `replace` lower to Z3 string theory; everything else is an
    /// honest error at emit time.
    Call(String, Vec<Expr>),
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
        Some(Tok::Ident(n)) => {
            // `f(a, b, …)` — a function-call expression. Recognized when the
            // identifier is immediately followed by `(`. A bare identifier (no
            // `(`) stays an `Ident`, preserving the existing path.
            if matches!(t.get(p + 1), Some(Tok::LParen)) {
                let (args, p2) = parse_call_args(t, p + 2)?;
                Ok((Expr::Call(n.clone(), args), p2))
            } else {
                Ok((Expr::Ident(n.clone()), p + 1))
            }
        }
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

/// Parse a comma-separated argument list of full expressions, starting at the
/// token AFTER the opening `(`, and consuming through the matching `)`. Returns
/// the parsed args and the position just past the `)`. Each arg is a full
/// expression, so arithmetic / nested calls / ternaries inside args work.
fn parse_call_args(t: &[Tok], mut p: usize) -> Result<(Vec<Expr>, usize), FrontendError> {
    let mut args = Vec::new();
    // Empty arg list `f()`.
    if matches!(t.get(p), Some(Tok::RParen)) {
        return Ok((args, p + 1));
    }
    loop {
        let (e, p2) = parse_expr(t, p)?;
        args.push(e);
        p = p2;
        match t.get(p) {
            Some(Tok::Comma) => { p += 1; }
            Some(Tok::RParen) => return Ok((args, p + 1)),
            other => return fe(format!("expected `,` or `)` in call args, got {other:?}")),
        }
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
        Expr::Call(name, args) => emit_call(name, args, rename),
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

/// Lower a function-call expression to SMT-LIB. The string-manipulation
/// builtins map to Z3 string-theory operators (the engine runs the same Z3, so
/// it decodes them natively — no engine change needed):
///   * `index_of(s, sub)`     → `(str.indexof <s> <sub> 0)`  (Int; first match)
///   * `substr(s, start, len)` → `(str.substr <s> <start> <len>)`
///   * `replace(s, from, to)`  → `(str.replace <s> <from> <to>)`  (first occurrence)
/// Args lower recursively through [`emit_expr_renamed`], so renames + arithmetic
/// in the args (e.g. `substr(g, lt + 1, gt - lt - 1)`) carry through. Any other
/// call name is an honest error — never a silent mis-handle.
fn emit_call(name: &str, args: &[Expr], rename: &HashMap<String, String>) -> Result<String, FrontendError> {
    let lowered: Result<Vec<String>, FrontendError> =
        args.iter().map(|a| emit_expr_renamed(a, rename)).collect();
    let a = lowered?;
    match (name, a.len()) {
        ("index_of", 2) => Ok(format!("(str.indexof {} {} 0)", a[0], a[1])),
        ("substr", 3) => Ok(format!("(str.substr {} {} {})", a[0], a[1], a[2])),
        ("replace", 3) => Ok(format!("(str.replace {} {} {})", a[0], a[1], a[2])),
        ("index_of", n) => fe(format!("`index_of` takes 2 args, got {n}")),
        ("substr", n) => fe(format!("`substr` takes 3 args, got {n}")),
        ("replace", n) => fe(format!("`replace` takes 3 args, got {n}")),
        (other, _) => fe(format!(
            "unsupported function-call `{other}` in fsm body \
             (string ops: index_of / substr / replace)"
        )),
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
    // Shared world array, only when there are world fields (a single-FSM
    // program with no `type World` emits exactly the same JSON it did before).
    if !prog.world.is_empty() {
        let entries: Vec<String> = prog
            .world
            .iter()
            .map(|w| format!("{{\"name\":\"{}\",\"sort\":\"{}\",\"init\":0}}", w.name, w.smt_sort))
            .collect();
        let _ = writeln!(out, ";   \"world\": [{}],", entries.join(", "));
    }
    out.push_str(";   \"fsms\": [\n");
    for (k, fsm) in prog.fsms.iter().enumerate() {
        emit_fsm_meta(&mut out, fsm);
        // Comma between fsm entries, none after the last.
        if k + 1 < prog.fsms.len() {
            out.push_str(";     },\n");
        } else {
            out.push_str(";     }\n");
        }
    }
    out.push_str(";   ]\n");
    out.push_str("; }\n");
    out.push_str("; @end\n");

    // ---- one transition block per FSM ----
    for fsm in &prog.fsms {
        emit_fsm_transition(&mut out, prog, fsm)?;
    }

    Ok(out)
}

/// Emit one FSM's `@meta` JSON entry (no trailing `},` — the caller closes it so
/// the inter-entry comma is placed correctly).
fn emit_fsm_meta(out: &mut String, fsm: &FsmDef) {
    out.push_str(";     { \"name\": \"");
    out.push_str(&fsm.fsm_name);
    out.push_str("\",\n");

    // state array — enum state first (if any), then scalar (if any).
    let mut state_entries: Vec<String> = Vec::new();
    if let Some(es) = &fsm.enum_state {
        state_entries.push(format!(
            "{{\"prev\":\"{}\",\"next\":\"{}\",\"sort\":\"{}\",\"init\":\"{}\"}}",
            es.prev, es.next, es.enum_name, es.init
        ));
    }
    if let Some(sc) = &fsm.scalar_state {
        state_entries.push(format!(
            "{{\"prev\":\"{}\",\"next\":\"{}\",\"sort\":\"Int\",\"init\":{}}}",
            sc.prev, sc.name, sc.init
        ));
    }
    let _ = write!(out, ";       \"state\": [{}]", state_entries.join(", "));

    if !fsm.world_writes.is_empty() {
        let names: Vec<String> = fsm.world_writes.iter().map(|w| format!("\"{w}\"")).collect();
        let _ = write!(out, ",\n;       \"world_writes\": [{}]", names.join(", "));
    }
    if !fsm.world_reads.is_empty() {
        let names: Vec<String> = fsm.world_reads.iter().map(|w| format!("\"{w}\"")).collect();
        let _ = write!(out, ",\n;       \"world_reads\": [{}]", names.join(", "));
    }
    if fsm.has_effects {
        out.push_str(",\n;       \"effects\": {\"var\":\"effects\"}");
    }
    if fsm.reads_last_results {
        out.push_str(",\n;       \"last_results\": {\"var\":\"last_results\",\"elem_sort\":\"Result\"}");
    }
    out.push('\n');
}

/// Emit one FSM's `; @transition <name>` SMT-LIB block. Each block is
/// self-contained: it re-declares the Effect datatype + this FSM's state enum,
/// declares its prev/next state consts, world read/write consts, intermediate
/// vars, and the per-binding asserts.
fn emit_fsm_transition(out: &mut String, prog: &Program, fsm: &FsmDef) -> Result<(), FrontendError> {
    // The scalar var rename: the .ev's current `X` is the engine prev `_X`.
    let mut rename: HashMap<String, String> = HashMap::new();
    if let Some(sc) = &fsm.scalar_state {
        rename.insert(sc.name.clone(), sc.prev.clone());
    }

    let _ = writeln!(out, "; @transition {}", fsm.fsm_name);

    // Datatypes: fixed Effect + (for enum state) the state enum, batched.
    emit_datatypes(out, prog, fsm);
    if fsm.reads_last_results {
        out.push_str(
            "(declare-datatypes ((Result 0))\n  \
             (((NoResult) (IntResult (IntResult_0 Int)) \
             (StringResult (StringResult_0 String)) \
             (ErrorResult (ErrorResult_0 String)))))\n",
        );
    }

    // Const declarations.
    if let Some(es) = &fsm.enum_state {
        let _ = writeln!(out, "(declare-const {} {})", es.prev, es.enum_name);
        let _ = writeln!(out, "(declare-const {} {})", es.next, es.enum_name);
    }
    if let Some(sc) = &fsm.scalar_state {
        let _ = writeln!(out, "(declare-const {} Int)", sc.prev);
        let _ = writeln!(out, "(declare-const {} Int)", sc.name);
    }
    // World reads + writes: each is a bare Int const. (test_09 never both reads
    // and writes the same var, so a name appears in at most one of the two
    // lists; declare each once.)
    for field in &fsm.world_reads {
        let sort = world_field_sort(prog, field);
        let _ = writeln!(out, "(declare-const {field} {sort})");
    }
    for field in &fsm.world_writes {
        if fsm.world_reads.iter().any(|r| r == field) {
            continue; // already declared as a read
        }
        let sort = world_field_sort(prog, field);
        let _ = writeln!(out, "(declare-const {field} {sort})");
    }
    if fsm.reads_last_results {
        out.push_str("(declare-const last_results (Seq Result))\n");
    }
    if fsm.has_effects {
        out.push_str("(declare-const effects (Seq Effect))\n");
    }

    // Intermediate var declarations (forward declarations, all up front so a
    // later binding can reference an earlier one in any order).
    for b in &fsm.bindings {
        if let Binding::IntermediateVar { name, smt_sort, .. } = b {
            let _ = writeln!(out, "(declare-const {name} {smt_sort})");
        }
    }

    // Fresh per-tick witness vars (multi-name decls like `g1_0, g1_1, … ∈
    // Color`). Bare consts; the body's `≠`/`=` constraints constrain them.
    for (name, smt_sort) in &fsm.fresh_vars {
        let _ = writeln!(out, "(declare-const {name} {smt_sort})");
    }

    // ---- asserts ----
    // Scalar transition (state).
    if let Some(sc) = &fsm.scalar_state {
        let _ = writeln!(out, "(assert (= {} {}))", sc.name, sc.transition_rhs);
    }

    // The enum match scrutinee (.ev's current `state` = engine prev).
    let enum_scrut = fsm.enum_state.as_ref().map(|es| es.prev.clone());

    for b in &fsm.bindings {
        match b {
            Binding::IntermediateVar { name, value, .. } => {
                let rhs = match value {
                    BindingValue::Expr(e) => emit_expr_renamed(e, &rename)?,
                    BindingValue::MatchLastResults { index, lr_var, arms, default } => {
                        emit_last_results_match(*index, lr_var, arms, default, &rename)?
                    }
                    BindingValue::MatchEnumState { arms, default } => {
                        let es = fsm.enum_state.as_ref().ok_or_else(|| {
                            FrontendError(format!(
                                "`{name} = match state` used without an enum state"
                            ))
                        })?;
                        // Arm bodies are scalar expressions; lower each via the
                        // arg-expr path (renaming + payload binding applied).
                        emit_enum_match(prog, fsm, &es.prev, arms, default, &rename, |body, r| {
                            emit_arg_expr(body, r)
                        })?
                    }
                };
                let _ = writeln!(out, "(assert (= {name} {rhs}))");
            }
            Binding::StateNextMatch { arms, default } => {
                let es = fsm.enum_state.as_ref().ok_or_else(|| {
                    FrontendError("`state_next = match` used without an enum state".into())
                })?;
                let scrut = enum_scrut.as_ref().unwrap();
                // Arm bodies are enum values (bare/applied variant or ternary).
                let ite = emit_enum_match(prog, fsm, scrut, arms, default, &rename, |body, r| {
                    emit_enum_value(body, r)
                })?;
                let _ = writeln!(out, "(assert (= {} {ite}))", es.next);
            }
            Binding::StateNextTernary { cond_expr, then_variant, else_variant } => {
                let es = fsm.enum_state.as_ref().ok_or_else(|| {
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
                // Arm bodies are seq literals / ternaries; lowered at emit time
                // so the scalar rename AND per-arm payload binding apply.
                let ite = emit_enum_match(prog, fsm, scrut, arms, default, &rename, |body, r| {
                    emit_effects_rhs(body, r)
                })?;
                let _ = writeln!(out, "(assert (= effects {ite}))");
            }
            Binding::EffectsExpr { raw } => {
                let term = emit_effects_rhs(raw, &rename)?;
                let _ = writeln!(out, "(assert (= effects {term}))");
            }
            Binding::WorldWrite { field, value } => {
                let rhs = emit_expr_renamed(value, &rename)?;
                let _ = writeln!(out, "(assert (= {field} {rhs}))");
            }
            Binding::Constraint { lhs, op, rhs } => {
                let l = emit_expr_renamed(lhs, &rename)?;
                let r = emit_expr_renamed(rhs, &rename)?;
                let assert = match op {
                    BinOp::Eq => format!("(= {l} {r})"),
                    BinOp::Neq => format!("(not (= {l} {r}))"),
                    _ => unreachable!("constraint op is always Eq/Neq"),
                };
                let _ = writeln!(out, "(assert {assert})");
            }
        }
    }

    Ok(())
}

/// SMT-LIB sort for a world field, looked up in the shared world. Defaults to
/// `Int` if the field isn't found (it always should be for valid programs).
fn world_field_sort(prog: &Program, field: &str) -> String {
    prog.world
        .iter()
        .find(|w| w.name == field)
        .map(|w| w.smt_sort.clone())
        .unwrap_or_else(|| "Int".to_string())
}

fn emit_datatypes(out: &mut String, prog: &Program, fsm: &FsmDef) {
    let effect_decl = "(Effect 0)";
    let effect_body =
        "((Println (Println_0 String)) (Exit (Exit_0 Int)) (IntToStr (IntToStr_0 Int)) (ParseInt (ParseInt_0 String)))";

    // The set of enum datatypes this FSM's transition block needs, in a stable
    // order: the state enum first (preserving the long-standing output shape),
    // then any OTHER enum referenced by a fresh var (e.g. `Color` for the
    // graph-coloring witnesses). Each enum is declared at most once.
    let mut enum_names: Vec<String> = Vec::new();
    if let Some(es) = &fsm.enum_state {
        enum_names.push(es.enum_name.clone());
    }
    for (_, smt_sort) in &fsm.fresh_vars {
        if prog.enums.contains_key(smt_sort) && !enum_names.iter().any(|n| n == smt_sort) {
            enum_names.push(smt_sort.clone());
        }
    }

    if enum_names.is_empty() {
        let _ = writeln!(out, "(declare-datatypes ({effect_decl}) ({effect_body}))");
        return;
    }

    let decl_heads: String = enum_names
        .iter()
        .map(|n| format!(" ({n} 0)"))
        .collect::<String>();
    let decl_bodies: String = enum_names
        .iter()
        .map(|n| {
            let variants = prog.enums.get(n).cloned().unwrap_or_default();
            let body: String = variants
                .iter()
                .map(emit_variant_decl)
                .collect::<Vec<_>>()
                .join(" ");
            format!(" ({body})")
        })
        .collect::<String>();
    let _ = writeln!(
        out,
        "(declare-datatypes ({effect_decl}{decl_heads}) ({effect_body}{decl_bodies}))"
    );
}

/// Emit one variant's datatype declaration: `(Done)` for a nullary variant,
/// `(Count (Count_0 Int))` for a payload variant. Field accessor = `<Ctor>_<i>`.
fn emit_variant_decl(v: &Variant) -> String {
    if v.arg_sorts.is_empty() {
        format!("({})", v.name)
    } else {
        let fields: String = v
            .arg_sorts
            .iter()
            .enumerate()
            .map(|(i, sort)| format!("({}_{i} {sort})", v.name))
            .collect::<Vec<_>>()
            .join(" ");
        format!("({} {fields})", v.name)
    }
}

/// Build a nested `(ite (is-Variant scrut) body rest)` over an enum scrutinee.
///
/// Each arm's payload binding (if any) is substituted by the field accessor
/// `(Ctor_<argidx> scrut)` inside the arm body before `lower_body` runs. The
/// last arm (or the `_` default) is the innermost else. `lower_body` lowers an
/// arm's raw body text given the rename map extended with the binding.
fn emit_enum_match<F>(
    prog: &Program,
    fsm: &FsmDef,
    scrut: &str,
    arms: &[EnumArm],
    default: &Option<String>,
    base_rename: &HashMap<String, String>,
    lower_body: F,
) -> Result<String, FrontendError>
where
    F: Fn(&str, &HashMap<String, String>) -> Result<String, FrontendError>,
{
    let enum_variants: Option<&Vec<Variant>> = fsm
        .enum_state
        .as_ref()
        .and_then(|es| prog.enums.get(&es.enum_name));

    // Validate every arm's ctor is a real variant, and resolve the per-arm
    // rename map (payload binding → field accessor).
    let resolve_arm = |arm: &EnumArm| -> Result<HashMap<String, String>, FrontendError> {
        let variant = enum_variants
            .and_then(|vs| vs.iter().find(|v| v.name == arm.ctor));
        if enum_variants.is_some() && variant.is_none() {
            return fe(format!(
                "match pattern `{}` is not a variant of the state enum",
                arm.ctor
            ));
        }
        let mut r = base_rename.clone();
        if let Some(bind) = &arm.bind {
            // Bind the payload to the first field accessor `(Ctor_0 scrut)`. The
            // variant must carry a payload to bind.
            if let Some(v) = variant {
                if v.arg_sorts.is_empty() {
                    return fe(format!(
                        "pattern `{}({bind})` binds a payload but `{}` is nullary",
                        arm.ctor, arm.ctor
                    ));
                }
            }
            r.insert(bind.clone(), format!("({}_0 {scrut})", arm.ctor));
        }
        Ok(r)
    };

    let mut elems = arms.to_vec();
    let base_else: String = if let Some(d) = default {
        lower_body(d, base_rename)?
    } else {
        let last = elems
            .pop()
            .ok_or_else(|| FrontendError("match has no arms".into()))?;
        let r = resolve_arm(&last)?;
        lower_body(&last.body, &r)?
    };

    let mut acc = base_else;
    for arm in elems.into_iter().rev() {
        let r = resolve_arm(&arm)?;
        let body = lower_body(&arm.body, &r)?;
        acc = format!("(ite (is-{} {scrut}) {body} {acc})", arm.ctor);
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
    fn payload_first_variant_is_err() {
        // The tick-0 init variant must be nullary (seeded as a bare ctor name);
        // a payload-carrying first variant is rejected.
        let src = "enum E = A(Int) | B\nfsm f(state ∈ E)\n    state_next = match state\n        A(n) ⇒ B\n        B    ⇒ B\n";
        assert!(transpile_fsm(src).is_err());
    }

    #[test]
    fn multiple_fsms_load_as_n() {
        // Two independent scalar FSMs (no shared world) → two FSM entries, two
        // transition blocks, no spurious `world` array.
        let src = "fsm a\n    x ∈ Int = (is_first_tick ? 0 : _x + 1)\nfsm b\n    y ∈ Int = (is_first_tick ? 0 : _y + 1)\n";
        let fix = transpile_fsm(src).expect("two independent fsms transpile");
        assert!(!fix.contains("\"world\""), "no world array for world-less fsms:\n{fix}");
        assert!(fix.contains("; @transition a"), "block a:\n{fix}");
        assert!(fix.contains("; @transition b"), "block b:\n{fix}");
        let prob = load_str(&fix).expect("engine loads two-fsm fixture");
        assert_eq!(prob.fsms.len(), 2);
        assert_eq!(prob.fsms[0].name, "a");
        assert_eq!(prob.fsms[1].name, "b");
        assert!(prob.world.is_empty(), "no world vars");
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

    // ---- payload-carrying enum state (test_02) ------------------------------

    const TEST_02: &str = "import \"stdlib/runtime.ev\"\n\nenum CountState = Start | Count(Int) | Format(Int) | Done\n\nfsm counter(state ∈ CountState)\n    state_next = match state\n        Start     ⇒ Count(5)\n        Count(n)  ⇒ Format(n)\n        Format(n) ⇒ (n ≤ 1 ? Done : Count(n - 1))\n        Done      ⇒ Done\n\n    n_str ∈ String\n    n_str = match last_results[0]\n        StringResult(s) ⇒ s\n        _               ⇒ \"?\"\n\n    effects = match state\n        Start     ⇒ ⟨Println(\"starting count\")⟩\n        Count(n)  ⇒ ⟨IntToStr(n)⟩\n        Format(_) ⇒ ⟨Println(\"tick \" ++ n_str)⟩\n        Done      ⇒ ⟨Println(\"bye\"), Exit(0)⟩\n";

    #[test]
    fn payload_enum_parses_variant_arg_sorts() {
        let (_name, variants) = parse_enum("CountState = Start | Count(Int) | Format(Int) | Done")
            .expect("parses payload enum");
        assert_eq!(variants.len(), 4);
        assert_eq!(variants[0].name, "Start");
        assert!(variants[0].arg_sorts.is_empty(), "Start nullary");
        assert_eq!(variants[1].name, "Count");
        assert_eq!(variants[1].arg_sorts, vec!["Int".to_string()]);
        assert_eq!(variants[2].name, "Format");
        assert_eq!(variants[2].arg_sorts, vec!["Int".to_string()]);
        assert_eq!(variants[3].name, "Done");
        assert!(variants[3].arg_sorts.is_empty(), "Done nullary");
    }

    #[test]
    fn payload_enum_datatype_uses_field_naming() {
        let fix = transpile_fsm(TEST_02).expect("transpile test_02");
        // Field accessor naming = <Ctor>_<argidx> (0-based), matching the
        // feedback_loop.smt2 convention. Nullary variants stay bare.
        assert!(
            fix.contains(
                "(declare-datatypes ((Effect 0) (CountState 0)) \
                 (((Println (Println_0 String)) (Exit (Exit_0 Int)) \
                 (IntToStr (IntToStr_0 Int)) (ParseInt (ParseInt_0 String))) \
                 ((Start) (Count (Count_0 Int)) (Format (Format_0 Int)) (Done))))"
            ),
            "datatypes:\n{fix}"
        );
    }

    #[test]
    fn state_next_match_binds_and_constructs_payload() {
        let fix = transpile_fsm(TEST_02).expect("transpile test_02");
        // The exact nested ite from the spec: payload binding `n` in Count(n) /
        // Format(n) resolves to the field accessor (Count_0 state) /
        // (Format_0 state); arm bodies construct payload values + a ternary.
        assert!(
            fix.contains(
                "(assert (= state_next \
                 (ite (is-Start state) (Count 5) \
                 (ite (is-Count state) (Format (Count_0 state)) \
                 (ite (is-Format state) \
                 (ite (<= (Format_0 state) 1) Done (Count (- (Format_0 state) 1))) \
                 Done)))))"
            ),
            "state_next:\n{fix}"
        );
    }

    #[test]
    fn effects_match_binds_and_ignores_payload() {
        let fix = transpile_fsm(TEST_02).expect("transpile test_02");
        // Count(n) ⇒ ⟨IntToStr(n)⟩ — payload bound into the effect arg.
        assert!(fix.contains("(seq.unit (IntToStr (Count_0 state)))"), "IntToStr(n):\n{fix}");
        // Format(_) ⇒ ⟨Println("tick " ++ n_str)⟩ — recognizer over (is-Format state),
        // payload ignored, intermediate n_str (from last_results[0]) concatenated.
        assert!(fix.contains("(is-Format state)"), "Format recognizer:\n{fix}");
        assert!(fix.contains("(str.++ \"tick \" n_str)"), "concat:\n{fix}");
        // n_str intermediate reads last_results[0].
        assert!(fix.contains("(declare-const n_str String)"), "n_str decl:\n{fix}");
        assert!(
            fix.contains("(StringResult_0 (seq.nth last_results 0))"),
            "n_str accessor:\n{fix}"
        );
        // Done ⇒ ⟨Println("bye"), Exit(0)⟩.
        assert!(fix.contains("(Println \"bye\")") && fix.contains("(Exit 0)"), "done arm:\n{fix}");
        let prob = load_str(&fix).expect("engine loads test_02 fixture");
        assert_eq!(prob.fsms[0].name, "counter");
        assert_eq!(prob.fsms[0].state.len(), 1, "one (enum) state var");
        assert_eq!(
            prob.fsms[0].state[0].sort,
            crate::spec::Sort::Datatype("CountState".into())
        );
        assert!(prob.fsms[0].last_results.is_some());
    }

    #[test]
    fn payload_binding_on_nullary_variant_is_err() {
        // Binding a payload on a nullary variant is rejected.
        let src = "enum E = Idle | Run(Int)\nfsm f(state ∈ E)\n    state_next = match state\n        Idle(x) ⇒ Run(0)\n        Run(n)  ⇒ Idle\n";
        assert!(transpile_fsm(src).is_err());
    }

    // ---- N3 multi-FSM + shared world (test_09) ------------------------------

    const TEST_09: &str = "import \"stdlib/runtime.ev\"\n\ntype World\n    n ∈ Int\n\nenum PState = PStart | PTick(Int) | PEnd\nenum CState = CWait | CFormat | CEnd\n\nfsm producer(world, world_next ∈ World,\n               state ∈ PState)\n    state_next = match state\n        PStart    ⇒ PTick(3)\n        PTick(k)  ⇒ (k ≤ 1 ? PEnd : PTick(k - 1))\n        PEnd      ⇒ PEnd\n\n    next_n ∈ Int\n    next_n = match state\n        PStart    ⇒ 3\n        PTick(k)  ⇒ k\n        PEnd      ⇒ 0\n    world_next.n = next_n\n\n    effects = match state\n        PEnd ⇒ ⟨Println(\"producer done\"), Exit(0)⟩\n        _    ⇒ ⟨⟩\n\nfsm consumer(world ∈ World,\n               state ∈ CState)\n    state_next = match state\n        CWait   ⇒ (world.n > 0 ? CFormat : CWait)\n        CFormat ⇒ CWait\n        CEnd    ⇒ CEnd\n\n    n_str ∈ String\n    n_str = match last_results[0]\n        StringResult(s) ⇒ s\n        _               ⇒ \"?\"\n\n    effects = match state\n        CWait   ⇒ (world.n > 0 ? ⟨IntToStr(world.n)⟩ : ⟨⟩)\n        CFormat ⇒ ⟨Println(\"consumer saw n = \" ++ n_str)⟩\n        CEnd    ⇒ ⟨⟩\n";

    #[test]
    fn test_09_world_meta_array() {
        let fix = transpile_fsm(TEST_09).expect("transpile test_09");
        // The shared world array — one Int field `n`, seeded init:0.
        assert!(
            fix.contains(";   \"world\": [{\"name\":\"n\",\"sort\":\"Int\",\"init\":0}],"),
            "world meta array:\n{fix}"
        );
    }

    #[test]
    fn test_09_two_fsm_blocks_with_own_datatypes() {
        let fix = transpile_fsm(TEST_09).expect("transpile test_09");
        // One @transition block per FSM, each re-declaring its OWN state enum +
        // the Effect datatype.
        assert!(fix.contains("; @transition producer"), "producer block:\n{fix}");
        assert!(fix.contains("; @transition consumer"), "consumer block:\n{fix}");
        assert!(
            fix.contains("(declare-datatypes ((Effect 0) (PState 0))"),
            "producer's PState declared in its block:\n{fix}"
        );
        assert!(
            fix.contains("(declare-datatypes ((Effect 0) (CState 0))"),
            "consumer's CState declared in its block:\n{fix}"
        );
    }

    #[test]
    fn test_09_world_writes_and_reads_inferred() {
        let fix = transpile_fsm(TEST_09).expect("transpile test_09");
        // producer writes `n`; consumer reads `n`.
        assert!(fix.contains("\"world_writes\": [\"n\"]"), "producer world_writes:\n{fix}");
        assert!(fix.contains("\"world_reads\": [\"n\"]"), "consumer world_reads:\n{fix}");
        // The write is `world_next.n = next_n` → bare const `n` is the target.
        assert!(fix.contains("(declare-const n Int)"), "world const decl:\n{fix}");
        assert!(fix.contains("(assert (= n next_n))"), "world write assert:\n{fix}");
        // The producer's intermediate `next_n = match state` lowers to a nested ite.
        assert!(fix.contains("(assert (= next_n (ite (is-PStart state) 3"), "next_n match:\n{fix}");
        // The consumer's reads lower `world.n` → bare `n` in conditions + IntToStr.
        assert!(fix.contains("(ite (> n 0) CFormat CWait)"), "state_next reads n:\n{fix}");
        assert!(fix.contains("(seq.unit (IntToStr n))"), "effects reads n:\n{fix}");
    }

    #[test]
    fn test_09_loads_as_two_fsms_with_world() {
        let fix = transpile_fsm(TEST_09).expect("transpile test_09");
        let prob = load_str(&fix).expect("engine loads test_09 fixture");
        assert_eq!(prob.fsms.len(), 2, "two FSMs");
        assert_eq!(prob.fsms[0].name, "producer");
        assert_eq!(prob.fsms[1].name, "consumer");
        // Shared world: one Int var `n` seeded init 0.
        assert_eq!(prob.world.len(), 1);
        assert_eq!(prob.world[0].name, "n");
        assert_eq!(prob.world[0].sort, crate::spec::Sort::Int);
        assert_eq!(prob.world[0].init, Some(crate::spec::Lit::Int(0)));
        // Writer/reader role assignment threaded into the spec.
        assert_eq!(prob.fsms[0].world_writes, vec!["n".to_string()]);
        assert!(prob.fsms[0].world_reads.is_empty());
        assert_eq!(prob.fsms[1].world_reads, vec!["n".to_string()]);
        assert!(prob.fsms[1].world_writes.is_empty());
        // Consumer reads last_results (n_str via StringResult).
        assert!(prob.fsms[1].last_results.is_some());
    }

    #[test]
    fn world_field_with_string_sort_is_err() {
        // Only Int world fields supported today.
        let src = "type World\n    s ∈ String\n\nfsm f(world ∈ World, state ∈ E)\nenum E = A | B\n";
        assert!(transpile_fsm(src).is_err());
    }

    #[test]
    fn reserved_async_world_field_is_gap() {
        // signal_received / tick_count / stdin_seq are plugin-owned in the legacy
        // runtime; the hybrid has no event source, so we reject (honest gap).
        let src = "type World\n    signal_received ∈ Int\n\nenum S = Running | Done\nfsm g(world ∈ World, state ∈ S)\n    state_next = match state\n        Running ⇒ (world.signal_received > 0 ? Done : Running)\n        Done    ⇒ Done\n";
        let err = transpile_fsm(src).unwrap_err();
        assert!(err.0.contains("reserved async-source"), "err: {}", err.0);
    }

    #[test]
    fn type_world_must_precede_fsms() {
        // `type World` after an fsm is rejected: the shared world declaration
        // must come before the FSMs that read/write it.
        let src = "enum E = A | B\nfsm f(state ∈ E)\n    state_next = match state\n        A ⇒ B\n        B ⇒ B\ntype World\n    n ∈ Int\n";
        let err = transpile_fsm(src).unwrap_err();
        assert!(err.0.contains("must precede"), "err: {}", err.0);
    }

    #[test]
    fn single_fsm_no_world_meta_unchanged() {
        // Regression guard: a single-FSM program with no `type World` emits the
        // exact `@meta` shape it always has — no `world` array, one fsm entry.
        let fix = transpile_fsm(TEST_08).expect("transpile test_08");
        assert!(!fix.contains("\"world\""), "no world array for single fsm:\n{fix}");
        assert!(!fix.contains("world_writes"), "no world_writes:\n{fix}");
        assert!(!fix.contains("world_reads"), "no world_reads:\n{fix}");
        // The fsms array opens directly after `{` (no world line between them).
        assert!(fix.contains("; {\n;   \"fsms\": [\n"), "fsms opens immediately:\n{fix}");
    }

    // ---- intermediate-Int vs scalar-state classification (test_29) ----------

    const TEST_29_SKEL: &str = "import \"stdlib/runtime.ev\"\n\nenum St = Looping | Done\n\nfsm heavy(state ∈ St)\n    tick ∈ Int = (is_first_tick ? 0 : _tick + 1)\n    state_next = (tick ≥ 19 ? Done : Looping)\n    seed_a ∈ Int = tick * 7 + 3\n    a01 ∈ Int = (seed_a > 50 ? seed_a - 7 : seed_a * 2 + 11)\n    a02 ∈ Int = (a01 > 100 ? a01 - 13 : a01 * 3 + 7)\n    effects = match state\n        Looping ⇒ ⟨Println(\"step\")⟩\n        Done    ⇒ ⟨Println(\"heavy compute: done\"), Exit(0)⟩\n";

    #[test]
    fn int_intermediate_is_not_scalar_state() {
        // `tick ∈ Int = (is_first_tick ? …)` IS scalar state; `seed_a ∈ Int =
        // tick * 7 + 3` (no is_first_tick) is an INTERMEDIATE var, not state.
        // The old code greedily errored "must use the is_first_tick idiom".
        let fix = transpile_fsm(TEST_29_SKEL).expect("transpile test_29 skeleton");
        // Exactly one scalar state var (tick); seed_a/a01/a02 are NOT in state.
        assert!(
            fix.contains("\"prev\":\"_tick\",\"next\":\"tick\",\"sort\":\"Int\",\"init\":0"),
            "tick is the scalar state:\n{fix}"
        );
        assert!(!fix.contains("\"next\":\"seed_a\""), "seed_a is not state:\n{fix}");
        // The intermediates are declared consts + defining asserts, with the
        // scalar rename (references to `tick` → engine prev `_tick`).
        assert!(fix.contains("(declare-const seed_a Int)"), "seed_a decl:\n{fix}");
        assert!(fix.contains("(assert (= seed_a (+ (* _tick 7) 3)))"), "seed_a uses _tick:\n{fix}");
        // a01 references seed_a (an intermediate) — NOT renamed.
        assert!(
            fix.contains("(assert (= a01 (ite (> seed_a 50) (- seed_a 7) (+ (* seed_a 2) 11))))"),
            "a01 chains seed_a:\n{fix}"
        );
        assert!(fix.contains("(assert (= a02 (ite (> a01 100)"), "a02 chains a01:\n{fix}");
        // state_next branches on the renamed _tick.
        assert!(
            fix.contains("(assert (= state_next (ite (>= _tick 19) Done Looping)))"),
            "state_next on _tick:\n{fix}"
        );
        let prob = load_str(&fix).expect("engine loads test_29 skeleton");
        assert_eq!(prob.fsms[0].name, "heavy");
        assert_eq!(prob.fsms[0].state.len(), 2, "enum + scalar state only");
    }

    #[test]
    fn chained_intermediate_bool_and_string() {
        // The chained-membership intermediate path also handles Bool/String.
        let src = "fsm f\n    n ∈ Int = (is_first_tick ? 0 : _n + 1)\n    big ∈ Bool = n > 3\n    msg ∈ String = \"v=\" ++ \"x\"\n    effects = (big ? ⟨Println(msg)⟩ : ⟨Exit(0)⟩)\n";
        let fix = transpile_fsm(src).expect("transpile chained intermediates");
        assert!(fix.contains("(declare-const big Bool)"), "big bool decl:\n{fix}");
        assert!(fix.contains("(assert (= big (> _n 3)))"), "big uses _n:\n{fix}");
        assert!(fix.contains("(declare-const msg String)"), "msg string decl:\n{fix}");
        assert!(fix.contains("(assert (= msg (str.++ \"v=\" \"x\")))"), "msg concat:\n{fix}");
        load_str(&fix).expect("engine loads chained-intermediate fixture");
    }

    // ---- multi-name decls + ≠/= constraints (test_28) -----------------------

    const TEST_28_SKEL: &str = "import \"stdlib/runtime.ev\"\n\nenum Color = Red | Green | Blue\n\nenum DemoState = Searching | Done\n\nfsm coloring(state ∈ DemoState)\n    tick ∈ Int = (is_first_tick ? 0 : _tick + 1)\n    state_next = (tick ≥ 19 ? Done : Searching)\n    g1_0, g1_1, g1_2 ∈ Color\n    g1_0 ≠ g1_1\n    g1_1 ≠ g1_2\n    g1_2 ≠ g1_0\n    effects = match state\n        Searching ⇒ ⟨Println(\"step\")⟩\n        Done      ⇒ ⟨Println(\"solved 6 independent graph 3-colorings\"), Exit(0)⟩\n";

    #[test]
    fn multiname_enum_decl_emits_one_const_per_name() {
        let fix = transpile_fsm(TEST_28_SKEL).expect("transpile test_28 skeleton");
        // One declare-const per name, sorted to the Color datatype.
        assert!(fix.contains("(declare-const g1_0 Color)"), "g1_0 decl:\n{fix}");
        assert!(fix.contains("(declare-const g1_1 Color)"), "g1_1 decl:\n{fix}");
        assert!(fix.contains("(declare-const g1_2 Color)"), "g1_2 decl:\n{fix}");
        // The non-state enum `Color` is batched into the block's declare-datatypes
        // alongside Effect + the state enum DemoState.
        assert!(
            fix.contains("(declare-datatypes ((Effect 0) (DemoState 0) (Color 0))"),
            "batched datatypes incl Color:\n{fix}"
        );
        assert!(fix.contains("((Red) (Green) (Blue))"), "Color variants:\n{fix}");
    }

    #[test]
    fn neq_constraints_emit_not_eq() {
        let fix = transpile_fsm(TEST_28_SKEL).expect("transpile test_28 skeleton");
        assert!(fix.contains("(assert (not (= g1_0 g1_1)))"), "g1_0 ≠ g1_1:\n{fix}");
        assert!(fix.contains("(assert (not (= g1_1 g1_2)))"), "g1_1 ≠ g1_2:\n{fix}");
        assert!(fix.contains("(assert (not (= g1_2 g1_0)))"), "g1_2 ≠ g1_0:\n{fix}");
        let prob = load_str(&fix).expect("engine loads test_28 skeleton");
        assert_eq!(prob.fsms[0].name, "coloring");
        // Only the enum + scalar are threaded state; the g* are fresh witnesses.
        assert_eq!(prob.fsms[0].state.len(), 2, "enum + scalar state only");
    }

    #[test]
    fn eq_constraint_between_two_vars_emits_assert() {
        // A bare `a = b` (neither a pending decl) is an `=` constraint, not an
        // assignment. Two fresh Int witnesses pinned equal.
        let src = "fsm f\n    n ∈ Int = (is_first_tick ? 0 : _n + 1)\n    a, b ∈ Int\n    a = b\n    a ≠ n\n    effects = ⟨Println(\"x\")⟩\n";
        let fix = transpile_fsm(src).expect("transpile eq-constraint");
        assert!(fix.contains("(declare-const a Int)"), "a decl:\n{fix}");
        assert!(fix.contains("(declare-const b Int)"), "b decl:\n{fix}");
        assert!(fix.contains("(assert (= a b))"), "a = b constraint:\n{fix}");
        // `a ≠ n`: n is the scalar state, renamed to _n on the constraint RHS.
        assert!(fix.contains("(assert (not (= a _n)))"), "a ≠ n renamed:\n{fix}");
        load_str(&fix).expect("engine loads eq-constraint fixture");
    }

    #[test]
    fn multiname_decl_unknown_type_is_err() {
        // A multi-name decl over a type that is neither a scalar nor a declared
        // enum is rejected (honest gap, not a silent free var).
        let src = "fsm f\n    n ∈ Int = (is_first_tick ? 0 : _n + 1)\n    a, b ∈ Widget\n    effects = ⟨Println(\"x\")⟩\n";
        let err = transpile_fsm(src).unwrap_err();
        assert!(err.0.contains("Widget"), "err: {}", err.0);
    }

    // ---- string-op function-call expressions (test_39) ----------------------

    /// Parse a single expression to its lowered SMT-LIB string (no renaming).
    fn lower_expr(src: &str) -> Result<String, FrontendError> {
        let toks = tokenize(src)?;
        let (e, used) = parse_expr(&toks, 0)?;
        assert_eq!(used, toks.len(), "did not consume all tokens of {src:?}");
        emit_expr(&e)
    }

    #[test]
    fn function_call_parses_as_call_node() {
        // An identifier immediately followed by `(` parses as a call, with each
        // arg a full expression.
        let toks = tokenize("index_of(g, \"<\")").expect("tokenize");
        let (e, used) = parse_expr(&toks, 0).expect("parse call");
        assert_eq!(used, toks.len());
        match e {
            Expr::Call(name, args) => {
                assert_eq!(name, "index_of");
                assert_eq!(args.len(), 2);
                assert!(matches!(args[0], Expr::Ident(ref n) if n == "g"));
                assert!(matches!(args[1], Expr::Str(ref s) if s == "<"));
            }
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn bare_identifier_still_parses_as_ident() {
        // Regression: a bare identifier with NO following `(` stays an Ident, and
        // a parenthesized expression `(a)` is unaffected by the call path.
        let toks = tokenize("g").expect("tokenize");
        let (e, _) = parse_expr(&toks, 0).expect("parse");
        assert!(matches!(e, Expr::Ident(ref n) if n == "g"), "{e:?}");
        assert_eq!(lower_expr("(lt + 1)").expect("paren expr"), "(+ lt 1)");
        // A bare ident feeding an effect arg (no call) still renders as-is.
        assert_eq!(lower_expr("head").expect("ident"), "head");
    }

    #[test]
    fn index_of_lowers_to_str_indexof() {
        assert_eq!(
            lower_expr("index_of(g, \"<\")").expect("index_of"),
            "(str.indexof g \"<\" 0)"
        );
    }

    #[test]
    fn substr_lowers_to_str_substr_with_arithmetic_args() {
        // Plain offsets.
        assert_eq!(
            lower_expr("substr(g, 0, lt)").expect("substr"),
            "(str.substr g 0 lt)"
        );
        // Arithmetic inside the args lowers recursively (offset + length).
        assert_eq!(
            lower_expr("substr(g, lt + 1, gt - lt - 1)").expect("substr arith"),
            "(str.substr g (+ lt 1) (- (- gt lt) 1))"
        );
    }

    #[test]
    fn replace_lowers_to_str_replace() {
        assert_eq!(
            lower_expr("replace(\"Seq(T)\", \"T\", arg)").expect("replace"),
            "(str.replace \"Seq(T)\" \"T\" arg)"
        );
    }

    #[test]
    fn call_composes_inside_concat_and_arithmetic() {
        // A call nested inside `++` and inside arithmetic lowers correctly.
        assert_eq!(
            lower_expr("substr(g, 0, lt) ++ \" / \"").expect("concat with call"),
            "(str.++ (str.substr g 0 lt) \" / \")"
        );
        assert_eq!(
            lower_expr("index_of(g, \"<\") + 1").expect("call in arith"),
            "(+ (str.indexof g \"<\" 0) 1)"
        );
    }

    #[test]
    fn unknown_function_call_is_err() {
        // An unsupported call name is an honest error, never a silent mis-handle.
        let err = lower_expr("frobnicate(g, 1)").unwrap_err();
        assert!(err.0.contains("frobnicate"), "err: {}", err.0);
        // Wrong arity is also rejected.
        let err = lower_expr("substr(g, 0)").unwrap_err();
        assert!(err.0.contains("substr"), "err: {}", err.0);
    }

    const TEST_39: &str = "import \"stdlib/runtime.ev\"\n\nenum StrDemoState = StrDemoRun | StrDemoHalt\n\nfsm string_demo(state ∈ StrDemoState)\n    state_next = match state\n        StrDemoRun ⇒ StrDemoHalt\n        StrDemoHalt ⇒ StrDemoHalt\n\n    g ∈ String = \"Edge<Rect>\"\n    lt ∈ Int = index_of(g, \"<\")\n    gt ∈ Int = index_of(g, \">\")\n    head ∈ String = substr(g, 0, lt)\n    arg ∈ String = substr(g, lt + 1, gt - lt - 1)\n    mono ∈ String = replace(\"Seq(T)\", \"T\", arg)\n\n    effects = match state\n        StrDemoRun ⇒ ⟨Println(head ++ \" / \" ++ arg ++ \" / \" ++ mono), Exit(0)⟩\n        StrDemoHalt ⇒ ⟨⟩\n";

    #[test]
    fn test_39_string_ops_lowering() {
        let fix = transpile_fsm(TEST_39).expect("transpile test_39");
        // The three string ops lower to Z3 string theory in the intermediate-var
        // defining asserts.
        assert!(fix.contains("(assert (= g \"Edge<Rect>\"))"), "g literal:\n{fix}");
        assert!(fix.contains("(assert (= lt (str.indexof g \"<\" 0)))"), "index_of <:\n{fix}");
        assert!(fix.contains("(assert (= gt (str.indexof g \">\" 0)))"), "index_of >:\n{fix}");
        assert!(fix.contains("(assert (= head (str.substr g 0 lt)))"), "substr head:\n{fix}");
        assert!(
            fix.contains("(assert (= arg (str.substr g (+ lt 1) (- (- gt lt) 1))))"),
            "substr arg (arithmetic args):\n{fix}"
        );
        assert!(
            fix.contains("(assert (= mono (str.replace \"Seq(T)\" \"T\" arg)))"),
            "replace mono:\n{fix}"
        );
        // The effects arm concatenates the three derived strings (bare-ident refs).
        assert!(
            fix.contains("(str.++ (str.++ (str.++ (str.++ head \" / \") arg) \" / \") mono)"),
            "effects concat:\n{fix}"
        );
        // The engine accepts the fixture (Z3 string theory decodes str.* natively).
        let prob = load_str(&fix).expect("engine loads test_39 fixture");
        assert_eq!(prob.fsms[0].name, "string_demo");
    }
}
