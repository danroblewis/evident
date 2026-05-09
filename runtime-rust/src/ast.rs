//! AST node types — mirrors `parser/src/ast.py` for the v0.1 subset.
//!
//! Only what the v0.1 milestone (`SimpleNat { n ∈ Nat ; n > 5 }`) needs.
//! Add more variants as we expand support.

/// One of the three keywords that all parse to a "schema" header. Kept
/// distinct because some downstream features (subclaim, the type/claim/
/// schema reading convention) check it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keyword {
    Schema,
    Claim,
    Type,
    Subclaim,
}

/// Top-level schema declaration:
///   schema Name
///       <body>
#[derive(Debug, Clone)]
pub struct SchemaDecl {
    pub keyword: Keyword,
    pub name: String,
    pub body: Vec<BodyItem>,
}

/// A single line in a schema body.
#[derive(Debug, Clone)]
pub enum BodyItem {
    /// `x ∈ Type`  — declares a typed variable. For v0.1 the right side
    /// is always a simple identifier (e.g. `Nat`, `Int`, `Bool`).
    ///
    /// `pins` narrows the declared variable to the subset of `Type`
    /// satisfying additional per-field equalities. Two forms:
    ///
    ///   - Named: `x ∈ Type (a mapsto v1, b mapsto v2)` — unambiguous,
    ///     order-independent.
    ///   - Positional: `x ∈ Type (v1, v2, …)` — uses the type's field
    ///     declaration order. Strict: must match the field count.
    ///
    /// Both desugar to a series of `name.field = value` constraints
    /// at translate time. `Pins::None` for the bare `x ∈ Type` form.
    Membership { name: String, type_name: String, pins: Pins },

    /// `..ClaimName` — passthrough composition. The named claim's body
    /// is inlined into the current schema's body using names-match: any
    /// variable declared in the included claim with the same name as a
    /// variable in scope here resolves to the same Z3 const.
    Passthrough(String),

    /// `subclaim Name` — a claim defined inside the parent's body.
    /// Has no effect on the parent at translation time (it doesn't
    /// contribute constraints) but is registered into the runtime's
    /// schemas table at load time so other ClaimCall / passthrough
    /// items can reference it by name.
    SubclaimDecl(SchemaDecl),

    /// `ClaimName(slot mapsto value, …)` — claim composition with
    /// explicit renaming. Each `Mapping` binds the included claim's
    /// `slot` (a variable name in its body) to `value` (a literal or
    /// an existing Z3 binding from the current env).
    ClaimCall { name: String, mappings: Vec<Mapping> },

    /// Any other constraint (comparison, arithmetic equality, etc.).
    Constraint(Expr),
}

#[derive(Debug, Clone)]
pub struct Mapping {
    pub slot: String,
    pub value: Expr,
}

/// Optional per-field pinning for a `Membership`. See `Membership` for
/// the source-level forms each variant comes from.
#[derive(Debug, Clone)]
pub enum Pins {
    /// `x ∈ Type` — no pins, bare declaration.
    None,
    /// `x ∈ Type (a mapsto v1, b mapsto v2)` — explicit field names.
    /// Order-independent, partial allowed.
    Named(Vec<Mapping>),
    /// `x ∈ Type (v1, v2, …)` — positional, by field declaration order
    /// in the type's body. Translator looks up the SchemaDecl to resolve
    /// each position to a field name. Strict count match required.
    Positional(Vec<Expr>),
}

