//! AST node types for the Evident language.

/// Bare-identifier body items that are runtime metadata, not translatable constraints.
/// The translator skips these; scheduler/runtime layers inspect them directly.
pub const BODY_MARKERS: &[&str] = &["spawnable_only"];

/// Schema keyword variant — kept distinct so subclaim/type/claim/schema
/// dispatch and the scheduler's FSM detection can check it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Keyword {
    Schema,
    Claim,
    Type,
    Subclaim,
    /// The sole FSM signal: scheduler auto-instantiates `fsm` schemas; injects
    /// `state_next` / `last_results` / `effects` if the body omits them.
    Fsm,
}

/// Top-level schema declaration.
#[derive(Debug, Clone)]
pub struct SchemaDecl {
    pub keyword: Keyword,
    pub name: String,
    /// Type params (`["T"]` for `type Edge<T>`). Empty for non-generic schemas.
    /// `monomorphize_generics` produces concrete copies; the template is never translated.
    pub type_params: Vec<String>,
    pub body: Vec<BodyItem>,
    /// Leading body items from the first-line param list; treated as outer-bound
    /// slots (positional/names-match). Body items past this index get fresh Z3 consts.
    pub param_count: usize,
    /// `external type/claim` — only external schemas may emit FFI/LibCall effects.
    /// Cannot combine with `fsm` or `schema` (rejected at parse time).
    pub external: bool,
}

/// A single line in a schema body.
#[derive(Debug, Clone)]
pub enum BodyItem {
    /// `x ∈ Type` — declares a typed variable; `pins` narrows it with per-field equalities.
    /// Named (`a ↦ v`) or positional (`v1, v2`); `Pins::None` for bare declaration.
    Membership { name: String, type_name: String, pins: Pins },

    /// `..ClaimName` — inline the named claim's body via names-match.
    Passthrough(String),

    /// `subclaim Name` — registered at load time; no constraints added to parent.
    SubclaimDecl(SchemaDecl),

    /// `ClaimName(slot ↦ value, …)` — claim composition with explicit slot renaming.
    ClaimCall { name: String, mappings: Vec<Mapping> },

    /// Any other constraint (comparison, arithmetic equality, etc.).
    Constraint(Expr),
}

#[derive(Debug, Clone)]
pub struct Mapping {
    pub slot: String,
    pub value: Expr,
}

/// Per-field pinning for a `Membership` (`x ∈ Type (a ↦ v1, …)`).
#[derive(Debug, Clone)]
pub enum Pins {
    None,
    Named(Vec<Mapping>),
    Positional(Vec<Expr>),
}

/// Expression tree.
#[derive(Debug, Clone)]
pub enum Expr {
    Identifier(String),
    Int(i64),
    Real(f64),
    Bool(bool),
    Str(String),
    /// Set literal `{e1, e2, …}` — only valid as the RHS of `∈`; not a first-class set value.
    SetLit(Vec<Expr>),
    /// Sequence literal `⟨e1, e2, …⟩`; translator pins length + per-element values.
    SeqLit(Vec<Expr>),
    /// `{lo..hi}` integer range — quantifier bound only.
    Range(Box<Expr>, Box<Expr>),
    /// `lhs ∈ rhs` as an expression; reduces to a disjunction of equalities.
    InExpr(Box<Expr>, Box<Expr>),
    /// `(e1, e2, …)` tuple; used as LHS of `∈ claim_name` for positional invocation.
    Tuple(Vec<Expr>),
    /// `∀ vars ∈ range : body`; `Vec<String>` supports tuple destructuring for coindexed/edges.
    Forall(Vec<String>, Box<Expr>, Box<Expr>),
    Exists(Vec<String>, Box<Expr>, Box<Expr>),
    /// `name(arg, …)` — builtins (`coindexed`, `edges`, …); unrecognized names error.
    Call(String, Vec<Expr>),
    /// `#expr` — cardinality; for Seq translates to Z3 Length.
    Cardinality(Box<Expr>),
    /// `expr[index]` — sequence indexing; translates to Z3 nth.
    Index(Box<Expr>, Box<Expr>),
    /// `expr.field` — field access on a non-Identifier expression (e.g. `pts[0].x`).
    /// Resolved via Datatype accessors, not env-key lookup.
    Field(Box<Expr>, String),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    /// `cond ? then : else` — same Z3 sort required on both branches; right-associative.
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `match scrutinee \n Ctor(n) ⇒ body …` — nested Z3 ITE chain over enum variants.
    /// All arms must share a sort; exhaustive or has wildcard `_ ⇒ …`.
    Match(Box<Expr>, Vec<MatchArm>),
    /// `e matches Ctor(_, …)` — Bool recognizer; payload bindings ignored.
    /// Use `match` to extract payload; use `e = Ctor(7)` for literal-payload comparison.
    Matches(Box<Expr>, MatchPattern),
}

/// One arm of a `match` expression.
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub body:    Box<Expr>,
}

/// Recursive match pattern. Uppercase-initial = constructor; lowercase-initial = binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {
    /// `Ctor(p0, p1, …)` — matches by variant tag; `binds[i]` sub-pattern per payload field.
    Ctor { name: String, binds: Vec<MatchPattern> },
    Bind(String),
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

/// A parsed program. `imports` consumed during loading; only `schemas` survive into the IR.
#[derive(Debug, Clone, Default)]
pub struct Program {
    pub schemas: Vec<SchemaDecl>,
    pub imports: Vec<String>,
    pub enums: Vec<EnumDecl>,
}

/// `enum Name = Variant1 | Variant2(T1, T2) | …`. Self-references allowed (recursive types).
/// Translates to a Z3 algebraic datatype; payload types resolve to primitive sorts or self-ref.
#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

/// One variant. Payload field names auto-generated (`f0`, `f1`, …) for positional syntax.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<EnumField>,
}

/// One payload field. `type_name` validated at registration against primitive sorts + EnumRegistry.
#[derive(Debug, Clone)]
pub struct EnumField {
    pub name: String,
    pub type_name: String,
}

