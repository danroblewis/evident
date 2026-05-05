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
    Membership { name: String, type_name: String },

    /// Any other constraint (comparison, arithmetic equality, etc.).
    Constraint(Expr),
}

/// Expression tree. Compact for v0.1.
#[derive(Debug, Clone)]
pub enum Expr {
    Identifier(String),
    Int(i64),
    Bool(bool),
    Str(String),
    /// `{e1, e2, …}` set literal — only used as the right side of `∈`
    /// (membership). Not a first-class set value (no Z3 set sort yet).
    SetLit(Vec<Expr>),
    /// `{lo..hi}` integer range — only used as a quantifier bound.
    Range(Box<Expr>, Box<Expr>),
    /// `lhs ∈ rhs` membership constraint as an expression. We always
    /// reduce this to a disjunction of equalities (lhs = e1 ∨ lhs = e2 ∨ …).
    InExpr(Box<Expr>, Box<Expr>),
    /// `∀ var ∈ range : body` and the existential variant.
    /// Translation requires `range` to be a literal `Range(Int, Int)`
    /// so we can unroll.
    Forall(String, Box<Expr>, Box<Expr>),
    Exists(String, Box<Expr>, Box<Expr>),
    /// Binary operation: `lhs op rhs`.
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// Unary `¬e`.
    Not(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinOp {
    // Comparisons → Bool
    Eq, Neq, Lt, Le, Gt, Ge,
    // Bool ops
    And, Or, Implies,
    // Arithmetic → Int
    Add, Sub, Mul, Div,
}

/// A parsed program (one or more top-level declarations).
#[derive(Debug, Clone, Default)]
pub struct Program {
    pub schemas: Vec<SchemaDecl>,
}