/// Expression tree. Compact for v0.1.
#[derive(Debug, Clone)]
pub enum Expr {
    Identifier(String),
    Int(i64),
    /// Real literal. Stored as f64 for ergonomics; converted to a Z3
    /// Real via the numeral's canonical decimal form (Rust's f64
    /// formatter gives `"3.14"` exactly for `3.14`, which Z3's
    /// `from_real_str` parses without precision loss for typical
    /// game/physics constants).
    Real(f64),
    Bool(bool),
    Str(String),
    /// `{e1, e2, …}` set literal — only used as the right side of `∈`
    /// (membership). Not a first-class set value (no Z3 set sort yet).
    SetLit(Vec<Expr>),
    /// `⟨e1, e2, …⟩` sequence literal. Used as the RHS of `=` against a
    /// `Seq(T)` variable. The translator pins both length and per-element
    /// values when it sees `seq_var = SeqLit(items)`.
    SeqLit(Vec<Expr>),
    /// `{lo..hi}` integer range — only used as a quantifier bound.
    Range(Box<Expr>, Box<Expr>),
    /// `lhs ∈ rhs` membership constraint as an expression. We always
    /// reduce this to a disjunction of equalities (lhs = e1 ∨ lhs = e2 ∨ …).
    InExpr(Box<Expr>, Box<Expr>),
    /// `∀ var ∈ range : body` and the existential variant.
    /// Translation requires `range` to be a literal `Range(Int, Int)`
    /// so we can unroll. The `Vec<String>` is the binding: usually
    /// length 1 (`∀ x ∈ …`), length ≥ 2 for tuple destructuring
    /// (`∀ (a, b) ∈ coindexed(A, B) : …` — pair iteration).
    Forall(Vec<String>, Box<Expr>, Box<Expr>),
    Exists(Vec<String>, Box<Expr>, Box<Expr>),
    /// `name(arg, …)` — function-call expression. Used for builtins
    /// like `coindexed(A, B, C)` and `edges(seq)` in quantifier source
    /// position. We don't have user-defined functions yet; the
    /// translator special-cases recognized names and errors on
    /// unrecognized ones.
    Call(String, Vec<Expr>),
    /// `#expr` — cardinality. For Seq translates to Z3 Length.
    Cardinality(Box<Expr>),
    /// `expr[index]` — sequence indexing. Translates to Z3 nth.
    Index(Box<Expr>, Box<Expr>),
    /// `expr.field` — field access on a non-Identifier expression
    /// (e.g. `pts[0].x`). Field access on a bare identifier still
    /// folds into a dotted `Identifier` at parse time; this variant
    /// is for cases where the receiver is itself an expression like
    /// `Index`. The runtime resolves these through the receiver's
    /// Datatype accessors rather than env-key lookup.
    Field(Box<Expr>, String),
    /// Binary operation: `lhs op rhs`.
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// Unary `¬e`.
    Not(Box<Expr>),
    /// Ternary conditional: `cond ? then_branch : else_branch`.
    /// Both branches must translate to the same Z3 sort. Maps to
    /// `Bool::ite(cond, then, else)` in the translator. Sits at
    /// lower precedence than `∨` and higher than `⇒`, so:
    ///   `a ∨ b ? c : d` parses as `(a ∨ b) ? c : d`
    ///   `a ⇒ b ? c : d` parses as `a ⇒ (b ? c : d)`
    /// Right-associative: `a ? b : c ? d : e` is `a ? b : (c ? d : e)`.
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// Pattern-match expression over an enum-typed scrutinee:
    /// ```text
    /// match r
    ///     Ok(n)  ⇒ n * 10
    ///     Err(_) ⇒ 0
    /// ```
    /// Translates to a nested Z3 ITE chain: `is_Ok(r) ? <arm1 with
    /// n bound to Ok_arg0(r)> : (is_Err(r) ? <arm2> : ...)`. All arm
    /// bodies must share a sort (same as ternary). Either all enum
    /// variants are covered or there's a wildcard `_ ⇒ ...` arm.
    Match(Box<Expr>, Vec<MatchArm>),
    /// Constructor-recognizer test: `e matches Ctor(_, _, ...)`.
    /// Returns Bool — true iff `e`'s variant tag is `Ctor`. Payload
    /// bindings are IGNORED in this form (`_` and bare names alike
    /// act as wildcards). For payload-aware extraction, use a `match`
    /// expression. For literal-payload comparison, use `e = Ctor(7)`.
    Matches(Box<Expr>, MatchPattern),
}

/// One arm of a `match` expression — a pattern + the body that fires
/// when the scrutinee matches.
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body:    Box<Expr>,
}

