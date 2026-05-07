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
    pub traces: Vec<TraceDecl>,
    pub shaders: Vec<ShaderDecl>,
}

/// A trace test: drives a named program through a sequence of input
/// commands and asserts state/output at each step. Source form:
///
/// ```text
/// trace name "path/to/program.ev"
///     send "command" => assertion
///     send "command" =>
///         assertion1
///         assertion2
///     send "command"
/// ```
///
/// The trace runner loads the named program, sets up its main schema
/// for step-by-step execution, feeds each command's chars (with
/// trailing newline) through Stdin, and checks every assertion
/// against the resulting state and accumulated output.
#[derive(Debug, Clone)]
pub struct TraceDecl {
    pub name: String,
    pub program_path: String,
    pub steps: Vec<TraceStep>,
}

/// One step inside a trace block. Two flavors: the Stdin-shaped
/// `Send` (used by char-stream programs like adventures), and the
/// SDL-shaped `KeyDown` / `KeyUp` / `Advance` triplet (used by
/// frame-loop programs). Held-key state and the simulated wall clock
/// thread through the runner across all steps in one trace.
#[derive(Debug, Clone)]
pub enum TraceStep {
    /// `send "command" [=> assertion[s]]` — feed the command string
    /// char-by-char through the program's Stdin var, then check
    /// assertions against the post-line state and accumulated output.
    Send {
        command: String,
        assertions: Vec<TraceAssertion>,
    },
    /// `key_down "Right"` — start holding a named key. Subsequent
    /// `Advance` steps emit `input.<key>_held = true` per frame
    /// until a matching `KeyUp`. Recognized key names: `Up`, `Down`,
    /// `Left`, `Right` (mapped to the SDLInput `*_held` Bools).
    KeyDown { key: String },
    /// `key_up "Right"` — release a previously held key. Idempotent
    /// (releasing an unheld key is a no-op, matching the real SDL
    /// keyboard event stream).
    KeyUp { key: String },
    /// `advance 0.5s [=> assertion[s]]` — tick the SDL frame loop at
    /// a fixed dt (16ms) until `duration_ms` of simulated time has
    /// elapsed, advancing the program's state pair each frame.
    /// Assertions run after the last frame.
    Advance {
        duration_ms: u32,
        assertions: Vec<TraceAssertion>,
    },
}

/// A single trace assertion: `var op "value"`. `var` is either a
/// state field name or the literal `output` (which checks the
/// per-step accumulated Stdout text). Two operators:
///   - `=`  exact equality
///   - `∋`  substring containment (actual contains value)
#[derive(Debug, Clone)]
pub struct TraceAssertion {
    pub var: String,
    pub op: AssertOp,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertOp {
    Eq,
    Contains,
}

/// A GLSL fragment-shader declaration. Sibling to `claim`/`type`/
/// `trace` — never inlined into another schema's constraint system.
/// The runtime parses + transpiles + caches GLSL source at load time;
/// `SDLShaderOutput` (the new plugin/output type) references one by
/// name and uploads `state.*` / `input.*` bindings as uniforms each
/// frame.
///
/// Source form:
///
/// ```text
/// shader StarsAndHero
///     pixel ∈ Vec2          -- the swept fragment coord
///     state ∈ GameState     -- expected uniform shape (main supplies)
///     input ∈ SDLInput      -- ditto
///     twinkle ∈ Real        -- FREE → transpiler emits noise(...)
///     col ∈ Color
///     col = mix(Color(0,0,0), Color(255,100,50), twinkle)
///     output.fragment = col
/// ```
///
/// Variables in the body fall into three buckets at transpile time:
///   - Uniform: declared as part of a sub-record (`state ∈ GameState`).
///     Each leaf becomes a `uniform` declaration.
///   - Local: pinned by some constraint inside the body. Becomes a
///     GLSL temporary.
///   - Noise: declared with no pin and no parent record. The
///     transpiler emits a hash-based noise expression seeded on
///     `pixel` (and `input.time` if available).
#[derive(Debug, Clone)]
pub struct ShaderDecl {
    pub name: String,
    pub body: Vec<BodyItem>,
}
