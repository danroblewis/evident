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

    /// `halts_within(F, N)` — REMOVED surface (halting is implicit in the embed
    /// constraint `F(seed, fsm_state)`). The parser no longer produces this; the
    /// variant is retained vestigially for the encode/decode AST mirror only.
    HaltsWithin { fsm_name: String, n: i64 },
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

    /// Settled-state of an embedded FSM. Produced ONLY by `lower_fsm_application`
    /// (`runtime/nested.rs`) from a 2-arg call to an `fsm`-keyword schema
    /// `F(seed, fsm_state)` → `fsm_state = RunFsm{ fsm: F, init: seed }`. No longer
    /// a parser hook (`run(...)` was removed). Resolved to a literal before the
    /// outer solve in the forward regime; FSM must be a single state pair (validated
    /// at load via `validate_run_targets`).
    RunFsm { fsm: String, init: Box<Expr> },
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

/// Mirror of `stdlib/runtime.ev`'s Effect enum. Dispatcher walks `Vec<Effect>`,
/// produces `Vec<EffectResult>`, decoded from `Value::SeqEnum` by `decode_effect_list`.
#[derive(Debug, Clone)]
pub enum Effect {
    NoEffect,
    Print(String),
    Println(String),
    ReadLine,
    Time,
    Exit(i64),
    ParseInt(String),
    ParseReal(String),
    IntToStr(i64),
    RealToStr(f64),
    /// Shell command (synchronous, stdout captured). StringResult on success (exit 0);
    /// ErrorResult with exit code + stderr otherwise. Trailing newline stripped.
    ShellRun(String),
    /// Spawn FSM of named claim; Int arg seeded into first state-variant payload.
    /// Next tick eligible; result is IntResult(instance_id). v1: shares parent's world.
    SpawnFsm(String, i64),
    FFIOpen(String),
    FFILookup(u64, String),
    FFICall(u64, String, Vec<EffectFfiArg>),
    CloseHandle(u64),
    /// Cached one-shot FFI call; lazily resolves and caches dlopen/dlsym.
    LibCall(String, String, String, Vec<EffectFfiArg>),
    /// `ReadByte(handle, offset)` → `IntResult(byte ∈ 0..255)`. Caller responsible for bounds;
    /// out-of-bounds is UB. Primary use: flat C memory (e.g. SDL_GetKeyboardState array).
    ReadByte(u64, i64),
    /// Signed reads, all unaligned; result sign-extended into i64.
    ReadI16(u64, i64),
    ReadI32(u64, i64),
    ReadI64(u64, i64),
    /// Float reads (unaligned); f32 widened to f64.
    ReadF32(u64, i64),
    ReadF64(u64, i64),
    /// Null-terminated UTF-8 read; invalid UTF-8 → ErrorResult. No length cap; caller must trust buffer.
    ReadStr(u64, i64),
    /// Unaligned memory writes; all return NoResult. Caller must hold a writable handle.
    WriteByte(u64, i64, i64),         // value's low byte stored
    WriteI16(u64, i64, i64),
    WriteI32(u64, i64, i64),
    WriteI64(u64, i64, i64),
    WriteF32(u64, i64, f64),          // narrowed to f32
    WriteF64(u64, i64, f64),
    /// Writes UTF-8 bytes + NUL terminator. Caller must have allocated `bytes + 1` at offset.
    WriteStr(u64, i64, String),
    /// Alloc `size` zeroed bytes; registered with `libc::free`. `IntResult(handle)`.
    Malloc(i64),
    /// Strictly-increasing nanosecond counter. `Time` gives Unix-epoch ms (NTP-affected); this doesn't.
    MonotonicTime,
    /// NOT YET IMPLEMENTED — register an Evident claim as a C-callable libffi closure.
    /// Needs: closure setup, thread-safe C-thread→scheduler dispatch, effect-in-callback decision.
    RegisterCallback(String, String),
}

/// One field of a packed C struct (`ArgPackedBuf`). The stdlib wrapper chooses
/// the field sequence to match the C layout; the runtime has no opinion on shape.
#[derive(Debug, Clone, PartialEq)]
pub enum PackedField {
    U8(u8),
    I32(i32),
    F32(f32),
}

impl PackedField {
    /// Append little-endian bytes to `out` for `ArgPackedBuf` heap assembly.
    pub fn write_le(&self, out: &mut Vec<u8>) {
        match self {
            PackedField::U8(b)  => out.push(*b),
            PackedField::I32(n) => out.extend_from_slice(&n.to_le_bytes()),
            PackedField::F32(f) => out.extend_from_slice(&f.to_le_bytes()),
        }
    }
}

/// FFICall argument. Distinct from `ffi::FfiArg` to avoid cross-module clash;
/// dispatcher converts when handing off to libffi.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectFfiArg {
    Int(i64),
    Bool(bool),
    Str(String),
    Real(f64),
    Handle(u64),
    /// `const char * const *` array; used by `glShaderSource` and similar.
    StrArr(Vec<String>),
    /// Back-reference to Nth prior call's result within an `Effect::Seq`. Index local to Seq.
    PriorResult(usize),
    /// N i32s packed into a heap buffer (call duration only); e.g. `SDL_Rect` (4×i32).
    I32Buf(Vec<i32>),
    /// `PackedField` sequence packed at natural widths; stdlib wrapper matches the C layout.
    PackedBuf(Vec<PackedField>),
    /// Output-i32 slot for void-return C funcs (`glGenVertexArrays(1, &vao)`).
    /// Dispatcher allocates stable i32, passes pointer, surfaces result as `IntResult`.
    IntOut,
}

/// Outcome of one performed effect, position-aligned with the prior tick's effect list.
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