/// Match pattern. Either a constructor with positional bindings
/// (`Ctor(name, _, name2)`) or a wildcard `_` that catches any value.
/// A binding of `_` discards the corresponding payload field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {
    /// `Ctor(b0, b1, ...)`. `binds[i]` is the variable name to bind
    /// the i-th payload field to, or `None` if the binding was `_`
    /// (discard). Length must match the constructor's arity.
    Ctor { name: String, binds: Vec<Option<String>> },
    /// `_` — matches any value, no bindings.
    Wildcard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinOp {
    // Comparisons → Bool
    Eq, Neq, Lt, Le, Gt, Ge,
    // Bool ops
    And, Or, Implies,
    // Arithmetic → Int
    Add, Sub, Mul, Div,
    // String concatenation (`++`) → String
    Concat,
}

/// A parsed program (one or more top-level declarations).
///
/// `imports` is captured at parse time but consumed during loading
/// (see `EvidentRuntime::load_source` / `load_file`). After loading,
/// only the `schemas` survive into the runtime's IR.
#[derive(Debug, Clone, Default)]
pub struct Program {
    pub schemas: Vec<SchemaDecl>,
    pub imports: Vec<String>,
    pub enums: Vec<EnumDecl>,
}

/// An enum declaration: `enum Name = Variant1 | Variant2(T1, T2) | …`.
/// Variants are either nullary (no payload) or carry an ordered tuple
/// of typed fields. Self-references are allowed inside payload field
/// types (the enum being declared can reference itself), enabling
/// recursive types like `enum Expr = Int(Int) | Binary(BinOp, Expr, Expr)`.
/// Translates to a Z3 algebraic datatype with one constructor per
/// variant; payload field types resolve to either a primitive Z3 sort
/// or a `DatatypeAccessor::Datatype(self_name)` for recursion.
#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

/// One variant of an `EnumDecl`. Payload field names are auto-generated
/// (`f0`, `f1`, …) when callers don't supply them — sufficient for
/// positional Variant(T1, T2) syntax. Named-payload variants
/// (`Variant { x ∈ Int, y ∈ Int }`) are out of scope for v0.1.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<EnumField>,
}

/// One field of an enum payload. `type_name` is a raw textual type
/// reference (validated at registration time by looking it up against
/// primitive sorts, the EnumRegistry itself for self-references, or
/// future user types).
#[derive(Debug, Clone)]
pub struct EnumField {
    pub name: String,
    pub type_name: String,
}

// TraceDecl, TraceStep, TraceAssertion, AssertOp, ShaderDecl removed
// in Phase 2 plugin removal — the runners that consumed them are gone.

// ── Effect / Result / FfiArg ─────────────────────────────────────
//
// Mirror of `stdlib/runtime.ev`'s Effect/Result/FFIArg enums. The
// effect dispatcher in `effect_dispatch.rs` (Phase 1.3) walks
// Vec<Effect> and produces Vec<EffectResult>. Decoded from Z3
// datatype values by `decode_ast::decode_effect_list`.

/// One side-effect to perform between solver steps. Variants align
/// with the `Effect` enum in stdlib/runtime.ev.
#[derive(Debug, Clone)]
pub enum Effect {
    NoEffect,
    Print(String),
    Println(String),
    ReadLine,
    Time,
    Exit(i64),
    FFIOpen(String),
    FFILookup(u64, String),
    FFICall(u64, String, Vec<EffectFfiArg>),
    CloseHandle(u64),
    /// Cached one-shot FFI call: `LibCall(library, symbol, signature, args)`.
    /// The runtime lazily resolves and caches `library` + `symbol` so
    /// repeated calls amortize dlopen/dlsym to once. See
    /// `effect_dispatch::dispatch_one` for the cache implementation.
    LibCall(String, String, String, Vec<EffectFfiArg>),
}

/// One argument to an FFICall effect. Distinct name from
/// `ffi::FfiArg` to avoid the cross-module type clash; the
/// dispatcher converts when handing off to libffi.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectFfiArg {
    Int(i64),
    Bool(bool),
    Str(String),
    Real(f64),
    Handle(u64),
    /// `ArgStrArr(StrList)` — array of strings, marshalled as
    /// `const char * const *`. Needed for `glShaderSource` and any
    /// other API that wants a multi-string buffer.
    StrArr(Vec<String>),
}

/// Outcome of one performed effect. Position-aligned with the
/// previous step's effect list.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectResult {
    NoResult,
    Int(i64),
    Str(String),
    Bool(bool),
    Real(f64),
    Handle(u64),
    Error(String),
}
